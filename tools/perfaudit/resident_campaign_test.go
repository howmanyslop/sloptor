package main

import (
	"math"
	"testing"
)

func TestResidentInventory(t *testing.T) {
	scenarios := []string{"API_L32F32", "API_L1F128", "Body_L32F32", "Rewrite_L32F32", "Invalidate_L32F32", "EmptyDrain_L32F32", "Chain32", "API_X1", "API_X10", "MixedAPI_M0", "MixedBody_M0", "MixedInvalidate_M0", "MixedAPI_M1", "MixedBody_M1", "MixedInvalidate_M1"}
	for _, comparator := range []string{"B:S", "B:C", "S:C"} {
		counts := map[string]int{}
		for _, scenario := range scenarios {
			for _, settings := range []string{"C1B1", "C4B4"} {
				key := ResidentKey{Comparator: comparator, Scenario: scenario, Settings: settings}
				role, ok := key.policyRole()
				if ok {
					counts[role]++
					counts["all"]++
				}
				if (scenario == "API_X1" || scenario == "API_X10") && role != "control" {
					t.Fatal("same-output API_X was labelled semantic cost")
				}
			}
		}
		wantAll, wantCost := 30, 0
		if comparator == "B:S" {
			wantAll, wantCost = 18, 4
		}
		if comparator == "B:C" {
			wantCost = 8
		}
		if counts["all"] != wantAll || counts[residentCost] != wantCost || counts["primary"] != 1 || counts["secondary"] != 1 {
			t.Fatalf("inventory %s: %v", comparator, counts)
		}
	}
}

func TestResidentBenefitPrecedence(t *testing.T) {
	for _, v := range []struct {
		p, l, u float64
		want    string
	}{
		{.96, .94, .99, criterionNotMet},
		{.96, .94, 1.01, inconclusive},
		{.95, .94, .99, criterionMet},
		{.95, .90, 1, inconclusive},
		{1.01, 1, 1.02, criterionNotMet},
	} {
		if residentPractical(estimate{v.p, interval{v.l, v.u}}) != v.want {
			t.Fatalf("frozen P02 vector %+v", v)
		}
	}
	n := math.Log(1.2)
	threshold := math.Exp(-n) // declared approximate transcendental convention
	for _, v := range []struct {
		l, u float64
		want string
	}{
		{.75, threshold, inconclusive}, {threshold, .95, criterionNotMet}, {.8, math.Nextafter(threshold, 0), criterionMet},
	} {
		if residentNoiseGate(estimate{.8, interval{v.l, v.u}}, n) != v.want {
			t.Fatal("strict noise boundary changed")
		}
	}
	// Failure dominates practical uncertainty, rather than granting a retry.
	s := ResidentSummary{Status: criterionNotMet, Populations: map[string]ResidentPopulation{"full": {Benefit: ResidentBenefit{Ratio: estimate{.96, interval{.94, 1.01}}}}}}
	if residentStage(s, true, 0).Status != criterionNotMet {
		t.Fatal("benefit uncertainty overrode failed control")
	}
}

