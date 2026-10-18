package main

import (
	"math"
	"reflect"
	"testing"
)

func closeTo(t *testing.T, got, want float64) {
	t.Helper()
	if math.Abs(got-want) > 1e-12*math.Max(1, math.Abs(want)) {
		t.Errorf("got %.17g, want %.17g", got, want)
	}
}

func TestHandComputedStatistics(t *testing.T) {
	// Marginal medians are 25/50, differences are 10,20,30,40,
	// ratios are 2,2,2,2: paired log median is log(2).
	m := measure([]pair{{10, 20, "AB"}, {40, 80, "BA"}, {20, 40, "AB"}, {30, 60, "BA"}})
	closeTo(t, m.BaselineMedianMS, 25)
	closeTo(t, m.CandidateMedianMS, 50)
	closeTo(t, m.BaselineP95MS, 40)
	closeTo(t, m.CandidateP95MS, 80)
	closeTo(t, m.PairedDifferenceMS, 25)
	closeTo(t, m.PairedLogRatio, math.Log(2))
	closeTo(t, m.PairedRatio, 2)
	values := make([]float64, 20)
	for i := range values {
		values[i] = float64(20 - i)
	}
	closeTo(t, p95(values), 19)
	closeTo(t, median([]float64{9, 1, 3}), 3)
	// Median ratio is the geometric mean of the two middle ratios, not
	// a ratio of marginal medians, arithmetic median ratio, or mean ratio.
	m = measure([]pair{{1, 1, "AB"}, {2, 8, "BA"}})
	closeTo(t, m.PairedRatio, 2)
	closeTo(t, m.CandidateMedianMS/m.BaselineMedianMS, 3)
	closeTo(t, m.PairedDifferenceMS, 3)
	closeTo(t, percentile([]float64{0, 10, 20, 30}, 0.025), 0.75)
	closeTo(t, percentile([]float64{0, 10, 20, 30}, 0.975), 29.25)
}

func TestConstantAAHasDegenerateIntervalsAndNoImprovement(t *testing.T) {
	pairs := []pair{{100, 100, "AB"}, {100, 100, "BA"}, {100, 100, "AB"}, {100, 100, "BA"}}
	s := summarize(pairs)
	if s.Estimates.PairedRatio != (estimate{1, interval{1, 1}}) || s.Estimates.P95Ratio != (estimate{1, interval{1, 1}}) {
		t.Fatalf("A/A ratios: %+v", s.Estimates)
	}
	if primaryDecision(s.Estimates.PairedRatio, 0.95).Status != criterionNotMet {
		t.Fatal("constant A/A must not count as improvement")
	}
}

func TestArmReversalInvertsIntervalsAndDifference(t *testing.T) {
	pairs := []pair{{100, 70, "AB"}, {110, 90, "BA"}, {20, 22, "AB"}, {25, 24, "BA"}, {300, 260, "AB"}, {340, 290, "BA"}}
	reversed := make([]pair, len(pairs))
	for i, p := range pairs {
		order := "AB"
		if p.order == "AB" {
			order = "BA"
		}
		reversed[i] = pair{p.candidate, p.baseline, order}
	}
	a, b := summarize(pairs).Estimates, summarize(reversed).Estimates
	closeTo(t, b.PairedRatio.Value, 1/a.PairedRatio.Value)
	closeTo(t, b.PairedRatio.CI95.Lower, 1/a.PairedRatio.CI95.Upper)
	closeTo(t, b.PairedRatio.CI95.Upper, 1/a.PairedRatio.CI95.Lower)
	closeTo(t, b.PairedDifferenceMS.Value, -a.PairedDifferenceMS.Value)
	closeTo(t, b.PairedDifferenceMS.CI95.Lower, -a.PairedDifferenceMS.CI95.Upper)
	closeTo(t, b.PairedDifferenceMS.CI95.Upper, -a.PairedDifferenceMS.CI95.Lower)
}

func TestCorrelatedBlocksRemainIntact(t *testing.T) {
	// Each block has order-dependent opposing effects, while block speeds
	// differ by 100x. Every intact draw has ratio 1 and difference 0; drawing
	// arms or pairs independently would destroy at least one invariant.
	pairs := []pair{{10, 20, "AB"}, {20, 10, "BA"}, {1000, 2000, "AB"}, {2000, 1000, "BA"}}
	rng := splitMix64{bootstrapSeed}
	sample := make([]pair, len(pairs))
	for range 100 {
		sampleBlocks(pairs, sample, &rng)
		for i := 0; i < len(sample); i += 2 {
			intact := sample[i] == pairs[0] && sample[i+1] == pairs[1] || sample[i] == pairs[2] && sample[i+1] == pairs[3]
			if !intact {
				t.Fatal("four-command block was split")
			}
		}
	}
	s := summarize(pairs)
	if s.Estimates.PairedRatio.CI95 != (interval{1, 1}) || s.Estimates.P95Ratio.CI95 != (interval{1, 1}) || s.Estimates.PairedDifferenceMS.CI95 != (interval{0, 0}) {
		t.Fatalf("correlated sample invariants lost: %+v", s.Estimates)
	}
	if s.Order["AB"].Metrics.PairedRatio != 2 || s.Order["BA"].Metrics.PairedRatio != 0.5 {
		t.Fatalf("strata lost: %+v", s.Order)
	}
}

func TestDeterministicStreamAndIntervals(t *testing.T) {
	// Published SplitMix64 reference stream at state zero.
	rng := splitMix64{}
	for _, want := range []uint64{0xe220a8397b1dcdaf, 0x6e789e6aa1b965f4, 0x06c45d188009454f} {
		if got := rng.next(); got != want {
			t.Fatalf("SplitMix64 got %x want %x", got, want)
		}
	}
	pairs := []pair{{10, 9, "AB"}, {12, 10, "BA"}, {20, 21, "AB"}, {22, 20, "BA"}}
	if !reflect.DeepEqual(summarize(pairs), summarize(pairs)) {
		t.Fatal("fixed seed changed results")
	}
}
