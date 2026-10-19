package main

import (
	"math/big"
	"slices"
)

// All source observations share an integer scale. Median numerators are the
// sum of the central two values; p95 numerators are twice the nearest-rank
// observation. A shared denominator of 2*scale preserves even median halves.
// The same exact distribution/percentile/decision code also serves R2 means.
type resourceOrderSamples struct {
	values              []resourceMeanTotals
	baseline, candidate []*big.Int
	statistics          map[string]*resourceExactSamples
}

func newResourceOrderSamples(batches [][]pair, metric string, names []string, counters ...[][]resourceCounterPair) *resourceOrderSamples {
	values, scale := resourceScaledValues(batches, metric, counters...)
	s := &resourceOrderSamples{values: values, statistics: map[string]*resourceExactSamples{}}
	for _, name := range names {
		if name != "mean" {
			d := &resourceExactSamples{draws: make([]resourceMeanTotals, 0, bootstrapReplicates)}
			d.denominator.Mul(scale, big.NewInt(2))
			s.statistics[name] = d
		}
	}
	for i := range values {
		s.baseline = append(s.baseline, &values[i].baseline)
		s.candidate = append(s.candidate, &values[i].candidate)
	}
	s.sort()
	for name, d := range s.statistics {
		d.point = s.marginals(name)
	}
	return s
}

func (s *resourceOrderSamples) sort() {
	compare := func(a, b *big.Int) int { return a.Cmp(b) }
	slices.SortFunc(s.baseline, compare)
	slices.SortFunc(s.candidate, compare)
}

func (s *resourceOrderSamples) marginals(name string) resourceMeanTotals {
	var result resourceMeanTotals
	for arm, values := range [][]*big.Int{s.baseline, s.candidate} {
		n := len(values)
		dst := &result.baseline
		if arm == 1 {
			dst = &result.candidate
		}
		switch name {
		case "median":
			dst.Add(values[(n-1)/2], values[n/2])
		case "p95":
			dst.Lsh(values[(95*n+99)/100-1], 1)
		default:
			panic("unknown exact order statistic: " + name)
		}
	}
	return result
}

func (s *resourceOrderSamples) record(indices []int) {
	for i, index := range indices {
		for offset := range 2 {
			p := &s.values[2*index+offset]
			s.baseline[2*i+offset], s.candidate[2*i+offset] = &p.baseline, &p.candidate
		}
	}
	s.sort()
	for name, d := range s.statistics {
		d.draws = append(d.draws, s.marginals(name))
	}
}

func (s *resourceOrderSamples) direction(name string, reverse bool) resourceDirection {
	d := s.statistics[name].direction(reverse)
	d.ExactMarginal, d.ExactMean = d.ExactMean, nil
	return d
}

func (d resourceDirection) exactMarginal() *resourceExactMarginal {
	if d.ExactMarginal != nil {
		return d.ExactMarginal
	}
	return d.ExactMean
}

func (d resourceDirection) ratioDefined() bool {
	if exact := d.exactMarginal(); exact != nil {
		return exact.Ratio != nil
	}
	return d.Ratio != nil
}
