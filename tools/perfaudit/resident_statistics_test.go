package main

import (
	"encoding/json"
	"fmt"
	"math"
	"math/big"
	"reflect"
	"slices"
	"testing"
)

func residentTestUint(v uint64) *uint64 { return &v }

// Every test session is invented. No fixture or constructor attests collection.
func residentInventedBatch(id, stage, purpose string, chronology int, key ResidentKey, a, c uint64) ResidentBatch {
	n := 5
	if stage == "follow_up" {
		n = 10
	}
	b := ResidentBatch{Label: residentSynthetic, ID: id, Chronology: chronology, Key: key, Stage: stage, Purpose: purpose}
	for i := range n {
		block := ResidentBlock{Position: i + 1}
		for j := range 4 {
			arm, order, value := "A", "AB", a
			if j >= 2 {
				order = "BA"
			}
			if j == 1 || j == 2 {
				arm, value = "B", c
			}
			semantic := "same-output"
			if key.Role == residentCost && purpose == "comparison" {
				semantic = "expected-stale-B"
			}
			s := ResidentSession{
				ID: fmt.Sprintf("invented/%s/%d/%d", id, i, j), Launch: 4*i + j + 1, Pair: 2*i + j/2 + 1, Arm: arm, Order: order, Semantic: semantic,
				Resources: ResidentResources{UserTicks: residentTestUint(625001), KernelTicks: residentTestUint(0), PeakBytes: residentTestUint(1024)},
			}
			for p := range 30 {
				s.Cycles = append(s.Cycles, ResidentCycle{p + 1, value})
			}
			for k := range s.Resources.IO {
				s.Resources.IO[k] = residentTestUint(0)
			}
			block.Sessions = append(block.Sessions, s)
		}
		b.Blocks = append(b.Blocks, block)
	}
	return b
}

func residentTestKey() ResidentKey { return ResidentKey{"B:C", "Body_L32F32", "C1B1", "control"} }

func residentAssertRat(t *testing.T, value *big.Rat, want string) {
	t.Helper()
	r, ok := new(big.Rat).SetString(want)
	if !ok || value == nil || value.Cmp(r) != 0 {
		t.Fatalf("exact value %v, want %s", value, want)
	}
}

func TestResidentFrozenStreams(t *testing.T) {
	for _, test := range []struct {
		counts []int
		sha    string
		first  []int
	}{
		{[]int{5}, "bb96399f4fd2037561586c5810bdbfdf29247365e318411724768abb5be13e5f", []int{1, 2, 4, 1, 4}},
		{[]int{10}, "4356b3c119decb94dbe7f4954d968af4fd26692079724cf68bb5891f6865a86d", []int{1, 7, 9, 6, 9, 3, 7, 8, 8, 3}},
		{[]int{5, 10}, "c6210df42eb9ec64c8bb054f437d386cd4f01c1969613167ca1c223e5069e5eb", []int{1, 2, 4, 1, 4, 8, 12, 13, 13, 8, 12, 8, 9, 13, 14}},
	} {
		p := residentPlan(test.counts)
		if p.sha != test.sha || !reflect.DeepEqual(p.first[0], test.first) || len(p.weights) != 20000 {
			t.Fatalf("independent policy stream mismatch: %+v", test.counts)
		}
		for _, w := range p.weights {
			offset := 0
			for _, n := range test.counts {
				sum := 0
				for _, count := range w[offset : offset+n] {
					sum += int(count)
				}
				if sum != n {
					t.Fatal("stratum count changed")
				}
				offset += n
			}
		}
	}
}

