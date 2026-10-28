package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestResidentReaderDiagnosticOrderingDeterministic(t *testing.T) {
	f := newResidentProtocolFixture(t, residentTestKey())
	b := f.add("calibration-initial", 1, 100)
	events := f.events(b)
	f.editReport(events[1], func(report map[string]any) {
		clock := report["clock"].(map[string]any)
		clock["backend"], clock["counterApi"], clock["scope"] = "bad-backend", "bad-api", "bad-scope"
		options := report["declaredOptions"].(map[string]any)
		options["Production"], options["Harness"] = "wrong-source", "wrong-harness"
	})
	f.seal(b, events)
	plan := f.write(filepath.Join(f.root, "reader.json"), f.plan)
	var first []byte
	var varied bool
	for i := 0; i < 10; i++ {
		var stdout, stderr bytes.Buffer
		out := filepath.Join(f.root, fmt.Sprintf("invalid-audit-%02d.json", i))
		if code := runResidentPlan(plan.Path, out, &stdout, &stderr); code != 1 {
			t.Fatalf("%d %s", code, &stderr)
		}
		data := mustReadTest(t, out)
		if i == 0 {
			first = bytes.Clone(data)
		} else if !bytes.Equal(data, first) {
			varied = true
		}
	}
	if varied {
		t.Fatal("resident diagnostic ordering varied on identical raw bytes")
	}
}

func (f *residentProtocolFixture) analyze() residentReaderReport {
	f.t.Helper()
	ref := f.write(filepath.Join(f.root, "reader.json"), f.plan)
	r := newResidentReader()
	report := r.reconstruct(ref.Path)
	r.decide(&report)
	return report
}

func TestResidentReaderAdmissionDeclarations(t *testing.T) {
	for _, tc := range []struct {
		name, want string
		edit       func(*residentProtocolFixture)
	}{
		{"absent", residentUnverified, func(f *residentProtocolFixture) { f.plan.Admission = nil }},
		{"unverified", residentUnverified, func(f *residentProtocolFixture) {
			m := f.read(f.plan.Admission.Path)
			m["prerequisites"].(map[string]any)["effect-precision"] = "unverified"
			ref := f.write(f.plan.Admission.Path, m)
			f.plan.Admission = &ref
		}},
		{"failed", residentUnverified, func(f *residentProtocolFixture) {
			m := f.read(f.plan.Admission.Path)
			m["prerequisites"].(map[string]any)["quiet-host"] = "failed"
			ref := f.write(f.plan.Admission.Path, m)
			f.plan.Admission = &ref
		}},
		{"late", invalidEvidence, func(f *residentProtocolFixture) {
			m := f.read(f.plan.Admission.Path)
			m["declared_utc"] = "2027-01-01T00:00:00Z"
			ref := f.write(f.plan.Admission.Path, m)
			f.plan.Admission = &ref
		}},
		{"missing-batch", invalidEvidence, func(f *residentProtocolFixture) {
			m := f.read(f.plan.Admission.Path)
			m["batches"] = []any{}
			ref := f.write(f.plan.Admission.Path, m)
			f.plan.Admission = &ref
		}},
		{"wrong-host", invalidEvidence, func(f *residentProtocolFixture) {
			m := f.read(f.plan.Admission.Path)
			m["batches"].([]any)[0].(map[string]any)["host_sha256"] = residentTestHash(nil)
			ref := f.write(f.plan.Admission.Path, m)
			f.plan.Admission = &ref
		}},
		{"real-claim-on-invented", invalidEvidence, func(f *residentProtocolFixture) {
			m := f.read(f.plan.Admission.Path)
			m["kind"] = "root-resident-admission-v1"
			ref := f.write(f.plan.Admission.Path, m)
			f.plan.Admission = &ref
		}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			f := newResidentProtocolFixture(t, residentTestKey())
			f.add("calibration-initial", 1, 100)
			f.authorize()
			tc.edit(f)
			got := f.analyze()
			if got.Outcome != tc.want {
				t.Fatalf("want %s got %s: %+v", tc.want, got.Outcome, got.Issues)
			}
			if got.PromotionReady {
				t.Fatal("promotion")
			}
			for _, track := range got.CalibrationOnly {
				if track.FollowUpAllowed {
					t.Fatal("unverified/invalid prerequisites authorized retry")
				}
			}
		})
	}
}

