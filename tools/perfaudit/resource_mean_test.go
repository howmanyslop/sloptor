package main

import (
	"encoding/json"
	"math"
	"math/big"
	"testing"
)

func exactMeanFixture(metric string, blocks int, a, c uint64) ([]pair, []resourceCounterPair) {
	scale := 1.0
	if metric == "job_cpu_ms" {
		scale = 10000
	}
	values := make([]pair, 0, 2*blocks)
	counters := make([]resourceCounterPair, 0, 2*blocks)
	for i := range 2 * blocks {
		order := "AB"
		if i%2 == 1 {
			order = "BA"
		}
		values = append(values, pair{float64(a) / scale, float64(c) / scale, order})
		counters = append(counters, resourceCounterPair{a, c})
	}
	return values, counters
}

func TestResourceExactMeanAbsoluteNeighbors(t *testing.T) {
	for _, tc := range []struct {
		metric    string
		allowance uint64
		scale     int64
	}{
		{"job_cpu_ms", 156250, 10000},
		{"read_operations", 16, 1},
		{"write_operations", 16, 1},
		{"other_operations", 16, 1},
		{"read_bytes", 65536, 1},
		{"write_bytes", 4096, 1},
		{"other_bytes", 65536, 1},
	} {
		t.Run(tc.metric, func(t *testing.T) {
			for _, offset := range []int64{-1, 0, 1} {
				a := uint64(1)
				if tc.metric == "job_cpu_ms" {
					a = 625001
				}
				increase := uint64(int64(tc.allowance) + offset)
				values, counters := exactMeanFixture(tc.metric, 2, a, a+increase)
				r := resourcePhaseStatistics([][]pair{values}, tc.metric, true, [][]resourceCounterPair{counters})
				row := r.Rows["mean"]
				want := criterionMet
				if offset > 0 {
					want = criterionNotMet
				}
				if row.Status != want || row.Forward.Gate.Status != want || row.Reverse.Gate.Status != criterionMet {
					t.Fatalf("offset %d: %+v", offset, row)
				}
				expected := new(big.Rat).SetFrac(new(big.Int).SetUint64(increase), big.NewInt(tc.scale))
				exact := row.Forward.ExactMean.Difference
				if exact.Value.Cmp(expected) != 0 || exact.CI95.Lower.Cmp(expected) != 0 || exact.CI95.Upper.Cmp(expected) != 0 {
					t.Fatalf("rounded a constant exact difference: %+v", exact)
				}
				if offset == 0 && row.Forward.DecisiveRoute != "absolute" {
					t.Fatal("exact inclusive boundary must pass through absolute route")
				}
			}
			for _, offset := range []uint64{0, 1} {
				v, c := exactMeanFixture(tc.metric, 1, 0, tc.allowance+offset)
				row := resourcePhaseStatistics([][]pair{v}, tc.metric, true, [][]resourceCounterPair{c}).Rows["mean"]
				want := criterionMet
				if offset > 0 {
					want = criterionNotMet
				}
				if row.Status != want || row.Forward.Ratio != nil || row.Forward.ExactMean.Ratio != nil || row.Forward.ZeroDenominatorDraws != bootstrapReplicates || row.Forward.DecisiveRoute != "absolute-only" {
					t.Fatalf("zero denominator offset %d: %+v", offset, row)
				}
			}
		})
	}
}

