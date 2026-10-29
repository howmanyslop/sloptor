package main

import (
	"bytes"
	"encoding/json"
	"math/big"
	"os"
	"testing"
)

func TestResourceExactMedianAllowanceNeighbors(t *testing.T) {
	for _, metric := range append([]string{"elapsed_ms"}, resourceMetricNames...) {
		for _, offset := range []int64{-1, 0, 1} {
			a, allowance := uint64(3), uint64(16)
			switch metric {
			case "job_cpu_ms":
				a, allowance = 500002, 156250
			case "elapsed_ms":
				allowance = 2
			case "job_peak_bytes":
				allowance = 2 * 1024 * 1024
			case "read_bytes", "other_bytes":
				allowance = 65536
			case "write_bytes":
				allowance = 4096
			}
			c := a + allowance
			if offset < 0 {
				c--
			} else {
				c += uint64(offset)
			}
			values, raw := exactMeanFixture(metric, 15, a, c)
			row := resourcePhaseStatistics([][]pair{values}, metric, true, [][]resourceCounterPair{raw}).Rows["median"]
			want := criterionMet
			if offset > 0 {
				want = criterionNotMet
			}
			if row.Status != want {
				t.Fatalf("%s offset %d: %s", metric, offset, row.Status)
			}
		}
	}
}

func TestResourceExactOrderStatisticsAndLargeHalves(t *testing.T) {
	// Central values differ by one beyond binary64 integer precision. The
	// median must retain the half, while p95 selects rank ceil(.95*4)=4.
	n := uint64(1<<63 + 17)
	raw := [][]resourceCounterPair{{{n, n + 1}, {n + 1, n + 2}, {n + 2, n + 3}, {n + 3, n + 4}}}
	values := [][]pair{resourceTestConstant(2, float64(n), float64(n))}
	s := newResourceOrderSamples(values, "job_peak_bytes", []string{"median", "p95"}, raw)
	if s == nil {
		t.Fatal("newResourceOrderSamples returned nil")
	}
	medianSamples := s.statistics["median"]
	if medianSamples == nil {
		t.Fatal("missing median order statistics")
	}
	median := &medianSamples.point.baseline
	want := new(big.Int).Add(new(big.Int).Lsh(new(big.Int).SetUint64(n), 1), big.NewInt(3))
	if median.Cmp(want) != 0 || medianSamples.denominator.Cmp(big.NewInt(2)) != 0 {
		t.Fatal("even median lost its exact half")
	}
	p95Samples := s.statistics["p95"]
	if p95Samples == nil {
		t.Fatal("missing p95 order statistics")
	}
	p95 := &p95Samples.point.baseline
	if p95.Cmp(new(big.Int).Lsh(new(big.Int).SetUint64(n+3), 1)) != 0 {
		t.Fatal("wrong nearest-rank p95 index")
	}
	for _, name := range []string{"median", "p95"} {
		samples := s.statistics[name]
		if samples == nil {
			t.Fatal("missing order statistics")
		}
		samples.draws = []resourceMeanTotals{samples.point, samples.point}
		d := s.direction(name, false)
		if d.ExactMarginal.Difference.Value.Cmp(big.NewRat(1, 1)) != 0 {
			t.Fatal("large adjacent order statistics collapsed")
		}
	}
	var twenty []resourceCounterPair
	for i := range 20 {
		twenty = append(twenty, resourceCounterPair{uint64(i), uint64(i + 1)})
	}
	ranks := newResourceOrderSamples([][]pair{resourceTestConstant(10, 1, 1)}, "job_peak_bytes", []string{"p95"}, [][]resourceCounterPair{twenty})
	if ranks == nil {
		t.Fatal("newResourceOrderSamples returned nil")
	}
	ranksP95 := ranks.statistics["p95"]
	if ranksP95 == nil {
		t.Fatal("missing p95 order statistics")
	}
	if ranksP95.point.baseline.Cmp(big.NewInt(36)) != 0 {
		t.Fatal("twenty observations must select rank 19, not the maximum")
	}
}

func TestResourceExactMarginalUpperInterpolationNeighbors(t *testing.T) {
	allowance := 16.0
	for _, offset := range []int64{-1, 0, 1} {
		s := &resourceOrderSamples{statistics: map[string]*resourceExactSamples{"median": {denominator: *big.NewInt(1)}}}
		dist := s.statistics["median"]
		if dist == nil {
			t.Fatal("missing median order statistics")
		}
		dist.point.baseline.SetInt64(30)
		dist.point.candidate.SetInt64(47)
		for i := range bootstrapReplicates {
			var point resourceMeanTotals
			point.baseline.SetInt64(30)
			point.candidate.SetInt64(30)
			if i >= 19500 {
				point.candidate.SetInt64(670 + offset)
			}
			dist.draws = append(dist.draws, point)
		}
		d := s.direction("median", false)
		want := criterionMet
		if offset > 0 {
			want = inconclusive
		}
		if got := resourceGate(d, 1.1, &allowance); got.Status != want {
			t.Fatalf("upper interpolated neighbor %d: %+v", offset, got)
		}
		if offset == 0 && d.ExactMarginal.Difference.CI95.Upper.Cmp(big.NewRat(16, 1)) != 0 {
			t.Fatal("upper equality lost exact 1/40 interpolation")
		}
	}
}

