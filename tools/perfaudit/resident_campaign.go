package main

import (
	"math"
)

type ResidentCampaign struct {
	Label         string          `json:"label"`
	Key           ResidentKey     `json:"key"`
	Prerequisites string          `json:"prerequisites"` // synthetic-assumed or root-declared; otherwise unverified/failed
	Calibration   []ResidentBatch `json:"calibration"`
	Comparison    []ResidentBatch `json:"comparison"`
}

type ResidentStageDecision struct {
	Controls  string `json:"controls"`
	Practical string `json:"practical_benefit,omitempty"`
	Noise     string `json:"noise_benefit,omitempty"`
	Status    string `json:"status"`
}

type ResidentTrackResult struct {
	Batches         []ResidentBatch                  `json:"typed_batches"`
	Issues          []issue                          `json:"issues"`
	Summaries       map[string]ResidentSummary       `json:"summaries"`
	Decisions       map[string]ResidentStageDecision `json:"decisions"`
	Status          string                           `json:"status"`
	FollowUpAllowed bool                             `json:"follow_up_allowed"`
	Action          string                           `json:"action"`
}

type ResidentCampaignResult struct {
	Input          ResidentCampaign    `json:"typed_input"`
	PolicySHA256   string              `json:"policy_sha256"`
	Issues         []issue             `json:"issues"`
	Calibration    ResidentTrackResult `json:"calibration"`
	Comparison     ResidentTrackResult `json:"comparison"`
	Noise          float64             `json:"retained_calibration_noise"`
	Status         string              `json:"status"`
	ExternalGates  []externalGate      `json:"external_gates"`
	PromotionReady bool                `json:"promotion_ready"`
}

func residentPractical(e estimate) string {
	switch {
	case e.CI95.Lower >= 1:
		return criterionNotMet
	case e.CI95.Upper >= 1:
		return inconclusive
	case e.Value > .95:
		return criterionNotMet
	default:
		return criterionMet
	}
}

func residentNoiseGate(e estimate, n float64) string {
	threshold := math.Exp(-n)
	switch {
	case e.CI95.Upper < threshold:
		return criterionMet
	case e.CI95.Lower >= threshold:
		return criterionNotMet
	default:
		return inconclusive
	}
}

func residentStage(s ResidentSummary, benefit bool, noise float64) ResidentStageDecision {
	r := ResidentStageDecision{Controls: s.Status, Status: s.Status}
	if benefit && (s.Status == criterionMet || s.Status == criterionNotMet || s.Status == inconclusive) {
		e := s.Populations["full"].Benefit.Ratio
		r.Practical, r.Noise = residentPractical(e), residentNoiseGate(e, noise)
		r.Status = resourceStatuses(s.Status, r.Practical, r.Noise)
	}
	return r
}

// Stage routing is independent of display fields. Every supplied stage remains
// in the report. Invalid/incomplete later evidence is checked before routing.
func residentTransition(stages map[string]ResidentStageDecision, confirmation bool) (status, action string, followUp bool) {
	initial, ok := stages["initial"]
	if !ok {
		return residentIncomplete, "complete initial batch required", false
	}
	for _, s := range stages {
		if s.Status == invalidEvidence {
			return invalidEvidence, "retain and remedy invalid typed evidence", false
		}
	}
	for _, s := range stages {
		if s.Status == residentIncomplete {
			return residentIncomplete, "required evidence is incomplete", false
		}
	}
	for _, s := range stages {
		if s.Status == residentUnverified {
			return residentUnverified, "required prerequisite is unverified", false
		}
	}
	if c, supplied := stages["confirmation"]; supplied && c.Status == criterionNotMet {
		return criterionNotMet, "supplied independent confirmation failed; no rescue", false
	}
	if initial.Status == criterionNotMet {
		return criterionNotMet, "initial failure; no rescue", false
	}
	if initial.Status == residentCost {
		return residentCost, "retain required-cost report for root adjudication", false
	}
	if initial.Status == inconclusive {
		follow, supplied := stages["follow_up"]
		if !supplied {
			if _, hasConfirmation := stages["confirmation"]; hasConfirmation {
				return inconclusive, "unresolved initial; confirmation cannot replace follow-up", false
			}
			return inconclusive, "one whole doubled follow-up permitted; retain initial", true
		}
		joint, exists := stages["joint"]
		if !exists {
			return residentIncomplete, "stratified initial+follow-up summary required", false
		}
		resolved := resourceStatuses(follow.Status, joint.Status)
		if resolved != criterionMet {
			return resolved, "standalone AND joint must pass; no further follow-up", false
		}
	} else if _, supplied := stages["follow_up"]; supplied {
		return invalidEvidence, "follow-up supplied without a valid inconclusive initial", false
	}
	if c, supplied := stages["confirmation"]; supplied {
		if c.Status != criterionMet {
			return c.Status, "independent confirmation deferred; no pooling or rescue", false
		}
	} else if confirmation {
		return residentUnverified, "independent ten-pair confirmation required", false
	}
	return criterionMet, "required numerical stages pass on invented synthetic data", false
}

