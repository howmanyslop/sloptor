package main

import (
	"math"
	"reflect"
	"testing"
)

// These arrays reproduce the independent R2/R3 offline policy examples. They
// are synthetic statistics inputs, never compiler measurements or scores.
func resourceTestBlocks(baseline float64, candidates []float64) []pair {
	var pairs []pair
	for _, candidate := range candidates {
		pairs = append(pairs, pair{baseline, candidate, "AB"}, pair{baseline, candidate, "BA"})
	}
	return pairs
}

func resourceTestConstant(blocks int, baseline, candidate float64) []pair {
	candidates := make([]float64, blocks)
	for i := range candidates {
		candidates[i] = candidate
	}
	return resourceTestBlocks(baseline, candidates)
}

func resourceTestLevels(highCount, lowCount int, high, low float64) []float64 {
	values := make([]float64, highCount+lowCount)
	for i := range values {
		values[i] = low
		if i < highCount {
			values[i] = high
		}
	}
	return values
}

func resourceTestEstimate(t *testing.T, got estimate, point, lower, upper float64) {
	t.Helper()
	closeTo(t, got.Value, point)
	closeTo(t, got.CI95.Lower, lower)
	closeTo(t, got.CI95.Upper, upper)
}

func TestResourceR2SymmetricCalibrationAndRareWork(t *testing.T) {
	biased := resourcePhaseStatistics([][]pair{resourceTestConstant(15, 62.5, 31.25)}, "job_cpu_ms", true)
	if biased.Status != criterionNotMet {
		t.Fatalf("R2-P01 biased calibration: %s", biased.Status)
	}
	for _, name := range []string{"mean", "median"} {
		row := biased.Rows[name]
		if row.Forward.Gate.Status != criterionMet || row.Reverse.Gate.Status != criterionNotMet {
			t.Fatalf("R2-P01 %s directions: %+v", name, row)
		}
		resourceTestEstimate(t, *row.Forward.Ratio, 0.5, 0.5, 0.5)
		resourceTestEstimate(t, *row.Reverse.Ratio, 2, 2, 2)
	}

	for _, test := range []struct {
		name, metric                        string
		baseline, expensive, totalA, totalB float64
		difference, differenceUpper         float64
	}{
		{"CPU", "job_cpu_ms", 62.5, 1062.5, 1875, 3875, 200.0 / 3, 200},
		{"operations", "read_operations", 100, 1700, 3000, 6200, 320.0 / 3, 320},
	} {
		t.Run(test.name, func(t *testing.T) {
			values := resourceTestLevels(1, 14, test.expensive, test.baseline)
			result := resourcePhaseStatistics([][]pair{resourceTestBlocks(test.baseline, values)}, test.metric, false)
			if result.Status != inconclusive || result.Rows["median"].Status != criterionMet || result.Rows["mean"].Status != inconclusive {
				t.Fatalf("R2-P02 concentrated work must retain mean uncertainty: %+v", result.Rows)
			}
			closeTo(t, *result.Baseline.Total, test.totalA)
			closeTo(t, *result.Candidate.Total, test.totalB)
			resourceTestEstimate(t, *result.Rows["mean"].Forward.Ratio, 31.0/15, 1, 4.2)
			resourceTestEstimate(t, result.Rows["mean"].Forward.Difference, test.difference, 0, test.differenceUpper)
		})
	}
}

