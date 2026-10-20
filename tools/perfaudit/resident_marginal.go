package main

import (
	"math/big"
	"slices"
)

var residentMetricNames = []string{"cycle_ns", "session_cpu_ms", "peak_job_commit_bytes", "read_operations", "write_operations", "other_operations", "read_bytes", "write_bytes", "other_bytes"}

type residentIntegerValue struct {
	value big.Int
	block int
}

type residentSeries struct {
	values [2][]residentIntegerValue // sorted original observations, never rounded arms
	scale  int64
}

type ResidentArmDistribution struct {
	Count  int      `json:"count"`
	Mean   *big.Rat `json:"mean"`
	Median *big.Rat `json:"median"`
	P95    *big.Rat `json:"p95"`
	Min    *big.Rat `json:"min"`
	Max    *big.Rat `json:"max"`
	Total  *big.Rat `json:"total,omitempty"` // additive quantities only
}

type ResidentSessionDistribution struct {
	SessionID string                  `json:"session_id"`
	Cycles    ResidentArmDistribution `json:"cycles_ns"`
}

type ResidentMetric struct {
	Metric    string                             `json:"metric"`
	Unit      string                             `json:"unit"`
	Reference ResidentArmDistribution            `json:"reference"`
	Candidate ResidentArmDistribution            `json:"candidate"`
	Rows      map[string]resourceStatisticResult `json:"rows"`
	Status    string                             `json:"status"`
}

func residentNativeValue(s ResidentSession, metric string) big.Int {
	var v big.Int
	switch metric {
	case "session_cpu_ms":
		v.SetUint64(*s.Resources.UserTicks)
		v.Add(&v, new(big.Int).SetUint64(*s.Resources.KernelTicks))
	case "peak_job_commit_bytes":
		v.SetUint64(*s.Resources.PeakBytes)
	default:
		index := slices.Index(residentMetricNames[3:], metric)
		v.SetUint64(*s.Resources.IO[index])
	}
	return v
}

func residentSeriesFor(blocks []ResidentBlock, metric string, order int) residentSeries {
	s := residentSeries{scale: 1}
	if metric == "session_cpu_ms" {
		s.scale = 10000
	}
	for b, block := range blocks {
		for j, session := range block.Sessions {
			o, arm := j/2, 0
			if j == 1 || j == 2 {
				arm = 1
			}
			if order != -1 && order != o {
				continue
			}
			if metric == "cycle_ns" {
				for _, cycle := range session.Cycles {
					var v big.Int
					v.SetUint64(cycle.Nanoseconds)
					s.values[arm] = append(s.values[arm], residentIntegerValue{v, b})
				}
			} else {
				s.values[arm] = append(s.values[arm], residentIntegerValue{residentNativeValue(session, metric), b})
			}
		}
	}
	for arm := range 2 {
		slices.SortFunc(s.values[arm], func(a, b residentIntegerValue) int { return a.value.Cmp(&b.value) })
	}
	return s
}

// Sorting original observations once and selecting ranks by block multiplicity
// exactly reproduces expansion of every selected session and every cycle.
func residentWeightedStatistic(values []residentIntegerValue, weights [15]uint8, name string) big.Int {
	var result big.Int
	n := 0
	for _, v := range values {
		n += int(weights[v.block])
		if name == "mean" && weights[v.block] > 0 {
			result.Add(&result, new(big.Int).Mul(&v.value, big.NewInt(int64(weights[v.block]))))
		}
	}
	if name == "mean" {
		return result
	}
	left, right := (n-1)/2, n/2
	if name == "p95" {
		left, right = (95*n+99)/100-1, (95*n+99)/100-1
	}
	cumulative := 0
	for _, v := range values {
		previous := cumulative
		cumulative += int(weights[v.block])
		if previous <= left && cumulative > left {
			result.Add(&result, &v.value)
		}
		if previous <= right && cumulative > right {
			result.Add(&result, &v.value)
			break
		}
	}
	return result // twice the named marginal statistic, including even halves
}

func (s residentSeries) samples(plan residentDrawPlan, name string) *resourceExactSamples {
	var ones [15]uint8
	for _, values := range s.values {
		for _, v := range values {
			ones[v.block] = 1
		}
	}
	d := &resourceExactSamples{draws: make([]resourceMeanTotals, len(plan.weights))}
	denominator := 2 * s.scale
	if name == "mean" {
		denominator = int64(len(s.values[0])) * s.scale
	}
	d.denominator.SetInt64(denominator)
	estimate := func(weights [15]uint8) resourceMeanTotals {
		return resourceMeanTotals{residentWeightedStatistic(s.values[0], weights, name), residentWeightedStatistic(s.values[1], weights, name)}
	}
	d.point = estimate(ones)
	cache := map[[15]uint8]resourceMeanTotals{}
	for i, weights := range plan.weights {
		v, ok := cache[weights]
		if !ok {
			v = estimate(weights)
			cache[weights] = v
		}
		d.draws[i] = v // immutable big.Int backing after insertion
	}
	return d
}

