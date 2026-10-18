package main

import (
	"crypto/sha256"
	"encoding/binary"
	"fmt"
	"math"
	"slices"
)

type resourceRawPair struct {
	Baseline  float64 `json:"baseline"`
	Candidate float64 `json:"candidate"`
	Order     string  `json:"order"`
}

type resourceArmSummary struct {
	Mean   float64  `json:"mean"`
	Median float64  `json:"median"`
	P95    float64  `json:"p95"`
	Min    float64  `json:"min"`
	Max    float64  `json:"max"`
	Total  *float64 `json:"total,omitempty"`
}

type resourceOrderSummary struct {
	PairCount int                `json:"pair_count"`
	Baseline  resourceArmSummary `json:"baseline"`
	Candidate resourceArmSummary `json:"candidate"`
}

type resourceDirection struct {
	Ratio                *estimate          `json:"ratio"`
	PercentageChange     *estimate          `json:"percentage_change"`
	Difference           estimate           `json:"difference"`
	PointZeroDenominator bool               `json:"point_zero_denominator"`
	ZeroDenominatorDraws int                `json:"zero_denominator_draws"`
	Gate                 decision           `json:"gate"`
	DecisiveRoute        string             `json:"decisive_route"`
	ExactMean            *resourceExactMean `json:"exact_mean,omitempty"`
}

type resourceStatisticResult struct {
	Statistic         string             `json:"statistic"`
	RelativeLimit     float64            `json:"relative_limit"`
	AbsoluteAllowance *float64           `json:"absolute_allowance"`
	Forward           resourceDirection  `json:"forward"`
	Reverse           *resourceDirection `json:"reverse,omitempty"`
	Status            string             `json:"status"`
}

type resourceMetricResult struct {
	Metric           string                             `json:"metric"`
	Unit             string                             `json:"unit"`
	PairCount        int                                `json:"pair_count"`
	BlockCount       int                                `json:"block_count"`
	BatchBlockCounts []int                              `json:"batch_block_counts"`
	RawBatches       [][]resourceRawPair                `json:"raw_batches"`
	Baseline         resourceArmSummary                 `json:"baseline"`
	Candidate        resourceArmSummary                 `json:"candidate"`
	Order            map[string]resourceOrderSummary    `json:"order_strata"`
	Rows             map[string]resourceStatisticResult `json:"rows"`
	DrawSHA256       string                             `json:"draw_indices_sha256"`
	Status           string                             `json:"status"`
}

type noiseSummary struct {
	PairCount        int                 `json:"pair_count"`
	BlockCount       int                 `json:"block_count"`
	BatchBlockCounts []int               `json:"batch_block_counts"`
	PopulationCounts map[string]int      `json:"population_pair_counts"`
	Populations      map[string]estimate `json:"log_populations"`
	N                float64             `json:"n"`
	RatioEnvelope    float64             `json:"ratio_envelope"`
	RatioCutoff      float64             `json:"candidate_ratio_cutoff"`
	DrawSHA256       string              `json:"draw_indices_sha256"`
}

// All callers supply admitted, nonempty complete batches. Draws retain both
// pairs of each block and each batch's original count, in chronological order.
// The hash covers local zero-based block indices as little-endian uint32s,
// replicate-major then batch-major, matching the independent policy vectors.
func resourceDraws(batches [][]pair, visit func([]pair), indexVisitors ...func([]int)) string {
	rng := splitMix64{bootstrapSeed}
	sample := make([]pair, len(joinResourceBatches(batches)))
	hash := sha256.New()
	var encoded [4]byte
	indices := make([]int, len(sample)/2)
	for range bootstrapReplicates {
		offset := 0
		for _, batch := range batches {
			for i := 0; i < len(batch); i += 2 {
				index := rng.index(len(batch) / 2)
				indices[(offset+i)/2] = offset/2 + index
				binary.LittleEndian.PutUint32(encoded[:], uint32(index))
				_, _ = hash.Write(encoded[:])
				copy(sample[offset+i:offset+i+2], batch[2*index:2*index+2])
			}
			offset += len(batch)
		}
		for _, visitor := range indexVisitors {
			visitor(indices)
		}
		visit(sample)
	}
	return fmt.Sprintf("%x", hash.Sum(nil))
}

func joinResourceBatches(batches [][]pair) []pair {
	var result []pair
	for _, batch := range batches {
		result = append(result, batch...)
	}
	return result
}

