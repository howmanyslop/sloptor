package main

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"slices"
	"sort"
	"strings"
	"time"
)

type residentReaderReport struct {
	Schema          int                      `json:"schema"`
	Mode            string                   `json:"mode"`
	EvidenceKind    string                   `json:"evidence_kind"`
	PolicySHA256    string                   `json:"policy_sha256"`
	Plan            residentReaderPlan       `json:"plan"`
	Admission       json.RawMessage          `json:"admission_declaration,omitempty"`
	Prerequisites   string                   `json:"prerequisite_status"`
	Issues          []issue                  `json:"issues"`
	Batches         []residentRawBatch       `json:"raw_batches"`
	Snapshots       []*residentSnapshot      `json:"snapshots"`
	CalibrationOnly []ResidentTrackResult    `json:"calibration_only,omitempty"`
	Campaigns       []ResidentCampaignResult `json:"campaigns,omitempty"`
	Question        *ResidentQuestionResult  `json:"question,omitempty"`
	Outcome         string                   `json:"outcome"`
	ExternalGates   []externalGate           `json:"external_gates"`
	PromotionReady  bool                     `json:"promotion_ready"`
	campaigns       []ResidentCampaign
}

func (r *residentReader) reconstruct(planPath string) residentReaderReport {
	snap := r.snapshot(planPath, false)
	plan := r.parsePlan(snap.data)
	result := residentReaderReport{Schema: 1, Mode: "resident-raw-schema2", EvidenceKind: r.kind, PolicySHA256: residentPolicySHA, Plan: plan, Prerequisites: residentUnverified, ExternalGates: residentExternalGates()}
	if len(r.issues) > 0 {
		result.Issues = residentOrderedIssues(r.issues)
		return result
	}
	batchIDs := map[string]bool{}
	usedPlans := map[string]bool{}
	for _, input := range plan.Campaigns {
		role, ok := input.Key.policyRole()
		if !ok || role != input.Key.Role {
			r.invalid("$.campaign.key", "not a frozen resident comparator/role")
		}
		c := ResidentCampaign{Label: residentRecorded, Key: input.Key, Prerequisites: residentUnverified}
		if r.kind == "invented-protocol" {
			c.Label = residentSynthetic
		}
		if len(input.Calibration) == 0 || len(input.Calibration) > 2 || len(input.Comparison) > 3 {
			r.invalid("$.campaign", "bounded initial calibration and initial/follow-up/confirmation required")
		}
		var prior time.Time
		for track, refs := range [][]residentBatchRef{input.Calibration, input.Comparison} {
			purpose := "calibration"
			if track == 1 {
				purpose = "comparison"
			}
			for _, ref := range refs {
				if usedPlans[residentPathKey(ref.Plan.Path)] {
					r.invalid(ref.Plan.Path, "batch plan reused across stages/leafs")
				}
				usedPlans[residentPathKey(ref.Plan.Path)] = true
				b := r.batch(ref, input.Key, purpose)
				if batchIDs[b.Reconstructed.ID] {
					r.invalid(ref.Directory, "batch identity reused")
				}
				batchIDs[b.Reconstructed.ID] = true
				if !prior.IsZero() && b.Start.Before(prior) {
					r.invalid(ref.Directory, "actual journal intervals overlap or precede prior batch")
				}
				prior = b.Finish
				if track == 0 {
					c.Calibration = append(c.Calibration, b.Reconstructed)
				} else {
					c.Comparison = append(c.Comparison, b.Reconstructed)
				}
				result.Batches = append(result.Batches, b)
			}
		}
		// Calibration reuse is nuisance evidence only on this exact population and
		// unchanged reference, fixture, physical path, runtime/host/input conditions.
		if len(result.Batches) > 0 {
			var first *residentRawBatch
			for i := range result.Batches {
				b := &result.Batches[i]
				if b.Reconstructed.Key != input.Key {
					continue
				}
				if first == nil {
					first = b
					continue
				}
				if b.HostSHA256 != first.HostSHA256 || b.FixtureSHA256 != first.FixtureSHA256 {
					r.invalid(b.Ref.Directory, "host/environment or fixture differs across calibration/comparison")
				}
				for _, k := range []string{"fixtureParent", "fixtureOwner", "fixtureInput", "workingDirectory", "cycles", "timeoutMs", "captureBytes", "reportBytes", "semanticLabel", "captureSha256", "launcherSha256", "protectedRoots"} {
					r.equal(first.plan[k], b.plan[k], b.Ref.Directory+".conditions."+k)
				}
				for _, role := range []string{"policy", "semantic-admission", "collector-interface", "dependencies"} {
					r.equal(r.inputDeclaration(first.plan, role), r.inputDeclaration(b.plan, role), b.Ref.Directory+".conditions."+role)
				}
				aa := r.obj(first.plan["arms"], "reference.arms", "A B", "")
				bb := r.obj(b.plan["arms"], "comparison.arms", "A B", "")
				r.equal(aa["A"], bb["A"], b.Ref.Directory+".reference_binding")
			}
		}
		result.campaigns = append(result.campaigns, c)
	}
	r.chronology(result.Batches)
	result.Prerequisites = r.declaration(plan, &result)
	result.Issues = residentOrderedIssues(r.issues)
	return result
}