func TestResidentReaderCalibrationBeforeCandidateAndCLI(t *testing.T) {
	f := newResidentProtocolFixture(t, residentTestKey())
	f.add("calibration-initial", 1, 100)
	f.authorize()
	input := f.write(filepath.Join(f.root, "reader.json"), f.plan)
	out := filepath.Join(f.root, "audit.json")
	var stdout, stderr bytes.Buffer
	before := mustReadTest(t, input.Path)
	if code := run([]string{"--resident-plan", input.Path, "--output", out}, &stdout, &stderr); code != 0 {
		t.Fatalf("exit=%d stdout=%s stderr=%s", code, &stdout, &stderr)
	}
	data := mustReadTest(t, out)
	var report residentReaderReport
	if err := json.Unmarshal(data, &report); err != nil {
		t.Fatal(err)
	}
	if report.Outcome != criterionMet || len(report.CalibrationOnly) != 1 || len(report.Campaigns) != 0 || len(report.Snapshots) == 0 || report.PromotionReady {
		t.Fatal("calibration-only or raw retention changed")
	}
	if code := runResidentPlan(input.Path, out, &stdout, &stderr); code != 2 {
		t.Fatal("O_EXCL collision accepted")
	}
	if !bytes.Equal(data, mustReadTest(t, out)) {
		t.Fatal("report overwritten")
	}
	if code := runResidentPlan(input.Path, input.Path, &stdout, &stderr); code != 2 {
		t.Fatal("input alias accepted")
	}
	if !bytes.Equal(before, mustReadTest(t, input.Path)) {
		t.Fatal("plan modified")
	}
	// A missing raw input still has an identity in the snapshot registry and
	// cannot be created as the report output after admission failed.
	missing := filepath.Join(f.root, "absent-plan.json")
	if code := runResidentPlan(missing, missing, &stdout, &stderr); code != 2 {
		t.Fatal("missing input alias accepted")
	}
}

func TestResidentReaderRawStageAndLabelBoundary(t *testing.T) {
	key := ResidentKey{Comparator: "B:C", Scenario: "Chain32", Settings: "C1B1", Role: "primary"}
	f := newResidentProtocolFixture(t, key)
	f.add("calibration-initial", 1, 100)
	f.add("initial", 2, 80)
	f.add("confirmation", 3, 80)
	f.authorize()
	got := f.analyze()
	if got.Outcome != criterionMet || len(got.Campaigns) != 1 || len(got.Campaigns[0].Comparison.Batches) != 2 {
		t.Fatalf("independent raw confirmation failed: %s %+v", got.Outcome, got.Issues)
	}
	c := got.campaigns[0]
	c.Label = residentRecorded
	c.Prerequisites = residentDeclared
	for _, track := range []*[]ResidentBatch{&c.Calibration, &c.Comparison} {
		for i := range *track {
			(*track)[i].Label = residentRecorded
		}
	}
	recorded := EvaluateResident(c)
	if recorded.Status != got.Campaigns[0].Status || recorded.Noise != got.Campaigns[0].Noise {
		t.Fatal("recorded label changed numerical semantics")
	}
	c.Prerequisites = residentAssumed
	if EvaluateResident(c).Status != invalidEvidence {
		t.Fatal("recorded data accepted synthetic assumptions")
	}
	c.Prerequisites = residentDeclared
	c.Comparison[0].Label = residentSynthetic
	if EvaluateResident(c).Status != invalidEvidence {
		t.Fatal("mixed labels accepted")
	}
	cal := c.Calibration
	cal = append(cal, cal[0])
	cal[1].Label = residentSynthetic
	cal[1].ID = "different"
	cal[1].Stage = "follow_up"
	cal[1].Chronology = 2
	if EvaluateResidentCalibration(cal, residentDeclared).Status != invalidEvidence {
		t.Fatal("mixed calibration labels accepted")
	}
}

func TestResidentReaderPreservesAdverseLaterEvidence(t *testing.T) {
	for _, tc := range []struct {
		name, stage string
		ratio       uint64
		want        string
	}{
		{"unrequested-followup", "followup", 80, invalidEvidence},
		{"adverse-confirmation", "confirmation", 120, criterionNotMet},
	} {
		t.Run(tc.name, func(t *testing.T) {
			key := ResidentKey{Comparator: "B:C", Scenario: "Chain32", Settings: "C1B1", Role: "primary"}
			f := newResidentProtocolFixture(t, key)
			f.add("calibration-initial", 1, 100)
			f.add("initial", 2, 80)
			f.add(tc.stage, 3, tc.ratio)
			f.authorize()
			got := f.analyze()
			if got.Outcome != tc.want || len(got.Batches) != 3 {
				t.Fatalf("%s %+v", got.Outcome, got.Issues)
			}
		})
	}
}

