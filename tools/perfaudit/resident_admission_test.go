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
)

// These fixtures are invented files. No test invokes a compiler, collector,
// launcher, or clock. Resealing repairs outer hashes after a deliberate fault
// so the targeted inner join must independently reject the contradiction.
func (f *residentProtocolFixture) events(ref residentBatchRef) []map[string]any {
	f.t.Helper()
	events := []map[string]any{}
	for _, line := range bytes.Split(bytes.TrimSpace(mustReadTest(f.t, filepath.Join(ref.Directory, "journal.ndjson"))), []byte{'\n'}) {
		var e map[string]any
		d := json.NewDecoder(bytes.NewReader(line))
		d.UseNumber()
		if err := d.Decode(&e); err != nil {
			f.t.Fatal(err)
		}
		if e == nil {
			f.t.Fatal("empty journal event")
		}
		events = append(events, e)
	}
	return events
}

func (f *residentProtocolFixture) seal(ref residentBatchRef, events []map[string]any) {
	f.t.Helper()
	var journal []byte
	for _, e := range events {
		journal = append(append(journal, mustJSON(e)...), '\n')
	}
	jp := filepath.Join(ref.Directory, "journal.ndjson")
	f.write(jp, journal)
	mp := filepath.Join(ref.Directory, "evidence.manifest.json")
	m := f.read(mp)
	m["journal"] = map[string]any{"path": jp, "bytes": len(journal), "sha256": residentTestHash(journal)}
	for _, row := range m["retained"].([]any) {
		v := row.(map[string]any)
		path := v["path"].(string)
		data := mustReadTest(f.t, path)
		v["expected_bytes"], v["expected_sha256"] = len(data), residentTestHash(data)
		v["observed"] = map[string]any{"path": path, "sha256": residentTestHash(data), "bytes": len(data)}
	}
	mr := f.write(mp, m)
	fp := filepath.Join(ref.Directory, "batch.finished.json")
	finished := f.read(fp)
	finished["evidence_manifest"] = map[string]any{"path": mp, "sha256": mr.SHA256, "bytes": len(mustReadTest(f.t, mp))}
	f.write(fp, finished)
}

func (f *residentProtocolFixture) editReport(e map[string]any, edit func(map[string]any)) {
	path := e["report_path"].(string)
	report := f.read(path)
	edit(report)
	ref := f.write(path, report)
	identity := e["report"].(map[string]any)
	f.write(identity["snapshot_path"].(string), mustReadTest(f.t, path))
	identity["sha256"], identity["bytes"] = ref.SHA256, len(mustReadTest(f.t, path))
}

func (f *residentProtocolFixture) batchReport(ref residentBatchRef) (residentRawBatch, *residentReader) {
	r := newResidentReader()
	r.kind = "invented-protocol"
	return r.batch(ref, f.plan.Campaigns[0].Key, "calibration"), r
}

func residentProblem(issues []issue, text string) bool {
	for _, i := range issues {
		if strings.Contains(i.Path+" "+i.Message, text) {
			return true
		}
	}
	return false
}

