package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// This generates protocol evidence, not a benchmark score. The paths are fresh
// but no input files are written; CLI tests can write the returned bytes there.
func resourceAdmissionFixture(t *testing.T, coldPairs, noChangePairs int) (manifest, journal, run []byte) {
	t.Helper()
	var m map[string]any
	if err := json.Unmarshal(fixture(t, 900, 1000, coldPairs, noChangePairs), &m); err != nil {
		t.Fatal(err)
	}
	dir := filepath.Join(t.TempDir(), "evidence")
	work := filepath.Join(dir, "work-0123456789abcdef0123456789abcdef")
	hash := strings.Repeat("a", 64)
	m["tree_digest_format"], m["measurement_boundary"] = "sha256-tree-v2", resourceBoundary
	before := map[string]any{"fixture": map[string]any{"path": filepath.Join(dir, "fixture"), "sha256": "sha256-tree-v2:" + hash}, "repositories": []any{}}
	for _, arm := range []string{"baseline", "candidate"} {
		executable := filepath.Join(dir, "bin", arm+".exe")
		before[arm] = map[string]any{"path": executable, "sha256": hash}
		m[arm] = map[string]any{"revision": "sha256:" + hash, "command": executable + " build --project <work-copy> --json"}
	}
	records := m["records"].([]any)
	stamp := time.Date(2026, 10, 18, 12, 0, 0, 0, time.UTC)
	for i := 0; i <= len(records); i++ {
		inv := map[string]any{"phase": "diagnostic", "binary": "candidate", "scored": false}
		duration := float64(40)
		prefix := "candidate-timings"
		if i < len(records) {
			r := records[i].(map[string]any)
			r["diagnostics_digest"], r["output_tree_digest"] = "sha256-diagnostics-v2:"+hash, "sha256-tree-v2:"+hash
			inv = map[string]any{"phase": r["phase"], "binary": r["binary"], "scored": true, "pair": r["pair"], "order": r["order"]}
			duration = r["duration_ms"].(float64)
			prefix = fmt.Sprintf("%s-%.0f-%s", r["phase"], r["pair"], r["binary"])
		}
		arm := inv["binary"].(string)
		entry := map[string]any{
			"event": "started", "invocation": inv, "at_utc": stamp.Add(time.Duration(2*i) * time.Millisecond).Format(time.RFC3339Nano), "status": "incomplete",
			"executable": before[arm].(map[string]any)["path"], "working_directory": filepath.Join(work, arm),
			"stdout": filepath.Join(dir, prefix+".stdout.json"), "stderr": filepath.Join(dir, prefix+".stderr.txt"), "diagnostics_digest_format": "sha256-diagnostics-v2", "resource_accounting_format": resourceFormat,
		}
		line, err := json.Marshal(entry)
		if err != nil {
			t.Fatal(err)
		}
		journal = append(append(journal, line...), '\n')
		entry["event"], entry["status"], entry["error"], entry["validation_completed"] = "finished", "completed", nil, true
		entry["at_utc"] = stamp.Add(time.Duration(2*i+1) * time.Millisecond).Format(time.RFC3339Nano)
		job := map[string]any{
			"UserTicks100ns": 156250, "KernelTicks100ns": 156250, "TotalProcesses": 1, "ActiveProcesses": 0, "LimitTerminatedProcesses": 0, "TotalPageFaults": 20,
			"ReadOperations": 1, "WriteOperations": 2, "OtherOperations": 3, "ReadBytes": 1024, "WriteBytes": 2048, "OtherBytes": 4096, "StructureBytes": 96,
		}
		available := func(values map[string]any) map[string]any {
			return map[string]any{"Status": "available", "Error": nil, "Values": values}
		}
		resources := map[string]any{
			"Schema": 1, "Status": "available", "Completion": "complete", "CompletionError": nil, "QueryElapsedMS": 0.01,
			"JobAccounting": available(job), "JobMemory": available(map[string]any{"PeakJobMemoryBytes": 1048576, "PeakProcessMemoryBytes": 1048576, "StructureBytes": 144}),
			"RootCpu": available(map[string]any{"UserTicks100ns": 156250, "KernelTicks100ns": 156250}), "OtherOwnedCpu": available(map[string]any{"UserTicks100ns": 0, "KernelTicks100ns": 0}),
		}
		entry["result"] = map[string]any{
			"DurationMS": duration, "ElapsedMS": duration, "ExitCode": 0, "Status": "completed", "Error": nil, "EvidencePersistenceMS": 0.1, "CaptureLimitBytes": 16777216,
			"ResourceAccounting": resources, "StdoutBytes": 100, "StderrBytes": 0, "CaptureOverflow": false, "CaptureSpoolWriteMS": 0.1, "DiagnosticsDigest": "sha256-diagnostics-v2:" + hash, "OutputTreeDigest": "sha256-tree-v2:" + hash,
		}
		line, err = json.Marshal(entry)
		if err != nil {
			t.Fatal(err)
		}
		journal = append(append(journal, line...), '\n')
	}
	manifest, _ = json.Marshal(m)
	r := map[string]any{
		"schema": 1, "status": "evaluated", "started_utc": stamp.Format(time.RFC3339Nano), "finished_utc": stamp.Add(time.Hour).Format(time.RFC3339Nano),
		"error": "perfcompare rejected benchmark evidence: synthetic legacy failure", "before": before, "after": before,
		"evaluation": map[string]any{"ExitCode": 1, "Status": "FAIL", "Output": "synthetic legacy failure"}, "invocation_journal": filepath.Join(dir, "invocations.ndjson"),
		"tree_digest_format": "sha256-tree-v2", "diagnostics_digest_format": "sha256-diagnostics-v2", "project_mode": "separate-work-copies", "project_state": nil,
		"resource_accounting":  map[string]any{"format": resourceFormat, "invocation_journal": filepath.Join(dir, "invocations.ndjson"), "boundary": "post-wall", "cpu_units": "100ns ticks", "memory": "job peak bytes, not RSS", "io": "OS I/O, not filesystem-only"},
		"measurement_boundary": resourceBoundary, "capture_limit_bytes_per_stream": 16777216, "machine": m["machine"],
	}
	run, _ = json.Marshal(r)
	return
}