func TestResourceR2AndR3StratifiedFollowUpVectors(t *testing.T) {
	oldInitial := resourceTestBlocks(1000, resourceTestLevels(10, 5, 1080, 1040))
	oldDoubled := resourceTestBlocks(1000, resourceTestLevels(7, 23, 1080, 1040))
	initial := resourcePhaseStatistics([][]pair{oldInitial}, "job_cpu_ms", false)
	doubled := resourcePhaseStatistics([][]pair{oldDoubled}, "job_cpu_ms", false)
	joint := resourcePhaseStatistics([][]pair{oldInitial, oldDoubled}, "job_cpu_ms", false)
	if initial.Status != criterionNotMet || initial.Rows["median"].Status != inconclusive {
		t.Fatalf("old R2-P03 example now fails earlier through the added mean gate: %+v", initial.Rows)
	}
	resourceTestEstimate(t, *initial.Rows["mean"].Forward.Ratio, 16.0/15, 1.056, 1.0746666666666667)
	resourceTestEstimate(t, initial.Rows["mean"].Forward.Difference, 200.0/3, 56, 224.0/3)
	resourceTestEstimate(t, *doubled.Rows["median"].Forward.Ratio, 1.04, 1.04, 1.04)
	resourceTestEstimate(t, *joint.Rows["median"].Forward.Ratio, 1.04, 1.04, 1.08)
	if joint.Rows["median"].Status != inconclusive {
		t.Fatal("a standalone median pass must not erase joint median uncertainty")
	}

	// R3's reachable example passes the initial mean and may use one doubled
	// batch. Standalone passes; batch-stratified joint median still does not.
	reachableInitial := resourceTestBlocks(1000, resourceTestLevels(10, 5, 1078.125, 843.75))
	reachableDoubled := resourceTestBlocks(1000, resourceTestLevels(7, 23, 1078.125, 843.75))
	initial = resourcePhaseStatistics([][]pair{reachableInitial}, "job_cpu_ms", false)
	doubled = resourcePhaseStatistics([][]pair{reachableDoubled}, "job_cpu_ms", false)
	joint = resourcePhaseStatistics([][]pair{reachableInitial, reachableDoubled}, "job_cpu_ms", false)
	if initial.Status != inconclusive || initial.Rows["mean"].Status != criterionMet || doubled.Status != criterionMet || joint.Status != inconclusive || joint.Rows["mean"].Status != criterionMet {
		t.Fatalf("R3 reachable follow-up: initial=%s/%s doubled=%s joint=%s/%s", initial.Status, initial.Rows["mean"].Status, doubled.Status, joint.Status, joint.Rows["mean"].Status)
	}
	resourceTestEstimate(t, *initial.Rows["mean"].Forward.Ratio, 1, 0.9375, 1.046875)
	resourceTestEstimate(t, *joint.Rows["median"].Forward.Ratio, 0.84375, 0.84375, 1.078125)
	if !reflect.DeepEqual(joint.BatchBlockCounts, []int{15, 30}) || joint.PairCount != 90 || joint.Order["AB"].PairCount != 45 || joint.Order["BA"].PairCount != 45 {
		t.Fatalf("joint membership lost: %+v", joint)
	}
}

func TestResourceZeroRowsAndWholeRowFallback(t *testing.T) {
	for _, test := range []struct {
		candidate float64
		status    string
	}{
		{0, criterionMet}, {15.625, criterionMet}, {31.25, criterionNotMet},
	} {
		result := resourcePhaseStatistics([][]pair{resourceTestConstant(15, 0, test.candidate)}, "job_cpu_ms", true)
		if result.Status != test.status || result.PairCount != 30 || len(result.RawBatches[0]) != 30 {
			t.Fatalf("zero -> %g: %+v", test.candidate, result)
		}
		for _, row := range result.Rows {
			if row.Forward.Ratio != nil || !row.Forward.PointZeroDenominator || row.Forward.ZeroDenominatorDraws != 20000 || row.Forward.DecisiveRoute != "absolute-only" {
				t.Fatalf("undefined forward ratio must remain absolute-only: %+v", row)
			}
			if row.Reverse.Gate.Status != criterionMet {
				t.Fatal("reverse zero-cost direction must pass")
			}
		}
	}

	pairs := append(resourceTestConstant(8, 31.25, 31.25), resourceTestConstant(7, 0, 0)...)
	result := resourcePhaseStatistics([][]pair{pairs}, "job_cpu_ms", true)
	row := result.Rows["median"]
	if row.Forward.PointZeroDenominator || row.Forward.Ratio != nil || row.Forward.ZeroDenominatorDraws != 7981 || row.Reverse.ZeroDenominatorDraws != 7981 || row.Status != criterionMet {
		t.Fatalf("R3 positive point / zero bootstrap denominators: %+v", row)
	}
	resourceTestEstimate(t, row.Forward.Difference, 0, 0, 0)
	if result.Rows["mean"].Forward.Ratio == nil {
		t.Fatal("median zero fallback must not incorrectly suppress the independent mean row")
	}
}