func TestResidentReaderIndependentRawJoins(t *testing.T) {
	cases := []struct {
		name, problem string
		edit          func(*residentProtocolFixture, residentBatchRef, []map[string]any)
	}{
		{"stopwatch-frequency", "stopwatch_frequency", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			e[1]["stopwatch_frequency"] = 1
		}},
		{"reason-scalar", "required string missing", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) { e[1]["reasons"] = []any{nil} }},
		{"plan-copy", "retained plan.raw", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			f.write(filepath.Join(b.Directory, "plan.raw.json"), []byte(`{}`))
		}},
		{"missing-retention", "required retained binding", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			p := filepath.Join(b.Directory, "evidence.manifest.json")
			m := f.read(p)
			m["retained"] = m["retained"].([]any)[1:]
			f.write(p, m)
		}},
		{"sticky-retention-error", "retention_error", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			p := filepath.Join(b.Directory, "evidence.manifest.json")
			m := f.read(p)
			m["retained"].([]any)[0].(map[string]any)["retention_error"] = "earlier failure; later bytes restored"
			f.write(p, m)
		}},
		{"omitted-input", "missing/duplicate original", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			p := filepath.Join(b.Directory, "inputs.before.json")
			var rows []any
			_ = json.Unmarshal(mustReadTest(f.t, p), &rows)
			f.write(p, rows[1:])
		}},
		{"copy-bytes", "frozen copy", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			p := filepath.Join(b.Directory, "inputs.before.json")
			var rows []map[string]any
			_ = json.Unmarshal(mustReadTest(f.t, p), &rows)
			f.write(rows[0]["copy"].(string), []byte(`{}`))
		}},
		{"host-drift", "host.after", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			p := filepath.Join(b.Directory, "host.after.json")
			m := f.read(p)
			m["environment"].(map[string]any)["__MISE_SESSION"] = residentTestHash([]byte("different"))
			f.write(p, m)
		}},
		{"pair", "unexpected integer", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			e[0]["pair"], e[1]["pair"] = 2, 2
		}},
		{"arm", "expected \"A\"", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			e[0]["arm"], e[1]["arm"] = "B", "B"
		}},
		{"argv", "arguments", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			e[0]["arguments"], e[1]["arguments"] = []string{"-test.count=2"}, []string{"-test.count=2"}
		}},
		{"time", "backwards", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			e[1]["at_utc"] = "2026-10-18T01:00:00Z"
		}},
		{"failed-prefix", "launch after failed prefix", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) { e[1]["status"] = "failed" }},
		{"failed-cleanup", "contradictory Boolean", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			e[1]["owner_disposed"] = false
		}},
		{"native-outer", "native/outer", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			e[1]["result"].(map[string]any)["Status"] = "partial"
		}},
		{"native-exit", "unexpected integer", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			n := e[1]["result"].(map[string]any)
			n["ExitCode"] = 7
			e[1]["native_result_json"] = string(mustJSON(n))
		}},
		{"stream-length", "stream length", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			f.write(e[1]["stdout_path"].(string), []byte("changed"))
		}},
		{"capture-overflow", "contradictory Boolean", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			n := e[1]["result"].(map[string]any)
			n["CaptureOverflow"] = true
			e[1]["native_result_json"] = string(mustJSON(n))
		}},
		{"schema1", "unexpected integer", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			f.editReport(e[1], func(m map[string]any) { m["schema"] = 1 })
		}},
		{"diagnostic", "classification", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			f.editReport(e[1], func(m map[string]any) { m["classification"] = "diagnostic" })
		}},
		{"cycle-count", "thirty", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			f.editReport(e[1], func(m map[string]any) { m["records"] = m["records"].([]any)[:29] })
		}},
		{"deep-cycle-shape", "array required", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			f.editReport(e[1], func(m map[string]any) { m["records"] = map[string]any{"0": nil} })
		}},
		{"ordinal", "unexpected integer", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			f.editReport(e[1], func(m map[string]any) { m["records"].([]any)[0].(map[string]any)["ordinal"] = 2 })
		}},
		{"input-state", "unexpected integer", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			f.editReport(e[1], func(m map[string]any) { m["records"].([]any)[0].(map[string]any)["inputState"] = 0 })
		}},
		{"mutation", "mutation", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			f.editReport(e[1], func(m map[string]any) { m["records"].([]any)[0].(map[string]any)["mutation"] = "empty-drain" })
		}},
		{"fixture", "fixture", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			f.editReport(e[1], func(m map[string]any) { m["fixture"].(map[string]any)["mutationPath"] = "different.ts" })
		}},
		{"declared-source", "Production", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			f.editReport(e[1], func(m map[string]any) {
				m["declaredOptions"].(map[string]any)["Production"] = residentProductionPins["S"]
			})
		}},
		{"runtime", "runtime", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			f.editReport(e[1], func(m map[string]any) { m["runtime"].(map[string]any)["gomaxprocs"] = 2 })
		}},
		{"runtime-env", "environment hash", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			f.editReport(e[1], func(m map[string]any) { m["runtime"].(map[string]any)["inheritedEnv"].(map[string]any)["GOGC"] = "off" })
		}},
		{"QPF", "QPF frequency", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			f.editReport(e[1], func(m map[string]any) {
				m["clock"].(map[string]any)["finalFrequency"].(map[string]any)["ticks"] = 9999999
			})
		}},
		{"BOOL32", "integer token/range", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			f.editReport(e[1], func(m map[string]any) {
				m["records"].([]any)[0].(map[string]any)["start"].(map[string]any)["bool"] = uint64(1) << 32
			})
		}},
		{"BOOL-status", "BOOL/status", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			f.editReport(e[1], func(m map[string]any) { m["records"].([]any)[0].(map[string]any)["start"].(map[string]any)["bool"] = 0 })
		}},
		{"conversion", "conversion.durationNs", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			f.editReport(e[1], func(m map[string]any) {
				m["records"].([]any)[0].(map[string]any)["conversion"].(map[string]any)["durationNs"] = 999
			})
		}},
		{"remainder", "remainder", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			f.editReport(e[1], func(m map[string]any) {
				m["records"].([]any)[0].(map[string]any)["conversion"].(map[string]any)["remainder"] = 1
			})
		}},
		{"claimed-assessment", "sum_ticks", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			e[1]["clock_assessment"].(map[string]any)["sum_ticks"] = 1
		}},
		{"outer-endpoint", "outside exact outer", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			e[1]["enclosure"].(map[string]any)["start"].(map[string]any)["ticks"] = 1002
		}},
		{"managed-BOOL", "must be absent", func(f *residentProtocolFixture, b residentBatchRef, e []map[string]any) {
			e[1]["enclosure"].(map[string]any)["start"].(map[string]any)["bool"] = 1
		}},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			f := newResidentProtocolFixture(t, residentTestKey())
			b := f.add("calibration-initial", 1, 100)
			e := f.events(b)
			tc.edit(f, b, e)
			f.seal(b, e)
			got, _ := f.batchReport(b)
			if got.Status != invalidEvidence || !residentProblem(got.Issues, tc.problem) {
				t.Fatalf("want invalid with %q, got %s: %+v", tc.problem, got.Status, got.Issues)
			}
		})
	}
}