func resourceAdmissionHasKind(b resourceBatch, kind string) bool {
	for _, problem := range b.Issues {
		if problem.Kind == kind {
			return true
		}
	}
	return false
}

func changeResourceJournal(t *testing.T, journal []byte, line int, change func(map[string]any)) []byte {
	t.Helper()
	lines := bytes.Split(bytes.TrimSuffix(journal, []byte{'\n'}), []byte{'\n'})
	if lines == nil {
		t.Fatal("empty journal")
	}
	var entry map[string]any
	if err := json.Unmarshal(lines[line], &entry); err != nil {
		t.Fatal(err)
	}
	change(entry)
	var err error
	lines[line], err = json.Marshal(entry)
	if err != nil {
		t.Fatal(err)
	}
	return append(bytes.Join(lines, []byte{'\n'}), '\n')
}

func TestResourceAdmissionCompleteAndDiagnostic(t *testing.T) {
	m, j, r := resourceAdmissionFixture(t, 10, 20)
	b := admitResourceBatch(m, j, r, config{cold, .95, 10, 20})
	if len(b.Issues) != 0 || b.ResourceStatus != "complete" || b.LegacyVerdict != "FAIL" {
		t.Fatalf("valid protocol rejected: %+v", b.Issues)
	}
	if len(b.Observations) != 122 || b.Observations[120].Classification != "post-sample-timings-diagnostic" || b.Observations[121].Scored {
		t.Fatal("separate diagnostic not retained/classified")
	}
	if len(b.Resources) != 8 || len(b.Resources["job_cpu_ms"][cold]) != 10 || b.Resources["job_cpu_ms"][cold][0].baseline != 31.25 || b.Resources["write_bytes"][noChange][19].candidate != 2048 {
		t.Fatal("wrong complete metric populations")
	}
	if *b.Observations[1].JobUserTicks100ns != 156250 || *b.Observations[1].JobTotalTicks100ns != 312500 {
		t.Fatal("exact CPU inputs/sum lost")
	}
	// Extreme diagnostic values remain visible and never enter the populations.
	j = changeResourceJournal(t, j, 121, func(e map[string]any) {
		resources := e["result"].(map[string]any)["ResourceAccounting"].(map[string]any)
		resources["JobMemory"].(map[string]any)["Values"].(map[string]any)["PeakJobMemoryBytes"] = uint64(math.MaxUint64)
	})
	b = admitResourceBatch(m, j, r, config{cold, .95, 10, 20})
	if len(b.Issues) != 0 || b.Resources["job_peak_bytes"][cold][0].candidate != 1048576 || b.Observations[121].CounterValues["job_peak_bytes"] != math.MaxUint64 {
		t.Fatalf("diagnostic contaminated scored metrics or was dropped: %+v", b.Issues)
	}
	if !bytes.Equal(b.RawManifest.Bytes, m) || !bytes.Equal(b.RawJournal.Bytes, j) || !bytes.Equal(b.RawRun.Bytes, r) {
		t.Fatal("raw evidence changed")
	}
}

