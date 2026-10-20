package main

import (
	"encoding/json"
	"fmt"
	"math"
	"math/big"
	"testing"
)

func residentTestExactDirection(ratio, difference *big.Rat) resourceDirection {
	e := resourceRationalEstimate{Value: difference, CI95: resourceRationalInterval{Lower: difference, Upper: difference}}
	d := resourceDirection{ExactMarginal: &resourceExactMarginal{Difference: e}}
	if ratio != nil {
		r := resourceRationalEstimate{Value: ratio, CI95: resourceRationalInterval{Lower: ratio, Upper: ratio}}
		d.ExactMarginal.Ratio = &r
	}
	return d
}

func TestResidentAllExactRoutes(t *testing.T) {
	// Direct endpoint discriminators are not purported block-generated data.
	// Tiny rational neighbors intentionally collide in their float displays.
	step := new(big.Rat).SetFrac(big.NewInt(1), new(big.Int).Lsh(big.NewInt(1), 100))
	for _, metric := range residentMetricNames {
		for _, name := range []string{"mean", "median", "p95"} {
			if ((metric == "cycle_ns" || metric == "peak_job_commit_bytes") && name == "mean") ||
				(metric != "cycle_ns" && metric != "peak_job_commit_bytes" && name == "p95") {
				continue
			}
			relative, absolute := residentGuard(metric, name, residentTestKey())
			for _, route := range []string{"relative", "absolute"} {
				for _, offset := range []int64{-1, 0, 1} {
					t.Run(fmt.Sprintf("%s/%s/%s/%d", metric, name, route, offset), func(t *testing.T) {
						r, d := resourceDecimal(relative), resourceDecimal(*absolute)
						delta := new(big.Rat).Mul(step, big.NewRat(offset, 1))
						if route == "relative" {
							r.Add(r, delta)
							d.Add(d, big.NewRat(1, 1))
						} else {
							d.Add(d, delta)
							r.Add(r, big.NewRat(1, 1))
						}
						direction := residentTestExactDirection(r, d)
						want := criterionMet
						if offset > 0 {
							want = criterionNotMet
						}
						if resourceGate(direction, relative, absolute).Status != want {
							t.Fatal("exact inclusive route/neighbor changed")
						}
						// The exact payload remains authoritative after JSON roundtrip,
						// including deliberately contrary display fields/availability.
						direction.Difference = estimate{Value: 1e300, CI95: interval{-1e300, 1e300}}
						direction.Ratio = &estimate{Value: 0, CI95: interval{0, 0}}
						encoded, err := json.Marshal(direction)
						if err != nil {
							t.Fatal(err)
						}
						var roundtrip resourceDirection
						if err := json.Unmarshal(encoded, &roundtrip); err != nil {
							t.Fatal(err)
						}
						if resourceGate(roundtrip, relative, absolute).Status != want {
							t.Fatal("poisoned displays decided control")
						}
						// A lower endpoint at equality cannot fail when the upper
						// misses both routes; its immediately higher neighbor can.
						if offset >= 0 {
							payload := direction.ExactMarginal
							payload.Ratio.CI95.Upper = new(big.Rat).Add(payload.Ratio.CI95.Upper, big.NewRat(1, 1))
							payload.Difference.CI95.Upper = new(big.Rat).Add(payload.Difference.CI95.Upper, big.NewRat(1, 1))
							lowerWant := inconclusive
							if offset > 0 {
								lowerWant = criterionNotMet
							}
							if resourceGate(direction, relative, absolute).Status != lowerWant {
								t.Fatal("strict lower endpoint equality/neighbor changed")
							}
						}
					})
				}
			}
		}
	}
}

func TestResidentNativeCPUTickNeighbors(t *testing.T) {
	for _, offset := range []int64{-1, 0, 1} {
		b := residentInventedBatch(fmt.Sprintf("CPU-%d", offset), "initial", "comparison", 1, residentTestKey(), 100, 100)
		for i := range b.Blocks {
			for _, j := range []int{1, 2} {
				b.Blocks[i].Sessions[j].Resources.UserTicks = residentTestUint(uint64(781251 + offset))
			}
		}
		m := residentMetricFor(b.Blocks, residentPlan([]int{5}), -1, "session_cpu_ms", b.Key, false)
		want := criterionMet
		if offset > 0 {
			want = criterionNotMet
		}
		for _, name := range []string{"mean", "median"} {
			r := m.Rows[name]
			if r.Status != want {
				t.Fatalf("CPU %s offset %d: %s", name, offset, r.Status)
			}
			residentAssertRat(t, r.Forward.exactMarginal().Difference.CI95.Upper, new(big.Rat).SetFrac(big.NewInt(156250+offset), big.NewInt(10000)).RatString())
			if offset == 0 && r.Forward.DecisiveRoute != "absolute" {
				t.Fatal("125/8ms equality must use absolute route")
			}
		}
	}
}