// Validate a separate schedule view. The report, raw journals and statistical
// populations keep their supplied order, including legal leaf interleavings.
func (r *residentReader) chronology(batches []residentRawBatch) {
	ordered := slices.Clone(batches)
	sort.SliceStable(ordered, func(i, j int) bool { return ordered[i].Ref.Chronology < ordered[j].Ref.Chronology })
	previous := map[string]residentClockObservation{}
	for i, b := range ordered {
		if b.Ref.Chronology <= 0 || i > 0 && b.Ref.Chronology == ordered[i-1].Ref.Chronology {
			r.invalid(b.Ref.Directory, "unique positive global batch chronology required")
		}
		if b.Start.IsZero() || b.Finish.IsZero() || b.Finish.Before(b.Start) || i > 0 && b.Start.Before(ordered[i-1].Finish) {
			r.invalid(b.Ref.Directory, "global recorded batch intervals overlap or precede declared chronology")
		}
		// A matching, independently checked host snapshot/support binding defines
		// a declared same-host QPC domain, not physical host attestation. Do not
		// compare arbitrary unrelated hosts or malformed clock observations.
		if !b.hostValid {
			continue
		}
		for _, clock := range b.Clocks {
			if !clock.serialValid {
				continue
			}
			domain := fmt.Sprintf("%s/%d", b.HostSHA256, clock.Frequency)
			if prior, ok := previous[domain]; ok {
				r.serialClock(prior, clock, b.Ref.Directory)
			}
			previous[domain] = clock
		}
	}
}

var residentPrerequisiteNames = []string{"clock-suitability", "effect-precision", "quiet-host", "source-build-fixture-dependencies", "semantic-write-mtime-persistence", "independent-sessions", "frozen-protocol-chronology"}

func (r *residentReader) declaration(plan residentReaderPlan, report *residentReaderReport) string {
	status := residentUnverified
	if plan.Admission == nil {
		return status
	}
	s := r.bound(*plan.Admission, false)
	// Malformed declaration bytes remain in the base64 snapshot. Never embed
	// invalid JSON as RawMessage and thereby lose the evidence report at encode.
	if resourceDocument(&r.validation, s.data, "$.admission.raw") {
		report.Admission = append(json.RawMessage(nil), s.data...)
	}
	d := r.obj(s.data, "$.admission", "schema kind policy_sha256 declared_utc prerequisites batches", "")
	r.number(d, "schema", "admission", 1)
	kind := "root-resident-admission-v1"
	if r.kind == "invented-protocol" {
		kind = "invented-root-assertions-v1"
	}
	r.want(d, "kind", "admission", kind)
	r.want(d, "policy_sha256", "admission", residentPolicySHA)
	declared := r.stamp(d, "declared_utc", "admission")
	assertions := r.obj(d["prerequisites"], "admission.prerequisites", strings.Join(residentPrerequisiteNames, " "), "")
	admitted := true
	for _, name := range residentPrerequisiteNames {
		v := r.str(assertions, name, "admission.prerequisites")
		if !slices.Contains([]string{"admitted", "unverified", "failed"}, v) {
			r.invalid("admission.prerequisites", "unknown admission declaration status")
		}
		if v != "admitted" {
			admitted = false
		}
	}
	rows := r.arr(d["batches"], "admission.batches", 10)
	if len(rows) != len(report.Batches) {
		r.invalid("admission.batches", "declaration must bind every supplied batch before collection")
	}
	seen := map[string]bool{}
	for i, raw := range rows {
		p := fmtResidentIndex("admission.batches", i)
		o := r.obj(raw, p, "plan_path plan_sha256 batch chronology host_sha256 fixture_sha256 authorized_utc", "")
		path := r.str(o, "plan_path", p)
		key := residentPathKey(path)
		if seen[key] {
			r.invalid(p, "duplicate admission batch")
		}
		seen[key] = true
		found := false
		for _, b := range report.Batches {
			if !resourcePathEqual(path, b.Ref.Plan.Path) {
				continue
			}
			found = true
			r.want(o, "plan_sha256", p, b.Ref.Plan.SHA256)
			r.want(o, "batch", p, b.Reconstructed.ID)
			r.number(o, "chronology", p, uint64(b.Ref.Chronology))
			r.want(o, "host_sha256", p, b.HostSHA256)
			r.want(o, "fixture_sha256", p, b.FixtureSHA256)
			authorized := r.stamp(o, "authorized_utc", p)
			if declared.After(b.Start) || authorized.After(b.Start) || authorized.Before(declared) {
				r.invalid(p, "root assertions/authorization were not frozen before recorded collection")
			}
			if b.OneTick {
				admitted = false
			}
		}
		if !found {
			r.invalid(p, "declaration references an unsupplied batch")
		}
	}
	if admitted {
		status = residentDeclared
		if r.kind == "invented-protocol" {
			status = residentAssumed
		}
	}
	return status
}