func TestResidentTypedValidation(t *testing.T) {
	for _, test := range []struct {
		name   string
		mutate func(*ResidentBatch)
		want   string
	}{
		{"missing-block", func(b *ResidentBatch) { b.Blocks = b.Blocks[:4] }, residentIncomplete},
		{"extra-block", func(b *ResidentBatch) { b.Blocks = append(b.Blocks, b.Blocks[0]) }, invalidEvidence},
		{"swapped-launches", func(b *ResidentBatch) {
			b.Blocks[0].Sessions[0], b.Blocks[0].Sessions[1] = b.Blocks[0].Sessions[1], b.Blocks[0].Sessions[0]
		}, invalidEvidence},
		{"pair", func(b *ResidentBatch) { b.Blocks[0].Sessions[2].Pair = 1 }, invalidEvidence},
		{"zero", func(b *ResidentBatch) { b.Blocks[0].Sessions[0].Cycles[0].Nanoseconds = 0 }, invalidEvidence},
		{"position", func(b *ResidentBatch) { b.Blocks[0].Sessions[0].Cycles[0].Position = 2 }, invalidEvidence},
		{"count", func(b *ResidentBatch) { b.Blocks[0].Sessions[0].Cycles = b.Blocks[0].Sessions[0].Cycles[:29] }, invalidEvidence},
		{"missing-CPU", func(b *ResidentBatch) { b.Blocks[0].Sessions[0].Resources.KernelTicks = nil }, residentIncomplete},
		{"missing-IO", func(b *ResidentBatch) { b.Blocks[0].Sessions[0].Resources.IO[5] = nil }, residentIncomplete},
		{"invalid-over-incomplete", func(b *ResidentBatch) {
			b.Blocks[0].Sessions[0].Resources.PeakBytes = nil
			b.Blocks[0].Sessions[0].Cycles[0].Nanoseconds = 0
		}, invalidEvidence},
		{"unexpected-semantics", func(b *ResidentBatch) { b.Blocks[0].Sessions[0].Semantic = "unexpected-mismatch" }, invalidEvidence},
		{"compiler-score", func(b *ResidentBatch) { b.Label = "collected-compiler" }, invalidEvidence},
		{"outside-matrix", func(b *ResidentBatch) { b.Key = ResidentKey{"B:S", "MixedAPI_M0", "C1B1", "primary"} }, invalidEvidence},
	} {
		t.Run(test.name, func(t *testing.T) {
			b := residentInventedBatch("invalid", "initial", "comparison", 1, residentTestKey(), 100, 100)
			test.mutate(&b)
			r := AnalyzeResident([]ResidentBatch{b})
			if r.Status != test.want || len(r.Populations) != 0 || len(r.Batches) != 1 || r.PromotionReady {
				t.Fatalf("invalid population was selected/scored: %s", r.Status)
			}
		})
	}
}

func TestResidentClusterAndOrderWitnesses(t *testing.T) {
	b := residentInventedBatch("cluster", "initial", "comparison", 1, residentTestKey(), 100000000, 90000000)
	for i := 3; i < 5; i++ {
		for _, j := range []int{1, 2} {
			for k := range b.Blocks[i].Sessions[j].Cycles {
				b.Blocks[i].Sessions[j].Cycles[k].Nanoseconds = 110000000
			}
		}
	}
	r := AnalyzeResident([]ResidentBatch{b})
	e := r.Populations["full"].Benefit.Ratio
	if math.Abs(e.Value-.9) > 1e-14 || math.Abs(e.CI95.Lower-.9) > 1e-14 || math.Abs(e.CI95.Upper-1.1) > 1e-14 || residentPractical(e) != inconclusive {
		t.Fatalf("whole-block CI must be [.9,1.1], not falsely independent [.9,.9]: %+v", e)
	}
	if r.Populations["full"].CyclePairs != 300 || r.Populations["AB"].CyclePairs != 150 || len(r.Sessions) != 20 || len(r.Batches[0].Blocks[0].Sessions[0].Cycles) != 30 {
		t.Fatal("raw/full/order/session populations changed")
	}
	b = residentInventedBatch("order", "initial", "calibration", 1, residentTestKey(), 10000000, 12000000)
	for i := range b.Blocks {
		for j := range 30 {
			b.Blocks[i].Sessions[2].Cycles[j].Nanoseconds = 10000000
			b.Blocks[i].Sessions[3].Cycles[j].Nanoseconds = 12000000
		}
	}
	r = AnalyzeResident([]ResidentBatch{b})
	if r.Status != criterionMet || math.Abs(r.Noise-math.Log(1.2)) > 1e-14 || math.Abs(r.Populations["full"].Benefit.Log.Value) > 1e-14 || residentNoiseGate(estimate{.9, interval{.9, .9}}, r.Noise) != criterionNotMet {
		t.Fatalf("cancelled full effect lost opposite order noise: %s %.17g", r.Status, r.Noise)
	}
	for _, name := range []string{"full", "AB", "BA"} {
		p := r.Populations[name]
		if math.Abs(p.Benefit.Ratio.Value*p.Benefit.Reverse.Value-1) > 1e-14 {
			t.Fatal("same-draw log reversal mismatch")
		}
	}
}

