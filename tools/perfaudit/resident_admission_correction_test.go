package main

import (
	"encoding/json"
	"fmt"
	"math"
	"math/big"
	"path/filepath"
	"strconv"
	"testing"
	"time"
)

func residentTestInt(t *testing.T, v any) int64 {
	t.Helper()
	n, err := strconv.ParseInt(fmt.Sprint(v), 10, 64)
	if err != nil {
		t.Fatal(err)
	}
	return n
}

func residentShiftInvocation(f *residentProtocolFixture, event map[string]any, shift int64) {
	f.t.Helper()
	move := func(ep map[string]any) {
		n := new(big.Int).Add(big.NewInt(residentTestInt(f.t, ep["ticks"])), big.NewInt(shift))
		if !n.IsInt64() {
			f.t.Fatal("invented endpoint would overflow")
		}
		ep["ticks"] = n.Int64()
	}
	enc := event["enclosure"].(map[string]any)
	for _, k := range []string{"start", "end"} {
		move(enc[k].(map[string]any))
	}
	f.editReport(event, func(report map[string]any) {
		for _, rec := range report["records"].([]any) {
			for _, k := range []string{"start", "end"} {
				move(rec.(map[string]any)[k].(map[string]any))
			}
		}
	})
}

func TestResidentReaderInvocationSerialBoundaries(t *testing.T) {
	for _, gap := range []int64{-2000000000, -20000000, -2, -1, 0, 1, 20000000} {
		t.Run(fmt.Sprint(gap), func(t *testing.T) {
			f := newResidentProtocolFixture(t, residentTestKey())
			b := f.add("calibration-initial", 1, 100)
			events := f.events(b)
			firstEnd := residentTestInt(t, events[1]["enclosure"].(map[string]any)["end"].(map[string]any)["ticks"])
			secondStart := residentTestInt(t, events[3]["enclosure"].(map[string]any)["start"].(map[string]any)["ticks"])
			residentShiftInvocation(f, events[3], firstEnd-secondStart+gap)
			f.seal(b, events)
			f.authorize()
			got := f.analyze()
			f.write(filepath.Join(f.root, "serial-audit.json"), got)
			want := criterionMet
			if gap < -1 {
				want = invalidEvidence
			}
			if got.Outcome != want || len(got.Batches[0].Clocks) != 20 || len(got.Batches[0].Reconstructed.Blocks) != 5 {
				t.Fatalf("gap=%d want=%s got=%s issues=%+v", gap, want, got.Outcome, got.Issues)
			}
			// Exact endpoints are retained alongside the unmodified durations.
			if got.Batches[0].Clocks[1].OuterStart != firstEnd+gap {
				t.Fatal("raw start lost")
			}
		})
	}
}

func TestResidentReaderSerialSignedExtremes(t *testing.T) {
	for _, tc := range []struct {
		name           string
		previous, next int64
		valid          bool
	}{
		{"huge-forward", math.MinInt64, math.MaxInt64, true},
		{"huge-backwards", math.MaxInt64, math.MinInt64, false},
		{"min-minus-one", math.MinInt64 + 1, math.MinInt64, true},
		{"min-minus-two", math.MinInt64 + 2, math.MinInt64, false},
		{"max-minus-one", math.MaxInt64, math.MaxInt64 - 1, true},
		{"max-minus-two", math.MaxInt64, math.MaxInt64 - 2, false},
		{"min-zero", math.MinInt64, math.MinInt64, true},
		{"max-zero", math.MaxInt64, math.MaxInt64, true},
	} {
		t.Run(tc.name, func(t *testing.T) {
			r := newResidentReader()
			r.serialClock(residentClockObservation{OuterEnd: tc.previous}, residentClockObservation{OuterStart: tc.next}, "invented")
			if (len(r.issues) == 0) != tc.valid {
				t.Fatal(r.issues)
			}
		})
	}
	for _, start := range []int64{math.MinInt64, math.MaxInt64 - 1000000000} {
		t.Run(fmt.Sprint(start), func(t *testing.T) {
			f := newResidentProtocolFixture(t, residentTestKey())
			b := f.add("calibration-initial", 1, 100)
			events := f.events(b)
			i := 1
			if start > 0 {
				i = len(events) - 1
			}
			old := residentTestInt(t, events[i]["enclosure"].(map[string]any)["start"].(map[string]any)["ticks"])
			residentShiftInvocation(f, events[i], start-old)
			f.seal(b, events)
			got, _ := f.batchReport(b)
			if got.Status != criterionMet || got.Clocks[i/2].OuterStart != start {
				t.Fatal(got.Issues)
			}
		})
	}
}