func resourceBatchCounts(batches [][]pair) []int {
	counts := make([]int, len(batches))
	for i, batch := range batches {
		counts[i] = len(batch) / 2
	}
	return counts
}

func resourceArm(values []float64, additive bool) resourceArmSummary {
	ordered := slices.Clone(values)
	slices.Sort(ordered)
	var total float64
	for _, value := range values {
		total += value
	}
	result := resourceArmSummary{
		Mean: total / float64(len(values)), Median: median(ordered), P95: p95(ordered),
		Min: ordered[0], Max: ordered[len(ordered)-1],
	}
	if additive {
		result.Total = &total
	}
	return result
}

func resourceArms(pairs []pair, additive bool) (resourceArmSummary, resourceArmSummary) {
	baseline, candidate := make([]float64, len(pairs)), make([]float64, len(pairs))
	for i, p := range pairs {
		baseline[i], candidate[i] = p.baseline, p.candidate
	}
	return resourceArm(baseline, additive), resourceArm(candidate, additive)
}

func resourceStatistic(arm resourceArmSummary, name string) float64 {
	switch name {
	case "mean":
		return arm.Mean
	case "median":
		return arm.Median
	case "p95":
		return arm.P95
	default:
		panic("unknown resource statistic: " + name)
	}
}

func resourceEstimate(point float64, values []float64) estimate {
	ordered := slices.Clone(values)
	slices.Sort(ordered)
	return estimate{point, interval{percentile(ordered, 0.025), percentile(ordered, 0.975)}}
}

type resourceMarginals struct{ baseline, candidate float64 }

// Ratio availability is a property of the entire row. No zero-denominator
// replicate is removed from the difference distribution or replaced by zero.
func resourceDirectionFor(point resourceMarginals, draws []resourceMarginals, reverse bool) resourceDirection {
	if reverse {
		point = resourceMarginals{point.candidate, point.baseline}
	}
	differences, ratios := make([]float64, len(draws)), make([]float64, len(draws))
	result := resourceDirection{PointZeroDenominator: point.baseline == 0}
	for i, draw := range draws {
		if reverse {
			draw = resourceMarginals{draw.candidate, draw.baseline}
		}
		differences[i] = draw.candidate - draw.baseline
		if draw.baseline == 0 {
			result.ZeroDenominatorDraws++
		} else {
			ratios[i] = draw.candidate / draw.baseline
		}
	}
	result.Difference = resourceEstimate(point.candidate-point.baseline, differences)
	if !result.PointZeroDenominator && result.ZeroDenominatorDraws == 0 {
		ratio := resourceEstimate(point.candidate/point.baseline, ratios)
		result.Ratio = &ratio
		result.PercentageChange = &estimate{
			100 * (ratio.Value - 1),
			interval{100 * (ratio.CI95.Lower - 1), 100 * (ratio.CI95.Upper - 1)},
		}
	}
	return result
}

// A nil allowance means a ratio-only guard, such as no-change elapsed p95.
func resourceGate(direction resourceDirection, relative float64, absolute *float64) decision {
	d := decision{Rule: fmt.Sprintf("marginal ratio CI upper <= %.17g", relative)}
	if absolute != nil {
		d.Rule += fmt.Sprintf(" OR marginal difference CI upper <= %.17g; whole-row absolute-only if any denominator is zero", *absolute)
	}
	ratioPass, absolutePass, ratioFail, absoluteFail := resourceComparisons(direction, relative, absolute)
	switch {
	case ratioPass || absolutePass:
		d.Status, d.Message = criterionMet, "interval is within a declared relative or absolute allowance"
	case direction.Ratio == nil && absolute == nil:
		d.Status, d.Message = inconclusive, "ratio is undefined and this guard has no absolute allowance"
	case direction.Ratio == nil && absoluteFail:
		d.Status, d.Message = criterionNotMet, "absolute-only interval exceeds the allowance"
	case absolute == nil && ratioFail:
		d.Status, d.Message = criterionNotMet, "ratio-only interval exceeds the limit"
	case ratioFail && absoluteFail:
		d.Status, d.Message = criterionNotMet, "both intervals exceed their declared allowances"
	default:
		d.Status, d.Message = inconclusive, "interval uncertainty crosses the declared allowance"
	}
	return d
}

