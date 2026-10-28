package main

import (
	"bytes"
	"encoding/json"
	"math"
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestResourceCampaignDecisionTable(t *testing.T) {
	status := func(s string) *string { return &s }
	for _, tc := range []struct {
		name, initial        string
		follow, confirmation *string
		needConfirmation     bool
		want                 string
	}{
		{"failure has no rescue", criterionNotMet, status(criterionMet), status(criterionMet), true, criterionNotMet},
		{"passing initial forbids follow-up", criterionMet, status(criterionMet), nil, false, invalidEvidence},
		{"inconclusive permits one follow-up", inconclusive, nil, nil, false, inconclusive},
		{"joint unresolved defers", inconclusive, status(inconclusive), nil, false, inconclusive},
		{"joint fail dominates", inconclusive, status(criterionNotMet), nil, false, criterionNotMet},
		{"passing follow-up control admitted", inconclusive, status(criterionMet), nil, false, criterionMet},
		{"confirmation required", criterionMet, nil, nil, true, unverified},
		{"confirmation unresolved no rescue", inconclusive, status(criterionMet), status(inconclusive), true, inconclusive},
		{"confirmation failure no rescue", criterionMet, nil, status(criterionNotMet), true, criterionNotMet},
		{"confirmation passes separately", inconclusive, status(criterionMet), status(criterionMet), true, criterionMet},
		{"missing collection is not uncertainty", incompleteResource, status(criterionMet), nil, false, incompleteResource},
	} {
		t.Run(tc.name, func(t *testing.T) {
			got, _ := campaignAction(tc.initial, tc.follow, tc.confirmation, tc.needConfirmation)
			if got != tc.want {
				t.Fatalf("got %s want %s", got, tc.want)
			}
		})
	}
	if got := resourceOutcome(inconclusive, criterionNotMet, criterionMet); got != criterionNotMet {
		t.Fatalf("all-gate failure precedence: %s", got)
	}
	if got := resourceOutcome(inconclusive, unverified); got != unverified {
		t.Fatalf("missing calibration cannot authorize a rescue: %s", got)
	}
}

func writeResourceCampaignFixture(t *testing.T, hourOffset int, candidateCold int, counts ...int) resourcePaths {
	t.Helper()
	coldCount, noChangeCount := 30, 100
	if len(counts) == 2 {
		coldCount, noChangeCount = counts[0], counts[1]
	}
	m, j, r := resourceAdmissionFixture(t, coldCount, noChangeCount)
	var manifest map[string]any
	var run map[string]any
	if err := json.Unmarshal(m, &manifest); err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(r, &run); err != nil {
		t.Fatal(err)
	}
	for _, item := range manifest["records"].([]any) {
		record := item.(map[string]any)
		if record["phase"] == cold && record["binary"] == "candidate" {
			record["duration_ms"] = candidateCold
		}
	}
	m, _ = json.Marshal(manifest)
	lines := bytes.Split(bytes.TrimSpace(j), []byte{'\n'})
	for i, line := range lines {
		var e map[string]any
		if err := json.Unmarshal(line, &e); err != nil {
			t.Fatal(err)
		}
		stamp, _ := time.Parse(time.RFC3339Nano, e["at_utc"].(string))
		e["at_utc"] = stamp.Add(time.Duration(hourOffset) * time.Hour).Format(time.RFC3339Nano)
		inv := e["invocation"].(map[string]any)
		if e["event"] == "finished" && inv["phase"] == cold && inv["binary"] == "candidate" {
			result := e["result"].(map[string]any)
			result["DurationMS"], result["ElapsedMS"] = candidateCold, candidateCold
		}
		lines[i], _ = json.Marshal(e)
	}
	j = append(bytes.Join(lines, []byte{'\n'}), '\n')
	for _, key := range []string{"started_utc", "finished_utc"} {
		stamp, _ := time.Parse(time.RFC3339Nano, run[key].(string))
		run[key] = stamp.Add(time.Duration(hourOffset) * time.Hour).Format(time.RFC3339Nano)
	}
	r, _ = json.Marshal(run)
	dir := filepath.Dir(run["invocation_journal"].(string))
	if err := os.MkdirAll(dir, 0o700); err != nil {
		t.Fatal(err)
	}
	p := resourcePaths{filepath.Join(dir, "windows-build-performance-manifest.json"), filepath.Join(dir, "invocations.ndjson"), filepath.Join(dir, "run.json")}
	for name, data := range map[string][]byte{p.Manifest: m, p.Journal: j, p.Run: r} {
		if err := os.WriteFile(name, data, 0o600); err != nil {
			t.Fatal(err)
		}
	}
	return p
}

func TestResourceCampaignCompleteCLI(t *testing.T) {
	aa := writeResourceCampaignFixture(t, 0, 1000)
	initial := writeResourceCampaignFixture(t, 2, 900)
	confirmation := writeResourceCampaignFixture(t, 4, 900)
	p := resourcePlan{Schema: 1, Fixture: "synthetic-only", Mode: "candidate", Role: "primary", Primary: cold, Initial: initial, Calibration: &calibrationPaths{Initial: aa}, Confirmation: &confirmation}
	data, err := json.Marshal(p)
	if err != nil {
		t.Fatal(err)
	}
	dir := t.TempDir()
	planPath, output := filepath.Join(dir, "plan.json"), filepath.Join(dir, "audit.json")
	if err := os.WriteFile(planPath, data, 0o600); err != nil {
		t.Fatal(err)
	}
	var stdout, stderr bytes.Buffer
	if code := run([]string{"--resource-plan", planPath, "--output", output}, &stdout, &stderr); code != 0 {
		t.Fatalf("CLI %d: %s %s", code, &stdout, &stderr)
	}
	encoded, err := os.ReadFile(output)
	if err != nil {
		t.Fatal(err)
	}
	var r resourceReport
	if err := json.Unmarshal(encoded, &r); err != nil {
		t.Fatal(err)
	}
	if r.Outcome != criterionMet || r.PromotionReady || r.Calibration.NoiseN != 0 || r.Comparison.NoiseN != 0 || !bytes.Equal(r.Input.Bytes, data) {
		t.Fatalf("bad final resource report: %s %+v", r.Outcome, r.Issues)
	}
	for _, gate := range r.ExternalGates {
		if gate.Status != unverified {
			t.Fatal("external gate promoted")
		}
	}
	if r.Calibration.Summaries["initial"].DescriptivePrimary.Status != criterionNotMet || r.Calibration.Batches["initial"].LegacyVerdict != "FAIL" {
		t.Fatal("AA original/legacy result hidden")
	}
	if code := run([]string{"--resource-plan", planPath, "--output", output}, &stdout, &stderr); code != 2 {
		t.Fatal("existing output overwritten")
	}
	for _, tc := range []struct {
		paths resourcePaths
		batch resourceBatch
	}{{aa, r.Calibration.Batches["initial"]}, {initial, r.Comparison.Batches["initial"]}, {confirmation, r.Comparison.Batches["confirmation"]}} {
		for name, want := range map[string][]byte{tc.paths.Manifest: tc.batch.RawManifest.Bytes, tc.paths.Journal: tc.batch.RawJournal.Bytes, tc.paths.Run: tc.batch.RawRun.Bytes} {
			got, err := os.ReadFile(name)
			if err != nil || !bytes.Equal(got, want) {
				t.Fatalf("raw input changed or report lost bytes: %s %v", name, err)
			}
		}
	}
}

func TestResourceCampaignIdentityAndUnusedInvalidEvidence(t *testing.T) {
	a := writeResourceCampaignFixture(t, 0, 2000)
	b := writeResourceCampaignFixture(t, 2, 1000, 60, 200)
	p := resourcePlan{Schema: 1, Fixture: "synthetic", Mode: "calibration", Role: "primary", Primary: cold, Initial: a, FollowUp: &b}
	// Initial A/A fails, so a follow-up cannot rescue it. Its malformed run
	// still invalidates supplied evidence rather than being hidden by that fail.
	bad, err := os.ReadFile(b.Run)
	if err != nil {
		t.Fatal(err)
	}
	bad = bytes.Replace(bad, []byte(`"status":"evaluated"`), []byte(`"status":"incomplete"`), 1)
	if err := os.WriteFile(b.Run, bad, 0o600); err != nil {
		t.Fatal(err)
	}
	data, _ := json.Marshal(p)
	r, err := evaluateResourcePlan(data, "")
	if err != nil {
		t.Fatal(err)
	}
	if r.Outcome != invalidEvidence {
		t.Fatalf("unused invalid stage hidden: %s", r.Outcome)
	}
	x, err := readResourceBatch(a, "", config{cold, .95, 30, 100})
	if err != nil {
		t.Fatal(err)
	}
	y := x
	var run object
	if err := json.Unmarshal(x.RawRun.Bytes, &run); err != nil {
		t.Fatal(err)
	}
	var before object
	if err := json.Unmarshal(run["before"], &before); err != nil {
		t.Fatal(err)
	}
	var fixture object
	if err := json.Unmarshal(before["fixture"], &fixture); err != nil {
		t.Fatal(err)
	}
	fixture["sha256"] = json.RawMessage(`"sha256-tree-v2:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"`)
	before["fixture"], _ = json.Marshal(fixture)
	run["before"], _ = json.Marshal(before)
	y.run = run
	if issues := compatibleResourceBatches(x, y, true); len(issues) == 0 {
		t.Fatal("wrong fixture accepted as calibration")
	}
	x.run = object{"capture_limit_bytes_per_stream": json.RawMessage(`9007199254740992`)}
	y.run = object{"capture_limit_bytes_per_stream": json.RawMessage(`9007199254740993`)}
	if issues := compatibleResourceBatches(x, y, true); len(issues) == 0 {
		t.Fatal("integer provenance mismatch rounded away")
	}
}

func TestResourceCampaignRetainsInitialNoiseAfterAdmittedFollowUp(t *testing.T) {
	makeBatch := func(blocks int) resourceBatch {
		b := resourceBatch{ResourceStatus: "complete", Phases: map[string][]pair{cold: resourceTestConstant(blocks, 10, 10), noChange: resourceTestConstant(blocks, 100, 100)}, Resources: map[string]map[string][]pair{}}
		for _, metric := range resourceMetricNames {
			baseline := 100.0
			if metric == "job_cpu_ms" {
				baseline = 62.5
			}
			b.Resources[metric] = map[string][]pair{cold: resourceTestConstant(blocks, baseline, baseline), noChange: resourceTestConstant(blocks, baseline, baseline)}
		}
		return b
	}
	a, b := makeBatch(15), makeBatch(30)
	coldPairs := a.Phases[cold]
	if a.Phases == nil || coldPairs == nil {
		t.Fatal("makeBatch must supply cold phase populations")
	}
	for i := range 20 {
		if i%2 == 0 {
			coldPairs[i] = pair{10, 12, "AB"}
		} else {
			coldPairs[i] = pair{12, 10, "BA"}
		}
	}
	a.Resources["job_cpu_ms"][cold][0].candidate = 250
	a.Resources["job_cpu_ms"][cold][1].candidate = 250
	p := resourcePlan{Role: "primary", Primary: cold}
	initial := summarizeResources([]resourceBatch{a}, p, true, nil)
	doubled := summarizeResources([]resourceBatch{b}, p, true, nil)
	joint := summarizeResources([]resourceBatch{a, b}, p, true, nil)
	if initial.Outcome != inconclusive || doubled.Outcome != criterionMet || joint.Outcome != criterionMet {
		t.Fatalf("R3 admitted AA follow-up path %s/%s/%s", initial.Outcome, doubled.Outcome, joint.Outcome)
	}
	n := retainedCalibrationNoise(map[string]resourceSummary{"initial": initial, "follow_up": doubled, "joint": joint})
	if math.Abs(n-math.Log(1.2)) > 1e-12 || doubled.Noise.N != 0 || joint.Noise.N != 0 {
		t.Fatalf("initial envelope erased: N=%g double=%g joint=%g", n, doubled.Noise.N, joint.Noise.N)
	}
	follow := resourceOutcome(doubled.Outcome, joint.Outcome)
	if outcome, _ := campaignAction(initial.Outcome, &follow, nil, false); outcome != criterionMet {
		t.Fatal("valid one-fixture AA follow-up not admitted")
	}
}

func TestResourceBiasedZeroWidthNoiseDecision(t *testing.T) {
	n := math.Log(1.04)
	if got := noiseDecision(estimate{.95, interval{.92, .98}}, n); got.Status != inconclusive {
		t.Fatalf("R3-P01 discriminator: %+v", got)
	}
	cutoff := math.Exp(-n)
	for _, tc := range []struct {
		lo, hi float64
		want   string
	}{
		{.9, math.Nextafter(cutoff, 0), criterionMet},
		{.9, cutoff, inconclusive},
		{cutoff, .99, criterionNotMet},
	} {
		if got := noiseDecision(estimate{.95, interval{tc.lo, tc.hi}}, n); got.Status != tc.want {
			t.Fatalf("%+v: %+v", tc, got)
		}
	}
}

func TestResourceCampaignNoChangeMetricStatusHasNoAbsoluteOverride(t *testing.T) {
	b := resourceBatch{ResourceStatus: "complete", Phases: map[string][]pair{cold: resourceTestConstant(15, 10, 10), noChange: resourceTestConstant(50, 10, 12)}, Resources: map[string]map[string][]pair{}}
	for _, metric := range resourceMetricNames {
		b.Resources[metric] = map[string][]pair{cold: resourceTestConstant(15, 0, 0), noChange: resourceTestConstant(50, 0, 0)}
	}
	s := summarizeResources([]resourceBatch{b}, resourcePlan{Role: "primary", Primary: cold}, true, nil)
	r := s.Metrics[noChange]["elapsed_ms"]
	if s.Outcome != criterionNotMet || r.Status != criterionNotMet || r.Rows["p95"].Status != criterionNotMet || r.Rows["p95"].AbsoluteAllowance != nil {
		t.Fatalf("ratio-only no-change row and metric disagree: %s/%s/%s", s.Outcome, r.Status, r.Rows["p95"].Status)
	}
}

func TestResourcePlanStrictAdmission(t *testing.T) {
	valid := `{"schema":1,"fixture":"synthetic","mode":"candidate","role":"primary","primary_phase":"cold","initial":{"manifest":"m","resource_journal":"j","run_metadata":"r"}}`
	if _, issues := parseResourcePlan([]byte(valid)); len(issues) != 0 {
		t.Fatal(issues)
	}
	for _, raw := range []string{
		`{"schema":1,"schema":1}`,
		`{"schema":1,"extension":"\ud800"}`,
		`{"schema":1} trailing`,
		`{"schema":1,"fixture":"x","mode":"calibration","role":"primary","primary_phase":"cold","initial":{"Manifest":"m","resource_journal":"j","run_metadata":"r"}}`,
	} {
		if _, issues := parseResourcePlan([]byte(raw)); len(issues) == 0 {
			t.Fatalf("accepted %s", raw)
		}
	}
}
