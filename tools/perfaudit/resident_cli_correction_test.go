package main

import (
	"bytes"
	"encoding/json"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestResidentReaderOutcomePrecedence(t *testing.T) {
	// Explicit policy/evidence precedence, independent of input order. Cost is
	// descriptive, whereas an unknown internal status supplies no acceptance.
	order := []string{invalidEvidence, correctnessFailure, residentIncomplete, residentUnverified, criterionNotMet, inconclusive, residentCost, criterionMet}
	for i, a := range order {
		for j, b := range order {
			want := a
			if j < i {
				want = b
			}
			if got := residentReaderOutcome(a, b); got != want {
				t.Errorf("fold(%q,%q)=%q want %q", a, b, got, want)
			}
		}
	}
	for _, statuses := range [][]string{nil, {""}, {"semantic-cots", criterionMet}, {residentCost, "future-status"}, {criterionNotMet, "future-status"}} {
		if got := residentReaderOutcome(statuses...); got != invalidEvidence {
			t.Errorf("unrecognized/absent status %q silently routed to %q", statuses, got)
		}
	}
}

func TestResidentReaderSemanticCostCLI(t *testing.T) {
	f := newResidentProtocolFixture(t, ResidentKey{"B:C", "API_L32F32", "C1B1", residentCost})
	f.add("calibration-initial", 1, 100)
	f.add("initial", 2, 1000) // Deliberately tenfold required-work cost, not equal-output failure.
	f.authorize()
	p := f.write(filepath.Join(f.root, "reader.json"), f.plan)
	var stdout, stderr bytes.Buffer
	out := filepath.Join(f.root, "cost-audit.json")
	code := run([]string{"--resident-plan", p.Path, "--output", out}, &stdout, &stderr)
	var got residentReaderReport
	if err := json.Unmarshal(mustReadTest(t, out), &got); err != nil {
		t.Fatal(err)
	}
	if len(got.Campaigns) != 1 || got.Campaigns[0].Status != residentCost || got.Campaigns[0].Comparison.Status != residentCost {
		t.Fatal("fixture did not retain the semantic-cost core decision")
	}
	s := got.Campaigns[0].Comparison.Summaries["initial"]
	if len(s.Populations) != 3 || s.Populations["full"].CyclePairs != 300 || s.Populations["full"].Benefit.Ratio.Value < 9.9 {
		t.Fatal("nested required-work estimates erased")
	}
	if got.Outcome != residentCost || code != 1 || !strings.HasPrefix(stdout.String(), residentCost+" ") || stderr.Len() != 0 || got.PromotionReady {
		t.Fatalf("cost reported as acceptance/failure: outcome=%s exit=%d stdout=%s stderr=%s", got.Outcome, code, &stdout, &stderr)
	}
}

// All files here are freshly invented protocol bytes, never collected runs.
// Equal host/support seals remove the independent review's first-witness
// confounder. Only UTC intervals/outer ordinals vary in chronology probes.
func residentChronologyFixture(t *testing.T, primary, secondary [3]int) (*residentProtocolFixture, *residentProtocolFixture) {
	t.Helper()
	p := newResidentProtocolFixture(t, ResidentKey{"B:C", "Chain32", "C1B1", "primary"})
	s := newResidentProtocolFixture(t, ResidentKey{"B:C", "Chain32", "C4B4", "secondary"})
	for i, f := range []*residentProtocolFixture{p, s} {
		ord := primary
		if i == 1 {
			ord = secondary
		}
		f.add("calibration-initial", ord[0], 100)
		f.add("initial", ord[1], 80)
		f.add("confirmation", ord[2], 80)
	}
	hostBytes := mustReadTest(t, filepath.Join(p.refs[0].Directory, "host.before.json"))
	host := p.read(filepath.Join(p.refs[0].Directory, "host.before.json"))
	if host["qpc_support"] == nil {
		t.Fatal("host fixture lost qpc_support")
	}
	for _, ref := range s.refs {
		s.write(filepath.Join(ref.Directory, "host.before.json"), hostBytes)
		s.write(filepath.Join(ref.Directory, "host.after.json"), hostBytes)
		events := s.events(ref)
		for i, event := range events {
			if i%2 == 1 {
				event["enclosure"].(map[string]any)["support"] = host["qpc_support"]
			}
		}
		s.seal(ref, events)
	}
	return p, s
}

func TestResidentReaderGlobalChronology(t *testing.T) {
	for _, tc := range []struct {
		name      string
		p, s      [3]int
		shift     time.Duration
		duplicate bool
		want      string
	}{
		{"normal", [3]int{1, 2, 3}, [3]int{4, 5, 6}, 0, false, criterionMet},
		{"interleaved-gaps", [3]int{1, 5, 9}, [3]int{3, 7, 11}, 0, false, criterionMet},
		{"same-host-reviewer-overlap", [3]int{1, 2, 3}, [3]int{4, 5, 6}, -9 * time.Minute, false, invalidEvidence},
		{"reverse-global-time", [3]int{1, 2, 3}, [3]int{4, 5, 6}, -12 * time.Minute, false, invalidEvidence},
		{"duplicate-global-ordinal", [3]int{1, 2, 3}, [3]int{4, 5, 6}, 0, true, invalidEvidence},
	} {
		t.Run(tc.name, func(t *testing.T) {
			p, s := residentChronologyFixture(t, tc.p, tc.s)
			if tc.shift != 0 {
				for _, ref := range s.refs {
					events := s.events(ref)
					for _, event := range events {
						stamp, err := time.Parse(time.RFC3339Nano, event["at_utc"].(string))
						if err != nil {
							t.Fatal(err)
						}
						event["at_utc"] = stamp.Add(tc.shift).Format(time.RFC3339Nano)
					}
					s.seal(ref, events)
				}
			}
			if tc.duplicate {
				s.plan.Campaigns[0].Calibration[0].Chronology = 1
				s.refs[0].Chronology = 1
			}
			p.plan.Campaigns = append(p.plan.Campaigns, s.plan.Campaigns[0])
			p.refs = append(p.refs, s.refs...)
			p.authorize() // An assertion cannot override the conflicting UTC/ordinal evidence.
			plan := p.write(filepath.Join(p.root, "reader.json"), p.plan)
			before := mustReadTest(t, plan.Path)
			r := newResidentReader()
			got := r.reconstruct(plan.Path)
			original := string(mustJSON(got.Batches))
			r.decide(&got)
			p.write(filepath.Join(p.root, "chronology-audit.json"), got)
			if len(got.Batches) != 6 {
				t.Fatal("batch population was filtered")
			}
			for i, b := range got.Batches {
				if b.HostSHA256 != got.Batches[0].HostSHA256 || b.Ref != p.refs[i] || len(b.Reconstructed.Blocks) != 5 {
					t.Fatal("same-host witness or original array/population changed")
				}
			}
			if original != string(mustJSON(got.Batches)) || !bytes.Equal(before, mustReadTest(t, plan.Path)) {
				t.Fatal("analysis mutated original arrays/bytes")
			}
			for _, snap := range r.files {
				if !bytes.Equal(snap.data, mustReadTest(t, snap.Path)) {
					t.Fatal("input snapshot changed")
				}
			}
			if got.Outcome != tc.want || got.PromotionReady {
				t.Fatalf("want=%s actual=%s issues=%+v", tc.want, got.Outcome, got.Issues)
			}
			if tc.want == invalidEvidence && (len(got.Campaigns) != 0 || got.Question != nil) {
				t.Fatal("contradictory global chronology reached numerical analysis")
			}
		})
	}
}

func TestResidentReaderLabelAwareActions(t *testing.T) {
	const syntheticAction = "required numerical stages pass on invented synthetic data"
	f := newResidentProtocolFixture(t, ResidentKey{"B:C", "Chain32", "C1B1", "primary"})
	f.add("calibration-initial", 1, 100)
	f.add("initial", 2, 80)
	f.add("confirmation", 3, 80)
	f.authorize()
	invented := f.analyze()
	if invented.Outcome != criterionMet || invented.EvidenceKind != "invented-protocol" {
		t.Fatal(invented.Outcome)
	}
	var c ResidentCampaign
	// Deep copy the invented typed seam. Relabeling it is a test of result
	// wording, not a raw admission path or an assertion of recorded collection.
	if err := json.Unmarshal(mustJSON(invented.campaigns[0]), &c); err != nil {
		t.Fatal(err)
	}
	c.Label, c.Prerequisites = residentRecorded, residentDeclared
	for _, track := range []*[]ResidentBatch{&c.Calibration, &c.Comparison} {
		for i := range *track {
			(*track)[i].Label = residentRecorded
		}
	}
	recorded := EvaluateResident(c)
	cal := EvaluateResidentCalibration(c.Calibration, residentDeclared)
	if recorded.Status != criterionMet || cal.Status != criterionMet || recorded.Noise != invented.Campaigns[0].Noise {
		t.Fatal("label changed arithmetic")
	}
	for _, track := range []ResidentTrackResult{recorded.Calibration, recorded.Comparison, cal} {
		if strings.Contains(track.Action, "invented synthetic") || !strings.Contains(track.Action, "recorded") || !strings.Contains(track.Action, "root declarations") {
			t.Errorf("recorded action has wrong provenance: %q", track.Action)
		}
		for _, batch := range track.Batches {
			if batch.Label != residentRecorded {
				t.Fatal("recorded batch label lost")
			}
		}
	}
	for _, track := range []ResidentTrackResult{invented.Campaigns[0].Calibration, invented.Campaigns[0].Comparison} {
		if track.Action != syntheticAction {
			t.Fatal("prior synthetic action changed")
		}
	}
	if !strings.Contains(string(mustJSON(invented)), syntheticAction) || strings.Contains(string(mustJSON(invented)), "admitted-recorded") {
		t.Fatal("invented raw report silently relabeled")
	}
	if strings.Contains(string(mustJSON(recorded)), "invented synthetic data") || recorded.PromotionReady {
		t.Fatal("recorded nested result retains synthetic action or promotes")
	}
	f.write(filepath.Join(f.root, "invented-action-audit.json"), invented)
	f.write(filepath.Join(f.root, "recorded-typed-seam-action-audit.json"), recorded)
}