func resourceRoute(direction resourceDirection, relative float64, absolute *float64) string {
	if direction.Ratio == nil {
		if absolute != nil {
			return "absolute-only"
		}
		return "undefined-ratio"
	}
	ratioPass, absolutePass, _, _ := resourceComparisons(direction, relative, absolute)
	switch {
	case ratioPass && absolutePass:
		return "both"
	case ratioPass:
		return "relative"
	case absolutePass:
		return "absolute"
	default:
		return "neither"
	}
}

func reconfigureResourceRow(row *resourceStatisticResult, relative float64, absolute *float64) {
	row.RelativeLimit, row.AbsoluteAllowance = relative, absolute
	row.Forward.Gate = resourceGate(row.Forward, relative, absolute)
	row.Forward.DecisiveRoute = resourceRoute(row.Forward, relative, absolute)
	row.Status = row.Forward.Gate.Status
	if row.Reverse != nil {
		row.Reverse.Gate = resourceGate(*row.Reverse, relative, absolute)
		row.Reverse.DecisiveRoute = resourceRoute(*row.Reverse, relative, absolute)
		row.Status = resourceStatuses(row.Status, row.Reverse.Gate.Status)
	}
}

func resourceStatuses(statuses ...string) string {
	result := criterionMet
	for _, status := range statuses {
		if status == criterionNotMet {
			return criterionNotMet
		}
		if status != criterionMet {
			result = status
		}
	}
	return result
}

// elapsed_ms defaults to the cold control/A/A limits. The caller reapplies the
// no-change p95 ratio-only rule with reconfigureResourceRow before admission.
func resourcePhaseStatistics(batches [][]pair, metric string, symmetric bool, counters ...[][]resourceCounterPair) resourceMetricResult {
	statistics := []string{"mean", "median"}
	unit, relative, allowance := "operations", 1.10, 16.0
	additive := true
	switch metric {
	case "job_cpu_ms":
		unit, relative, allowance = "ms", 1.05, 15.625
	case "job_peak_bytes":
		unit, allowance, additive = "bytes", 2*1024*1024, false
		statistics = []string{"median", "p95"}
	case "read_operations", "write_operations", "other_operations":
	case "read_bytes", "other_bytes":
		unit, allowance = "bytes", 64*1024
	case "write_bytes":
		unit, allowance = "bytes", 4*1024
	case "elapsed_ms":
		unit, relative, allowance = "ms", 1.05, 2
		statistics = []string{"median", "p95"}
	default:
		panic("unknown resource metric: " + metric)
	}
	pairs := joinResourceBatches(batches)
	result := resourceMetricResult{
		Metric: metric, Unit: unit, PairCount: len(pairs), BlockCount: len(pairs) / 2,
		BatchBlockCounts: resourceBatchCounts(batches), RawBatches: make([][]resourceRawPair, len(batches)),
		Order: map[string]resourceOrderSummary{}, Rows: map[string]resourceStatisticResult{},
		Status: criterionMet,
	}
	for i, batch := range batches {
		for _, p := range batch {
			result.RawBatches[i] = append(result.RawBatches[i], resourceRawPair{p.baseline, p.candidate, p.order})
		}
	}
	result.Baseline, result.Candidate = resourceArms(pairs, additive)
	for _, order := range []string{"AB", "BA"} {
		var subset []pair
		for _, p := range pairs {
			if p.order == order {
				subset = append(subset, p)
			}
		}
		baseline, candidate := resourceArms(subset, additive)
		result.Order[order] = resourceOrderSummary{len(subset), baseline, candidate}
	}
	draws := make(map[string][]resourceMarginals, len(statistics))
	for _, name := range statistics {
		draws[name] = make([]resourceMarginals, 0, bootstrapReplicates)
	}
	var exactMean *resourceMeanSamples
	var indexVisitors []func([]int)
	if statistics[0] == "mean" {
		exactMean = newResourceMeanSamples(batches, metric, counters...)
		indexVisitors = append(indexVisitors, exactMean.record)
	}
	result.DrawSHA256 = resourceDraws(batches, func(sample []pair) {
		baseline, candidate := resourceArms(sample, false)
		for _, name := range statistics {
			draws[name] = append(draws[name], resourceMarginals{resourceStatistic(baseline, name), resourceStatistic(candidate, name)})
		}
	}, indexVisitors...)
	for _, name := range statistics {
		point := resourceMarginals{resourceStatistic(result.Baseline, name), resourceStatistic(result.Candidate, name)}
		row := resourceStatisticResult{Statistic: name, Forward: resourceDirectionFor(point, draws[name], false)}
		if name == "mean" {
			row.Forward = exactMean.direction(false)
		}
		if symmetric {
			reversed := resourceDirectionFor(point, draws[name], true)
			if name == "mean" {
				reversed = exactMean.direction(true)
			}
			row.Reverse = &reversed
		}
		rowRelative, rowAllowance := relative, allowance
		if name == "p95" {
			rowRelative = 1.10
			switch metric {
			case "elapsed_ms":
				rowAllowance = 4
			case "job_peak_bytes":
				rowAllowance = 4 * 1024 * 1024
			}
		}
		reconfigureResourceRow(&row, rowRelative, &rowAllowance)
		result.Rows[name] = row
		result.Status = resourceStatuses(result.Status, row.Status)
	}
	return result
}