func TestResidentZeroRowsAndCostlySession(t *testing.T) {
	plan := residentPlan([]int{5})
	for _, mixed := range []bool{false, true} {
		b := residentInventedBatch(fmt.Sprintf("zero-%t", mixed), "initial", "comparison", 1, residentTestKey(), 100, 100)
		if mixed {
			for i := 2; i < 5; i++ {
				for j := range 4 {
					b.Blocks[i].Sessions[j].Resources.IO[0] = residentTestUint(100)
				}
			}
		}
		m := residentMetricFor(b.Blocks, plan, -1, "read_operations", b.Key, true)
		for _, row := range m.Rows {
			for _, d := range []resourceDirection{row.Forward, *row.Reverse} {
				if d.Ratio != nil || d.exactMarginal().Ratio != nil || d.ZeroDenominatorDraws == 0 || d.Gate.Status != criterionMet || d.Difference.CI95.Lower != 0 || d.Difference.CI95.Upper != 0 {
					t.Fatal("whole ratio suppression/difference draw retention failed")
				}
				if !mixed && (d.ZeroDenominatorDraws != 20000 || !d.PointZeroDenominator) {
					t.Fatal("all-zero row count changed")
				}
				if mixed && d.PointZeroDenominator {
					t.Fatal("mixed positive point should stay positive despite suppressed ratio")
				}
			}
		}
	}
	// Independent policy witness: one of ten sessions adds 1000ms CPU.
	b := residentInventedBatch("spike", "initial", "comparison", 1, residentTestKey(), 100, 100)
	for i := range b.Blocks {
		for j := range 4 {
			b.Blocks[i].Sessions[j].Resources.UserTicks = residentTestUint(625000)
		}
	}
	b.Blocks[0].Sessions[1].Resources.UserTicks = residentTestUint(10625000)
	m := residentMetricFor(b.Blocks, plan, -1, "session_cpu_ms", b.Key, false)
	if m.Rows["median"].Status != criterionMet || m.Rows["mean"].Status != inconclusive || m.Status != inconclusive {
		t.Fatalf("one costly CPU session omitted: %s", m.Status)
	}
	residentAssertRat(t, m.Candidate.Total, "1625")
	residentAssertRat(t, m.Reference.Total, "625")
	for i := range b.Blocks {
		for j := range 4 {
			b.Blocks[i].Sessions[j].Resources.IO[0] = residentTestUint(100)
		}
	}
	b.Blocks[0].Sessions[1].Resources.IO[0] = residentTestUint(1700)
	m = residentMetricFor(b.Blocks, plan, -1, "read_operations", b.Key, false)
	if m.Rows["median"].Status != criterionMet || m.Rows["mean"].Status != inconclusive {
		t.Fatal("one costly IO session omitted")
	}
	b.Blocks[0].Sessions[1].Resources.PeakBytes = residentTestUint(1024 + 4*1024*1024 + 1)
	m = residentMetricFor(b.Blocks, plan, -1, "peak_job_commit_bytes", b.Key, false)
	if m.Rows["median"].Status != criterionMet || m.Rows["p95"].Status != inconclusive || m.Candidate.Total != nil {
		t.Fatal("one session peak was collapsed or summed")
	}
}

func TestResidentRatioOnlyNoChangeTail(t *testing.T) {
	for _, scenario := range []string{"Rewrite_L32F32", "Invalidate_L32F32", "EmptyDrain_L32F32", "MixedInvalidate_M0", "MixedInvalidate_M1"} {
		key := ResidentKey{"B:C", scenario, "C1B1", "control"}
		relative, absolute := residentGuard("cycle_ns", "p95", key)
		if absolute != nil {
			t.Fatal("no-change tail gained an absolute route")
		}
		for _, test := range []struct {
			ratio *big.Rat
			want  string
		}{{big.NewRat(11, 10), criterionMet}, {big.NewRat(11000001, 10000000), criterionNotMet}, {big.NewRat(6, 5), criterionNotMet}} {
			d := residentTestExactDirection(test.ratio, big.NewRat(20000, 1))
			if resourceGate(d, relative, absolute).Status != test.want {
				t.Fatal("no-change ratio-only threshold changed")
			}
		}
	}
}

func TestResidentType7ExactDifferenceAndReverse(t *testing.T) {
	// Difference interpolation is performed before any display rounding. These
	// arm values exceed 2^53: separately rounded subtraction gives zero, not one.
	s := &resourceExactSamples{draws: make([]resourceMeanTotals, 20000)}
	s.denominator.SetInt64(1)
	s.point.baseline.SetUint64(9007199254740992)
	s.point.candidate.SetUint64(9007199254740993)
	for i := range s.draws {
		s.draws[i].baseline.SetUint64(9007199254740992)
		s.draws[i].candidate.SetUint64(9007199254740993)
	}
	d := s.direction(false)
	residentAssertRat(t, d.ExactMean.Difference.Value, "1")
	residentAssertRat(t, d.ExactMean.Difference.CI95.Lower, "1")
	// The independent type7 vector has 500 values 1, and 19500 values 4.
	// Reverse upper=43/160 differs from reciprocal forward lower=40/157.
	s.point.baseline.SetInt64(1)
	s.point.candidate.SetInt64(4)
	for i := range s.draws {
		s.draws[i].baseline.SetInt64(1)
		s.draws[i].candidate.SetInt64(4)
		if i < 500 {
			s.draws[i].candidate.SetInt64(1)
		}
	}
	forward, reverse := s.direction(false), s.direction(true)
	residentAssertRat(t, forward.ExactMean.Ratio.CI95.Lower, "157/40")
	residentAssertRat(t, reverse.ExactMean.Ratio.CI95.Upper, "43/160")
	if math.Abs(reverse.Ratio.CI95.Upper-1/forward.Ratio.CI95.Lower) < .01 {
		t.Fatal("reverse endpoints incorrectly inverted")
	}
	// Independently prescribed differences -23 (500), +17 (19500).
	for i := range s.draws {
		s.draws[i].baseline.SetInt64(30)
		s.draws[i].candidate.SetInt64(47)
		if i < 500 {
			s.draws[i].candidate.SetInt64(7)
		}
	}
	s.point.baseline.SetInt64(30)
	s.point.candidate.SetInt64(47)
	d = s.direction(false)
	residentAssertRat(t, d.ExactMean.Difference.CI95.Lower, "16")
	allowance := 16.0
	if resourceGate(d, 1.1, &allowance).Status != inconclusive {
		t.Fatal("type7 equality must remain inconclusive, not fail")
	}
}
