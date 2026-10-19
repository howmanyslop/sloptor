package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"math"
	"os"
	"testing"
)

// Keep JSON integers exact while editing every finished observation once.
func changeResourceMemoryJournal(t *testing.T, journal []byte, change func(int, map[string]any)) []byte {
	t.Helper()
	lines := bytes.Split(bytes.TrimSuffix(journal, []byte{'\n'}), []byte{'\n'})
	for i, line := range lines {
		var event map[string]any
		decoder := json.NewDecoder(bytes.NewReader(line))
		decoder.UseNumber()
		if err := decoder.Decode(&event); err != nil {
			t.Fatal(err)
		}
		if event["event"] == "finished" {
			change(i, event["result"].(map[string]any)["ResourceAccounting"].(map[string]any))
			var err error
			lines[i], err = json.Marshal(event)
			if err != nil {
				t.Fatal(err)
			}
		}
	}
	return append(bytes.Join(lines, []byte{'\n'}), '\n')
}

func resourceMemoryLayoutJournal(t *testing.T, journal []byte, size uint64) []byte {
	t.Helper()
	return changeResourceMemoryJournal(t, journal, func(_ int, accounting map[string]any) {
		accounting["JobMemory"].(map[string]any)["Values"].(map[string]any)["StructureBytes"] = size
	})
}

func assertResourceMemoryRetention(t *testing.T, b resourceBatch, manifest, journal, run []byte, invalid bool) {
	t.Helper()
	if resourceAdmissionHasKind(b, invalidEvidence) != invalid || len(b.Observations) != 122 ||
		!bytes.Equal(b.RawManifest.Bytes, manifest) || !bytes.Equal(b.RawJournal.Bytes, journal) || !bytes.Equal(b.RawRun.Bytes, run) {
		t.Fatalf("wrong admission or lost evidence: %+v", b.Issues)
	}
	lines := bytes.Split(bytes.TrimSuffix(journal, []byte{'\n'}), []byte{'\n'})
	for i, o := range b.Observations {
		if !bytes.Equal(o.Raw, lines[i]) {
			t.Fatal("journal observation changed")
		}
	}
	if b.Observations[121].Scored || b.Observations[121].Classification != "post-sample-timings-diagnostic" {
		t.Fatal("diagnostic lost its unscored classification")
	}
	if invalid && len(b.Resources) != 0 {
		t.Fatal("invalid evidence produced a scored resource population")
	}
}

func TestResourceMemoryNativeRange(t *testing.T) {
	for _, size := range []uint64{112, 144} {
		for _, field := range []string{"PeakJobMemoryBytes", "PeakProcessMemoryBytes"} {
			for _, value := range []uint64{0, math.MaxUint32 - 1, math.MaxUint32, math.MaxUint32 + 1, math.MaxUint64} {
				t.Run(fmt.Sprintf("%d/%s/%d", size, field, value), func(t *testing.T) {
					m, j, r := resourceAdmissionFixture(t, 10, 20)
					r = resourceAdmissionNativeMetadata(t, r)
					if size == 144 { // Deliberately opposite child and collector widths.
						r = bytes.ReplaceAll(r, []byte(`"GOARCH":"amd64"`), []byte(`"GOARCH":"386"`))
					}
					j = resourceMemoryLayoutJournal(t, j, size)
					j = changeResourceMemoryJournal(t, j, func(i int, a map[string]any) {
						if i == 1 {
							a["JobMemory"].(map[string]any)["Values"].(map[string]any)[field] = value
							a["Status"] = "partial"
							a["OtherOwnedCpu"] = map[string]any{"Status": "failed", "Error": "optional attribution failed", "Values": nil}
						}
					})
					b := admitResourceBatch(m, j, r, config{cold, .95, 10, 20})
					invalid := size == 112 && value > math.MaxUint32
					assertResourceMemoryRetention(t, b, m, j, r, invalid)
					if !invalid && (len(b.Issues) != 0 || len(b.Resources) != 8 || b.ResourceStatus != "complete") {
						t.Fatalf("valid native range or optional failure rejected: %+v", b.Issues)
					}
					if !invalid && field == "PeakJobMemoryBytes" && b.Observations[1].CounterValues["job_peak_bytes"] != value {
						t.Fatal("exact peak counter lost")
					}
				})
			}
		}
	}
}