func TestResourceAdmissionUnavailableAndOptionalComponents(t *testing.T) {
	for _, name := range []string{"optional other-owned failure", "optional root failure", "required job unsupported", "required memory failure"} {
		t.Run(name, func(t *testing.T) {
			m, j, r := resourceAdmissionFixture(t, 10, 20)
			j = changeResourceJournal(t, j, 1, func(e map[string]any) {
				resources := e["result"].(map[string]any)["ResourceAccounting"].(map[string]any)
				resources["Status"] = "partial"
				switch name {
				case "optional other-owned failure":
					resources["OtherOwnedCpu"] = map[string]any{"Status": "failed", "Error": "negative subtraction is not clamped", "Values": nil}
				case "optional root failure":
					resources["RootCpu"] = map[string]any{"Status": "failed", "Error": "GetProcessTimes failed", "Values": nil}
					resources["OtherOwnedCpu"] = map[string]any{"Status": "unavailable", "Error": "root unavailable", "Values": nil}
				case "required job unsupported":
					resources["JobAccounting"] = map[string]any{"Status": "unsupported", "Error": "class 8 unsupported", "Values": nil}
					resources["OtherOwnedCpu"] = map[string]any{"Status": "unsupported", "Error": "job unavailable", "Values": nil}
				case "required memory failure":
					resources["JobMemory"] = map[string]any{"Status": "failed", "Error": "class 9 getter failed", "Values": nil}
				}
			})
			b := admitResourceBatch(m, j, r, config{cold, .95, 10, 20})
			if resourceAdmissionHasKind(b, invalidEvidence) || len(b.Observations) != 122 {
				t.Fatalf("truthful status rejected: %+v", b.Issues)
			}
			if strings.HasPrefix(name, "optional") {
				if b.ResourceStatus != "complete" || len(b.Resources) != 8 || len(b.Issues) != 0 {
					t.Fatal("optional failure discarded required counters")
				}
			} else {
				if b.ResourceStatus != incompleteResource || !resourceAdmissionHasKind(b, incompleteResource) {
					t.Fatal("missing counter was not marked unmeasured")
				}
				if name == "required job unsupported" && (len(b.Resources) != 1 || b.Resources["job_cpu_ms"] != nil) {
					t.Fatal("unavailable job metric obtained a shortened population")
				}
				if name == "required memory failure" && (len(b.Resources) != 7 || b.Resources["job_peak_bytes"] != nil) {
					t.Fatal("unavailable memory obtained a shortened population")
				}
			}
		})
	}
}

func TestResourceAdmissionRejectsInvalidEvidence(t *testing.T) {
	changes := map[string]func(map[string]any){
		"missing resource": func(e map[string]any) { delete(e["result"].(map[string]any), "ResourceAccounting") },
		"missing component": func(e map[string]any) {
			delete(e["result"].(map[string]any)["ResourceAccounting"].(map[string]any), "JobMemory")
		},
		"nonzero invocation":    func(e map[string]any) { e["result"].(map[string]any)["ExitCode"] = 1 },
		"incomplete invocation": func(e map[string]any) { e["validation_completed"] = false },
		"interrupted resources": func(e map[string]any) {
			e["result"].(map[string]any)["ResourceAccounting"].(map[string]any)["Completion"] = "partial"
		},
		"wrong order":            func(e map[string]any) { e["invocation"].(map[string]any)["order"] = "BA" },
		"wrong phase":            func(e map[string]any) { e["invocation"].(map[string]any)["phase"] = noChange },
		"wrong pair":             func(e map[string]any) { e["invocation"].(map[string]any)["pair"] = 2 },
		"wrong binary":           func(e map[string]any) { e["invocation"].(map[string]any)["binary"] = "candidate" },
		"wrong executable":       func(e map[string]any) { e["executable"] = "C:/other.exe" },
		"wrong duration":         func(e map[string]any) { e["result"].(map[string]any)["DurationMS"] = 999 },
		"wrong elapsed rounding": func(e map[string]any) { e["result"].(map[string]any)["ElapsedMS"] = 1001.5 },
		"wrong digest": func(e map[string]any) {
			e["result"].(map[string]any)["DiagnosticsDigest"] = "sha256-diagnostics-v2:" + strings.Repeat("b", 64)
		},
		"mixed format": func(e map[string]any) { e["resource_accounting_format"] = "windows-job-v0" },
		"missing available counter": func(e map[string]any) {
			delete(e["result"].(map[string]any)["ResourceAccounting"].(map[string]any)["JobAccounting"].(map[string]any)["Values"].(map[string]any), "ReadBytes")
		},
		"negative counter": func(e map[string]any) {
			e["result"].(map[string]any)["ResourceAccounting"].(map[string]any)["JobAccounting"].(map[string]any)["Values"].(map[string]any)["ReadBytes"] = -1
		},
		"CPU overflow": func(e map[string]any) {
			e["result"].(map[string]any)["ResourceAccounting"].(map[string]any)["JobAccounting"].(map[string]any)["Values"].(map[string]any)["UserTicks100ns"] = uint64(math.MaxInt64) + 1
		},
		"active job": func(e map[string]any) {
			e["result"].(map[string]any)["ResourceAccounting"].(map[string]any)["JobAccounting"].(map[string]any)["Values"].(map[string]any)["ActiveProcesses"] = 1
		},
		"unavailable disguised as zero": func(e map[string]any) {
			resources := e["result"].(map[string]any)["ResourceAccounting"].(map[string]any)
			resources["Status"] = "partial"
			resources["JobMemory"].(map[string]any)["Status"] = "failed"
			resources["JobMemory"].(map[string]any)["Error"] = "failed"
		},
	}
	for name, change := range changes {
		t.Run(name, func(t *testing.T) {
			m, j, r := resourceAdmissionFixture(t, 10, 20)
			j = changeResourceJournal(t, j, 1, change)
			b := admitResourceBatch(m, j, r, config{cold, .95, 10, 20})
			if !resourceAdmissionHasKind(b, invalidEvidence) || len(b.Resources) != 0 || len(b.Observations) != 122 {
				t.Fatalf("invalid evidence accepted/dropped: %+v", b.Issues)
			}
		})
	}
}

