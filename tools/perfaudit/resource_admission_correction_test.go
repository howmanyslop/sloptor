package main

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestResourceCorrectionMalformedRevisionCLI(t *testing.T) {
	for _, revision := range []string{`{"value":"sha256:bad"}`, `["sha256:bad"]`, `true`} {
		t.Run(revision, func(t *testing.T) {
			paths := writeResourceCampaignFixture(t, 0, 1000)
			m, err := os.ReadFile(paths.Manifest)
			if err != nil {
				t.Fatal(err)
			}
			var root object
			_ = json.Unmarshal(m, &root)
			for _, arm := range []string{"baseline", "candidate"} {
				var identity object
				_ = json.Unmarshal(root[arm], &identity)
				identity["revision"] = json.RawMessage(revision)
				root[arm], _ = json.Marshal(identity)
			}
			m, _ = json.Marshal(root)
			if err := os.WriteFile(paths.Manifest, m, 0o600); err != nil {
				t.Fatal(err)
			}
			plan, _ := json.Marshal(resourcePlan{Schema: 1, Fixture: "synthetic", Mode: "calibration", Role: "primary", Primary: cold, Initial: paths})
			dir := t.TempDir()
			input, output := filepath.Join(dir, "plan.json"), filepath.Join(dir, "audit.json")
			if err := os.WriteFile(input, plan, 0o600); err != nil {
				t.Fatal(err)
			}
			var stdout, stderr bytes.Buffer
			if code := run([]string{"--resource-plan", input, "--output", output}, &stdout, &stderr); code != 1 {
				t.Fatalf("got exit %d: %s", code, &stderr)
			}
			encoded, err := os.ReadFile(output)
			if err != nil {
				t.Fatal(err)
			}
			var report resourceReport
			if err := json.Unmarshal(encoded, &report); err != nil {
				t.Fatal(err)
			}
			batch := report.Comparison.Batches["initial"]
			if report.Outcome != invalidEvidence || !resourceAdmissionHasKind(batch, invalidEvidence) || !bytes.Equal(m, batch.RawManifest.Bytes) || len(batch.Observations) != 522 {
				t.Fatal("invalid shape lost its original issues or raw observations")
			}
		})
	}
}

func TestResourceCorrectionRunFieldsAndDescriptors(t *testing.T) {
	m, j, r := resourceAdmissionFixture(t, 10, 20)
	for _, tc := range []struct {
		key   string
		value any
	}{
		{"cleanup_error", "ownership-safe cleanup failed"}, {"cleanup_error", false}, {"CLEANUP_ERROR", nil},
		{"scope", 42}, {"scope", ""}, {"SCOPE", "shadow"},
		{"tools", 42}, {"tools", []any{}}, {"tools", map[string]any{"go": 42}}, {"tools", map[string]any{"GO": "shadow"}}, {"TOOLS", map[string]any{}},
	} {
		t.Run(tc.key+"/"+string(mustResourceJSON(t, tc.value)), func(t *testing.T) {
			bad := changeResourceRun(t, r, func(run map[string]any) { run[tc.key] = tc.value })
			b := admitResourceBatch(m, j, bad, config{cold, .95, 10, 20})
			if !resourceAdmissionHasKind(b, invalidEvidence) || !bytes.Equal(b.RawRun.Bytes, bad) {
				t.Fatalf("known field contradiction accepted: %+v", b.Issues)
			}
		})
	}
	for key, contradiction := range map[string]string{"boundary": "during wall", "cpu_units": "seconds", "memory": "RSS bytes", "io": "durable filesystem writes"} {
		t.Run(key, func(t *testing.T) {
			bad := changeResourceRun(t, r, func(run map[string]any) { run["resource_accounting"].(map[string]any)[key] = contradiction })
			if b := admitResourceBatch(m, j, bad, config{cold, .95, 10, 20}); !resourceAdmissionHasKind(b, invalidEvidence) {
				t.Fatal("contradictory fixed format semantics admitted")
			}
		})
	}
	valid := changeResourceRun(t, r, func(run map[string]any) {
		run["cleanup_error"], run["scope"] = nil, "synthetic unscored"
		run["tools"] = map[string]any{"go": "go1.26.9", "future": "extension"}
		run["future"] = map[string]any{"nested": []any{1, true, nil}}
	})
	if b := admitResourceBatch(m, j, valid, config{cold, .95, 10, 20}); len(b.Issues) != 0 {
		t.Fatalf("valid optional/future fields rejected: %+v", b.Issues)
	}
}

func mustResourceJSON(t *testing.T, value any) []byte {
	t.Helper()
	data, err := json.Marshal(value)
	if err != nil {
		t.Fatal(err)
	}
	return data
}

