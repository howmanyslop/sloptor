package main

import (
	"encoding/json"
	"fmt"
	"io"
	"math"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"
)

const resourcePolicySHA256 = "3c234c019869f91a6bb811b04b20ce15d5b4d52fd96f6e2e8a984afd8dda6072"
const unverified = "unverified"

type resourcePaths struct {
	Manifest string `json:"manifest"`
	Journal  string `json:"resource_journal"`
	Run      string `json:"run_metadata"`
}

type calibrationPaths struct {
	Initial  resourcePaths  `json:"initial"`
	FollowUp *resourcePaths `json:"follow_up,omitempty"`
}

type resourcePlan struct {
	Schema       int               `json:"schema"`
	Fixture      string            `json:"fixture"`
	Mode         string            `json:"mode"`
	Role         string            `json:"role"`
	Primary      string            `json:"primary_phase"`
	Initial      resourcePaths     `json:"initial"`
	FollowUp     *resourcePaths    `json:"follow_up,omitempty"`
	Calibration  *calibrationPaths `json:"calibration,omitempty"`
	Confirmation *resourcePaths    `json:"confirmation,omitempty"`
}

type resourceSummary struct {
	Outcome            string                                     `json:"outcome"`
	Membership         []string                                   `json:"manifest_sha256_in_chronological_order"`
	Elapsed            map[string]phaseStatistics                 `json:"elapsed"`
	Metrics            map[string]map[string]resourceMetricResult `json:"metrics"`
	Noise              noiseSummary                               `json:"primary_phase_noise"`
	Gates              map[string]decision                        `json:"gates"`
	DescriptivePrimary *decision                                  `json:"original_primary_decision_not_aa_gate,omitempty"`
}

type resourceCampaign struct {
	Outcome       string                     `json:"outcome"`
	Action        string                     `json:"action"`
	Batches       map[string]resourceBatch   `json:"batches"`
	Summaries     map[string]resourceSummary `json:"summaries"`
	NoiseN        float64                    `json:"retained_noise_n"`
	NoiseEnvelope float64                    `json:"retained_noise_ratio_envelope"`
}

type resourceReport struct {
	Schema         int               `json:"schema"`
	PolicySHA256   string            `json:"policy_sha256"`
	Outcome        string            `json:"outcome"`
	Scope          string            `json:"scope"`
	Input          rawInput          `json:"plan_input"`
	Plan           resourcePlan      `json:"plan"`
	Issues         []issue           `json:"issues"`
	Comparison     *resourceCampaign `json:"comparison,omitempty"`
	Calibration    *resourceCampaign `json:"calibration,omitempty"`
	ExternalGates  []externalGate    `json:"external_gates"`
	Limitations    []string          `json:"limitations"`
	PromotionReady bool              `json:"promotion_ready"`
}

func resourcePlanObject(v *validation, raw json.RawMessage, path string, required, optional []string) object {
	o := v.object(raw, path, required, optional)
	for key := range o {
		known := false
		for _, name := range append(append([]string{}, required...), optional...) {
			known = known || key == name
		}
		if !known {
			v.invalid(path+"."+key, "unknown plan field; evidence extensions are supported, plan typos are rejected")
		}
	}
	return o
}