func TestResourceAdmissionExactIntegerRangeAndStrictExtensions(t *testing.T) {
	m, j, r := resourceAdmissionFixture(t, 10, 20)
	j = bytes.Replace(j, []byte(`"ReadBytes":1024`), []byte(`"ReadBytes":18446744073709551615`), 1)
	j = bytes.Replace(j, []byte(`"UserTicks100ns":156250`), []byte(`"UserTicks100ns":9007199254740993`), 1)
	// Derived attribution becomes a truthful optional failure for this vector.
	j = changeResourceJournal(t, j, 1, func(e map[string]any) {
		a := e["result"].(map[string]any)["ResourceAccounting"].(map[string]any)
		a["Status"] = "partial"
		a["OtherOwnedCpu"] = map[string]any{"Status": "failed", "Error": "synthetic attribution unavailable", "Values": nil}
	})
	// The generic mutation helper roundtrips float64, so insert large integers last.
	j = bytes.Replace(j, []byte(`"ReadBytes":18446744073709552000`), []byte(`"ReadBytes":18446744073709551615`), 1)
	j = bytes.Replace(j, []byte(`"UserTicks100ns":9007199254740992`), []byte(`"UserTicks100ns":9007199254740993`), 1)
	b := admitResourceBatch(m, j, r, config{cold, .95, 10, 20})
	if len(b.Issues) != 0 || b.Observations[1].CounterValues["read_bytes"] != math.MaxUint64 || *b.Observations[1].JobUserTicks100ns != 9007199254740993 {
		t.Fatalf("integer precision lost: %+v", b.Issues)
	}
	for name, replace := range map[string]string{
		"overflow":                   `"ReadBytes":18446744073709551616`,
		"fractional":                 `"ReadBytes":1.5`,
		"nonfinite":                  `"ReadBytes":1e309`,
		"duplicate extension":        `"extension":{"a":1,"a":2},"ReadBytes":1`,
		"invalid string escape":      `"extension":"\q","ReadBytes":1`,
		"invalid Unicode":            `"extension":"\ud800","ReadBytes":1`,
		"malformed number extension": `"extension":01,"ReadBytes":1`,
	} {
		t.Run(name, func(t *testing.T) {
			bad := bytes.Replace(j, []byte(`"ReadBytes":18446744073709551615`), []byte(replace), 1)
			b := admitResourceBatch(m, bad, r, config{cold, .95, 10, 20})
			if !resourceAdmissionHasKind(b, invalidEvidence) || !bytes.Equal(b.RawJournal.Bytes, bad) {
				t.Fatalf("malformed/overflow input admitted: %+v", b.Issues)
			}
		})
	}
}