func (r *residentReader) decide(result *residentReaderReport) {
	result.Outcome = residentIssueStatus(result.Issues)
	if result.Outcome != criterionMet {
		return
	}
	for _, b := range result.Batches {
		if b.OneTick {
			result.Prerequisites = residentUnverified
		}
	}
	for i := range result.campaigns {
		result.campaigns[i].Prerequisites = result.Prerequisites
	}
	for _, c := range result.campaigns {
		if len(c.Comparison) == 0 {
			track := EvaluateResidentCalibration(c.Calibration, c.Prerequisites)
			result.CalibrationOnly = append(result.CalibrationOnly, track)
			result.Outcome = residentReaderOutcome(result.Outcome, track.Status)
		} else {
			evaluated := EvaluateResident(c)
			result.Campaigns = append(result.Campaigns, evaluated)
			result.Outcome = residentReaderOutcome(result.Outcome, evaluated.Status)
		}
	}
	if len(result.campaigns) == 2 {
		q := EvaluateResidentQuestion(result.campaigns[0], result.campaigns[1])
		result.Question = &q
		result.Outcome = residentReaderOutcome(result.Outcome, q.Status)
	}
}

func residentReaderOutcome(statuses ...string) string {
	priorities := []string{invalidEvidence, correctnessFailure, residentIncomplete, residentUnverified, criterionNotMet, inconclusive, residentCost, criterionMet}
	if len(statuses) == 0 {
		return invalidEvidence
	}
	for _, status := range statuses {
		if !slices.Contains(priorities, status) {
			return invalidEvidence
		}
	}
	for _, priority := range priorities {
		if slices.Contains(statuses, priority) {
			return priority
		}
	}
	return invalidEvidence
}

func (r *residentReader) retained(result *residentReaderReport) {
	for _, s := range r.files {
		result.Snapshots = append(result.Snapshots, s)
	}
	sort.Slice(result.Snapshots, func(i, j int) bool { return result.Snapshots[i].Path < result.Snapshots[j].Path })
}

func runResidentPlan(path, output string, stdout, stderr io.Writer) int {
	absolute, err := filepath.Abs(path)
	if err != nil {
		fmt.Fprintf(stderr, "resident input path: %v\n", err)
		return 2
	}
	reader := newResidentReader()
	result := reader.reconstruct(absolute)
	outPath, err := filepath.Abs(output)
	if err != nil {
		fmt.Fprintf(stderr, "resident output path: %v\n", err)
		return 2
	}
	// Input aliases are refused even when a raw referenced input is missing.
	if _, exists := reader.files[residentPathKey(outPath)]; exists {
		fmt.Fprintln(stderr, "resident output aliases an input snapshot path")
		return 2
	}
	if !residentLocalPath(outPath) {
		fmt.Fprintln(stderr, "resident output requires a literal local path")
		return 2
	}
	for _, b := range result.Batches {
		if residentWithin(b.Ref.Directory, outPath) {
			fmt.Fprintln(stderr, "resident output is inside retained batch evidence")
			return 2
		}
	}
	f, err := os.OpenFile(output, os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0644)
	if err != nil {
		fmt.Fprintf(stderr, "create resident output: %v\n", err)
		return 2
	}
	reader.decide(&result)
	reader.retained(&result)
	encoder := json.NewEncoder(f)
	encoder.SetIndent("", "  ")
	encodeErr := encoder.Encode(result)
	closeErr := f.Close()
	if encodeErr != nil || closeErr != nil {
		fmt.Fprintf(stderr, "write resident output: encode=%v close=%v (partial retained)\n", encodeErr, closeErr)
		return 2
	}
	fmt.Fprintf(stdout, "%s (resident raw schema2 consistency; %s; root declarations are assertions; promotion_ready=false)\n", result.Outcome, result.EvidenceKind)
	for _, problem := range result.Issues {
		fmt.Fprintf(stdout, "%s %s: %s\n", problem.Kind, problem.Path, problem.Message)
	}
	fmt.Fprintln(stdout, "External gates independently unchecked: physical provenance, host/clock precision, semantics/writes, actual calibration/independence, complete matrix/confirmations, supplemental means, root adjudications and final review.")
	if result.Outcome == criterionMet {
		return 0
	}
	return 1
}
