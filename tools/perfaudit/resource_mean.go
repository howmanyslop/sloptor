package main

import (
	"encoding/json"
	"math/big"
	"slices"
	"strconv"
)

// Rat marshals as a reduced fraction string. These values, not their float64
// display copies, determine marginal gates. Units match the metric row.
type resourceRationalInterval struct {
	Lower *big.Rat `json:"lower"`
	Upper *big.Rat `json:"upper"`
}

type resourceRationalEstimate struct {
	Value *big.Rat                 `json:"value"`
	CI95  resourceRationalInterval `json:"ci95"`
}

type resourceExactMarginal struct {
	Ratio      *resourceRationalEstimate `json:"ratio"`
	Difference resourceRationalEstimate  `json:"difference"`
}

// Retain the mean payload/API introduced in R2; order-statistic rows use the
// same rational representation under their separate exact_marginal field.
type resourceExactMean = resourceExactMarginal

type (
	resourceCounterPair  struct{ baseline, candidate uint64 }
	resourceMeanTotals   struct{ baseline, candidate big.Int }
	resourceExactSamples struct {
		blocks      []resourceMeanTotals
		point       resourceMeanTotals
		draws       []resourceMeanTotals
		denominator big.Int // common denominator for the named statistic's numerators
	}
)

type resourceMeanSamples = resourceExactSamples

func resourceDecimal(value float64) *big.Rat {
	r, ok := new(big.Rat).SetString(strconv.FormatFloat(value, 'g', -1, 64))
	if !ok {
		panic("nonfinite internal resource value")
	}
	return r
}

// Admission has already verified every scored observation and its ordering.
// Keep raw counter integers through the mean seam; CPU scale is applied later.
// In-memory synthetic callers without journals retain the decimal helper API.
func resourceCounterBatches(batches []resourceBatch, metric, phase string) [][]resourceCounterPair {
	key := metric
	if metric == "job_cpu_ms" {
		key = "job_cpu_ticks_100ns"
	}
	result := make([][]resourceCounterPair, len(batches))
	for i, batch := range batches {
		if len(batch.Observations) == 0 {
			return nil
		}
		var observations []resourceObservation
		for _, o := range batch.Observations {
			if o.Scored && o.Event == "finished" && o.Phase == phase {
				observations = append(observations, o)
			}
		}
		count := len(batch.Resources[metric][phase])
		if metric == "elapsed_ms" {
			count = len(batch.Phases[phase])
		}
		if len(observations) != 2*count {
			panic("admitted resource count mismatch")
		}
		for j := 0; j < len(observations); j += 2 {
			a, c := observations[j], observations[j+1]
			if a.Binary == "candidate" {
				a, c = c, a
			}
			av, aok := a.CounterValues[key]
			cv, cok := c.CounterValues[key]
			if metric == "elapsed_ms" {
				// Admission binds these integers to manifest duration_ms and to
				// the collector's normalized DurationMS. ElapsedMS stays raw
				// correspondence evidence, not a substitute fractional estimand.
				get := func(o resourceObservation) (uint64, bool) {
					var result struct{ DurationMS int64 }
					err := json.Unmarshal(o.entry["result"], &result)
					return uint64(result.DurationMS), err == nil && result.DurationMS > 0
				}
				av, aok = get(a)
				cv, cok = get(c)
			}
			if !aok || !cok {
				panic("admitted exact resource counter missing")
			}
			result[i] = append(result[i], resourceCounterPair{av, cv})
		}
	}
	return result
}

// A common integer scale permits allocation-light, overflow-safe block sums.
// Native CPU inputs have scale 10000 and I/O inputs scale 1. Only the synthetic
// float helper path obtains exact rationals from shortest decimal spellings.
func resourceScaledValues(batches [][]pair, metric string, counters ...[][]resourceCounterPair) ([]resourceMeanTotals, *big.Int) {
	var exact [][]resourceCounterPair
	if len(counters) > 0 {
		exact = counters[0]
	}
	var values [][2]*big.Rat
	scale := big.NewInt(1)
	for b, batch := range batches {
		for i, p := range batch {
			var a, c *big.Rat
			if exact != nil {
				unit := int64(1)
				if metric == "job_cpu_ms" {
					unit = 10000
				}
				a = new(big.Rat).SetFrac(new(big.Int).SetUint64(exact[b][i].baseline), big.NewInt(unit))
				c = new(big.Rat).SetFrac(new(big.Int).SetUint64(exact[b][i].candidate), big.NewInt(unit))
			} else {
				a, c = resourceDecimal(p.baseline), resourceDecimal(p.candidate)
			}
			values = append(values, [2]*big.Rat{a, c})
			for _, v := range []*big.Rat{a, c} {
				gcd := new(big.Int).GCD(nil, nil, scale, v.Denom())
				scale.Mul(new(big.Int).Quo(scale, gcd), v.Denom())
			}
		}
	}
	integers := make([]resourceMeanTotals, len(values))
	for i, p := range values {
		for arm, v := range p {
			n := new(big.Int).Mul(v.Num(), new(big.Int).Quo(new(big.Int).Set(scale), v.Denom()))
			dst := &integers[i].baseline
			if arm == 1 {
				dst = &integers[i].candidate
			}
			dst.Set(n)
		}
	}
	return integers, scale
}