func TestResourceMemorySingleCollectorLayout(t *testing.T) {
	for _, size := range []uint64{112, 144} {
		for _, line := range []int{-1, 1, 119, 121} { // Uniform control, first, last scored, diagnostic.
			t.Run(fmt.Sprintf("%d/line%d", size, line), func(t *testing.T) {
				m, j, r := resourceAdmissionFixture(t, 10, 20)
				j = resourceMemoryLayoutJournal(t, j, size)
				j = changeResourceMemoryJournal(t, j, func(i int, a map[string]any) {
					if i == line {
						a["JobMemory"].(map[string]any)["Values"].(map[string]any)["StructureBytes"] = 256 - size
					}
				})
				b := admitResourceBatch(m, j, r, config{cold, .95, 10, 20})
				assertResourceMemoryRetention(t, b, m, j, r, line != -1)
				if line == -1 && (len(b.Issues) != 0 || len(b.Resources) != 8) {
					t.Fatalf("uniform layout rejected: %+v", b.Issues)
				}
			})
		}
	}
}

func TestResourceMemoryMissingAndDiagnostic(t *testing.T) {
	for _, status := range []string{"failed", "unsupported", "unavailable", "available"} {
		for _, diagnostic := range []bool{false, true} {
			t.Run(fmt.Sprintf("%s/diagnostic%t", status, diagnostic), func(t *testing.T) {
				m, j, r := resourceAdmissionFixture(t, 10, 20)
				j = resourceMemoryLayoutJournal(t, j, 112)
				j = changeResourceMemoryJournal(t, j, func(i int, a map[string]any) {
					if i == 1 && !diagnostic || i == 121 && diagnostic {
						if status == "available" {
							a["JobMemory"].(map[string]any)["Values"].(map[string]any)["PeakProcessMemoryBytes"] = uint64(math.MaxUint32) + 1
						} else {
							a["Status"] = "partial"
							a["JobMemory"] = map[string]any{"Status": status, "Error": "native snapshot missing", "Values": nil}
						}
					}
				})
				b := admitResourceBatch(m, j, r, config{cold, .95, 10, 20})
				assertResourceMemoryRetention(t, b, m, j, r, status == "available")
				if status == "available" {
					return
				}
				line := 1
				if diagnostic {
					line = 121
				}
				if b.Observations[line].ComponentCauses["JobMemory"] != "native snapshot missing" {
					t.Fatal("missing snapshot cause lost")
				}
				if diagnostic {
					if len(b.Issues) != 0 || len(b.Resources) != 8 || b.ResourceStatus != "complete" {
						t.Fatalf("missing diagnostic polluted scored evidence: %+v", b.Issues)
					}
				} else if b.ResourceStatus != incompleteResource || len(b.Resources) != 7 || b.Resources["job_peak_bytes"] != nil {
					t.Fatal("missing required snapshot became zero or filtered population")
				}
			})
		}
	}
}

func TestResourceMemoryLayoutsAcrossSeparateBatches(t *testing.T) {
	aa := writeResourceCampaignFixture(t, 0, 1000)
	initial := writeResourceCampaignFixture(t, 2, 900)
	confirmation := writeResourceCampaignFixture(t, 4, 900)
	j, err := os.ReadFile(aa.Journal)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(aa.Journal, resourceMemoryLayoutJournal(t, j, 112), 0o600); err != nil {
		t.Fatal(err)
	}
	p := resourcePlan{Schema: 1, Fixture: "synthetic-layouts", Mode: "candidate", Role: "primary", Primary: cold, Initial: initial, Calibration: &calibrationPaths{Initial: aa}, Confirmation: &confirmation}
	data, err := json.Marshal(p)
	if err != nil {
		t.Fatal(err)
	}
	r, err := evaluateResourcePlan(data, t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	if r.Outcome != criterionMet || r.PromotionReady {
		t.Fatalf("independent batches incorrectly require equal collector layouts: %+v", r.Issues)
	}
}