func TestResourcePracticalBudgetsAndDescriptions(t *testing.T) {
	for _, blocks := range []int{15, 30} {
		result := resourcePhaseStatistics([][]pair{resourceTestConstant(blocks, 62.5, 78.125)}, "job_cpu_ms", false)
		if result.Status != criterionMet {
			t.Fatalf("quantum equality must pass per invocation for %d pairs", 2*blocks)
		}
		closeTo(t, *result.Candidate.Total-*result.Baseline.Total, float64(2*blocks)*15.625)
		for _, row := range result.Rows {
			if row.Forward.DecisiveRoute != "absolute" {
				t.Fatalf("25%% added CPU passes through the explicit absolute allowance: %+v", row)
			}
			resourceTestEstimate(t, *row.Forward.Ratio, 1.25, 1.25, 1.25)
			resourceTestEstimate(t, *row.Forward.PercentageChange, 25, 25, 25)
			resourceTestEstimate(t, row.Forward.Difference, 15.625, 15.625, 15.625)
		}
	}

	for _, test := range []struct {
		metric, unit string
		allowance    float64
	}{
		{"read_operations", "operations", 16}, {"write_operations", "operations", 16}, {"other_operations", "operations", 16},
		{"read_bytes", "bytes", 65536}, {"write_bytes", "bytes", 4096}, {"other_bytes", "bytes", 65536},
	} {
		result := resourcePhaseStatistics([][]pair{resourceTestConstant(1, 0, test.allowance)}, test.metric, false)
		if result.Unit != test.unit || result.Status != criterionMet || len(result.Rows) != 2 {
			t.Fatalf("wrong metric unit or gates for %s: %+v", test.metric, result)
		}
		for _, name := range []string{"mean", "median"} {
			row := result.Rows[name]
			if row.RelativeLimit != 1.10 || *row.AbsoluteAllowance != test.allowance {
				t.Fatalf("wrong %s %s limits: %+v", test.metric, name, row)
			}
		}
	}

	peak := resourcePhaseStatistics([][]pair{resourceTestConstant(1, 0, 2*1024*1024)}, "job_peak_bytes", false)
	if peak.Baseline.Total != nil || peak.Candidate.Total != nil || peak.Order["AB"].Baseline.Total != nil || peak.Order["BA"].Candidate.Total != nil {
		t.Fatal("peak committed memory must never be summed")
	}
	if len(peak.Rows) != 2 || *peak.Rows["median"].AbsoluteAllowance != 2*1024*1024 || *peak.Rows["p95"].AbsoluteAllowance != 4*1024*1024 {
		t.Fatalf("peak memory requires separate median/p95 budgets: %+v", peak.Rows)
	}
}

func TestResourceMarginalStatisticsStaySeparateFromElapsedEffects(t *testing.T) {
	var pairs []pair
	for range 15 {
		pairs = append(pairs, pair{10, 8, "AB"}, pair{100, 100, "BA"})
	}
	resource := resourcePhaseStatistics([][]pair{pairs}, "elapsed_ms", true)
	elapsed := phaseElapsedStatistics([][]pair{pairs})
	closeTo(t, resource.Rows["median"].Forward.Ratio.Value, 54.0/55)
	closeTo(t, elapsed.Estimates.PairedRatio.Value, math.Sqrt(0.8))
	closeTo(t, phaseNoise([][]pair{pairs}).RatioEnvelope, 1.25)
	if !reflect.DeepEqual(elapsed, summarize(pairs)) {
		t.Fatal("optional statistics path changed original standalone elapsed statistics")
	}

	// Hand-computable marginal medians and nearest-rank p95; order-stratum
	// descriptions and raw observations remain available without filtering.
	r := resourcePhaseStatistics([][]pair{{{10, 20, "AB"}, {40, 80, "BA"}, {20, 40, "AB"}, {30, 60, "BA"}}}, "read_operations", false)
	if r.Baseline.Mean != 25 || r.Candidate.Median != 50 || r.Baseline.P95 != 40 || r.Candidate.P95 != 80 || r.Baseline.Min != 10 || r.Candidate.Max != 80 || r.Order["AB"].PairCount != 2 || len(r.RawBatches[0]) != 4 {
		t.Fatalf("absolute or order descriptions incorrect: %+v", r)
	}
}