func TestResidentTransitionTable(t *testing.T) {
	for _, test := range []struct {
		name, initial, follow, joint, confirmation string
		requireConfirmation                        bool
		want                                       string
		retry                                      bool
	}{
		{"control-pass", criterionMet, "", "", "", false, criterionMet, false},
		{"primary-missing-confirmation", criterionMet, "", "", "", true, residentUnverified, false},
		{"both-confirmed", criterionMet, "", "", criterionMet, true, criterionMet, false},
		{"one-follow-up", inconclusive, "", "", "", false, inconclusive, true},
		{"standalone-and-joint", inconclusive, criterionMet, criterionMet, criterionMet, true, criterionMet, false},
		{"standalone-only", inconclusive, criterionMet, inconclusive, "", false, inconclusive, false},
		{"joint-only", inconclusive, criterionNotMet, criterionMet, "", false, criterionNotMet, false},
		{"no-failure-rescue", criterionNotMet, criterionMet, criterionMet, criterionMet, true, criterionNotMet, false},
		{"bad-confirmation-no-pooling", criterionMet, "", "", criterionNotMet, true, criterionNotMet, false},
		{"inconclusive-confirmation", criterionMet, "", "", inconclusive, true, inconclusive, false},
		{"adverse-confirmation-after-uncertainty", inconclusive, "", "", criterionNotMet, true, criterionNotMet, false},
		{"adverse-confirmation-after-joint-uncertainty", inconclusive, criterionMet, inconclusive, criterionNotMet, true, criterionNotMet, false},
		{"unverified-not-retry", residentUnverified, "", "", "", false, residentUnverified, false},
		{"missing-not-retry", residentIncomplete, "", "", "", false, residentIncomplete, false},
		{"invalid-after-fail", criterionNotMet, invalidEvidence, invalidEvidence, "", false, invalidEvidence, false},
		{"missing-after-fail", criterionNotMet, residentIncomplete, residentIncomplete, "", false, residentIncomplete, false},
		{"semantic-cost", residentCost, "", "", "", false, residentCost, false},
		{"unneeded-follow-up", criterionMet, criterionMet, criterionMet, "", false, invalidEvidence, false},
	} {
		t.Run(test.name, func(t *testing.T) {
			stages := map[string]ResidentStageDecision{"initial": {Status: test.initial}}
			for name, value := range map[string]string{"follow_up": test.follow, "joint": test.joint, "confirmation": test.confirmation} {
				if value != "" {
					stages[name] = ResidentStageDecision{Status: value}
				}
			}
			status, _, retry := residentTransition(stages, test.requireConfirmation)
			if status != test.want || retry != test.retry {
				t.Fatalf("got %s retry=%t, want %s retry=%t", status, retry, test.want, test.retry)
			}
		})
	}
}

func TestResidentBiasedAAAndRetainedNoise(t *testing.T) {
	key := ResidentKey{"B:C", "Chain32", "C1B1", "primary"}
	biased := residentInventedBatch("biased", "initial", "calibration", 1, key, 100000000, 104000000)
	s := AnalyzeResident([]ResidentBatch{biased})
	if s.Status != criterionMet || math.Abs(s.Noise-math.Log(1.04)) > 1e-14 || s.Noise == 0 {
		t.Fatal("biased zero-width AA lost its center")
	}
	initial := residentInventedBatch("noise-initial", "initial", "calibration", 1, key, 10000000, 12000000)
	for i := range initial.Blocks {
		for j := range 30 {
			initial.Blocks[i].Sessions[2].Cycles[j].Nanoseconds = 10000000
			initial.Blocks[i].Sessions[3].Cycles[j].Nanoseconds = 12000000
		}
		for j := range 4 {
			initial.Blocks[i].Sessions[j].Resources.UserTicks = residentTestUint(625000)
		}
	}
	initial.Blocks[0].Sessions[1].Resources.UserTicks = residentTestUint(1875000) // one session +125ms
	follow := residentInventedBatch("noise-follow", "follow_up", "calibration", 2, key, 10000000, 10000000)
	for i := range follow.Blocks {
		for j := range 4 {
			follow.Blocks[i].Sessions[j].Resources.UserTicks = residentTestUint(625000)
		}
	}
	c := ResidentCampaign{Label: residentSynthetic, Key: key, Prerequisites: residentAssumed, Calibration: []ResidentBatch{initial, follow}, Comparison: []ResidentBatch{
		residentInventedBatch("noise-candidate", "initial", "comparison", 3, key, 100000000, 90000000),
		residentInventedBatch("noise-confirmation", "confirmation", "comparison", 4, key, 100000000, 90000000),
	}}
	r := EvaluateResident(c)
	if r.Calibration.Status != criterionMet || r.Comparison.Status != criterionNotMet || r.Status != criterionNotMet || math.Abs(r.Noise-math.Log(1.2)) > 1e-14 || r.Comparison.FollowUpAllowed {
		t.Fatalf("initial noise was discarded: cal=%s compare=%s final=%s N=%g", r.Calibration.Status, r.Comparison.Status, r.Status, r.Noise)
	}
}

func residentWitnessCPU(b *ResidentBatch, highBlocks int) {
	for i := range b.Blocks {
		for j := range 4 {
			v := uint64(10000000)
			if j == 1 || j == 2 {
				v = 8437500
				if i < highBlocks {
					v = 10781250
				}
			}
			b.Blocks[i].Sessions[j].Resources.UserTicks = residentTestUint(v)
		}
	}
}