func parseResourcePlan(data []byte) (resourcePlan, []issue) {
	v := validation{}
	p := resourcePlan{}
	if !v.strictJSON(data) {
		return p, v.issues
	}
	obj := resourcePlanObject(&v, data, "$", []string{"schema", "fixture", "mode", "role", "primary_phase", "initial"}, []string{"follow_up", "calibration", "confirmation"})
	v.field(obj, "schema", "$", &p.Schema)
	p.Fixture = v.text(obj, "fixture", "$")
	p.Mode = v.text(obj, "mode", "$")
	p.Role = v.text(obj, "role", "$")
	p.Primary = v.text(obj, "primary_phase", "$")
	if p.Schema != 1 || (p.Mode != "calibration" && p.Mode != "candidate") ||
		(p.Role != "primary" && p.Role != "secondary" && p.Role != "negative-control") ||
		(p.Primary != cold && p.Primary != noChange) {
		v.invalid("$", "require schema=1, mode=calibration|candidate, role=primary|secondary|negative-control, primary_phase=cold|no_change")
	}
	paths := func(raw json.RawMessage, path string) resourcePaths {
		o := resourcePlanObject(&v, raw, path, []string{"manifest", "resource_journal", "run_metadata"}, nil)
		return resourcePaths{v.text(o, "manifest", path), v.text(o, "resource_journal", path), v.text(o, "run_metadata", path)}
	}
	p.Initial = paths(obj["initial"], "$.initial")
	if raw, ok := obj["follow_up"]; ok {
		x := paths(raw, "$.follow_up")
		p.FollowUp = &x
	}
	if raw, ok := obj["confirmation"]; ok {
		x := paths(raw, "$.confirmation")
		p.Confirmation = &x
	}
	if raw, ok := obj["calibration"]; ok {
		o := resourcePlanObject(&v, raw, "$.calibration", []string{"initial"}, []string{"follow_up"})
		p.Calibration = &calibrationPaths{Initial: paths(o["initial"], "$.calibration.initial")}
		if follow, ok := o["follow_up"]; ok {
			x := paths(follow, "$.calibration.follow_up")
			p.Calibration.FollowUp = &x
		}
	}
	if p.Mode == "calibration" && (p.Calibration != nil || p.Confirmation != nil) || p.Role == "negative-control" && p.Confirmation != nil {
		v.invalid("$", "calibration has no nested calibration/confirmation; negative controls have no confirmation obligation")
	}
	return p, v.issues
}

func readResourceBatch(paths resourcePaths, base string, cfg config) (resourceBatch, error) {
	resolve := func(path string) string {
		if filepath.IsAbs(path) {
			return filepath.Clean(path)
		}
		return filepath.Join(base, path)
	}
	m, j, r := resolve(paths.Manifest), resolve(paths.Journal), resolve(paths.Run)
	manifest, err := os.ReadFile(m)
	if err != nil {
		return resourceBatch{}, fmt.Errorf("read manifest %s: %w", m, err)
	}
	journal, err := os.ReadFile(j)
	if err != nil {
		return resourceBatch{}, fmt.Errorf("read journal %s: %w", j, err)
	}
	run, err := os.ReadFile(r)
	if err != nil {
		return resourceBatch{}, fmt.Errorf("read run metadata %s: %w", r, err)
	}
	b := admitResourceBatch(manifest, journal, run, cfg)
	validateResourcePaths(&b, m, j, r)
	return b, nil
}

func resourceOutcome(statuses ...string) string {
	for _, precedence := range []string{invalidEvidence, correctnessFailure, incompleteResource, criterionNotMet, unverified, inconclusive} {
		for _, s := range statuses {
			if s == precedence {
				return s
			}
		}
	}
	return criterionMet
}

func noiseDecision(ratio estimate, n float64) decision {
	cutoff := math.Exp(-n)
	d := decision{Rule: fmt.Sprintf("primary paired-ratio CI upper < exp(-N)=%.17g; N=%.17g", cutoff, n)}
	switch {
	case ratio.CI95.Upper < cutoff:
		d.Status, d.Message = criterionMet, "benefit interval exceeds admitted A/A envelope"
	case ratio.CI95.Lower >= cutoff:
		d.Status, d.Message = criterionNotMet, "benefit interval does not exceed admitted A/A envelope"
	default:
		d.Status, d.Message = inconclusive, "benefit interval crosses admitted A/A envelope"
	}
	return d
}