func TestResidentReaderRecordedCannotUseInventedDescriptors(t *testing.T) {
	f := newResidentProtocolFixture(t, residentTestKey())
	f.add("calibration-initial", 1, 100)
	f.plan.EvidenceKind = "recorded-evidence"
	got := f.analyze()
	if got.Outcome != invalidEvidence || !residentProblem(got.Issues, "final reviewed pin") {
		t.Fatalf("invented source admitted as recorded: %s %+v", got.Outcome, got.Issues)
	}
}

// Existing retained diagnostics/stubs are read-only negative controls. The
// portable invented tests above run without their environment-specific paths.
func TestResidentReaderRetainedDiagnosticsExcluded(t *testing.T) {
	root := os.Getenv("RESIDENT_READER_DIAGNOSTICS")
	if root == "" {
		t.Skip("retained six-process diagnostic directory not supplied")
	}
	paths, err := filepath.Glob(filepath.Join(root, "*", "plan.raw.json"))
	if err != nil || len(paths) != 6 {
		t.Fatalf("want exact six diagnostic batches, got %d %v", len(paths), err)
	}
	for _, path := range paths {
		t.Run(filepath.Base(filepath.Dir(path)), func(t *testing.T) {
			data := mustReadTest(t, path)
			r := newResidentReader()
			p := r.obj(data, "plan", "mode stage order", "")
			r.want(p, "mode", "plan", "session")
			if len(r.issues) == 0 {
				t.Fatal("diagnostic accepted as session")
			}
			journals := bytes.Split(bytes.TrimSpace(mustReadTest(t, filepath.Join(filepath.Dir(path), "journal.ndjson"))), []byte{'\n'})
			if len(journals) != 2 {
				t.Fatal("diagnostic population changed")
			}
			var event map[string]any
			_ = json.Unmarshal(journals[1], &event)
			reportBytes := mustReadTest(t, event["report_path"].(string))
			report := r.obj(reportBytes, "report", "classification", "")
			r.want(report, "classification", "report", "session")
			if !residentProblem(r.issues, "classification") {
				t.Fatal("diagnostic classification accepted")
			}
			// Clock/native reconstruction can be checked without changing the
			// diagnostic classification or calculating any scored statistic.
			arithmetic := newResidentReader()
			var rawEvent object
			if err := json.Unmarshal(journals[1], &rawEvent); err != nil {
				t.Fatal(err)
			}
			host := jsonObjectTest(json.RawMessage(mustReadTest(t, filepath.Join(filepath.Dir(path), "host.before.json"))))
			settings := "C1B1"
			if strings.Contains(path, "C4B4") {
				settings = "C4B4"
			}
			diagnosticKey := ResidentKey{Comparator: "B:C", Scenario: "EmptyDrain_L32F32", Settings: settings, Role: "control"}
			arithmetic.fixture(report["fixture"], arithmetic.str(report, "fixtureSha256", "diagnostic"), diagnosticKey, "diagnostic-only")
			arithmetic.runtimeEnvironment(report, host, "diagnostic-only")
			obs, _ := arithmetic.clock(report, rawEvent, host, "diagnostic-only")
			var layout uint64
			arithmetic.native(rawEvent, p, "diagnostic-only", &layout)
			if len(arithmetic.issues) > 0 || len(obs.TickSpans) != 30 {
				t.Fatalf("final diagnostic protocol arithmetic mismatch: %+v", arithmetic.issues)
			}
			if !bytes.Equal(data, mustReadTest(t, path)) || !bytes.Equal(reportBytes, mustReadTest(t, event["report_path"].(string))) {
				t.Fatal("negative control mutated")
			}
			// Feed the unmodified retained tree through the actual admission path,
			// rather than treating the classification-only check as integration.
			var inventory []map[string]any
			if err := json.Unmarshal(mustReadTest(t, filepath.Join(filepath.Dir(path), "inputs.before.json")), &inventory); err != nil {
				t.Fatal(err)
			}
			var original residentFileRef
			for _, row := range inventory {
				if row["role"] == "plan" {
					original = residentFileRef{row["path"].(string), row["sha256"].(string)}
				}
			}
			reader := newResidentReader()
			reader.kind = "recorded-evidence"
			key := ResidentKey{Comparator: "B:C", Scenario: "EmptyDrain_L32F32", Settings: "C1B1", Role: "control"}
			if strings.Contains(path, "C4B4") {
				key.Settings = "C4B4"
			}
			batch := reader.batch(residentBatchRef{Directory: filepath.Dir(path), Plan: original, Chronology: 1}, key, "calibration")
			if batch.Status != invalidEvidence || !residentProblem(batch.Issues, "classification") {
				t.Fatalf("actual raw diagnostic admitted: %s %+v", batch.Status, batch.Issues)
			}
			if len(batch.Clocks) != 1 || batch.Clocks[0].OneTick || batch.Clocks[0].Sum.Sign() <= 0 {
				t.Fatal("diagnostic clock arithmetic was lost")
			}
		})
	}
}