func TestResidentReaderGlobalDomainJoins(t *testing.T) {
	for _, sameDomain := range []bool{true, false} {
		t.Run(fmt.Sprint(sameDomain), func(t *testing.T) {
			p, s := residentChronologyFixture(t, [3]int{1, 2, 3}, [3]int{4, 5, 6})
			if !sameDomain {
				// Restore each secondary host/support to its independent fixture
				// binding, so cross-host QPC values have no justified common origin.
				for _, ref := range s.refs {
					host := s.read(filepath.Join(ref.Directory, "host.before.json"))
					host["qpc_support"].(map[string]any)["corelib_path"] = filepath.Join(s.root, "unrelated-CoreLib.dll")
					s.write(filepath.Join(ref.Directory, "host.before.json"), host)
					s.write(filepath.Join(ref.Directory, "host.after.json"), host)
					events := s.events(ref)
					for i, event := range events {
						if i%2 == 1 {
							event["enclosure"].(map[string]any)["support"] = host["qpc_support"]
						}
					}
					s.seal(ref, events)
				}
			}
			// Shift every secondary enclosure/cycle back into the primary QPC
			// range; UTC, IDs and each individual invocation remain consistent.
			for _, ref := range s.refs {
				events := s.events(ref)
				for i, event := range events {
					if i%2 == 1 {
						residentShiftInvocation(s, event, -3000000000000)
					}
				}
				s.seal(ref, events)
			}
			p.plan.Campaigns = append(p.plan.Campaigns, s.plan.Campaigns[0])
			p.refs = append(p.refs, s.refs...)
			p.authorize()
			got := p.analyze()
			want := criterionMet
			if sameDomain {
				want = invalidEvidence
			}
			if got.Outcome != want {
				t.Fatalf("sameDomain=%v want=%s got=%s issues=%v", sameDomain, want, got.Outcome, got.Issues)
			}
		})
	}
	// Invalid/zero ordinals and backwards intervals cannot be rescued by an
	// otherwise well-formed sorted schedule. A clone preserves the supplied view.
	for _, ordinal := range []int{0, -1} {
		r := newResidentReader()
		stamp := time.Date(2026, 10, 18, 12, 0, 0, 0, time.UTC)
		b := residentRawBatch{Ref: residentBatchRef{Chronology: ordinal}, Start: stamp, Finish: stamp}
		r.chronology([]residentRawBatch{b})
		if len(r.issues) == 0 {
			t.Fatal("nonpositive global ordinal")
		}
	}
	// Valid clock evidence remains adverse even if another required metric is
	// missing. Incompleteness cannot hide a contradictory cross-batch join.
	f := newResidentProtocolFixture(t, residentTestKey())
	f.add("calibration-initial", 1, 100)
	b := f.add("initial", 2, 80)
	events := f.events(b)
	for i, event := range events {
		if i%2 == 1 {
			residentShiftInvocation(f, event, -1000000000000)
		}
	}
	native := events[1]["result"].(map[string]any)
	delete(native["ResourceAccounting"].(map[string]any)["JobAccounting"].(map[string]any)["Values"].(map[string]any), "WriteBytes")
	events[1]["native_result_json"] = string(mustJSON(native))
	f.seal(b, events)
	f.authorize()
	got := f.analyze()
	if got.Outcome != invalidEvidence || !residentProblem(got.Issues, "serial invocation") || got.Batches[1].Status != residentIncomplete {
		t.Fatalf("incomplete resource hid adverse clock evidence: %s %+v", got.Outcome, got.Issues)
	}
}