// This reproduces the original elapsed estimator and classifiers' inputs. Only
// the resampling boundary changes for a declared multi-batch joint summary.
func phaseElapsedStatistics(batches [][]pair) phaseStatistics {
	if len(batches) == 1 {
		return summarize(batches[0])
	}
	pairs := joinResourceBatches(batches)
	draws := make([]metrics, 0, bootstrapReplicates)
	resourceDraws(batches, func(sample []pair) { draws = append(draws, measure(sample)) })
	point := measure(pairs)
	result := phaseStatistics{PairCount: len(pairs), BlockCount: len(pairs) / 2, Order: map[string]stratum{}}
	for _, order := range []string{"AB", "BA"} {
		var subset []pair
		for _, p := range pairs {
			if p.order == order {
				subset = append(subset, p)
			}
		}
		result.Order[order] = stratum{len(subset), measure(subset)}
	}
	ci := func(get func(metrics) float64) estimate {
		values := make([]float64, len(draws))
		for i, draw := range draws {
			values[i] = get(draw)
		}
		return resourceEstimate(get(point), values)
	}
	result.Estimates = estimates{
		BaselineMedianMS:   ci(func(m metrics) float64 { return m.BaselineMedianMS }),
		CandidateMedianMS:  ci(func(m metrics) float64 { return m.CandidateMedianMS }),
		BaselineP95MS:      ci(func(m metrics) float64 { return m.BaselineP95MS }),
		CandidateP95MS:     ci(func(m metrics) float64 { return m.CandidateP95MS }),
		PairedDifferenceMS: ci(func(m metrics) float64 { return m.PairedDifferenceMS }),
		PairedLogRatio:     ci(func(m metrics) float64 { return m.PairedLogRatio }),
		P95Ratio:           ci(func(m metrics) float64 { return m.P95Ratio }),
	}
	log := result.Estimates.PairedLogRatio
	result.Estimates.PairedRatio = estimate{math.Exp(log.Value), interval{math.Exp(log.CI95.Lower), math.Exp(log.CI95.Upper)}}
	return result
}

func phaseNoise(batches [][]pair) noiseSummary {
	pairs := joinResourceBatches(batches)
	result := noiseSummary{
		PairCount: len(pairs), BlockCount: len(pairs) / 2, BatchBlockCounts: resourceBatchCounts(batches),
		PopulationCounts: map[string]int{}, Populations: map[string]estimate{},
	}
	populations := []string{"full", "AB", "BA"}
	logs := func(sample []pair, population string) []float64 {
		values := make([]float64, 0, len(sample))
		for _, p := range sample {
			if population == "full" || p.order == population {
				values = append(values, math.Log(p.candidate/p.baseline))
			}
		}
		return values
	}
	draws := map[string][]float64{}
	for _, population := range populations {
		draws[population] = make([]float64, 0, bootstrapReplicates)
		result.PopulationCounts[population] = len(logs(pairs, population))
	}
	result.DrawSHA256 = resourceDraws(batches, func(sample []pair) {
		for _, population := range populations {
			draws[population] = append(draws[population], median(logs(sample, population)))
		}
	})
	for _, population := range populations {
		summary := resourceEstimate(median(logs(pairs, population)), draws[population])
		result.Populations[population] = summary
		// Width does not enter this definition: a biased zero-width interval
		// retains its nonzero center, including initial/follow-up/joint inputs.
		result.N = math.Max(result.N, math.Abs(summary.Value))
		result.N = math.Max(result.N, math.Abs(summary.CI95.Lower))
		result.N = math.Max(result.N, math.Abs(summary.CI95.Upper))
	}
	result.RatioEnvelope, result.RatioCutoff = math.Exp(result.N), math.Exp(-result.N)
	return result
}