func TestResidentEqualMeanUnequalTail(t *testing.T) {
	b := residentInventedBatch("tail", "initial", "comparison", 1, residentTestKey(), 10000000, 1000000)
	for i := range b.Blocks {
		for _, j := range []int{1, 2} {
			b.Blocks[i].Sessions[j].Cycles[28].Nanoseconds = 136000000
			b.Blocks[i].Sessions[j].Cycles[29].Nanoseconds = 136000000
		}
	}
	m := residentMetricFor(b.Blocks, residentPlan([]int{5}), -1, "cycle_ns", b.Key, false)
	residentAssertRat(t, m.Reference.Mean, "10000000")
	residentAssertRat(t, m.Candidate.Mean, "10000000")
	residentAssertRat(t, m.Rows["p95"].Forward.ExactMarginal.Ratio.Value, "68/5")
	residentAssertRat(t, m.Rows["p95"].Forward.ExactMarginal.Difference.Value, "126000000")
	if m.Rows["p95"].Status != criterionNotMet {
		t.Fatal("equal means cannot substitute for cycle-tail control")
	}
}

func TestResidentIndependentBlockEnumeration(t *testing.T) {
	// Three invented blocks; all 27 ordered draws enumerated independently.
	// The production entry point still requires 5/10 and always uses 20,000.
	b := residentInventedBatch("enumeration", "initial", "comparison", 1, residentTestKey(), 10, 10)
	b.Blocks = b.Blocks[:3]
	for i := range b.Blocks {
		for j := range 4 {
			for k := range 30 {
				b.Blocks[i].Sessions[j].Cycles[k].Nanoseconds = uint64(10 + 20*i + 3*j + k)
			}
			b.Blocks[i].Sessions[j].Resources.IO[0] = residentTestUint(uint64(100*i + 10*j))
		}
	}
	for _, metric := range []string{"cycle_ns", "read_operations"} {
		for _, order := range []int{-1, 0, 1} {
			series := residentSeriesFor(b.Blocks, metric, order)
			for x := range 3 {
				for y := range 3 {
					for z := range 3 {
						indices := []int{x, y, z}
						var weights [15]uint8
						var arms [2][]uint64
						for _, i := range indices {
							weights[i]++
							for j, session := range b.Blocks[i].Sessions {
								if order != -1 && j/2 != order {
									continue
								}
								arm := 0
								if j == 1 || j == 2 {
									arm = 1
								}
								if metric == "cycle_ns" {
									for _, c := range session.Cycles {
										arms[arm] = append(arms[arm], c.Nanoseconds)
									}
								} else {
									arms[arm] = append(arms[arm], *session.Resources.IO[0])
								}
							}
						}
						for arm, raw := range arms {
							slices.Sort(raw)
							for _, stat := range []string{"median", "p95", "mean"} {
								var want uint64
								switch stat {
								case "median":
									want = raw[(len(raw)-1)/2] + raw[len(raw)/2]
								case "p95":
									want = 2 * raw[(19*len(raw)+19)/20-1]
								case "mean":
									for _, v := range raw {
										want += v
									}
								}
								got := residentWeightedStatistic(series.values[arm], weights, stat)
								if !got.IsUint64() || got.Uint64() != want {
									t.Fatalf("explicit expansion mismatch %s/%d/%s %v", metric, order, stat, indices)
								}
							}
						}
					}
				}
			}
		}
	}
	// Independently expand matched cycle positions under every ordered draw.
	// Check full and order log medians, including their symmetric reversal.
	for _, order := range []int{-1, 0, 1} {
		var source []residentLogValue
		perBlock := make([][]float64, 3)
		for i, block := range b.Blocks {
			for o, indices := range [][2]int{{0, 1}, {3, 2}} {
				if order != -1 && order != o {
					continue
				}
				for p := range 30 {
					a := block.Sessions[indices[0]].Cycles[p].Nanoseconds
					c := block.Sessions[indices[1]].Cycles[p].Nanoseconds
					v := math.Log(float64(c) / float64(a))
					source = append(source, residentLogValue{v, i, o})
					perBlock[i] = append(perBlock[i], v)
				}
			}
		}
		slices.SortFunc(source, func(a, b residentLogValue) int {
			if a.value < b.value {
				return -1
			}
			if a.value > b.value {
				return 1
			}
			return 0
		})
		for x := range 3 {
			for y := range 3 {
				for z := range 3 {
					var weights [15]uint8
					forward, reversed := []float64{}, []float64{}
					for _, i := range []int{x, y, z} {
						weights[i]++
						forward = append(forward, perBlock[i]...)
					}
					for _, v := range forward {
						reversed = append(reversed, -v)
					}
					slices.Sort(forward)
					slices.Sort(reversed)
					n := len(forward)
					want := (forward[(n-1)/2] + forward[n/2]) / 2
					wantReverse := (reversed[(n-1)/2] + reversed[n/2]) / 2
					got := residentLogMedian(source, weights, order)
					if math.Abs(got-want) > 1e-14 || math.Abs(-got-wantReverse) > 1e-14 {
						t.Fatal("matched-cycle/full-order/reversal index alignment changed")
					}
				}
			}
		}
	}
	t.Log("invented 3-block enumeration: 972 exact marginal comparisons and 162 approximate log/reversal comparisons")
}