func TestResidentReaderRetainedLauncherStubExcluded(t *testing.T) {
	root := os.Getenv("RESIDENT_READER_STUB")
	if root == "" {
		t.Skip("retained final launcher stub directory not supplied")
	}
	paths, err := filepath.Glob(filepath.Join(root, "case-001-happy", "*", "plan.raw.json"))
	if err != nil || len(paths) != 1 {
		t.Fatalf("stub raw plan shape %v %v", paths, err)
	}
	data := mustReadTest(t, paths[0])
	r := newResidentReader()
	p := r.obj(data, "stub", "mode stage", "")
	r.want(p, "mode", "stub", "session")
	if !residentProblem(r.issues, "mode") {
		t.Fatal("exhausted smoke/diagnostic stub accepted")
	}
}

func TestResidentReaderNoPartialSuccessfulSubset(t *testing.T) {
	f := newResidentProtocolFixture(t, residentTestKey())
	b := f.add("calibration-initial", 1, 100)
	e := f.events(b)
	e = e[:3]
	f.seal(b, e)
	p := filepath.Join(b.Directory, "batch.finished.json")
	m := f.read(p)
	m["status"], m["finished"], m["completed"] = "incomplete", 1, 1
	f.write(p, m)
	got, _ := f.batchReport(b)
	if got.Status != residentIncomplete || len(got.Reconstructed.Blocks) != 1 || len(got.Reconstructed.Blocks[0].Sessions) != 1 {
		t.Fatalf("partial prefix lost or selected: %s %+v", got.Status, got.Issues)
	}
}

func TestResidentReaderEnvironmentIdentityUnchanged(t *testing.T) {
	f := newResidentProtocolFixture(t, residentTestKey())
	f.add("calibration-initial", 1, 100)
	b := f.add("initial", 2, 80)
	e := f.events(b)
	for _, name := range []string{"host.before.json", "host.after.json"} {
		p := filepath.Join(b.Directory, name)
		m := f.read(p)
		m["environment"].(map[string]any)["__MISE_SESSION"] = residentTestHash([]byte("distinct inherited parent"))
		f.write(p, m)
	}
	f.seal(b, e)
	f.authorize()
	got := f.analyze()
	if got.Outcome != invalidEvidence || !residentProblem(got.Issues, "host/environment") {
		t.Fatalf("shell token drift normalized: %s %+v", got.Outcome, got.Issues)
	}
	if strings.Contains(string(got.Admission), "synthetic-assumed") {
		t.Fatal("declaration substituted for actual identity")
	}
}

func TestResidentReaderFrozenConditionsDrift(t *testing.T) {
	for _, field := range []string{"timeoutMs", "reportBytes", "fixtureInput"} {
		t.Run(field, func(t *testing.T) {
			f := newResidentProtocolFixture(t, residentTestKey())
			f.add("calibration-initial", 1, 100)
			b := f.add("initial", 2, 80)
			p := f.read(b.Plan.Path)
			switch field {
			case "timeoutMs":
				p[field] = 240001
			case "reportBytes":
				p[field] = residentFileLimit - 1
			case "fixtureInput":
				old := p[field].(map[string]any)
				newRef := f.write(filepath.Join(f.root, "inputs", "duplicate-fixture.json"), mustReadTest(t, old["path"].(string)))
				oldPath := old["path"].(string)
				p[field] = newRef
				for _, name := range []string{"inputs.before.json", "inputs.after.json"} {
					path := filepath.Join(b.Directory, name)
					var rows []map[string]any
					_ = json.Unmarshal(mustReadTest(t, path), &rows)
					for _, row := range rows {
						if row["path"] == oldPath {
							row["path"] = newRef.Path
						}
					}
					f.write(path, rows)
				}
				mp := filepath.Join(b.Directory, "evidence.manifest.json")
				m := f.read(mp)
				for _, raw := range m["retained"].([]any) {
					row := raw.(map[string]any)
					if row["original"] == oldPath {
						row["original"] = newRef.Path
					}
				}
				f.write(mp, m)
			}
			b = f.rebindInput(b, b.Plan.Path, p)
			f.plan.Campaigns[0].Comparison[0] = b
			f.refs[1] = b
			f.raw[filepath.Base(b.Directory)] = p
			f.seal(b, f.events(b))
			f.authorize()
			got := f.analyze()
			if got.Outcome != invalidEvidence || !residentProblem(got.Issues, "conditions."+field) {
				t.Fatalf("frozen condition drift accepted: %s %+v", got.Outcome, got.Issues)
			}
		})
	}
	// Independent shared-input join: equal roles with different original paths
	// or bytes cannot silently reuse calibration nuisance evidence.
	for _, role := range []string{"policy", "semantic-admission", "collector-interface", "dependencies"} {
		r := newResidentReader()
		a := jsonObjectTest(map[string]any{"inputs": []any{map[string]any{"role": role, "path": "old", "sha256": residentPolicySHA}}})
		b := jsonObjectTest(map[string]any{"inputs": []any{map[string]any{"role": role, "path": "new", "sha256": residentPolicySHA}}})
		r.equal(r.inputDeclaration(a, role), r.inputDeclaration(b, role), "conditions."+role)
		if len(r.issues) == 0 {
			t.Fatal("shared input path drift accepted", role)
		}
	}
}

