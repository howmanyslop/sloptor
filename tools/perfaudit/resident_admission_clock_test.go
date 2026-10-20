package main

import (
	"math"
	"testing"
)

func TestResidentReaderClockConversionStatusWitnesses(t *testing.T) {
	for _, tc := range []struct {
		name          string
		start, end, f int64
		status        string
		fields        map[string]any
		valid         bool
	}{
		{"fractional", 0, 1, 3, "ok", map[string]any{"deltaTicks": 1, "durationNs": 333333333, "remainder": 1}, true},
		{"signed-ticks", -5, -3, 10000000, "ok", map[string]any{"deltaTicks": 2, "durationNs": 200, "remainder": 0}, true},
		{"negative", 10, 9, 10000000, "negative-delta", map[string]any{"deltaTicks": -1}, false},
		{"negative-overflow", math.MaxInt64, math.MinInt64, 1, "negative-delta", map[string]any{}, false},
		{"span-overflow", math.MinInt64, math.MaxInt64, 1, "tick-span-overflow", map[string]any{}, false},
		{"ns-overflow", 0, math.MaxInt64, 1, "nanosecond-overflow", map[string]any{"deltaTicks": math.MaxInt64, "remainder": 0}, false},
		{"bad-frequency", 0, 1, 0, "nonpositive-frequency", map[string]any{"deltaTicks": 1}, false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			r := newResidentReader()
			conversion := map[string]any{"status": tc.status}
			for k, v := range tc.fields {
				conversion[k] = v
			}
			record := map[string]any{"accepted": tc.valid, "operationStatus": "ok", "conversion": conversion}
			if tc.valid {
				record["durationNs"] = tc.fields["durationNs"]
			}
			r.conversion(jsonObjectTest(record), residentEndpoint{"ok", tc.start, true}, residentEndpoint{"ok", tc.end, true}, tc.f, "test")
			if tc.valid != (len(r.issues) == 0) {
				t.Fatal(r.issues)
			}
			for _, i := range r.issues {
				if i.Path != "test" {
					t.Fatalf("status or presence calculation disagrees: %+v", r.issues)
				}
			}
		})
	}
	for _, boolValue := range []uint64{1, 1 << 31, math.MaxUint32} {
		r := newResidentReader()
		ep := r.endpoint(mustJSON(map[string]any{"status": "ok", "ticks": -1, "bool": boolValue, "lastError": uint64(math.MaxUint64)}), "ep")
		if !ep.Valid || len(r.issues) > 0 {
			t.Fatal("BOOL32 or stale lastError incorrectly rejected", r.issues)
		}
	}
	r := newResidentReader()
	r.endpoint([]byte(`{"status":"not-attempted","ticks":0}`), "ep")
	if len(r.issues) == 0 {
		t.Fatal("unattempted raw presence accepted")
	}
}

func TestResidentReaderClockEnclosureBoundaries(t *testing.T) {
	for _, tc := range []struct {
		name  string
		start int64
		want  string
	}{{"minus-one-allowed", 1001, criterionMet}, {"minus-two-rejected", 1002, invalidEvidence}} {
		t.Run(tc.name, func(t *testing.T) {
			f := newResidentProtocolFixture(t, residentTestKey())
			b := f.add("calibration-initial", 1, 100)
			e := f.events(b)
			enc := e[1]["enclosure"].(map[string]any)
			enc["start"].(map[string]any)["ticks"] = tc.start
			a := e[1]["clock_assessment"].(map[string]any)
			a["outer_span_ticks"], a["aggregate_bound_ticks"] = 1000000000-tc.start, 1000000031-tc.start
			f.seal(b, e)
			got, _ := f.batchReport(b)
			if got.Status != tc.want {
				t.Fatalf("%s %+v", got.Status, got.Issues)
			}
		})
	}
	for _, gap := range []int64{-1, -2} {
		t.Run(map[int64]string{-1: "gap-minus-one", -2: "gap-minus-two"}[gap], func(t *testing.T) {
			f := newResidentProtocolFixture(t, residentTestKey())
			b := f.add("calibration-initial", 1, 100)
			e := f.events(b)
			f.editReport(e[1], func(report map[string]any) {
				second := report["records"].([]any)[1].(map[string]any)
				second["start"].(map[string]any)["ticks"] = 101000 + gap
				second["end"].(map[string]any)["ticks"] = 201000 + gap
			})
			f.seal(b, e)
			got, _ := f.batchReport(b)
			want := criterionMet
			if gap < -1 {
				want = invalidEvidence
			}
			if got.Status != want {
				t.Fatalf("%s %+v", got.Status, got.Issues)
			}
		})
	}
}

func TestResidentReaderOneTickIsExternalAndZeroInvalid(t *testing.T) {
	for _, delta := range []int{0, 1} {
		t.Run(map[int]string{0: "zero", 1: "one-tick"}[delta], func(t *testing.T) {
			f := newResidentProtocolFixture(t, residentTestKey())
			b := f.add("calibration-initial", 1, 100)
			e := f.events(b)
			f.editReport(e[1], func(report map[string]any) {
				record := report["records"].([]any)[0].(map[string]any)
				record["end"].(map[string]any)["ticks"] = 1000 + delta
				record["durationNs"] = delta * 100
				record["conversion"] = map[string]any{"status": "ok", "deltaTicks": delta, "durationNs": delta * 100, "remainder": 0}
			})
			a := e[1]["clock_assessment"].(map[string]any)
			a["sum_ticks"] = 2900000 + delta
			a["represented_tick_spans"].([]any)[0] = delta
			if delta == 0 {
				a["zero_tick_count"] = 1
			} else {
				a["one_tick_count"], a["suitability"] = 1, "unresolved-one-tick"
			}
			f.seal(b, e)
			f.authorize()
			got := f.analyze()
			want := residentUnverified
			if delta == 0 {
				want = invalidEvidence
			}
			if got.Outcome != want || got.PromotionReady {
				t.Fatalf("%s %+v", got.Outcome, got.Issues)
			}
			if delta == 1 && (!got.Batches[0].OneTick || got.CalibrationOnly[0].FollowUpAllowed) {
				t.Fatal("readiness became retry authority")
			}
		})
	}
}