func TestResourceGatesBoundaryAndNoChangeRatioOnly(t *testing.T) {
	allowance := 2.0
	for _, test := range []struct {
		name       string
		ratio      *estimate
		difference estimate
		absolute   *float64
		status     string
	}{
		{"relative equality", &estimate{1.05, interval{1.05, 1.05}}, estimate{3, interval{3, 3}}, &allowance, criterionMet},
		{"absolute equality", &estimate{1.2, interval{1.2, 1.2}}, estimate{2, interval{2, 2}}, &allowance, criterionMet},
		{"both exceed", &estimate{1.2, interval{1.1, 1.3}}, estimate{3, interval{2.1, 4}}, &allowance, criterionNotMet},
		{"ratio crosses", &estimate{1.05, interval{1.01, 1.1}}, estimate{3, interval{3, 3}}, &allowance, inconclusive},
		{"absolute crosses", &estimate{1.2, interval{1.2, 1.2}}, estimate{2, interval{1, 3}}, &allowance, inconclusive},
		{"absolute-only equality", nil, estimate{2, interval{2, 2}}, &allowance, criterionMet},
		{"absolute-only fail", nil, estimate{3, interval{3, 3}}, &allowance, criterionNotMet},
		{"absolute-only uncertain", nil, estimate{2, interval{1, 3}}, &allowance, inconclusive},
		{"ratio-only does not borrow floor", &estimate{1.2, interval{1.2, 1.2}}, estimate{1, interval{1, 1}}, nil, criterionNotMet},
	} {
		t.Run(test.name, func(t *testing.T) {
			d := resourceGate(resourceDirection{Ratio: test.ratio, Difference: test.difference}, 1.05, test.absolute)
			if d.Status != test.status {
				t.Fatalf("got %s want %s: %+v", d.Status, test.status, d)
			}
		})
	}
	elapsed := resourcePhaseStatistics([][]pair{resourceTestConstant(1, 10, 12)}, "elapsed_ms", true)
	row := elapsed.Rows["p95"]
	if row.Status != criterionMet {
		t.Fatal("cold tail is inside 4ms practical allowance")
	}
	reconfigureResourceRow(&row, 1.10, nil)
	if row.Status != criterionNotMet || row.AbsoluteAllowance != nil || row.Forward.Gate.Status != criterionNotMet || row.Reverse.Gate.Status != criterionMet {
		t.Fatalf("no-change p95 requires ratio-only guard in both directions: %+v", row)
	}
	if resourceStatuses(inconclusive, criterionNotMet, criterionMet) != criterionNotMet || resourceStatuses(criterionMet, inconclusive) != inconclusive {
		t.Fatal("a failed required gate dominates inconclusive gates")
	}
}

func TestResourceReversalUsesDrawsBeforeType7Interpolation(t *testing.T) {
	// Independent R3 endpoint arithmetic: exactly 500/20000 draws equal 1,
	// all remaining ratios equal 4. Reciprocation and type7 do not commute.
	draws := make([]resourceMarginals, bootstrapReplicates)
	for i := range draws {
		candidate := 4.0
		if i < 500 {
			candidate = 1
		}
		draws[i] = resourceMarginals{1, candidate}
	}
	forward := resourceDirectionFor(resourceMarginals{1, 4}, draws, false)
	reverse := resourceDirectionFor(resourceMarginals{1, 4}, draws, true)
	closeTo(t, forward.Ratio.CI95.Lower, 3.925)
	// The decimal 0.975 position has a small float64 interpolation error at
	// this 20000-draw boundary; the independently derived endpoint is .26875.
	if math.Abs(reverse.Ratio.CI95.Upper-0.26875) > 2e-12 {
		t.Fatalf("reversed type7 upper = %.17g, want .26875", reverse.Ratio.CI95.Upper)
	}
	if math.Abs(reverse.Ratio.CI95.Upper-1/forward.Ratio.CI95.Lower) < 0.01 {
		t.Fatal("reversed interval was obtained by naive endpoint reciprocals")
	}
}

func TestResourceIndependentR3DrawHashes(t *testing.T) {
	initial := resourceTestConstant(15, 100, 104)
	doubled := resourceTestConstant(30, 100, 104)
	for _, test := range []struct {
		batches [][]pair
		hash    string
	}{
		{[][]pair{initial}, "6b64c39287f8b242124c5ac231912349c17afa4b6fae4d07e641aa5709a45c78"},
		{[][]pair{initial, doubled}, "101923e8da0d033e820746df1927071455989a29646095686c08115d91cd1da5"},
	} {
		noise := phaseNoise(test.batches)
		resource := resourcePhaseStatistics(test.batches, "job_cpu_ms", true)
		if noise.DrawSHA256 != test.hash || resource.DrawSHA256 != test.hash {
			t.Fatalf("wrong replicate/batch/phase RNG order: noise=%s resource=%s want=%s", noise.DrawSHA256, resource.DrawSHA256, test.hash)
		}
	}
	// The original standalone estimator and the stratified elapsed wrapper
	// use the same block stream; compare its joint CI with reviewed vectors.
	joint := phaseElapsedStatistics([][]pair{resourceTestBlocks(1000, resourceTestLevels(10, 5, 1080, 1040)), resourceTestBlocks(1000, resourceTestLevels(7, 23, 1080, 1040))})
	resourceTestEstimate(t, joint.Estimates.PairedRatio, 1.04, 1.04, 1.08)
}