func TestResourceAdmissionCountRunAndPathValidation(t *testing.T) {
	m, j, r := resourceAdmissionFixture(t, 10, 20)
	lines := bytes.Split(bytes.TrimSuffix(j, []byte{'\n'}), []byte{'\n'})
	if lines == nil {
		t.Fatal("empty journal")
	}
	for name, bad := range map[string][]byte{
		"no diagnostic":   bytes.Join(lines[:120], []byte{'\n'}),
		"unfinished":      bytes.Join(lines[:121], []byte{'\n'}),
		"duplicate event": append(append(bytes.Clone(j), lines[121]...), '\n'),
		"blank line":      append(bytes.Clone(j), '\n'),
	} {
		t.Run(name, func(t *testing.T) {
			b := admitResourceBatch(m, bad, r, config{cold, .95, 10, 20})
			if !resourceAdmissionHasKind(b, invalidEvidence) {
				t.Fatal("bad journal sequence admitted")
			}
		})
	}
	for name, bad := range map[string][]byte{
		"incomplete run":       bytes.Replace(r, []byte(`"status":"evaluated"`), []byte(`"status":"incomplete"`), 1),
		"changed identity":     bytes.Replace(r, []byte(`"sha256":"`+strings.Repeat("a", 64)+`"`), []byte(`"sha256":"`+strings.Repeat("b", 64)+`"`), 1),
		"inconsistent verdict": bytes.Replace(r, []byte(`"ExitCode":1`), []byte(`"ExitCode":0`), 1),
		"duplicate extension":  bytes.Replace(r, []byte(`"schema":1`), []byte(`"schema":1,"unknown":{"x":1,"x":2}`), 1),
	} {
		t.Run(name, func(t *testing.T) {
			b := admitResourceBatch(m, j, bad, config{cold, .95, 10, 20})
			if !resourceAdmissionHasKind(b, invalidEvidence) {
				t.Fatal("bad run admitted")
			}
		})
	}
	var metadata struct {
		Journal string `json:"invocation_journal"`
	}
	_ = json.Unmarshal(r, &metadata)
	dir := filepath.Dir(metadata.Journal)
	b := admitResourceBatch(m, j, r, config{cold, .95, 10, 20})
	validateResourcePaths(&b, filepath.Join(dir, "windows-build-performance-manifest.json"), metadata.Journal, filepath.Join(dir, "run.json"))
	if len(b.Issues) != 0 {
		t.Fatalf("valid paths rejected: %+v", b.Issues)
	}
	validateResourcePaths(&b, filepath.Join(dir, "windows-build-performance-manifest.json.pending"), metadata.Journal, filepath.Join(dir, "run.json"))
	if !resourceAdmissionHasKind(b, invalidEvidence) || len(b.Resources) != 0 {
		t.Fatal("pending manifest admitted")
	}
}

func changeResourceRun(t *testing.T, raw []byte, change func(map[string]any)) []byte {
	t.Helper()
	var run map[string]any
	if err := json.Unmarshal(raw, &run); err != nil {
		t.Fatal(err)
	}
	change(run)
	// Shape tests deliberately keep both snapshots identical so their failure
	// cannot be attributed to the older before/after equality check.
	run["after"] = run["before"]
	result, err := json.Marshal(run)
	if err != nil {
		t.Fatal(err)
	}
	return result
}

func resourceAdmissionNativeMetadata(t *testing.T, raw []byte) []byte {
	t.Helper()
	return changeResourceRun(t, raw, func(run map[string]any) {
		before := run["before"].(map[string]any)
		for _, arm := range []string{"baseline", "candidate"} {
			identity := before[arm].(map[string]any)
			identity["provided_source_sha"] = ""
			identity["observed_build"] = map[string]any{"go_toolchain": "go1.26.9", "vcs.revision": strings.Repeat("b", 40), "vcs.modified": "false", "GOOS": "windows", "GOARCH": "amd64"}
			identity["build_setting_hashes"] = map[string]any{"GOARCH": strings.Repeat("c", 64)}
			identity["build_info_sha256"] = strings.Repeat("d", 64)
		}
		fixture := before["fixture"].(map[string]any)
		fixture["provided_identity"] = ""
		fixture["provenance"] = map[string]any{"input_file_hashes": map[string]any{"package.json": strings.Repeat("e", 64)}, "observed_package_versions": map[string]any{"typescript": map[string]any{"version": "5.9.0", "manifest_sha256": strings.Repeat("f", 64)}}}
		root := filepath.Dir(fixture["path"].(string))
		before["repositories"] = []any{
			map[string]any{"root": root, "identity": map[string]any{"root": root, "sha": strings.Repeat("a", 40), "status": ""}},
			map[string]any{"root": fixture["path"], "identity": nil},
		}
	})
}