func TestResidentReaderRawStandaloneAndJointWitness(t *testing.T) {
	key := ResidentKey{Comparator: "B:C", Scenario: "Chain32", Settings: "C1B1", Role: "primary"}
	f := newResidentProtocolFixture(t, key)
	f.add("calibration-initial", 1, 100)
	initial := f.add("initial", 2, 80)
	f.add("followup", 3, 80)
	f.add("confirmation", 4, 80)
	e := f.events(initial)
	native := e[3]["result"].(map[string]any)
	resources := native["ResourceAccounting"].(map[string]any)
	for _, name := range []string{"JobAccounting", "RootCpu"} {
		resources[name].(map[string]any)["Values"].(map[string]any)["UserTicks100ns"] = 1875001
	}
	e[3]["native_result_json"] = string(mustJSON(native))
	f.seal(initial, e)
	f.authorize()
	got := f.analyze()
	if got.Outcome != criterionMet {
		t.Fatalf("raw witness failed: %s %+v", got.Outcome, got.Issues)
	}
	d := got.Campaigns[0].Comparison.Decisions
	if d["initial"].Status != inconclusive || d["follow_up"].Status != criterionMet || d["joint"].Status != criterionMet || d["confirmation"].Status != criterionMet {
		t.Fatal(d)
	}
}

func TestResidentReaderRawPrimaryAndSecondary(t *testing.T) {
	primary := newResidentProtocolFixture(t, ResidentKey{Comparator: "B:C", Scenario: "Chain32", Settings: "C1B1", Role: "primary"})
	secondary := newResidentProtocolFixture(t, ResidentKey{Comparator: "B:C", Scenario: "Chain32", Settings: "C4B4", Role: "secondary"})
	for i, f := range []*residentProtocolFixture{primary, secondary} {
		f.add("calibration-initial", 1+i*3, 100)
		f.add("initial", 2+i*3, 80)
		f.add("confirmation", 3+i*3, 80)
	}
	primary.plan.Campaigns = append(primary.plan.Campaigns, secondary.plan.Campaigns[0])
	primary.refs = append(primary.refs, secondary.refs...)
	for k, v := range secondary.raw {
		primary.raw[k] = v
	}
	primary.authorize()
	got := primary.analyze()
	if got.Outcome != criterionMet || got.Question == nil || got.Question.Status != criterionMet || got.Question.PromotionReady {
		t.Fatalf("raw primary AND secondary failed: %s %+v", got.Outcome, got.Issues)
	}
}

func TestResidentReaderMalformedDeclarationStillWritesEvidence(t *testing.T) {
	f := newResidentProtocolFixture(t, residentTestKey())
	f.add("calibration-initial", 1, 100)
	bad := []byte(`{"schema":1} truncated`)
	ref := f.write(filepath.Join(f.root, "bad-admission.json"), bad)
	f.plan.Admission = &ref
	plan := f.write(filepath.Join(f.root, "reader.json"), f.plan)
	out := filepath.Join(f.root, "invalid-audit.json")
	var stdout, stderr bytes.Buffer
	if code := runResidentPlan(plan.Path, out, &stdout, &stderr); code != 1 {
		t.Fatalf("malformed raw declaration prevented report: %d %s", code, &stderr)
	}
	var got residentReaderReport
	if err := json.Unmarshal(mustReadTest(t, out), &got); err != nil {
		t.Fatal(err)
	}
	if got.Outcome != invalidEvidence {
		t.Fatal(got.Outcome)
	}
	found := false
	for _, s := range got.Snapshots {
		if s.Path == ref.Path {
			found = bytes.Equal(s.Raw, bad)
		}
	}
	if !found {
		t.Fatal("malformed declaration bytes lost")
	}
}