func summarizeResources(batches []resourceBatch, p resourcePlan, symmetric bool, noise *float64) resourceSummary {
	s := resourceSummary{Outcome: criterionMet, Elapsed: map[string]phaseStatistics{}, Metrics: map[string]map[string]resourceMetricResult{}, Gates: map[string]decision{}}
	for _, b := range batches {
		s.Membership = append(s.Membership, b.RawManifest.SHA256)
		for _, problem := range b.Issues {
			s.Outcome = resourceOutcome(s.Outcome, problem.Kind)
		}
		if b.ResourceStatus != "complete" {
			s.Outcome = resourceOutcome(s.Outcome, incompleteResource)
		}
	}
	if s.Outcome == invalidEvidence {
		return s
	}
	for _, phase := range []string{cold, noChange} {
		var elapsed [][]pair
		for _, b := range batches {
			elapsed = append(elapsed, b.Phases[phase])
		}
		s.Elapsed[phase] = phaseElapsedStatistics(elapsed)
		if phase == p.Primary {
			s.Noise = phaseNoise(elapsed)
		}
		s.Metrics[phase] = map[string]resourceMetricResult{}
		for _, metric := range []string{"elapsed_ms", "job_cpu_ms", "job_peak_bytes", "read_operations", "write_operations", "other_operations", "read_bytes", "write_bytes", "other_bytes"} {
			if metric == "elapsed_ms" && !symmetric && (phase != cold || p.Role != "negative-control") {
				continue
			}
			var values [][]pair
			complete := true
			for i, b := range batches {
				v := b.Resources[metric][phase]
				if metric == "elapsed_ms" {
					v = elapsed[i]
				}
				if len(v) != len(elapsed[i]) {
					complete = false
				}
				values = append(values, v)
			}
			if !complete {
				s.Gates[phase+"/"+metric] = decision{incompleteResource, "required metric on every scored record", "whole metric population unmeasured; observations and component causes retained"}
				continue
			}
			r := resourcePhaseStatistics(values, metric, symmetric, resourceCounterBatches(batches, metric, phase))
			r.Status = criterionMet
			for name, row := range r.Rows {
				if metric == "elapsed_ms" && phase == noChange && name == "p95" {
					reconfigureResourceRow(&row, guardrailLimit, nil)
					r.Rows[name] = row
				}
				r.Status = resourceOutcome(r.Status, row.Status)
				s.Gates[phase+"/"+metric+"/"+name+"/forward"] = row.Forward.Gate
				if row.Reverse != nil {
					s.Gates[phase+"/"+metric+"/"+name+"/reverse"] = row.Reverse.Gate
				}
			}
			s.Metrics[phase][metric] = r
		}
	}
	if !symmetric {
		s.Gates["no_change/p95"] = guardrailDecision(s.Elapsed[noChange].Estimates.P95Ratio)
		if p.Role != "negative-control" {
			ratio := s.Elapsed[p.Primary].Estimates.PairedRatio
			s.Gates["primary_benefit"] = primaryDecision(ratio, .95)
			if noise == nil {
				s.Gates["calibrated_noise"] = decision{unverified, "admitted fixture-specific A/A calibration", "no admitted calibration supplied"}
			} else {
				s.Gates["calibrated_noise"] = noiseDecision(ratio, *noise)
			}
		}
	} else {
		d := primaryDecision(s.Elapsed[p.Primary].Estimates.PairedRatio, .95)
		s.DescriptivePrimary = &d
	}
	for _, d := range s.Gates {
		s.Outcome = resourceOutcome(s.Outcome, d.Status)
	}
	return s
}