func TestResidentReaderNativeUnsignedAndMissing(t *testing.T) {
	for _, test := range []struct {
		name               string
		user, kernel, peak uint64
		size               uint64
		optional, missing  bool
		want               string
	}{
		{"beyond-float", 1<<53 + 1, 1<<53 + 3, 1<<53 + 5, 144, false, false, criterionMet},
		{"uint64-sum-overflow", math.MaxUint64, math.MaxUint64, math.MaxUint64, 144, false, false, criterionMet},
		{"collector32", 1, 2, math.MaxUint32, 112, false, false, criterionMet},
		{"collector32-overflow", 1, 2, 1 << 32, 112, false, false, invalidEvidence},
		{"optional-attribution", 1, 2, 3, 144, true, false, criterionMet},
		{"required-missing", 1, 2, 3, 144, true, true, residentIncomplete},
	} {
		t.Run(test.name, func(t *testing.T) {
			n := residentInventedNative(0)
			a := n["ResourceAccounting"].(map[string]any)
			job := a["JobAccounting"].(map[string]any)["Values"].(map[string]any)
			job["UserTicks100ns"], job["KernelTicks100ns"] = test.user, test.kernel
			a["RootCpu"].(map[string]any)["Values"] = map[string]any{"UserTicks100ns": test.user, "KernelTicks100ns": test.kernel}
			mem := a["JobMemory"].(map[string]any)["Values"].(map[string]any)
			mem["PeakJobMemoryBytes"], mem["StructureBytes"] = test.peak, test.size
			if test.optional {
				for _, name := range []string{"RootCpu", "OtherOwnedCpu"} {
					a[name] = map[string]any{"Status": "failed", "Error": "invented optional failure", "Values": nil}
				}
				a["Status"] = "partial"
			}
			if test.missing {
				a["JobMemory"] = map[string]any{"Status": "unavailable", "Error": "invented missing getter", "Values": nil}
			}
			r := newResidentReader()
			var layout uint64
			got := r.resources(mustJSON(a), "native", &layout)
			if residentIssueStatus(r.issues) != test.want {
				t.Fatalf("%s %+v", residentIssueStatus(r.issues), r.issues)
			}
			if test.want == criterionMet && (*got.UserTicks != test.user || *got.KernelTicks != test.kernel || *got.PeakBytes != test.peak) {
				t.Fatal("native integer precision lost")
			}
		})
	}
}