func TestResourceR3BiasedZeroWidthNoiseRetainsCenter(t *testing.T) {
	calibration := resourceTestConstant(15, 100, 104)
	guards := resourcePhaseStatistics([][]pair{calibration}, "elapsed_ms", true)
	if guards.Status != criterionMet {
		t.Fatal("100->104 A/A cold guards pass practical equivalence")
	}
	noise := phaseNoise([][]pair{calibration})
	closeTo(t, noise.N, math.Log(1.04))
	closeTo(t, noise.RatioEnvelope, 1.04)
	closeTo(t, noise.RatioCutoff, 1/1.04)
	for _, name := range []string{"full", "AB", "BA"} {
		resourceTestEstimate(t, noise.Populations[name], math.Log(1.04), math.Log(1.04), math.Log(1.04))
	}
	candidates := append(resourceTestLevels(7, 1, 92, 95), resourceTestLevels(7, 0, 98, 0)...)
	claim := phaseElapsedStatistics([][]pair{resourceTestBlocks(100, candidates)}).Estimates.PairedRatio
	resourceTestEstimate(t, claim, 0.95, 0.92, 0.98)
	if primaryDecision(claim, 0.95).Status != criterionMet || !(claim.CI95.Lower < noise.RatioCutoff && claim.CI95.Upper >= noise.RatioCutoff) {
		t.Fatal("R3-P01 candidate passes original primary but is inconclusive against the biased A/A envelope")
	}
	zero := phaseNoise([][]pair{resourceTestConstant(15, 100, 100)})
	if zero.N != 0 || zero.RatioEnvelope != 1 || zero.RatioCutoff != 1 {
		t.Fatal("exact identity centers/endpoints should retain N=0")
	}
}

func TestResourceNoiseIncludesOrderCancellationAndUncertainty(t *testing.T) {
	var cancel, uncertain []pair
	for i := range 15 {
		cancel = append(cancel, pair{10, 12, "AB"}, pair{12, 10, "BA"})
		candidate := 4.0
		if i < 7 {
			candidate = 3
		} else if i > 7 {
			candidate = 5
		}
		uncertain = append(uncertain, pair{4, candidate, "AB"}, pair{4, 4, "BA"})
	}
	noise := phaseNoise([][]pair{cancel})
	resourceTestEstimate(t, noise.Populations["full"], 0, 0, 0)
	closeTo(t, noise.N, math.Log(1.2))
	closeTo(t, noise.RatioCutoff, 1/1.2)
	closeTo(t, noise.Populations["AB"].Value, math.Log(1.2))
	closeTo(t, noise.Populations["BA"].Value, -math.Log(1.2))
	noise = phaseNoise([][]pair{uncertain})
	resourceTestEstimate(t, noise.Populations["full"], 0, 0, 0)
	resourceTestEstimate(t, noise.Populations["AB"], 0, math.Log(0.75), math.Log(1.25))
	closeTo(t, noise.RatioEnvelope, 4.0/3)
	closeTo(t, noise.RatioCutoff, 0.75)
}

func TestResourceFollowUpCannotEraseInitialNoise(t *testing.T) {
	var initial []pair
	for i := range 15 {
		if i < 10 {
			initial = append(initial, pair{10, 12, "AB"}, pair{12, 10, "BA"})
		} else {
			initial = append(initial, pair{10, 10, "AB"}, pair{10, 10, "BA"})
		}
	}
	doubled := resourceTestConstant(30, 10, 10)
	before, after, joint := phaseNoise([][]pair{initial}), phaseNoise([][]pair{doubled}), phaseNoise([][]pair{initial, doubled})
	closeTo(t, before.N, math.Log(1.2))
	if after.N != 0 || joint.N != 0 {
		t.Fatalf("reviewed later summaries are exact zero: doubled=%g joint=%g", after.N, joint.N)
	}
	// Resource rows establish that this fixture can reach and pass a follow-up.
	cpuInitial := resourceTestBlocks(62.5, resourceTestLevels(1, 14, 250, 62.5))
	cpuDoubled := resourceTestConstant(30, 62.5, 62.5)
	first := resourcePhaseStatistics([][]pair{cpuInitial}, "job_cpu_ms", true)
	second := resourcePhaseStatistics([][]pair{cpuDoubled}, "job_cpu_ms", true)
	combined := resourcePhaseStatistics([][]pair{cpuInitial, cpuDoubled}, "job_cpu_ms", true)
	if first.Status != inconclusive || second.Status != criterionMet || combined.Status != criterionMet {
		t.Fatalf("reviewed symmetric A/A follow-up resource path: %s/%s/%s", first.Status, second.Status, combined.Status)
	}
	resourceTestEstimate(t, first.Rows["mean"].Forward.Difference, 12.5, 0, 37.5)
	resourceTestEstimate(t, combined.Rows["mean"].Forward.Difference, 25.0/6, 0, 12.5)
}