func TestResourceAdmissionOptionalIdentityShapes(t *testing.T) {
	m, j, r := resourceAdmissionFixture(t, 10, 20)
	r = resourceAdmissionNativeMetadata(t, r)
	if b := admitResourceBatch(m, j, r, config{cold, .95, 10, 20}); len(b.Issues) != 0 {
		t.Fatalf("native-shaped optional metadata rejected: %+v", b.Issues)
	}
	tests := []struct {
		name, path string
		change     func(map[string]any)
	}{
		{"provided source object", ".provided_source_sha", func(b map[string]any) { b["baseline"].(map[string]any)["provided_source_sha"] = map[string]any{} }},
		{"provided source null", ".provided_source_sha", func(b map[string]any) { b["baseline"].(map[string]any)["provided_source_sha"] = nil }},
		{"provided source hash", ".provided_source_sha", func(b map[string]any) { b["baseline"].(map[string]any)["provided_source_sha"] = "not-a-git-hash" }},
		{"provided source mismatch", ".provided_source_sha", func(b map[string]any) {
			b["baseline"].(map[string]any)["provided_source_sha"] = strings.Repeat("c", 40)
		}},
		{"observed build array", ".observed_build", func(b map[string]any) { b["baseline"].(map[string]any)["observed_build"] = []any{} }},
		{"observed build nonstring", ".observed_build.GOOS", func(b map[string]any) { b["baseline"].(map[string]any)["observed_build"].(map[string]any)["GOOS"] = 42 }},
		{"observed revision hash", ".observed_build.vcs.revision", func(b map[string]any) {
			b["baseline"].(map[string]any)["observed_build"].(map[string]any)["vcs.revision"] = "bad"
		}},
		{"build setting map null", ".build_setting_hashes", func(b map[string]any) { b["baseline"].(map[string]any)["build_setting_hashes"] = nil }},
		{"build setting hash object", ".build_setting_hashes.GOARCH", func(b map[string]any) {
			b["baseline"].(map[string]any)["build_setting_hashes"].(map[string]any)["GOARCH"] = map[string]any{}
		}},
		{"build setting bad hash", ".build_setting_hashes.GOARCH", func(b map[string]any) {
			b["baseline"].(map[string]any)["build_setting_hashes"].(map[string]any)["GOARCH"] = "bad"
		}},
		{"build info hash number", ".build_info_sha256", func(b map[string]any) { b["baseline"].(map[string]any)["build_info_sha256"] = 1 }},
		{"build info hash malformed", ".build_info_sha256", func(b map[string]any) { b["baseline"].(map[string]any)["build_info_sha256"] = "bad" }},
		{"repositories object", ".repositories", func(b map[string]any) { b["repositories"] = map[string]any{} }},
		{"repositories null", ".repositories", func(b map[string]any) { b["repositories"] = nil }},
		{"repository entry scalar", ".repositories[0]", func(b map[string]any) { b["repositories"] = []any{42} }},
		{"repository identity array", ".repositories[0].identity", func(b map[string]any) { b["repositories"].([]any)[0].(map[string]any)["identity"] = []any{} }},
		{"repository relative root", ".repositories[0].root", func(b map[string]any) { b["repositories"].([]any)[0].(map[string]any)["root"] = "relative" }},
		{"repository different root", ".repositories[0].identity.root", func(b map[string]any) {
			b["repositories"].([]any)[0].(map[string]any)["identity"].(map[string]any)["root"] = filepath.Join(t.TempDir(), "different")
		}},
		{"repository bad sha", ".repositories[0].identity.sha", func(b map[string]any) {
			b["repositories"].([]any)[0].(map[string]any)["identity"].(map[string]any)["sha"] = "bad"
		}},
		{"repository status object", ".repositories[0].identity.status", func(b map[string]any) {
			b["repositories"].([]any)[0].(map[string]any)["identity"].(map[string]any)["status"] = map[string]any{}
		}},
		{"relative fixture", ".fixture.path", func(b map[string]any) { b["fixture"].(map[string]any)["path"] = "relative" }},
		{"fixture assertion array", ".fixture.provided_identity", func(b map[string]any) { b["fixture"].(map[string]any)["provided_identity"] = []any{} }},
		{"fixture provenance number", ".fixture.provenance", func(b map[string]any) { b["fixture"].(map[string]any)["provenance"] = 1 }},
		{"fixture input hashes array", ".fixture.provenance.input_file_hashes", func(b map[string]any) {
			b["fixture"].(map[string]any)["provenance"].(map[string]any)["input_file_hashes"] = []any{}
		}},
		{"fixture input hash bad", ".fixture.provenance.input_file_hashes.package.json", func(b map[string]any) {
			b["fixture"].(map[string]any)["provenance"].(map[string]any)["input_file_hashes"].(map[string]any)["package.json"] = "bad"
		}},
		{"fixture packages null", ".fixture.provenance.observed_package_versions", func(b map[string]any) {
			b["fixture"].(map[string]any)["provenance"].(map[string]any)["observed_package_versions"] = nil
		}},
		{"fixture package scalar", ".fixture.provenance.observed_package_versions.typescript", func(b map[string]any) {
			b["fixture"].(map[string]any)["provenance"].(map[string]any)["observed_package_versions"].(map[string]any)["typescript"] = "5.9.0"
		}},
		{"fixture package version number", ".fixture.provenance.observed_package_versions.typescript.version", func(b map[string]any) {
			b["fixture"].(map[string]any)["provenance"].(map[string]any)["observed_package_versions"].(map[string]any)["typescript"].(map[string]any)["version"] = 5
		}},
		{"fixture package manifest hash bad", ".fixture.provenance.observed_package_versions.typescript.manifest_sha256", func(b map[string]any) {
			b["fixture"].(map[string]any)["provenance"].(map[string]any)["observed_package_versions"].(map[string]any)["typescript"].(map[string]any)["manifest_sha256"] = "bad"
		}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			bad := changeResourceRun(t, r, func(run map[string]any) { tc.change(run["before"].(map[string]any)) })
			b := admitResourceBatch(m, j, bad, config{cold, .95, 10, 20})
			found := false
			for _, problem := range b.Issues {
				if problem.Kind == invalidEvidence && strings.Contains(problem.Path, tc.path) {
					found = true
				}
			}
			if !found || len(b.Resources) != 0 || !bytes.Equal(b.RawRun.Bytes, bad) {
				t.Fatalf("malformed recognized metadata admitted or lost: %+v", b.Issues)
			}
		})
	}
}