func TestResidentReachableJointDeferral(t *testing.T) {
	key := residentTestKey()
	c := ResidentCampaign{
		Label: residentSynthetic, Key: key, Prerequisites: residentAssumed,
		Calibration: []ResidentBatch{residentInventedBatch("joint-AA", "initial", "calibration", 1, key, 10000000, 10000000)},
		Comparison:  []ResidentBatch{residentInventedBatch("joint-initial", "initial", "comparison", 2, key, 10000000, 10000000), residentInventedBatch("joint-follow", "follow_up", "comparison", 3, key, 10000000, 10000000)},
	}
	residentWitnessCPU(&c.Comparison[0], 3)
	residentWitnessCPU(&c.Comparison[1], 2)
	r := EvaluateResident(c)
	if r.Comparison.Decisions["initial"].Status != inconclusive || r.Comparison.Decisions["follow_up"].Status != criterionMet || r.Comparison.Decisions["joint"].Status != inconclusive || r.Status != inconclusive || r.Comparison.FollowUpAllowed {
		t.Fatalf("reachable policy witness disagrees: %+v", r.Comparison.Decisions)
	}
	joint := r.Comparison.Summaries["joint"].Populations["full"].Metrics["session_cpu_ms"].Rows["median"].Forward
	residentAssertRat(t, joint.ExactMarginal.Ratio.CI95.Upper, "69/64")
	residentAssertRat(t, joint.ExactMarginal.Difference.CI95.Upper, "625/8")
}

func TestResidentConfirmationAndPrerequisites(t *testing.T) {
	var confirmed []ResidentCampaign
	for _, settings := range []string{"C1B1", "C4B4"} {
		role := "primary"
		if settings == "C4B4" {
			role = "secondary"
		}
		key := ResidentKey{"B:C", "Chain32", settings, role}
		c := ResidentCampaign{
			Label: residentSynthetic, Key: key, Prerequisites: residentAssumed,
			Calibration: []ResidentBatch{residentInventedBatch("confirm-AA-"+settings, "initial", "calibration", 1, key, 100000000, 100000000)},
			Comparison:  []ResidentBatch{residentInventedBatch("confirm-initial-"+settings, "initial", "comparison", 2, key, 100000000, 80000000), residentInventedBatch("confirm-confirmation-"+settings, "confirmation", "comparison", 3, key, 100000000, 110000000)},
		}
		r := EvaluateResident(c)
		if r.Comparison.Decisions["initial"].Status != criterionMet || r.Comparison.Decisions["confirmation"].Status != criterionNotMet || r.Status != criterionNotMet || len(r.Comparison.Summaries) != 2 {
			t.Fatal("bad confirmation rescued/pooled")
		}
		for i := range c.Comparison[1].Blocks {
			for _, j := range []int{1, 2} {
				for p := range 30 {
					c.Comparison[1].Blocks[i].Sessions[j].Cycles[p].Nanoseconds = 80000000
				}
			}
		}
		r = EvaluateResident(c)
		if r.Status != criterionMet || r.PromotionReady || len(r.ExternalGates) == 0 {
			t.Fatal("independent confirmed synthetic pass missing or promoted")
		}
		confirmed = append(confirmed, c)
		c.Prerequisites = residentUnverified
		r = EvaluateResident(c)
		if r.Status != residentUnverified || r.Comparison.FollowUpAllowed || r.Calibration.FollowUpAllowed {
			t.Fatal("unverified prerequisite authorized retry")
		}
	}
	question := EvaluateResidentQuestion(confirmed[0], confirmed[1])
	if question.Status != criterionMet || question.PromotionReady {
		t.Fatal("independent primary AND secondary did not pass as a synthetic question")
	}
	if EvaluateResidentQuestion(confirmed[0], confirmed[0]).Status != invalidEvidence {
		t.Fatal("one primary substituted for independent secondary")
	}
}