func TestResourceCorrectionAAProvenance(t *testing.T) {
	m, j, r := resourceAdmissionFixture(t, 10, 20)
	r = resourceAdmissionNativeMetadata(t, r)
	// The same binary path makes differing transcript hashes contradictory too.
	m, j, r = bytes.ReplaceAll(m, []byte("candidate.exe"), []byte("baseline.exe")), bytes.ReplaceAll(j, []byte("candidate.exe"), []byte("baseline.exe")), bytes.ReplaceAll(r, []byte("candidate.exe"), []byte("baseline.exe"))
	for _, key := range []string{"provided_source_sha", "observed_build", "build_setting_hashes", "build_info_sha256"} {
		for _, missing := range []bool{false, true} {
			t.Run(key+"/missing="+string(mustResourceJSON(t, missing)), func(t *testing.T) {
				bad := changeResourceRun(t, r, func(run map[string]any) {
					before := run["before"].(map[string]any)
					candidate := before["candidate"].(map[string]any)
					if missing {
						delete(candidate, key)
						return
					}
					switch key {
					case "provided_source_sha":
						for _, arm := range []string{"baseline", "candidate"} {
							delete(before[arm].(map[string]any)["observed_build"].(map[string]any), "vcs.revision")
						}
						candidate[key] = strings.Repeat("e", 40)
					case "observed_build":
						candidate[key].(map[string]any)["GOARCH"] = "arm64"
					case "build_setting_hashes":
						candidate[key].(map[string]any)["GOARCH"] = strings.Repeat("e", 64)
					case "build_info_sha256":
						candidate[key] = strings.Repeat("e", 64)
					}
				})
				b := admitResourceBatch(m, j, bad, config{cold, .95, 10, 20})
				if resourceAdmissionHasKind(b, invalidEvidence) {
					t.Fatalf("vector must reach within-A/A consistency check: %+v", b.Issues)
				}
				if issues := identicalCalibrationArms(b); len(issues) == 0 {
					t.Fatal("contradictory/asymmetric A/A provenance admitted")
				}
			})
		}
	}
	// Copies with distinct path spellings may have different go-version-m
	// transcript hashes. Comparable typed fields must still agree.
	x, y, z := resourceAdmissionFixture(t, 10, 20)
	z = resourceAdmissionNativeMetadata(t, z)
	z = changeResourceRun(t, z, func(run map[string]any) {
		run["before"].(map[string]any)["candidate"].(map[string]any)["build_info_sha256"] = strings.Repeat("e", 64)
	})
	b := admitResourceBatch(x, y, z, config{cold, .95, 10, 20})
	if issues := identicalCalibrationArms(b); len(b.Issues) != 0 || len(issues) != 0 {
		t.Fatalf("path-dependent transcript rejected: %+v %+v", b.Issues, issues)
	}
}

func TestResourceCorrectionNativeDurationsAndGetters(t *testing.T) {
	m, j, r := resourceAdmissionFixture(t, 10, 20)
	for _, key := range []string{"ElapsedMS", "EvidencePersistenceMS", "CaptureSpoolWriteMS", "QueryElapsedMS"} {
		for _, lexeme := range []string{"-1e-400", "-0.0001e-999", "-0", "-0.0e-400", "1e-400", "0.125"} {
			t.Run(key+"/"+lexeme, func(t *testing.T) {
				bad := changeResourceJournal(t, j, 1, func(e map[string]any) {
					result := e["result"].(map[string]any)
					if key == "QueryElapsedMS" {
						result["ResourceAccounting"].(map[string]any)[key] = json.Number(lexeme)
					} else {
						result[key] = json.Number(lexeme)
					}
				})
				// Match the native max-one rule for every small elapsed control.
				mm := m
				if key == "ElapsedMS" {
					bad = bytes.Replace(bad, []byte(`"DurationMS":1000`), []byte(`"DurationMS":1`), 1)
					mm = bytes.Replace(m, []byte(`"duration_ms":1000`), []byte(`"duration_ms":1`), 1)
				}
				b := admitResourceBatch(mm, bad, r, config{cold, .95, 10, 20})
				wantInvalid := lexeme == "-1e-400" || lexeme == "-0.0001e-999"
				if resourceAdmissionHasKind(b, invalidEvidence) != wantInvalid {
					t.Fatalf("duration sign/zero semantics lost: %+v", b.Issues)
				}
			})
		}
	}
	for _, name := range []string{"available null query", "accounting zero structure", "memory zero structure", "unsupported null query", "available zero query"} {
		t.Run(name, func(t *testing.T) {
			changed := changeResourceJournal(t, j, 1, func(e map[string]any) {
				a := e["result"].(map[string]any)["ResourceAccounting"].(map[string]any)
				switch name {
				case "available null query":
					a["QueryElapsedMS"] = nil
				case "accounting zero structure":
					a["JobAccounting"].(map[string]any)["Values"].(map[string]any)["StructureBytes"] = 0
				case "memory zero structure":
					a["JobMemory"].(map[string]any)["Values"].(map[string]any)["StructureBytes"] = 0
				case "unsupported null query":
					a["Status"], a["QueryElapsedMS"] = "unsupported", nil
					for _, component := range []string{"JobAccounting", "JobMemory", "RootCpu", "OtherOwnedCpu"} {
						a[component] = map[string]any{"Status": "unsupported", "Error": "not supported", "Values": nil}
					}
				case "available zero query":
					a["QueryElapsedMS"] = 0
				}
			})
			b := admitResourceBatch(m, changed, r, config{cold, .95, 10, 20})
			wantInvalid := !strings.HasPrefix(name, "unsupported") && name != "available zero query"
			if resourceAdmissionHasKind(b, invalidEvidence) != wantInvalid || name == "unsupported null query" && b.ResourceStatus != incompleteResource {
				t.Fatalf("getter state confused with measured zero: %+v", b.Issues)
			}
		})
	}
}