// Numeric comparisons cannot prove physical provenance. These checks prevent
// internally inconsistent batch labels from being combined in this mode.
func compatibleResourceBatches(a, b resourceBatch, aa bool) []issue {
	var x, y map[string]json.RawMessage
	if json.Unmarshal(a.RawManifest.Bytes, &x) != nil || json.Unmarshal(b.RawManifest.Bytes, &y) != nil {
		return nil // Admission already reports syntax errors.
	}
	v := validation{}
	compare := func(key string, left, right json.RawMessage) {
		if len(left) == 0 && len(right) == 0 {
			return
		}
		if !resourceJSONEqual(left, right) {
			v.invalid("batch_identity."+key, "declared fixture/configuration differs across batches")
		}
	}
	child := func(raw json.RawMessage, key string) json.RawMessage {
		var o object
		_ = json.Unmarshal(raw, &o)
		return o[key]
	}
	for _, key := range []string{"tree_digest_format", "measurement_boundary"} {
		compare(key, x[key], y[key])
	}
	compare("baseline.revision", child(x["baseline"], "revision"), child(y["baseline"], "revision"))
	// Storage descriptions include changing free-byte observations. Preserve
	// those raw observations, while comparing the declared stable host settings.
	for _, key := range []string{"os", "version", "cpu", "ram_bytes", "power", "run_order", "sidecar_timeout", "environment"} {
		compare("machine."+key, child(x["machine"], key), child(y["machine"], key))
	}
	stableStorage := func(raw json.RawMessage) json.RawMessage {
		var s string
		_ = json.Unmarshal(raw, &s)
		if i := strings.LastIndex(s, " free="); i >= 0 {
			if _, err := strconv.ParseUint(s[i+6:], 10, 64); err == nil {
				s = s[:i]
			}
		}
		value, _ := json.Marshal(s)
		return value
	}
	compare("machine.storage_volume_and_size", stableStorage(child(x["machine"], "storage")), stableStorage(child(y["machine"], "storage")))
	for _, key := range []string{"project_mode", "capture_limit_bytes_per_stream", "diagnostics_digest_format", "measurement_boundary", "tools", "scope"} {
		compare("run."+key, a.run[key], b.run[key])
	}
	for _, key := range []string{"sha256", "provided_identity", "provenance"} {
		compare("fixture."+key, child(child(a.run["before"], "fixture"), key), child(child(b.run["before"], "fixture"), key))
	}
	compare("input_metadata_sha256", child(a.run["project_state"], "input_metadata_sha256"), child(b.run["project_state"], "input_metadata_sha256"))
	for _, key := range []string{"format", "boundary", "cpu_units", "memory", "io"} {
		compare("resource_accounting."+key, child(a.run["resource_accounting"], key), child(b.run["resource_accounting"], key))
	}
	arms := []string{"baseline"}
	if !aa {
		arms = append(arms, "candidate")
	}
	for _, arm := range arms {
		for _, key := range []string{"sha256", "provided_source_sha", "observed_build", "build_setting_hashes", "build_info_sha256"} {
			compare(arm+"."+key, child(child(a.run["before"], arm), key), child(child(b.run["before"], arm), key))
		}
	}
	if !aa {
		compare("candidate.revision", child(x["candidate"], "revision"), child(y["candidate"], "revision"))
	}
	return v.issues
}

func identicalCalibrationArms(b resourceBatch) []issue {
	var root map[string]json.RawMessage
	_ = json.Unmarshal(b.RawManifest.Bytes, &root)
	var baseline, candidate map[string]any
	_ = json.Unmarshal(root["baseline"], &baseline)
	_ = json.Unmarshal(root["candidate"], &candidate)
	if baseline != nil && candidate != nil && baseline["revision"] != candidate["revision"] {
		return []issue{{invalidEvidence, "calibration.binary", "A/A requires identical baseline and candidate binary identities"}}
	}
	return nil
}

func chronologicalResourceBatches(a, b resourceBatch) []issue {
	var finish, start string
	_ = json.Unmarshal(a.run["finished_utc"], &finish)
	_ = json.Unmarshal(b.run["started_utc"], &start)
	f, fe := time.Parse(time.RFC3339Nano, finish)
	s, se := time.Parse(time.RFC3339Nano, start)
	if fe == nil && se == nil && s.Before(f) {
		return []issue{{invalidEvidence, "batch_chronology", "later batch started before preceding batch finished"}}
	}
	return nil // Individual timestamp shape is checked during admission.
}

func campaignAction(initial string, follow *string, confirmation *string, needsConfirmation bool) (string, string) {
	if initial != criterionMet && initial != inconclusive {
		return initial, "defer; initial fail/unverified/incomplete evidence cannot receive statistical rescue"
	}
	if initial == inconclusive {
		if follow == nil {
			return inconclusive, "one whole doubled follow-up permitted; retain initial result"
		}
		if *follow != criterionMet {
			return *follow, "defer; standalone doubled AND batch-stratified joint must pass; no further follow-up"
		}
	} else if follow != nil {
		return invalidEvidence, "follow-up forbidden after a passing initial result"
	}
	if needsConfirmation {
		if confirmation == nil {
			return unverified, "initial comparison admitted numerically; separate 30/100 confirmation required"
		}
		if *confirmation != criterionMet {
			return *confirmation, "defer; confirmation must independently pass and has no rescue"
		}
	}
	return criterionMet, "all supplied required numeric stages pass; external promotion gates remain unverified"
}

func retainedCalibrationNoise(summaries map[string]resourceSummary) float64 {
	n := 0.0
	for name, s := range summaries {
		if name != "confirmation" {
			n = math.Max(n, s.Noise.N)
		}
	}
	return n
}

