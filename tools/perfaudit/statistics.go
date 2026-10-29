package main

import (
	"math"
	"slices"
)

const (
	bootstrapSeed       uint64 = 20261018
	bootstrapReplicates int    = 20000
)

type interval struct {
	Lower float64 `json:"lower"`
	Upper float64 `json:"upper"`
}

type estimate struct {
	Value float64  `json:"value"`
	CI95  interval `json:"ci95"`
}

type metrics struct {
	BaselineMedianMS   float64 `json:"baseline_median_ms"`
	CandidateMedianMS  float64 `json:"candidate_median_ms"`
	BaselineP95MS      float64 `json:"baseline_p95_ms"`
	CandidateP95MS     float64 `json:"candidate_p95_ms"`
	PairedDifferenceMS float64 `json:"median_paired_difference_ms"`
	PairedLogRatio     float64 `json:"median_paired_log_ratio"`
	PairedRatio        float64 `json:"paired_ratio"`
	P95Ratio           float64 `json:"p95_ratio"`
}

type estimates struct {
	BaselineMedianMS   estimate `json:"baseline_median_ms"`
	CandidateMedianMS  estimate `json:"candidate_median_ms"`
	BaselineP95MS      estimate `json:"baseline_p95_ms"`
	CandidateP95MS     estimate `json:"candidate_p95_ms"`
	PairedDifferenceMS estimate `json:"median_paired_difference_ms"`
	PairedLogRatio     estimate `json:"median_paired_log_ratio"`
	PairedRatio        estimate `json:"paired_ratio"`
	P95Ratio           estimate `json:"p95_ratio"`
}

type stratum struct {
	PairCount int     `json:"pair_count"`
	Metrics   metrics `json:"metrics"`
}

type phaseStatistics struct {
	PairCount  int                `json:"pair_count"`
	BlockCount int                `json:"block_count"`
	Estimates  estimates          `json:"estimates"`
	Order      map[string]stratum `json:"order_strata"`
}

func median(values []float64) float64 {
	ordered := slices.Clone(values)
	if ordered == nil {
		ordered = []float64{}
	}
	slices.Sort(ordered)
	m := len(ordered) / 2
	if len(ordered)%2 == 1 {
		return ordered[m]
	}
	return ordered[m-1]/2 + ordered[m]/2
}

func p95(values []float64) float64 {
	ordered := slices.Clone(values)
	if ordered == nil {
		ordered = []float64{}
	}
	slices.Sort(ordered)
	return ordered[(95*len(ordered)+99)/100-1]
}

func measure(pairs []pair) metrics {
	baseline := make([]float64, len(pairs))
	candidate := make([]float64, len(pairs))
	differences := make([]float64, len(pairs))
	logs := make([]float64, len(pairs))
	for i, p := range pairs {
		baseline[i], candidate[i] = p.baseline, p.candidate
		differences[i] = p.candidate - p.baseline
		logs[i] = math.Log(p.candidate / p.baseline)
	}
	logRatio := median(logs)
	return metrics{
		median(baseline), median(candidate), p95(baseline), p95(candidate),
		median(differences), logRatio, math.Exp(logRatio), p95(candidate) / p95(baseline),
	}
}

// SplitMix64's explicit uint64 operations make the stream independent of the
// Go standard library RNG version. Uniform block selection rejects modulo bias.
type splitMix64 struct{ state uint64 }

func (r *splitMix64) next() uint64 {
	r.state += 0x9e3779b97f4a7c15
	z := r.state
	z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9
	z = (z ^ (z >> 27)) * 0x94d049bb133111eb
	return z ^ (z >> 31)
}

func (r *splitMix64) index(n int) int {
	bound := uint64(n)
	threshold := -bound % bound
	for {
		x := r.next()
		if x >= threshold {
			return int(x % bound)
		}
	}
}

func sampleBlocks(pairs, sample []pair, rng *splitMix64) {
	for i := 0; i < len(pairs); i += 2 {
		start := 2 * rng.index(len(pairs)/2)
		copy(sample[i:i+2], pairs[start:start+2])
	}
}

// percentile interpolates the sorted zero-based position (N-1)*p linearly
// (Hyndman-Fan type 7). The ratio CI is exp of the log-median CI, preserving
// inversion under arm reversal even when an endpoint needs interpolation.
func percentile(ordered []float64, p float64) float64 {
	h := float64(len(ordered)-1) * p
	i := int(h)
	if i == len(ordered)-1 {
		return ordered[i]
	}
	return ordered[i] + (h-float64(i))*(ordered[i+1]-ordered[i])
}

func summarize(pairs []pair) phaseStatistics {
	rng := splitMix64{bootstrapSeed} // Restart identically for each phase.
	sample := make([]pair, len(pairs))
	draws := make([]metrics, bootstrapReplicates)
	for i := range draws {
		sampleBlocks(pairs, sample, &rng)
		draws[i] = measure(sample)
	}
	point := measure(pairs)
	result := phaseStatistics{
		PairCount: len(pairs), BlockCount: len(pairs) / 2,
		Order: map[string]stratum{},
	}
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
		slices.Sort(values)
		return estimate{get(point), interval{percentile(values, 0.025), percentile(values, 0.975)}}
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