func resourceAdmissionStableFixture(t *testing.T) (m, j, r []byte) {
	t.Helper()
	m, j, r = resourceAdmissionFixture(t, 10, 20)
	var root map[string]any
	if err := json.Unmarshal(r, &root); err != nil {
		t.Fatal(err)
	}
	dir := filepath.Dir(root["invocation_journal"].(string))
	work := filepath.Join(dir, "work-0123456789abcdef0123456789abcdef")
	active := filepath.Join(work, "active")
	root["project_mode"] = "stable-project-path-v1"
	root["project_state"] = map[string]any{
		"mode": "stable-project-path-v1", "evidence_root": dir, "work_root": work, "active_path": active,
		"arms": map[string]any{"baseline": filepath.Join(work, "baseline"), "candidate": filepath.Join(work, "candidate")}, "active_owner": nil, "pending_arm": nil,
		"transition": "cleaned", "preserve_work": false, "cleanup_completed": true, "locations": map[string]any{"active_exists": false, "baseline_exists": false, "candidate_exists": false}, "error": nil,
		"state_file": filepath.Join(dir, "project-state.json"), "journal": filepath.Join(dir, "project-state.ndjson"), "canonical_root": root["before"].(map[string]any)["fixture"].(map[string]any)["path"],
		"input_metadata_file": filepath.Join(dir, "project-input-metadata.json"), "input_metadata_sha256": "sha256-project-state-v1:" + strings.Repeat("a", 64),
	}
	r, _ = json.Marshal(root)
	var manifest map[string]any
	if err := json.Unmarshal(m, &manifest); err != nil {
		t.Fatal(err)
	}
	for _, arm := range []string{"baseline", "candidate"} {
		manifest[arm].(map[string]any)["command"] = root["before"].(map[string]any)[arm].(map[string]any)["path"].(string) + " build --project " + active + " --json"
	}
	m, _ = json.Marshal(manifest)
	lines := bytes.Split(bytes.TrimSuffix(j, []byte{'\n'}), []byte{'\n'})
	for i, line := range lines {
		var entry map[string]any
		if err := json.Unmarshal(line, &entry); err != nil {
			t.Fatal(err)
		}
		entry["working_directory"] = active
		lines[i], _ = json.Marshal(entry)
	}
	j = append(bytes.Join(lines, []byte{'\n'}), '\n')
	return
}

