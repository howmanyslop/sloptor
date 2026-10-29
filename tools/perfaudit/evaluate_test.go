package main

import (
	"encoding/json"
	"strings"
	"testing"
)

func fixture(t *testing.T, coldCandidate, noChangeCandidate int64, coldCount, noChangeCount int) []byte {
	t.Helper()
	root := map[string]any{
		"schema": 1,
		"machine": map[string]any{
			"os": "Windows", "version": "11", "cpu": "fixture", "ram_bytes": 1024,
			"storage": "fixture", "power": "fixture", "run_order": []string{"AB", "BA"}, "sidecar_timeout": "300s", "environment": map[string]string{"x": "y"},
		},
		"baseline":  map[string]string{"revision": "baseline", "command": "baseline build"},
		"candidate": map[string]string{"revision": "candidate", "command": "candidate build"},
	}
	var records []record
	for _, phase := range []string{cold, noChange} {
		count, duration := coldCount, coldCandidate
		if phase == noChange {
			count, duration = noChangeCount, noChangeCandidate
		}
		for i := 1; i <= count; i++ {
			order := "AB"
			if i%2 == 0 {
				order = "BA"
			}
			b := record{i, phase, order, "baseline", 1000, 0, "diagnostics", "output"}
			c := record{i, phase, order, "candidate", duration, 0, "diagnostics", "output"}
			if order == "AB" {
				records = append(records, b, c)
			} else {
				records = append(records, c, b)
			}
		}
	}
	root["records"] = records
	data, err := json.Marshal(root)
	if err != nil {
		t.Fatal(err)
	}
	return data
}

func TestAuditOutcomesAndExternalGates(t *testing.T) {
	for _, tc := range []struct {
		name           string
		cold, noChange int64
		want           string
	}{
		{"five percent boundary", 950, 1100, criterionMet},
		{"AA", 1000, 1000, criterionNotMet},
		{"smaller than default", 960, 1000, criterionNotMet},
		{"guardrail demonstrated failure", 900, 1101, criterionNotMet},
	} {
		t.Run(tc.name, func(t *testing.T) {
			r := evaluate(fixture(t, tc.cold, tc.noChange, 10, 20), config{cold, 0.95, 10, 20})
			if r.Outcome != tc.want || len(r.Issues) != 0 {
				t.Fatalf("outcome %s want %s, issues %+v", r.Outcome, tc.want, r.Issues)
			}
			if r.PromotionReady || !strings.HasPrefix(r.LegacyVerdict, "unverified:") {
				t.Fatal("audit cannot establish promotion/legacy verdict")
			}
			for _, gate := range r.ExternalGates {
				if gate.Status != "unverified" {
					t.Fatalf("gate silently passed: %+v", gate)
				}
			}
		})
	}
}

func TestUncertaintyAndThresholdBoundaries(t *testing.T) {
	for _, tc := range []struct {
		name      string
		value     estimate
		guardrail bool
		want      string
	}{
		{"primary inclusive point", estimate{0.95, interval{0.9, 0.99}}, false, criterionMet},
		{"primary strict CI", estimate{0.95, interval{0.9, 1}}, false, inconclusive},
		{"primary includes harm", estimate{0.98, interval{0.9, 1.02}}, false, inconclusive},
		{"primary insufficient point", estimate{0.96, interval{0.9, 0.99}}, false, criterionNotMet},
		{"primary no improvement", estimate{1, interval{1, 1}}, false, criterionNotMet},
		{"guard inclusive upper", estimate{1.05, interval{1, 1.1}}, true, criterionMet},
		{"guard uncertain", estimate{1.09, interval{1, 1.2}}, true, inconclusive},
		{"guard lower on boundary", estimate{1.15, interval{1.1, 1.2}}, true, inconclusive},
		{"guard demonstrated fail", estimate{1.15, interval{1.11, 1.2}}, true, criterionNotMet},
	} {
		t.Run(tc.name, func(t *testing.T) {
			got := primaryDecision(tc.value, 0.95)
			if tc.guardrail {
				got = guardrailDecision(tc.value)
			}
			if got.Status != tc.want {
				t.Fatalf("%+v want %s", got, tc.want)
			}
		})
	}
	input := fixture(t, 900, 1000, 10, 20)
	// One no-change block at 1.2 and the rest at 1.0 yields a p95 CI
	// including both, so even a good primary cannot pass the audit.
	var root map[string]json.RawMessage
	if err := json.Unmarshal(input, &root); err != nil {
		t.Fatal(err)
	}
	var records []record
	if err := json.Unmarshal(root["records"], &records); err != nil {
		t.Fatal(err)
	}
	for i := range records {
		if records[i].Phase == noChange && records[i].Binary == "candidate" && records[i].Pair <= 2 {
			records[i].DurationMS = 1200
		}
	}
	root["records"], _ = json.Marshal(records)
	input, _ = json.Marshal(root)
	if r := evaluate(input, config{cold, 0.95, 10, 20}); r.Outcome != inconclusive || r.Guardrail.Status != inconclusive {
		t.Fatalf("crossing guardrail must be inconclusive: %+v", r.Guardrail)
	}
}

func TestAllCorrectnessFailuresReportedEvenWithMalformedRecord(t *testing.T) {
	input := string(fixture(t, 900, 1000, 10, 20))
	input = strings.Replace(input, `"exit_code":0`, `"exit_code":7`, 1)
	input = strings.Replace(input, `"diagnostics_digest":"diagnostics"`, `"diagnostics_digest":"different"`, 1)
	input = strings.Replace(input, `"output_tree_digest":"output"`, `"output_tree_digest":"different"`, 1)
	r := evaluate([]byte(input), config{cold, 0.95, 10, 20})
	if r.Outcome != correctnessFailure || len(r.Issues) != 3 {
		t.Fatalf("want three correctness issues: %s %+v", r.Outcome, r.Issues)
	}
	input = strings.Replace(input, `"duration_ms":1000`, `"duration_ms":0`, 1)
	r = evaluate([]byte(input), config{cold, 0.95, 10, 20})
	counts := map[string]int{}
	for _, problem := range r.Issues {
		counts[problem.Kind]++
	}
	if r.Outcome != invalidEvidence || counts[correctnessFailure] != 3 || counts[invalidEvidence] == 0 || r.Statistics != nil {
		t.Fatalf("invalid evidence must preserve correctness issues and suppress statistics: %+v", r)
	}
}