func residentTrack(batches []ResidentBatch, benefit bool, noise float64, confirmation bool) ResidentTrackResult {
	r := ResidentTrackResult{Batches: batches, Summaries: map[string]ResidentSummary{}, Decisions: map[string]ResidentStageDecision{}}
	for _, b := range batches {
		s := AnalyzeResident([]ResidentBatch{b})
		r.Summaries[b.Stage] = s
		r.Decisions[b.Stage] = residentStage(s, benefit, noise)
	}
	if len(batches) >= 2 && batches[0].Stage == "initial" && batches[1].Stage == "follow_up" {
		s := AnalyzeResident(batches[:2])
		r.Summaries["joint"] = s
		r.Decisions["joint"] = residentStage(s, benefit, noise)
	}
	r.Status, r.Action, r.FollowUpAllowed = residentTransition(r.Decisions, confirmation)
	return r
}

// Provenance wording is separate from numerical decisions and stage routing.
func residentAction(label, action string) string {
	if label == residentRecorded && action == "required numerical stages pass on invented synthetic data" {
		return "required numerical stages pass on admitted recorded data, conditional on root declarations"
	}
	return action
}

// EvaluateResidentCalibration supports the prospective A/A decision before any
// candidate is supplied. An inconclusive calibration has its own one-follow-up
// allowance, conditioned on label-appropriate prerequisite declarations.
func EvaluateResidentCalibration(batches []ResidentBatch, prerequisites string) ResidentTrackResult {
	r := ResidentTrackResult{Batches: batches}
	if len(batches) < 1 || len(batches) > 2 {
		r.Issues = append(r.Issues, issue{invalidEvidence, "calibration", "initial and at most one follow-up required"})
	} else {
		for i, b := range batches {
			r.Issues = append(r.Issues, residentTypedIssues(b)...)
			stage := "initial"
			if i == 1 {
				stage = "follow_up"
			}
			if b.Label != batches[0].Label || b.Purpose != "calibration" || b.Stage != stage || b.Key != batches[0].Key ||
				(i == 1 && (b.Chronology <= batches[0].Chronology || b.ID == batches[0].ID)) {
				r.Issues = append(r.Issues, issue{invalidEvidence, "calibration", "matching chronological calibration initial+follow-up required"})
			}
		}
		r.Issues = append(r.Issues, residentDuplicateSessions(batches)...)
	}
	label := ""
	if len(batches) > 0 {
		label = batches[0].Label
	}
	if !residentPrerequisiteAllowed(label, prerequisites) {
		r.Issues = append(r.Issues, issue{invalidEvidence, "prerequisites", "label-appropriate prerequisite status required"})
	}
	if status := residentIssueStatus(r.Issues); status != criterionMet {
		r.Status = status
		return r
	}
	r = residentTrack(batches, false, 0, false)
	r.Action = residentAction(label, r.Action)
	if !residentPrerequisiteMet(label, prerequisites) && r.Status != invalidEvidence && r.Status != residentIncomplete {
		r.Status, r.FollowUpAllowed = prerequisites, false
		r.Action = "unverified or failed prerequisite; no statistical retry"
	}
	return r
}