func TestResourceAdmissionStableStateShapesAndCompletion(t *testing.T) {
	m, j, r := resourceAdmissionStableFixture(t)
	if b := admitResourceBatch(m, j, r, config{cold, .95, 10, 20}); len(b.Issues) != 0 {
		t.Fatalf("native-shaped stable final state rejected: %+v", b.Issues)
	}
	tests := []struct {
		name, field string
		value       any
	}{
		{"arms null", "arms", nil},
		{"arms array", "arms", []any{}},
		{"arms missing candidate", "arms", map[string]any{"baseline": "C:/wrong"}},
		{"arms wrong slots", "arms", map[string]any{"baseline": "C:/wrong", "candidate": "C:/wrong"}},
		{"locations null", "locations", nil},
		{"locations array", "locations", []any{}},
		{"locations missing slot", "locations", map[string]any{"active_exists": false}},
		{"locations nonboolean", "locations", map[string]any{"active_exists": 0, "baseline_exists": false, "candidate_exists": false}},
		{"active remains after cleanup", "locations", map[string]any{"active_exists": true, "baseline_exists": false, "candidate_exists": false}},
		{"baseline remains after cleanup", "locations", map[string]any{"active_exists": false, "baseline_exists": true, "candidate_exists": false}},
		{"candidate remains after cleanup", "locations", map[string]any{"active_exists": false, "baseline_exists": false, "candidate_exists": true}},
		{"relative evidence root", "evidence_root", "relative"},
		{"wrong owned work root", "work_root", "C:/work-other"},
		{"state file number", "state_file", 1},
		{"state file wrong path", "state_file", "C:/wrong"},
		{"state journal wrong path", "journal", "C:/wrong"},
		{"canonical root mismatch", "canonical_root", "C:/different-fixture"},
		{"metadata path mismatch", "input_metadata_file", "C:/wrong"},
		{"metadata digest object", "input_metadata_sha256", map[string]any{}},
		{"metadata digest malformed", "input_metadata_sha256", "sha256-project-state-v1:bad"},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			bad := changeResourceRun(t, r, func(run map[string]any) { run["project_state"].(map[string]any)[tc.field] = tc.value })
			b := admitResourceBatch(m, j, bad, config{cold, .95, 10, 20})
			if !resourceAdmissionHasKind(b, invalidEvidence) || len(b.Resources) != 0 || !bytes.Equal(b.RawRun.Bytes, bad) {
				t.Fatalf("malformed/incomplete stable state admitted: %+v", b.Issues)
			}
		})
	}
}

// Read-only retained integration is opt-in so the focused portable suite does
// not require the audit evidence directory on another developer's machine.
func TestResourceAdmissionRetainedHarness(t *testing.T) {
	dir := os.Getenv("PERFAUDIT_RESOURCE_HARNESS_DIR")
	if dir == "" {
		t.Skip("set PERFAUDIT_RESOURCE_HARNESS_DIR to retained completed harness evidence (read only)")
	}
	t.Logf("read-only retained integration directory: %s", dir)
	read := func(name string) []byte {
		data, err := os.ReadFile(filepath.Join(dir, name))
		if err != nil {
			t.Fatal(err)
		}
		return data
	}
	m, j, r := read("windows-build-performance-manifest.json"), read("invocations.ndjson"), read("run.json")
	var manifest struct {
		Records []record `json:"records"`
	}
	if err := json.Unmarshal(m, &manifest); err != nil {
		t.Fatal(err)
	}
	counts := map[string]int{}
	for _, record := range manifest.Records {
		counts[record.Phase]++
	}
	b := admitResourceBatch(m, j, r, config{cold, .95, counts[cold] / 2, counts[noChange] / 2})
	validateResourcePaths(&b, filepath.Join(dir, "windows-build-performance-manifest.json"), filepath.Join(dir, "invocations.ndjson"), filepath.Join(dir, "run.json"))
	if len(b.Observations) != 2*(len(manifest.Records)+1) {
		t.Fatalf("retained completed harness rejected: %+v", b.Issues)
	}
	// Retained harness publication controls deliberately substitute synthetic
	// durations in their final manifest. They are integration rejection inputs,
	// never successful compiler-score/resource populations.
	if resourceAdmissionHasKind(b, invalidEvidence) {
		mismatches := 0
		for _, problem := range b.Issues {
			if problem.Kind == invalidEvidence {
				if problem.Message != "duration, exit or digest differs from corresponding manifest record" {
					t.Fatalf("unexpected retained admission failure: %+v", b.Issues)
				}
				mismatches++
			}
		}
		if mismatches == 0 || len(b.Resources) != 0 || !bytes.Equal(b.RawJournal.Bytes, j) || !bytes.Equal(b.RawManifest.Bytes, m) {
			t.Fatal("retained correspondence failure did not preserve and suppress exact evidence")
		}
		t.Logf("retained %d scored records and timing diagnostic; correctly rejected %d synthetic manifest/journal correspondence mismatches, no resource populations scored", len(manifest.Records), mismatches)
		return
	}
	for _, metric := range b.Resources {
		if len(metric[cold]) != counts[cold]/2 || len(metric[noChange]) != counts[noChange]/2 {
			t.Fatal("retained diagnostic entered or observations were dropped from scored population")
		}
	}
	t.Logf("admitted %d scored records and separate timing diagnostic; resource status %s; actual legacy %s", len(manifest.Records), b.ResourceStatus, b.LegacyVerdict)
}