func evaluateResourceCampaign(paths calibrationPaths, confirmation *resourcePaths, p resourcePlan, base string, aa bool, noise *float64) (resourceCampaign, error) {
	c := resourceCampaign{Batches: map[string]resourceBatch{}, Summaries: map[string]resourceSummary{}, NoiseEnvelope: 1}
	read := func(name string, batchPaths resourcePaths, multiplier int) error {
		b, err := readResourceBatch(batchPaths, base, config{p.Primary, .95, 30 * multiplier, 100 * multiplier})
		if err != nil {
			return err
		}
		if aa {
			b.Issues = append(b.Issues, identicalCalibrationArms(b)...)
		}
		if name != "initial" {
			b.Issues = append(b.Issues, compatibleResourceBatches(c.Batches["initial"], b, false)...)
			previous := c.Batches["initial"]
			if name == "confirmation" && paths.FollowUp != nil {
				previous = c.Batches["follow_up"]
			}
			b.Issues = append(b.Issues, chronologicalResourceBatches(previous, b)...)
		}
		c.Batches[name] = b
		c.Summaries[name] = summarizeResources([]resourceBatch{b}, p, aa, noise)
		return nil
	}
	if err := read("initial", paths.Initial, 1); err != nil {
		return c, err
	}
	var follow, confirmed *string
	if paths.FollowUp != nil {
		if err := read("follow_up", *paths.FollowUp, 2); err != nil {
			return c, err
		}
		joint := summarizeResources([]resourceBatch{c.Batches["initial"], c.Batches["follow_up"]}, p, aa, noise)
		c.Summaries["joint"] = joint
		x := resourceOutcome(c.Summaries["follow_up"].Outcome, joint.Outcome)
		follow = &x
	}
	if confirmation != nil {
		if err := read("confirmation", *confirmation, 1); err != nil {
			return c, err
		}
		x := c.Summaries["confirmation"].Outcome
		confirmed = &x
	}
	c.Outcome, c.Action = campaignAction(c.Summaries["initial"].Outcome, follow, confirmed, !aa && p.Role != "negative-control")
	// No-rescue routing does not hide malformed supplied evidence in a stage
	// that could not otherwise have changed the initial disposition.
	for _, batch := range c.Batches {
		for _, problem := range batch.Issues {
			if problem.Kind == invalidEvidence {
				c.Outcome = invalidEvidence
			}
		}
	}
	if aa {
		c.NoiseN = retainedCalibrationNoise(c.Summaries)
	} else if noise != nil {
		c.NoiseN = *noise
	}
	c.NoiseEnvelope = math.Exp(c.NoiseN)
	return c, nil
}

