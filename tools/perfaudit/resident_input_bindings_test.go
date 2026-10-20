package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"math"
	"math/big"
	"path/filepath"
	"testing"
)

func (f *residentProtocolFixture) rebindInput(b residentBatchRef, path string, data any) residentBatchRef {
	f.t.Helper()
	changed := f.write(path, data)
	plan := f.read(b.Plan.Path)
	for _, raw := range plan["inputs"].([]any) {
		v := raw.(map[string]any)
		if v["path"] == path {
			v["sha256"] = changed.SHA256
		}
	}
	for _, name := range []string{"fixtureInput", "fixtureOwner"} {
		v := plan[name].(map[string]any)
		if v["path"] == path {
			v["sha256"] = changed.SHA256
		}
	}
	updated := f.write(b.Plan.Path, plan)
	f.write(filepath.Join(b.Directory, "plan.raw.json"), mustReadTest(f.t, b.Plan.Path))
	b.Plan = updated
	for _, name := range []string{"inputs.before.json", "inputs.after.json"} {
		file := filepath.Join(b.Directory, name)
		var rows []map[string]any
		_ = json.Unmarshal(mustReadTest(f.t, file), &rows)
		for _, row := range rows {
			p := row["path"].(string)
			if p == path || p == b.Plan.Path {
				bytes := mustReadTest(f.t, p)
				row["sha256"] = residentTestHash(bytes)
				if name == "inputs.before.json" {
					row["bytes"] = len(bytes)
					f.write(row["copy"].(string), bytes)
				}
			}
		}
		f.write(file, rows)
	}
	return b
}

func TestResidentReaderBuildAndFixtureProofJoins(t *testing.T) {
	for _, tc := range []struct {
		name, problem string
		path          func(*residentProtocolFixture, residentBatchRef) string
		edit          func(map[string]any)
	}{
		{"build-source", "buildRevision", func(f *residentProtocolFixture, b residentBatchRef) string {
			return filepath.Join(f.root, "inputs", "build-B.json")
		}, func(m map[string]any) { m["buildRevision"] = residentProductionPins["S"] }},
		{"manifest-lines", "effectiveProductionGoManifestSHA256", func(f *residentProtocolFixture, b residentBatchRef) string {
			return filepath.Join(f.root, "inputs", "build-B.json")
		}, func(m map[string]any) { m["effectiveProductionGoManifest"] = []string{"changed source graph"} }},
		{"harness-blob", "Git blob identity", func(f *residentProtocolFixture, b residentBatchRef) string {
			return filepath.Join(f.root, "inputs", "build-B.json")
		}, func(m map[string]any) {
			m["harness"].(map[string]any)["internal/compile/source_signature_resident_cycles_test.go"].(map[string]any)["gitBlob"] = residentProductionPins["B"]
		}},
		{"harness-sha", "source SHA256", func(f *residentProtocolFixture, b residentBatchRef) string {
			return filepath.Join(f.root, "inputs", "build-B.json")
		}, func(m map[string]any) {
			m["harness"].(map[string]any)["internal/compile/source_signature_resident_cycles_test.go"].(map[string]any)["sha256"] = residentTestHash(nil)
		}},
		{"overlay-join", "overlay", func(f *residentProtocolFixture, b residentBatchRef) string {
			return filepath.Join(f.root, "inputs", "build-B.json")
		}, func(m map[string]any) { m["overlay"].(map[string]any)["Replace"] = map[string]any{} }},
		{"fixture-seal", "fixture seal", func(f *residentProtocolFixture, b residentBatchRef) string {
			return filepath.Join(f.root, "inputs", "fixture.json")
		}, func(m map[string]any) {
			m["fixture"].(map[string]any)["initialInputs"].(map[string]any)["core/src/producer.ts"] = "changed fixture"
		}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			f := newResidentProtocolFixture(t, residentTestKey())
			b := f.add("calibration-initial", 1, 100)
			path := tc.path(f, b)
			m := f.read(path)
			tc.edit(m)
			b = f.rebindInput(b, path, m)
			f.seal(b, f.events(b))
			got, _ := f.batchReport(b)
			if got.Status != invalidEvidence || !residentProblem(got.Issues, tc.problem) {
				t.Fatalf("%s %+v", got.Status, got.Issues)
			}
		})
	}
}

func TestResidentReaderNativeLayoutIndependentBetweenBatches(t *testing.T) {
	f := newResidentProtocolFixture(t, residentTestKey())
	f.add("calibration-initial", 1, 100)
	b := f.add("initial", 2, 80)
	events := f.events(b)
	for i, e := range events {
		if i%2 == 0 {
			continue
		}
		n := e["result"].(map[string]any)
		n["ResourceAccounting"].(map[string]any)["JobMemory"].(map[string]any)["Values"].(map[string]any)["StructureBytes"] = 112
		e["native_result_json"] = string(mustJSON(n))
	}
	f.seal(b, events)
	f.authorize()
	got := f.analyze()
	if got.Outcome != criterionMet {
		t.Fatalf("cross-batch layout incorrectly pooled: %s %+v", got.Outcome, got.Issues)
	}
	events[1]["result"].(map[string]any)["ResourceAccounting"].(map[string]any)["JobMemory"].(map[string]any)["Values"].(map[string]any)["StructureBytes"] = 144
	events[1]["native_result_json"] = string(mustJSON(events[1]["result"]))
	f.seal(b, events)
	got = f.analyze()
	if got.Outcome != invalidEvidence || !residentProblem(got.Issues, "layout changed within") {
		t.Fatalf("mixed layout accepted: %s %+v", got.Outcome, got.Issues)
	}
}