func residentDistribution(values []residentIntegerValue, scale int64, additive bool) ResidentArmDistribution {
	var ones [15]uint8
	for _, v := range values {
		ones[v.block] = 1
	}
	mean := residentWeightedStatistic(values, ones, "mean")
	median := residentWeightedStatistic(values, ones, "median")
	tail := residentWeightedStatistic(values, ones, "p95")
	result := ResidentArmDistribution{
		Count: len(values), Mean: new(big.Rat).SetFrac(&mean, big.NewInt(int64(len(values))*scale)),
		Median: new(big.Rat).SetFrac(&median, big.NewInt(2*scale)), P95: new(big.Rat).SetFrac(&tail, big.NewInt(2*scale)),
		Min: new(big.Rat).SetFrac(&values[0].value, big.NewInt(scale)), Max: new(big.Rat).SetFrac(&values[len(values)-1].value, big.NewInt(scale)),
	}
	if additive {
		result.Total = new(big.Rat).SetFrac(&mean, big.NewInt(scale))
	}
	return result
}

func residentSessionDistributions(blocks []ResidentBlock) []ResidentSessionDistribution {
	var result []ResidentSessionDistribution
	for _, block := range blocks {
		for _, s := range block.Sessions {
			values := make([]residentIntegerValue, len(s.Cycles))
			for i, c := range s.Cycles {
				values[i].value.SetUint64(c.Nanoseconds)
			}
			slices.SortFunc(values, func(a, b residentIntegerValue) int { return a.value.Cmp(&b.value) })
			result = append(result, ResidentSessionDistribution{s.ID, residentDistribution(values, 1, true)})
		}
	}
	return result
}

func residentGuard(metric, name string, key ResidentKey) (relative float64, absolute *float64) {
	relative, allowance := 1.10, 16.0
	switch metric {
	case "cycle_ns":
		relative, allowance = 1.05, 50000
		if name == "p95" {
			relative, allowance = 1.10, 100000
			if key.ratioOnlyTail() {
				return relative, nil
			}
		}
	case "session_cpu_ms":
		relative, allowance = 1.05, 15.625
	case "peak_job_commit_bytes":
		allowance = 2 * 1024 * 1024
		if name == "p95" {
			allowance *= 2
		}
	case "read_bytes", "other_bytes":
		allowance = 64 * 1024
	case "write_bytes":
		allowance = 4 * 1024
	}
	return relative, &allowance
}

func residentMetricFor(blocks []ResidentBlock, plan residentDrawPlan, order int, metric string, key ResidentKey, symmetric bool) ResidentMetric {
	s := residentSeriesFor(blocks, metric, order)
	names := []string{"mean", "median"}
	var unit string
	if metric == "cycle_ns" || metric == "peak_job_commit_bytes" {
		names = []string{"median", "p95"}
	}
	switch metric {
	case "cycle_ns":
		unit = "ns/cycle"
	case "session_cpu_ms":
		unit = "ms/session"
	case "peak_job_commit_bytes", "read_bytes", "write_bytes", "other_bytes":
		unit = "bytes/session"
	default:
		unit = "operations/session"
	}
	r := ResidentMetric{Metric: metric, Unit: unit, Reference: residentDistribution(s.values[0], s.scale, metric != "peak_job_commit_bytes"), Candidate: residentDistribution(s.values[1], s.scale, metric != "peak_job_commit_bytes"), Rows: map[string]resourceStatisticResult{}, Status: criterionMet}
	for _, name := range names {
		draws := s.samples(plan, name)
		forward := draws.direction(false)
		if name != "mean" {
			forward.ExactMarginal, forward.ExactMean = forward.ExactMean, nil
		}
		// A reverse distribution is computed from every original draw before
		// quantiles. Reciprocal-of-type7-endpoints is not equivalent.
		reverse := draws.direction(true)
		if name != "mean" {
			reverse.ExactMarginal, reverse.ExactMean = reverse.ExactMean, nil
		}
		row := resourceStatisticResult{Statistic: name, Forward: forward, Reverse: &reverse}
		relative, absolute := residentGuard(metric, name, key)
		reconfigureResourceRow(&row, relative, absolute)
		if !symmetric {
			row.Status = row.Forward.Gate.Status
		}
		if key.Role == residentCost && !symmetric {
			row.Status = residentCost
			row.Forward.Gate = decision{Status: residentCost, Rule: "descriptive required semantic cost", Message: "root cost adjudication required"}
			row.Reverse.Gate = row.Forward.Gate
		}
		r.Rows[name] = row
		r.Status = resourceStatuses(r.Status, row.Status)
	}
	return r
}