func TestResidentNativeExtremesAndRoundtrip(t *testing.T) {
	b := residentInventedBatch("native", "initial", "comparison", 1, residentTestKey(), 9007199254740993, 9007199254740994)
	for i := range b.Blocks {
		for j := range b.Blocks[i].Sessions {
			s := &b.Blocks[i].Sessions[j]
			s.Resources.UserTicks, s.Resources.KernelTicks = residentTestUint(math.MaxUint64), residentTestUint(math.MaxUint64)
			s.Resources.IO[0] = residentTestUint(9007199254740993)
		}
	}
	raw, err := json.Marshal(b)
	if err != nil {
		t.Fatal(err)
	}
	var decoded ResidentBatch
	if err := json.Unmarshal(raw, &decoded); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(b, decoded) {
		t.Fatal("native integer source roundtrip lost precision")
	}
	s := residentSeriesFor(decoded.Blocks, "session_cpu_ms", -1)
	residentAssertRat(t, residentDistribution(s.values[0], s.scale, true).Mean, "3689348814741910323/1000")
	residentAssertRat(t, residentDistribution(s.values[0], s.scale, true).Total, "3689348814741910323/100")
	peak := residentSeriesFor(decoded.Blocks, "peak_job_commit_bytes", -1)
	if residentDistribution(peak.values[0], 1, false).Total != nil {
		t.Fatal("peak memory summed")
	}
	cycle := residentSeriesFor(decoded.Blocks, "cycle_ns", -1)
	residentAssertRat(t, residentDistribution(cycle.values[0], 1, true).Median, "9007199254740993")
}