func residentCampaignIssues(c ResidentCampaign) []issue {
	var issues []issue
	add := func(kind, path, message string) { issues = append(issues, issue{kind, path, message}) }
	role, valid := c.Key.policyRole()
	if (c.Label != residentSynthetic && c.Label != residentRecorded) || !valid || c.Key.Role != role {
		add(invalidEvidence, "campaign", "explicit input label and frozen population role required")
	}
	if !residentPrerequisiteAllowed(c.Label, c.Prerequisites) {
		add(invalidEvidence, "prerequisites", "label-appropriate assumption/declaration or unverified/failed prerequisite required")
	}
	if len(c.Calibration) > 2 || len(c.Comparison) > 3 {
		add(invalidEvidence, "campaign", "bounded initial, optional follow-up and independent confirmation only")
		return issues
	}
	all := append(append([]ResidentBatch{}, c.Calibration...), c.Comparison...)
	ids := map[string]bool{}
	for track, batches := range [][]ResidentBatch{c.Calibration, c.Comparison} {
		purpose := "calibration"
		if track == 1 {
			purpose = "comparison"
		}
		previous := 0
		seenStages := map[string]bool{}
		for i, b := range batches {
			issues = append(issues, residentTypedIssues(b)...)
			if b.Label != c.Label || b.Key != c.Key || b.Purpose != purpose || b.Chronology <= previous || ids[b.ID] || seenStages[b.Stage] ||
				(i == 0 && b.Stage != "initial") || (i > 0 && b.Stage == "initial") || (i == 2 && b.Stage != "confirmation") {
				add(invalidEvidence, purpose, "inconsistent identity, stage, chronology, key or purpose")
			}
			ids[b.ID], seenStages[b.Stage] = true, true
			previous = b.Chronology
		}
		if len(batches) == 0 {
			add(residentIncomplete, purpose, "complete initial population required")
		}
	}
	if len(c.Calibration) > 0 && len(c.Comparison) > 0 && c.Calibration[len(c.Calibration)-1].Chronology >= c.Comparison[0].Chronology {
		add(invalidEvidence, "chronology", "calibration must precede candidate sessions")
	}
	issues = append(issues, residentDuplicateSessions(all)...)
	return issues
}

// EvaluateResident evaluates typed arithmetic and protocol transitions only.
// No field can certify honest collection, external readiness or promotion.
func EvaluateResident(c ResidentCampaign) ResidentCampaignResult {
	r := ResidentCampaignResult{Input: c, PolicySHA256: residentPolicySHA, Status: criterionMet, ExternalGates: residentExternalGates()}
	r.Issues = residentCampaignIssues(c)
	if status := residentIssueStatus(r.Issues); status != criterionMet {
		r.Status = status
		return r
	}
	r.Calibration = residentTrack(c.Calibration, false, 0, false)
	for _, s := range r.Calibration.Summaries {
		r.Noise = math.Max(r.Noise, s.Noise)
	}
	benefit := c.Key.Role == "primary" || c.Key.Role == "secondary"
	r.Comparison = residentTrack(c.Comparison, benefit, r.Noise, benefit)
	r.Calibration.Action = residentAction(c.Label, r.Calibration.Action)
	r.Comparison.Action = residentAction(c.Label, r.Comparison.Action)
	switch {
	case r.Calibration.Status == invalidEvidence || r.Comparison.Status == invalidEvidence:
		r.Status = invalidEvidence
		r.Calibration.FollowUpAllowed, r.Comparison.FollowUpAllowed = false, false
	case r.Calibration.Status == residentIncomplete || r.Comparison.Status == residentIncomplete:
		r.Status = residentIncomplete
		r.Calibration.FollowUpAllowed, r.Comparison.FollowUpAllowed = false, false
	case !residentPrerequisiteMet(c.Label, c.Prerequisites):
		r.Status = c.Prerequisites
		r.Calibration.FollowUpAllowed, r.Comparison.FollowUpAllowed = false, false
	case r.Calibration.Status == criterionNotMet || r.Comparison.Status == criterionNotMet:
		r.Status = criterionNotMet
		r.Calibration.FollowUpAllowed, r.Comparison.FollowUpAllowed = false, false
	case r.Calibration.Status != criterionMet:
		r.Status = r.Calibration.Status
		r.Comparison.FollowUpAllowed = false
		r.Comparison.Action = "calibration not admitted; candidate cannot authorize retry or acceptance"
	default:
		r.Status = r.Comparison.Status
	}
	return r
}