func newResourceMeanSamples(batches [][]pair, metric string, counters ...[][]resourceCounterPair) *resourceMeanSamples {
	values, scale := resourceScaledValues(batches, metric, counters...)
	s := &resourceMeanSamples{blocks: make([]resourceMeanTotals, len(values)/2), draws: make([]resourceMeanTotals, 0, bootstrapReplicates)}
	s.denominator.Mul(scale, big.NewInt(int64(len(values))))
	for i := range values {
		s.blocks[i/2].baseline.Add(&s.blocks[i/2].baseline, &values[i].baseline)
		s.blocks[i/2].candidate.Add(&s.blocks[i/2].candidate, &values[i].candidate)
	}
	for i := range s.blocks {
		s.point.baseline.Add(&s.point.baseline, &s.blocks[i].baseline)
		s.point.candidate.Add(&s.point.candidate, &s.blocks[i].candidate)
	}
	return s
}

func (s *resourceMeanSamples) record(indices []int) {
	var draw resourceMeanTotals
	for _, index := range indices {
		draw.baseline.Add(&draw.baseline, &s.blocks[index].baseline)
		draw.candidate.Add(&draw.candidate, &s.blocks[index].candidate)
	}
	s.draws = append(s.draws, draw)
}

// The central type7 probabilities are exactly 1/40 and 39/40. Sorting and
// interpolation remain rational, including ratios and reversed A/A draws.
func resourceRationalPercentile(ordered []*big.Rat, numerator int) *big.Rat {
	h := (len(ordered) - 1) * numerator
	i, remainder := h/40, h%40
	if remainder == 0 {
		return new(big.Rat).Set(ordered[i])
	}
	delta := new(big.Rat).Sub(ordered[i+1], ordered[i])
	return delta.Add(ordered[i], delta.Mul(delta, big.NewRat(int64(remainder), 40)))
}

func resourceRationalEstimateFor(point *big.Rat, draws []*big.Rat) resourceRationalEstimate {
	slices.SortFunc(draws, func(a, b *big.Rat) int { return a.Cmp(b) })
	return resourceRationalEstimate{point, resourceRationalInterval{resourceRationalPercentile(draws, 1), resourceRationalPercentile(draws, 39)}}
}

func (e resourceRationalEstimate) display() estimate {
	value, _ := e.Value.Float64()
	lower, _ := e.CI95.Lower.Float64()
	upper, _ := e.CI95.Upper.Float64()
	return estimate{value, interval{lower, upper}}
}

func (s *resourceMeanSamples) direction(reverse bool) resourceDirection {
	point := s.point
	if reverse {
		point = resourceMeanTotals{point.candidate, point.baseline}
	}
	result := resourceDirection{PointZeroDenominator: point.baseline.Sign() == 0}
	differences, ratios := make([]*big.Rat, len(s.draws)), make([]*big.Rat, len(s.draws))
	for i, draw := range s.draws {
		if reverse {
			draw = resourceMeanTotals{draw.candidate, draw.baseline}
		}
		differences[i] = new(big.Rat).SetFrac(new(big.Int).Sub(&draw.candidate, &draw.baseline), &s.denominator)
		if draw.baseline.Sign() == 0 {
			result.ZeroDenominatorDraws++
		} else {
			ratios[i] = new(big.Rat).SetFrac(&draw.candidate, &draw.baseline)
		}
	}
	difference := resourceRationalEstimateFor(new(big.Rat).SetFrac(new(big.Int).Sub(&point.candidate, &point.baseline), &s.denominator), differences)
	result.ExactMean = &resourceExactMean{Difference: difference}
	result.Difference = difference.display()
	if !result.PointZeroDenominator && result.ZeroDenominatorDraws == 0 {
		ratio := resourceRationalEstimateFor(new(big.Rat).SetFrac(&point.candidate, &point.baseline), ratios)
		result.ExactMean.Ratio = &ratio
		display := ratio.display()
		result.Ratio = &display
		percent := func(v *big.Rat) float64 {
			r := new(big.Rat).Sub(v, big.NewRat(1, 1))
			r.Mul(r, big.NewRat(100, 1))
			f, _ := r.Float64()
			return f
		}
		result.PercentageChange = &estimate{percent(ratio.Value), interval{percent(ratio.CI95.Lower), percent(ratio.CI95.Upper)}}
	}
	return result
}

func resourceComparisons(d resourceDirection, relative float64, absolute *float64) (ratioPass, absolutePass, ratioFail, absoluteFail bool) {
	if e := d.exactMarginal(); e != nil {
		if e.Ratio != nil {
			limit := resourceDecimal(relative)
			ratioPass = e.Ratio.CI95.Upper.Cmp(limit) <= 0
			ratioFail = e.Ratio.CI95.Lower.Cmp(limit) > 0
		}
		if absolute != nil {
			limit := resourceDecimal(*absolute)
			absolutePass = e.Difference.CI95.Upper.Cmp(limit) <= 0
			absoluteFail = e.Difference.CI95.Lower.Cmp(limit) > 0
		}
		return
	}
	ratioPass = d.Ratio != nil && d.Ratio.CI95.Upper <= relative
	ratioFail = d.Ratio != nil && d.Ratio.CI95.Lower > relative
	absolutePass = absolute != nil && d.Difference.CI95.Upper <= *absolute
	absoluteFail = absolute != nil && d.Difference.CI95.Lower > *absolute
	return
}