func TestResidentReaderProducerErrorStrings(t *testing.T) {
	for _, field := range []string{"report-error", "firstFailure", "record-error", "initializationError", "verificationError", "call_error", "assessment-error"} {
		for _, null := range []bool{true, false} {
			t.Run(fmt.Sprintf("%s/null=%v", field, null), func(t *testing.T) {
				f := newResidentProtocolFixture(t, residentTestKey())
				b := f.add("calibration-initial", 1, 100)
				events := f.events(b)
				var value any = ""
				if null {
					value = nil
				}
				switch field {
				case "call_error":
					events[1]["enclosure"].(map[string]any)[field] = value
				case "assessment-error":
					events[1]["clock_assessment"].(map[string]any)["error"] = value
				default:
					f.editReport(events[1], func(m map[string]any) {
						switch field {
						case "report-error":
							m["error"] = value
						case "firstFailure":
							m[field] = value
						case "record-error":
							m["records"].([]any)[0].(map[string]any)["error"] = value
						default:
							m["clock"].(map[string]any)[field] = value
						}
					})
				}
				f.seal(b, events)
				f.authorize()
				got := f.analyze()
				want := criterionMet
				if null {
					want = invalidEvidence
				}
				f.write(filepath.Join(f.root, "string-audit.json"), got)
				if got.Outcome != want {
					t.Fatalf("want=%s got=%s issues=%v", want, got.Outcome, got.Issues)
				}
			})
		}
	}
}

func TestResidentReaderManifestStringKinds(t *testing.T) {
	for _, null := range []bool{true, false} {
		t.Run(fmt.Sprint(null), func(t *testing.T) {
			f := newResidentProtocolFixture(t, residentTestKey())
			b := f.add("calibration-initial", 1, 100)
			path := filepath.Join(f.root, "inputs", "build-B.json")
			build := f.read(path)
			lines := build["effectiveProductionGoManifest"].([]any)
			var value any = ""
			if null {
				value = nil
			}
			build["effectiveProductionGoManifest"] = append(lines, value)
			hash := residentTestHash([]byte(lines[0].(string) + "\n\n"))
			build["effectiveProductionGoManifestSHA256"] = hash
			plan := f.read(b.Plan.Path)
			for _, a := range plan["arms"].(map[string]any) {
				a.(map[string]any)["productionManifestSha256"] = hash
			}
			f.write(b.Plan.Path, plan)
			b = f.rebindInput(b, path, build)
			f.refs[0] = b
			f.plan.Campaigns[0].Calibration[0] = b
			events := f.events(b)
			for _, e := range events {
				e["production_manifest_sha256"] = hash
			}
			f.seal(b, events)
			f.authorize()
			got := f.analyze()
			want := criterionMet
			if null {
				want = invalidEvidence
			}
			if got.Outcome != want {
				t.Fatalf("want=%s got=%s issues=%v", want, got.Outcome, got.Issues)
			}
		})
	}
}

func TestResidentReaderNativeAndRetentionNullPreserved(t *testing.T) {
	f := newResidentProtocolFixture(t, residentTestKey())
	b := f.add("calibration-initial", 1, 100)
	events := f.events(b)
	native := events[1]["result"].(map[string]any)
	if native["Error"] != nil || native["ResourceAccounting"].(map[string]any)["CompletionError"] != nil {
		t.Fatal("bad native null control")
	}
	manifest := f.read(filepath.Join(b.Directory, "evidence.manifest.json"))
	for _, raw := range manifest["retained"].([]any) {
		row := raw.(map[string]any)
		if row["error"] != nil || row["retention_error"] != nil {
			t.Fatal("bad retention null control")
		}
	}
	f.authorize()
	got := f.analyze()
	if got.Outcome != criterionMet {
		t.Fatal(got.Issues)
	}
	data := mustJSON(got.Batches[0].Clocks[0])
	var clock residentClockObservation
	if err := json.Unmarshal(data, &clock); err != nil || clock.OuterStart != 0 || clock.OuterEnd != 1000000000 {
		t.Fatal("endpoint JSON precision lost")
	}
}