func TestResourceExactType7RealizableBoundary(t *testing.T) {
	for _, high := range []uint64{46, 47, 48} {
		values, raw := exactMeanFixture("write_operations", 15, 30, high)
		for _, block := range []int{0, 1, 5, 14} {
			for _, i := range []int{2 * block, 2*block + 1} {
				values[i].candidate, raw[i].candidate = 7, 7
			}
		}
		r := resourcePhaseStatistics([][]pair{values}, "write_operations", false, [][]resourceCounterPair{raw})
		want := criterionMet
		switch high {
		case 47:
			want = inconclusive
		case 48:
			want = criterionNotMet
		}
		row := r.Rows["median"]
		if row.Status != want || r.DrawSHA256 != "6b64c39287f8b242124c5ac231912349c17afa4b6fae4d07e641aa5709a45c78" {
			t.Fatalf("realized high %d: %+v", high, row)
		}
		if high == 47 && (row.Forward.ExactMarginal.Difference.CI95.Lower.Cmp(big.NewRat(16, 1)) != 0 || row.Forward.ExactMarginal.Ratio.CI95.Lower.Cmp(big.NewRat(23, 15)) != 0) {
			t.Fatal("type7 boundary must retain exactly 16 and 23/15")
		}
	}
}

func TestResourceExactMarginalRoundtripIgnoresDisplays(t *testing.T) {
	for _, metric := range []string{"job_cpu_ms", "job_peak_bytes", "elapsed_ms", "write_operations"} {
		values, raw := exactMeanFixture(metric, 3, 0, 1)
		result := resourcePhaseStatistics([][]pair{values}, metric, true, [][]resourceCounterPair{raw})
		for _, row := range result.Rows {
			encoded, err := json.Marshal(row)
			if err != nil {
				t.Fatal(err)
			}
			var decoded resourceStatisticResult
			if err := json.Unmarshal(encoded, &decoded); err != nil {
				t.Fatal(err)
			}
			for _, d := range []*resourceDirection{&decoded.Forward, decoded.Reverse} {
				d.Difference = estimate{1e300, interval{1e300, 1e300}}
				// Poison availability too: only the exact row owns the zero rule.
				d.Ratio = &estimate{1e300, interval{1e300, 1e300}}
			}
			reconfigureResourceRow(&decoded, row.RelativeLimit, row.AbsoluteAllowance)
			if decoded.Status != row.Status || decoded.Forward.DecisiveRoute != row.Forward.DecisiveRoute || decoded.Reverse.DecisiveRoute != row.Reverse.DecisiveRoute {
				t.Fatal("display poisoning changed exact decisions/zero fallback")
			}
		}
	}
}

func TestResourceElapsedFractionalJournalKeepsManifestEstimand(t *testing.T) {
	paths := writeResourceCampaignFixture(t, 0, 1000)
	journal, err := os.ReadFile(paths.Journal)
	if err != nil {
		t.Fatal(err)
	}
	lines := bytes.Split(bytes.TrimSuffix(journal, []byte{'\n'}), []byte{'\n'})
	for i := 1; i < len(lines); i += 2 {
		var event map[string]any
		if err := json.Unmarshal(lines[i], &event); err != nil {
			t.Fatal(err)
		}
		inv := event["invocation"].(map[string]any)
		result := event["result"].(map[string]any)
		fraction := 0.49
		if inv["binary"] == "candidate" {
			fraction = 0.01
		}
		result["ElapsedMS"] = result["DurationMS"].(float64) + fraction
		lines[i], _ = json.Marshal(event)
	}
	journal = append(bytes.Join(lines, []byte{'\n'}), '\n')
	if err := os.WriteFile(paths.Journal, journal, 0o600); err != nil {
		t.Fatal(err)
	}
	p := resourcePlan{Schema: 1, Fixture: "fractional-journal-synthetic", Mode: "calibration", Role: "primary", Primary: cold, Initial: paths}
	data, _ := json.Marshal(p)
	report, err := evaluateResourcePlan(data, "")
	if err != nil || report.Outcome != criterionMet {
		t.Fatalf("valid collector rounding rejected: %v %+v", err, report)
	}
	for _, phase := range []string{cold, noChange} {
		for _, row := range report.Comparison.Summaries["initial"].Metrics[phase]["elapsed_ms"].Rows {
			if row.Forward.ExactMarginal.Difference.Value.Sign() != 0 || row.Forward.ExactMarginal.Ratio.Value.Cmp(big.NewRat(1, 1)) != 0 {
				t.Fatal("fractional journal value substituted for normalized manifest duration")
			}
		}
	}
	if !bytes.Equal(report.Comparison.Batches["initial"].RawJournal.Bytes, journal) {
		t.Fatal("fractional provenance was not retained")
	}
}