func TestResidentReaderMissingRequiredValueIsIncomplete(t *testing.T) {
	f := newResidentProtocolFixture(t, residentTestKey())
	b := f.add("calibration-initial", 1, 100)
	e := f.events(b)
	native := e[1]["result"].(map[string]any)
	delete(native["ResourceAccounting"].(map[string]any)["JobAccounting"].(map[string]any)["Values"].(map[string]any), "UserTicks100ns")
	e[1]["native_result_json"] = string(mustJSON(native))
	f.seal(b, e)
	got, _ := f.batchReport(b)
	if got.Status != residentIncomplete || got.Reconstructed.Blocks[0].Sessions[0].Resources.UserTicks != nil {
		t.Fatalf("missing counter defaulted or malformed: %s %+v", got.Status, got.Issues)
	}
}

func TestResidentReaderNativePayloadSurvivesAnalysisAndJSON(t *testing.T) {
	f := newResidentProtocolFixture(t, residentTestKey())
	b := f.add("calibration-initial", 1, 100)
	events := f.events(b)
	for i, e := range events {
		if i%2 == 0 {
			continue
		}
		n := e["result"].(map[string]any)
		a := n["ResourceAccounting"].(map[string]any)
		for _, name := range []string{"JobAccounting", "RootCpu"} {
			v := a[name].(map[string]any)["Values"].(map[string]any)
			v["UserTicks100ns"], v["KernelTicks100ns"] = uint64(math.MaxUint64), uint64(math.MaxUint64)
		}
		a["JobAccounting"].(map[string]any)["Values"].(map[string]any)["ReadBytes"] = uint64(1<<53 + 1)
		e["native_result_json"] = string(mustJSON(n))
	}
	f.seal(b, events)
	f.authorize()
	plan := f.write(filepath.Join(f.root, "reader.json"), f.plan)
	reader := newResidentReader()
	report := reader.reconstruct(plan.Path)
	before := map[string][]byte{}
	for k, s := range reader.files {
		before[k] = bytes.Clone(s.data)
	}
	reader.decide(&report)
	reader.retained(&report)
	if report.Outcome != criterionMet {
		t.Fatalf("%s %+v", report.Outcome, report.Issues)
	}
	for k, s := range reader.files {
		if !bytes.Equal(s.data, before[k]) || !bytes.Equal(s.data, mustReadTest(t, s.Path)) {
			t.Fatal("analysis mutated original source", s.Path)
		}
	}
	var decoded residentReaderReport
	if err := json.Unmarshal(mustJSON(report), &decoded); err != nil {
		t.Fatal(err)
	}
	session := decoded.Batches[0].Reconstructed.Blocks[0].Sessions[0]
	if *session.Resources.UserTicks != math.MaxUint64 || *session.Resources.KernelTicks != math.MaxUint64 || *session.Resources.IO[3] != 1<<53+1 {
		t.Fatal("native precision lost through raw report JSON")
	}
	series := residentSeriesFor(decoded.Batches[0].Reconstructed.Blocks, "session_cpu_ms", -1)
	residentAssertRat(t, residentDistribution(series.values[0], series.scale, true).Mean, "3689348814741910323/1000")
}

func TestResidentReaderExactCPU125Over8Neighbors(t *testing.T) {
	for _, offset := range []int64{-1, 0, 1} {
		t.Run(fmt.Sprint(offset), func(t *testing.T) {
			f := newResidentProtocolFixture(t, residentTestKey())
			f.add("calibration-initial", 1, 100)
			b := f.add("initial", 2, 80)
			events := f.events(b)
			for i, e := range events {
				if i%2 == 0 || e["arm"] != "B" {
					continue
				}
				n := e["result"].(map[string]any)
				a := n["ResourceAccounting"].(map[string]any)
				for _, name := range []string{"JobAccounting", "RootCpu"} {
					a[name].(map[string]any)["Values"].(map[string]any)["UserTicks100ns"] = uint64(781251 + offset)
				}
				e["native_result_json"] = string(mustJSON(n))
			}
			f.seal(b, events)
			f.authorize()
			got := f.analyze()
			want := criterionMet
			if offset > 0 {
				want = criterionNotMet
			}
			if got.Outcome != want {
				t.Fatalf("%s %+v", got.Outcome, got.Issues)
			}
			rows := got.Campaigns[0].Comparison.Summaries["initial"].Populations["full"].Metrics["session_cpu_ms"].Rows
			for _, name := range []string{"mean", "median"} {
				row := rows[name].Forward
				residentAssertRat(t, row.exactMarginal().Difference.CI95.Upper, new(big.Rat).SetFrac(big.NewInt(156250+offset), big.NewInt(10000)).RatString())
				if offset == 0 && row.DecisiveRoute != "absolute" {
					t.Fatal("raw equality route changed")
				}
			}
		})
	}
}