func evaluateResourcePlan(data []byte, base string) (resourceReport, error) {
	p, issues := parseResourcePlan(data)
	r := resourceReport{Schema: 1, PolicySHA256: resourcePolicySHA256, Outcome: invalidEvidence, Input: resourceRaw(data), Plan: p, Issues: issues,
		Scope: "recorded Windows owned-job resource/control statistics, internal harness correspondence and supplied campaign decision table only",
		ExternalGates: []externalGate{
			{"runtime_source_fixture_and_host", unverified, "labels/hashes do not establish source correctness, full fixture/dependency identity, quiet host or honest collection"},
			{"semantic_write_mtime_contracts", unverified, "limited recorded parity does not prove literal full output, actual writes, mtimes or all diagnostic contracts"},
			{"confirmation_independence_and_preregistration", unverified, "supplied distinct batch hashes/counts do not prove independent collection, chronological order or pre-score declarations"},
			{"final_sha_independent_review", unverified, "independent code review must close R3-P01 against frozen policy and verify exact implementation SHA before scoring"},
		},
		Limitations: []string{
			"CPU uses owned-job user+kernel 100ns ticks/10000 ms; exact integers are retained. Float64 statistics can lose precision at extreme counters.",
			"Job peak is simultaneous committed memory, not RSS or summed peaks. OS I/O is aggregate job accounting, not filesystem writes, application output or durable flush.",
			"Root/other-owned CPU, individual peak, faults/process counts and query duration are descriptive only; unsupported attribution/RSS/physical I/O remain unmeasured.",
			"Whole-block percentile bootstrap is descriptive finite-sample uncertainty, not exact or simultaneous family-wide coverage. Noise envelopes do not prove absence of drift.",
			"Calibration N always retains maximum absolute point/endpoints, including biased zero-width intervals and initial/follow-up/joint summaries. No counter/record/draw is dropped.",
			"Candidate/confirmation batches supplied after a forbidden stage are retained and summarized but cannot rescue it. Invalid collection needs external diagnosis/remedy; this tool authorizes no retry.",
		},
	}
	if len(issues) != 0 {
		return r, nil
	}
	var noise *float64
	if p.Calibration != nil {
		c, err := evaluateResourceCampaign(*p.Calibration, nil, p, base, true, nil)
		if err != nil {
			return r, err
		}
		r.Calibration = &c
		if c.Outcome == criterionMet {
			noise = &c.NoiseN
		}
	}
	c, err := evaluateResourceCampaign(calibrationPaths{p.Initial, p.FollowUp}, p.Confirmation, p, base, p.Mode == "calibration", noise)
	if err != nil {
		return r, err
	}
	r.Comparison, r.Outcome = &c, c.Outcome
	if p.Mode == "candidate" {
		if r.Calibration == nil {
			r.Outcome = resourceOutcome(r.Outcome, unverified)
		} else {
			r.Issues = append(r.Issues, compatibleResourceBatches(r.Calibration.Batches["initial"], c.Batches["initial"], true)...)
			previous := r.Calibration.Batches["initial"]
			if p.Calibration.FollowUp != nil {
				previous = r.Calibration.Batches["follow_up"]
			}
			r.Issues = append(r.Issues, chronologicalResourceBatches(previous, c.Batches["initial"])...)
			r.Outcome = resourceOutcome(r.Outcome, r.Calibration.Outcome)
		}
	}
	// Identical observations can legitimately yield identical manifests. Reusing
	// the entire run/journal identity cannot establish a separate batch.
	seen := map[string]bool{}
	campaigns := []*resourceCampaign{r.Calibration, r.Comparison}
	for _, campaign := range campaigns {
		if campaign == nil {
			continue
		}
		for name, batch := range campaign.Batches {
			identity := batch.RawJournal.SHA256
			if seen[identity] {
				r.Issues = append(r.Issues, issue{invalidEvidence, name, "same journal evidence reused for distinct campaign batches"})
			}
			seen[identity] = true
		}
	}
	for _, problem := range r.Issues {
		r.Outcome = resourceOutcome(r.Outcome, problem.Kind)
	}
	return r, nil
}

func runResourcePlan(path, output string, stdout, stderr io.Writer) int {
	data, err := os.ReadFile(path)
	if err != nil {
		fmt.Fprintf(stderr, "read resource plan: %v\n", err)
		return 2
	}
	if _, err := os.Lstat(output); err == nil || !os.IsNotExist(err) {
		fmt.Fprintf(stderr, "output must be a new path: %s\n", output)
		return 2
	}
	r, evaluationErr := evaluateResourcePlan(data, filepath.Dir(path))
	if evaluationErr != nil {
		fmt.Fprintf(stderr, "resource input I/O: %v\n", evaluationErr)
		return 2
	}
	// Read every input before creating output, including a missing referenced
	// input that might otherwise alias a new output path. O_EXCL also handles
	// an output created concurrently during analysis.
	f, err := os.OpenFile(output, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o644)
	if err != nil {
		fmt.Fprintf(stderr, "create output: %v\n", err)
		return 2
	}
	e := json.NewEncoder(f)
	e.SetIndent("", "  ")
	encodeErr := e.Encode(r)
	closeErr := f.Close()
	if encodeErr != nil || closeErr != nil {
		fmt.Fprintf(stderr, "write resource output: encode=%v close=%v (partial output retained)\n", encodeErr, closeErr)
		return 2
	}
	fmt.Fprintf(stdout, "%s (resource/control subset only; promotion_ready=false)\n", r.Outcome)
	if r.Comparison != nil {
		fmt.Fprintln(stdout, r.Comparison.Action)
	}
	fmt.Fprintln(stdout, "Source/correctness/host, independent collection/preregistration and final-SHA review remain external/unverified; actual legacy verdicts retained separately in each batch.")
	if r.Outcome == criterionMet {
		return 0
	}
	return 1
}