func residentPrerequisiteMet(label, status string) bool {
	return label == residentSynthetic && status == residentAssumed || label == residentRecorded && status == residentDeclared
}

func residentPrerequisiteAllowed(label, status string) bool {
	return residentPrerequisiteMet(label, status) || status == residentUnverified || status == correctnessFailure
}

func residentExternalGates() []externalGate {
	return []externalGate{
		{"raw-admission", "unverified", "later reader must bind final committed/reviewed launcher schema2 and all raw reports, counts, chronology, clock and enclosure evidence"},
		{"clock-suitability", "unverified", "root must resolve actual-operation suitability, one-tick readiness and effect precision; these are not statistical retries"},
		{"collection-and-identities", "unverified", "root must bind source/binary/fixture/dependency/environment hashes, quiet host, independent sessions and exact collection plan"},
		{"empirical-calibration", "unverified", "new identical-reference B:B or S:S cycle/session calibration must match the exact leaf, settings, path and unchanged host/input conditions; benchmark means and fresh-process samples cannot substitute"},
		{"semantics-and-writes", "unverified", "independent semantic, fresh-output, diagnostic, actual write/mtime and persistence admission required"},
		{"semantic-cost-adjudication", "unverified", "root must independently bind every predefined stale-B API population and explicitly adjudicate its disclosed required-work costs"},
		{"supplemental-means", "unverified", "all 81 separate benchmark populations require root signed metric/adverse/order/concentration dispositions; no numerical mean acceptance or follow-up"},
		{"matrix-and-confirmations", "unverified", "all required comparator/leaf controls and separate primary AND secondary confirmations must pass; no cross-leaf pooling"},
		{"independent-review", "unverified", "independent exact committed-source statistics review and later raw-reader review required"},
	}
}

type ResidentQuestionResult struct {
	Primary        ResidentCampaignResult `json:"primary"`
	Secondary      ResidentCampaignResult `json:"secondary"`
	Issues         []issue                `json:"issues"`
	Status         string                 `json:"numerical_question_status"`
	PromotionReady bool                   `json:"promotion_ready"`
}

// EvaluateResidentQuestion requires the predeclared primary AND secondary,
// independently confirmed. Other matrix controls and all external gates remain
// explicit obligations; this function cannot grant compiler acceptance.
func EvaluateResidentQuestion(primary, secondary ResidentCampaign) ResidentQuestionResult {
	r := ResidentQuestionResult{Primary: ResidentCampaignResult{Input: primary}, Secondary: ResidentCampaignResult{Input: secondary}}
	if primary.Key.Comparator != secondary.Key.Comparator || primary.Key.Role != "primary" || secondary.Key.Role != "secondary" {
		r.Issues = append(r.Issues, issue{invalidEvidence, "question", "matching comparator and predefined primary/secondary required"})
	}
	all := []ResidentBatch{}
	for _, c := range []ResidentCampaign{primary, secondary} {
		r.Issues = append(r.Issues, residentCampaignIssues(c)...)
		if len(c.Calibration) <= 2 && len(c.Comparison) <= 3 {
			all = append(all, c.Calibration...)
			all = append(all, c.Comparison...)
		}
	}
	r.Issues = append(r.Issues, residentDuplicateSessions(all)...)
	if status := residentIssueStatus(r.Issues); status != criterionMet {
		r.Status = status
		return r
	}
	r.Primary, r.Secondary = EvaluateResident(primary), EvaluateResident(secondary)
	r.Status = criterionMet
	for _, status := range []string{invalidEvidence, residentIncomplete, correctnessFailure, residentUnverified, criterionNotMet, inconclusive} {
		if r.Primary.Status == status || r.Secondary.Status == status {
			r.Status = status
			break
		}
	}
	return r
}