func TestResourceExactMeanRelativeNeighborsAndLargeSums(t *testing.T) {
	for _, metric := range resourceMetricNames {
		if metric == "job_peak_bytes" {
			continue
		}
		t.Run(metric, func(t *testing.T) {
			numerator, denominator := uint64(11), uint64(10)
			if metric == "job_cpu_ms" {
				numerator, denominator = 21, 20
			}
			unit := uint64(math.MaxUint64) / numerator
			a, atBoundary := unit*denominator, unit*numerator
			for _, offset := range []int64{-1, 0, 1} {
				var c uint64 // Keep additions in uint64 without signed overflow.
				if offset < 0 {
					c = atBoundary - 1
				} else {
					c = atBoundary + uint64(offset)
				}
				v, raw := exactMeanFixture(metric, 15, a, c)
				r := resourcePhaseStatistics([][]pair{v}, metric, true, [][]resourceCounterPair{raw})
				row := r.Rows["mean"]
				want := criterionMet
				if offset > 0 {
					want = criterionNotMet
				}
				if row.Status != want {
					t.Fatalf("one-unit relative neighbor %d: %s (%s)", offset, row.Status, row.Forward.ExactMean.Ratio.Value)
				}
				if offset == 0 && row.Forward.DecisiveRoute != "relative" {
					t.Fatalf("equality did not use relative route: %s", row.Forward.DecisiveRoute)
				}
				// Each arm total exceeds uint64. Equal counts cancel exactly in
				// the ratio, even when float64 cannot display the one-unit change.
				expected := new(big.Rat).SetFrac(new(big.Int).SetUint64(c), new(big.Int).SetUint64(a))
				if row.Forward.ExactMean.Ratio.CI95.Upper.Cmp(expected) != 0 {
					t.Fatal("large sums lost integer precision")
				}
				v, raw = exactMeanFixture(metric, 1, c, a)
				reversed := resourcePhaseStatistics([][]pair{v}, metric, true, [][]resourceCounterPair{raw}).Rows["mean"]
				if reversed.Reverse.Gate.Status != want || reversed.Forward.Gate.Status != criterionMet {
					t.Fatal("reversed A/A boundary changed semantics")
				}
			}
		})
	}
}

func TestResourceExactMeanJointPreservesQuantizedAllowance(t *testing.T) {
	var values [][]pair
	var counters [][]resourceCounterPair
	for _, blocks := range []int{15, 30} {
		v, c := exactMeanFixture("job_cpu_ms", blocks, 156250, 312500)
		for i := range 4 {
			v[i].baseline, v[i].candidate = 46.875, 62.5
			c[i] = resourceCounterPair{468750, 625000}
		}
		values = append(values, v)
		counters = append(counters, c)
	}
	r := resourcePhaseStatistics(values, "job_cpu_ms", true, counters)
	row := r.Rows["mean"]
	if row.Status != criterionMet || row.Forward.ExactMean.Difference.CI95.Upper.Cmp(big.NewRat(125, 8)) != 0 || row.Reverse.ExactMean.Difference.CI95.Lower.Cmp(big.NewRat(-125, 8)) != 0 {
		t.Fatalf("joint/reversed exact allowance: %+v", row)
	}
	if r.DrawSHA256 != "101923e8da0d033e820746df1927071455989a29646095686c08115d91cd1da5" {
		t.Fatal("exact means changed the joint sampling stream")
	}
}

func TestResourceExactMeanType7InterpolationAndDisplay(t *testing.T) {
	allowance := 16.0
	for _, sign := range []int64{-1, 0, 1} {
		// At h=19499.025 the upper endpoint is exactly 11/10 when
		// 19500 ratios are 1 and 500 are 5. Adjacent rationals still
		// display as 1.1, but must decide on their actual side of 11/10.
		step := new(big.Rat).SetFrac(big.NewInt(sign), new(big.Int).Lsh(big.NewInt(1), 64))
		high := new(big.Rat).Add(big.NewRat(5, 1), step)
		draws := make([]*big.Rat, bootstrapReplicates)
		for i := range draws {
			draws[i] = big.NewRat(1, 1)
			if i >= 19500 {
				draws[i] = high
			}
		}
		ratio := resourceRationalEstimateFor(big.NewRat(1, 1), draws)
		display := ratio.display()
		if display.CI95.Upper != 1.1 {
			t.Fatal("test no longer discriminates display rounding")
		}
		difference := resourceRationalEstimateFor(big.NewRat(17, 1), []*big.Rat{big.NewRat(17, 1), big.NewRat(17, 1)})
		d := resourceDirection{Ratio: &display, Difference: difference.display(), ExactMean: &resourceExactMean{Ratio: &ratio, Difference: difference}}
		want := criterionMet
		if sign > 0 {
			want = inconclusive
		}
		if got := resourceGate(d, 1.1, &allowance); got.Status != want {
			t.Fatalf("exact percentile neighbor %d: %+v", sign, got)
		}
		encoded, err := json.Marshal(d)
		if err != nil {
			t.Fatal(err)
		}
		var decoded resourceDirection
		if err := json.Unmarshal(encoded, &decoded); err != nil {
			t.Fatal(err)
		}
		if resourceGate(decoded, 1.1, &allowance).Status != want {
			t.Fatal("serialized rational decision cannot be independently reproduced")
		}
	}
}