func TestResidentReaderStrictScalarsAndDocuments(t *testing.T) {
	for _, raw := range []string{`{"v":null}`, `{"v":1.0}`, `{"v":1e0}`, `{"v":-0}`, `{"v":"1"}`, `{"v":18446744073709551616}`} {
		r := newResidentReader()
		o := r.obj([]byte(raw), "test", "v", "")
		r.uint(o, "v", "test", math.MaxUint64)
		if len(r.issues) == 0 {
			t.Fatalf("accepted %s", raw)
		}
	}
	for _, raw := range []string{`{"v":null}`, `{"v":true}`, `{"v":123}`} {
		r := newResidentReader()
		o := r.obj([]byte(raw), "test", "v", "")
		r.str(o, "v", "test")
		if len(r.issues) == 0 {
			t.Fatalf("string accepted %s", raw)
		}
	}
	for _, raw := range []string{`{"v":null}`, `{"v":"true"}`, `{"v":1}`} {
		r := newResidentReader()
		o := r.obj([]byte(raw), "test", "v", "")
		r.boolean(o, "v", "test")
		if len(r.issues) == 0 {
			t.Fatalf("bool accepted %s", raw)
		}
	}
	for _, raw := range [][]byte{[]byte(`{"v":1,"v":2}`), []byte(`{"v":1,"V":2}`), []byte(`{"v":"\ud800"}`), {0xff}, []byte(`{"v":1} false`), []byte(strings.Repeat("[", 70) + strings.Repeat("]", 70))} {
		r := newResidentReader()
		r.obj(raw, "test", "v", "")
		if len(r.issues) == 0 {
			t.Fatalf("document accepted %q", raw)
		}
	}
}

func TestResidentReaderSnapshotImmutableAndBounded(t *testing.T) {
	p := filepath.Join(t.TempDir(), "input.json")
	data := []byte(`{"invented":true}`)
	if err := os.WriteFile(p, data, 0o600); err != nil {
		t.Fatal(err)
	}
	r := newResidentReader()
	first := r.snapshot(p, false)
	if err := os.WriteFile(p, []byte("replacement"), 0o600); err != nil {
		t.Fatal(err)
	}
	if got := r.snapshot(p, false); got != first || !bytes.Equal(got.data, data) {
		t.Fatal("snapshot reread mutable source")
	}
	if !bytes.Equal(first.Raw, data) {
		t.Fatal("original snapshot lost")
	}
	oversize := filepath.Join(t.TempDir(), "oversize")
	f, err := os.Create(oversize)
	if err != nil {
		t.Fatal(err)
	}
	if err = f.Truncate(residentFileLimit + 1); err != nil {
		t.Fatal(err)
	}
	if err = f.Close(); err != nil {
		t.Fatal(err)
	}
	r.snapshot(oversize, false)
	if !residentProblem(r.issues, "bound exceeded") {
		t.Fatal(r.issues)
	}
	for _, bad := range []string{"relative.json", `\\server\share\input.json`, ""} {
		r.snapshot(bad, false)
	}
}

func TestResidentReaderMalformedShapesNoPanic(t *testing.T) {
	for _, value := range []any{nil, []any{}, map[string]any{}, "bad", -1} {
		t.Run(fmt.Sprintf("%T-%v", value, value), func(t *testing.T) {
			f := newResidentProtocolFixture(t, residentTestKey())
			f.plan.Campaigns = nil
			p := f.write(filepath.Join(f.root, "malformed.json"), map[string]any{"schema": 1, "evidence_kind": "invented-protocol", "policy_sha256": residentPolicySHA, "campaigns": value})
			r := newResidentReader()
			got := r.reconstruct(p.Path)
			r.decide(&got)
			if got.Outcome != invalidEvidence {
				t.Fatal(got.Outcome)
			}
		})
	}
}

func TestResidentReaderUnknownFinalStatusAndDepth(t *testing.T) {
	f := newResidentProtocolFixture(t, residentTestKey())
	b := f.add("calibration-initial", 1, 100)
	p := filepath.Join(b.Directory, "batch.finished.json")
	m := f.read(p)
	m["status"] = "arbitrary-success-word"
	f.write(p, m)
	got, _ := f.batchReport(b)
	if got.Status != invalidEvidence || !residentProblem(got.Issues, "unknown final batch status") {
		t.Fatal(got.Status, got.Issues)
	}
	r := newResidentReader()
	r.obj([]byte(`{"v":`+strings.Repeat("[", 130)+`0`+strings.Repeat("]", 130)+`}`), "test", "v", "")
	if !residentProblem(r.issues, "nesting exceeds") {
		t.Fatal("deep extension was not bounded", r.issues)
	}
}