func TestResidentSuccessfulCandidateFollowUp(t *testing.T) {
	key := ResidentKey{"B:C", "Chain32", "C1B1", "primary"}
	c := ResidentCampaign{
		Label: residentSynthetic, Key: key, Prerequisites: residentAssumed,
		Calibration: []ResidentBatch{residentInventedBatch("follow-AA", "initial", "calibration", 1, key, 100000000, 100000000)},
		Comparison:  []ResidentBatch{residentInventedBatch("follow-initial", "initial", "comparison", 2, key, 100000000, 80000000), residentInventedBatch("follow-doubled", "follow_up", "comparison", 3, key, 100000000, 80000000), residentInventedBatch("follow-confirmation", "confirmation", "comparison", 4, key, 100000000, 80000000)},
	}
	c.Comparison[0].Blocks[0].Sessions[1].Resources.UserTicks = residentTestUint(1875001) // +125ms
	r := EvaluateResident(c)
	if r.Comparison.Decisions["initial"].Status != inconclusive || r.Comparison.Decisions["follow_up"].Status != criterionMet || r.Comparison.Decisions["joint"].Status != criterionMet || r.Comparison.Decisions["confirmation"].Status != criterionMet || r.Status != criterionMet || r.Comparison.FollowUpAllowed {
		t.Fatal("valid standalone AND joint plus independent confirmation did not pass")
	}
}

func TestResidentCalibrationBeforeCandidate(t *testing.T) {
	b := residentInventedBatch("prospective-AA", "initial", "calibration", 1, residentTestKey(), 10000000, 10000000)
	b.Blocks[0].Sessions[1].Resources.UserTicks = residentTestUint(1875001)
	r := EvaluateResidentCalibration([]ResidentBatch{b}, residentAssumed)
	if r.Status != inconclusive || !r.FollowUpAllowed || len(r.Batches) != 1 {
		t.Fatal("valid inconclusive AA cannot obtain its own one follow-up")
	}
	r = EvaluateResidentCalibration([]ResidentBatch{b}, residentUnverified)
	if r.Status != residentUnverified || r.FollowUpAllowed {
		t.Fatal("unverified AA prerequisite authorized a retry")
	}
	follow := residentInventedBatch("prospective-AA-follow", "follow_up", "calibration", 2, b.Key, 10000000, 10000000)
	r = EvaluateResidentCalibration([]ResidentBatch{b, follow}, residentAssumed)
	if r.Status != criterionMet || r.FollowUpAllowed || r.Decisions["follow_up"].Status != criterionMet || r.Decisions["joint"].Status != criterionMet {
		t.Fatal("independent AA standalone AND joint did not resolve initial uncertainty")
	}
}

func TestResidentPreservesInvalidLaterAndSemanticCost(t *testing.T) {
	key := ResidentKey{"B:C", "API_L32F32", "C1B1", residentCost}
	c := ResidentCampaign{
		Label: residentSynthetic, Key: key, Prerequisites: residentAssumed,
		Calibration: []ResidentBatch{residentInventedBatch("cost-AA", "initial", "calibration", 1, key, 10000000, 10000000)},
		Comparison:  []ResidentBatch{residentInventedBatch("cost-initial", "initial", "comparison", 2, key, 10000000, 1000000000)},
	}
	r := EvaluateResident(c)
	if r.Status != residentCost || r.Comparison.Decisions["initial"].Practical != "" || r.Comparison.FollowUpAllowed {
		t.Fatalf("required semantic work classified as equal-output regression: %s", r.Status)
	}
	for _, metric := range r.Comparison.Summaries["initial"].Populations["full"].Metrics {
		for _, row := range metric.Rows {
			if row.Status != residentCost {
				t.Fatal("semantic-cost row became acceptance/failure")
			}
		}
	}
	c.Comparison = append(c.Comparison, residentInventedBatch("cost-broken", "follow_up", "comparison", 3, key, 1, 1))
	c.Comparison[1].Blocks[9].Sessions[3].Cycles[29].Nanoseconds = 0
	r = EvaluateResident(c)
	if r.Status != invalidEvidence || len(r.Input.Comparison) != 2 || len(r.Input.Comparison[1].Blocks) != 10 || r.Comparison.FollowUpAllowed {
		t.Fatal("invalid later evidence disappeared behind earlier disposition")
	}
	// Independently supplied session IDs cannot be reused for confirmations.
	c.Comparison[1] = c.Comparison[0]
	c.Comparison[1].Stage, c.Comparison[1].ID, c.Comparison[1].Chronology = "confirmation", "distinct-batch-same-sessions", 3
	r = EvaluateResident(c)
	if r.Status != invalidEvidence {
		t.Fatal("same sessions reused as independent confirmation")
	}
}
