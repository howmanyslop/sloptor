package main

import (
	"encoding/json"
	"math"
	"math/big"
)

type residentEndpoint struct {
	Status string
	Ticks  int64
	Valid  bool
}
type residentClockObservation struct {
	Frequency   int64    `json:"frequency_hz"`
	OuterStart  int64    `json:"outer_start_ticks"`
	OuterEnd    int64    `json:"outer_end_ticks"`
	TickSpans   []int64  `json:"represented_tick_spans"`
	OneTick     bool     `json:"one_tick_unresolved"`
	Sum         *big.Int `json:"sum_ticks"`
	OuterSpan   *big.Int `json:"outer_span_ticks"`
	serialValid bool
}

// The frozen ordering tolerance permits a gap of exactly -1 tick. Subtract
// using arbitrary precision so neither signed endpoint extreme can wrap.
func (r *residentReader) serialClock(previous, current residentClockObservation, p string) {
	gap := new(big.Int).Sub(big.NewInt(current.OuterStart), big.NewInt(previous.OuterEnd))
	if gap.Cmp(big.NewInt(-1)) < 0 {
		r.invalid(p, "serial invocation enclosure gap below minus one tick")
	}
}

func (r *residentReader) endpoint(raw json.RawMessage, p string) residentEndpoint {
	o := r.obj(raw, p, "status", "ticks bool lastError")
	e := residentEndpoint{Status: r.str(o, "status", p)}
	if e.Status == "not-attempted" {
		r.absent(o, p, "ticks", "bool", "lastError")
		return e
	}
	if e.Status != "ok" && e.Status != "api-failed" {
		r.invalid(p, "unknown endpoint status")
	}
	e.Ticks = r.sint(o, "ticks", p)
	b := r.uint(o, "bool", p, math.MaxUint32)
	r.uint(o, "lastError", p, math.MaxUint64)
	e.Valid = b != 0
	if (e.Status == "ok") != e.Valid {
		r.invalid(p, "native BOOL/status contradiction")
	}
	return e
}

func (r *residentReader) bigNumber(o object, k, p string) *big.Int {
	raw := string(o[k])
	n, ok := new(big.Int).SetString(raw, 10)
	if !residentSignedToken.MatchString(raw) || !ok || n.BitLen() > 128 {
		r.invalid(p+"."+k, "bounded exact integer required")
		return new(big.Int)
	}
	return n
}

// Derive conversion status and field presence using arbitrary precision.
// Failed raw clocks remain evidence, never a successful duration or retry.
func (r *residentReader) conversion(record object, start, end residentEndpoint, f int64, p string) (uint64, int64) {
	expected := object{}
	status := "unavailable-endpoints"
	put := func(k string, v *big.Int) { expected[k] = json.RawMessage(v.String()) }
	var delta big.Int
	if start.Valid && end.Valid {
		delta.Sub(big.NewInt(end.Ticks), big.NewInt(start.Ticks))
		status = "ok"
		switch {
		case delta.Sign() < 0:
			status = "negative-delta"
			if delta.IsInt64() {
				put("deltaTicks", &delta)
			}
		case !delta.IsInt64():
			status = "tick-span-overflow"
		default:
			put("deltaTicks", &delta)
			if f <= 0 {
				status = "nonpositive-frequency"
			} else {
				numerator := new(big.Int).Mul(&delta, big.NewInt(1000000000))
				remainder := new(big.Int)
				ns := new(big.Int)
				ns.QuoRem(numerator, big.NewInt(f), remainder)
				put("remainder", remainder)
				if !ns.IsInt64() {
					status = "nanosecond-overflow"
				} else {
					put("durationNs", ns)
				}
			}
		}
	}
	c := r.obj(record["conversion"], p+".conversion", "status", "deltaTicks durationNs remainder")
	r.want(c, "status", p+".conversion", status)
	for _, k := range []string{"deltaTicks", "durationNs", "remainder"} {
		if value, ok := expected[k]; ok {
			r.equal(c[k], value, p+".conversion."+k)
			if k == "remainder" {
				r.uint(c, k, p, math.MaxUint64)
			} else {
				r.sint(c, k, p)
			}
		} else {
			r.absent(c, p+".conversion", k)
		}
	}
	accepted := r.boolean(record, "accepted", p)
	if !accepted {
		r.absent(record, p, "durationNs")
		r.invalid(p, "operation was not accepted; whole collection invalid")
		return 0, 0
	}
	r.want(record, "operationStatus", p, "ok")
	r.noError(record, p, "error")
	if status != "ok" {
		r.invalid(p, "accepted record has invalid QPC calculation")
		return 0, 0
	}
	ns := r.uint(record, "durationNs", p, math.MaxInt64)
	r.equal(record["durationNs"], expected["durationNs"], p+".durationNs")
	if ns == 0 {
		r.invalid(p, "zero ns invalidates whole collection")
	}
	return ns, delta.Int64()
}

func (r *residentReader) clock(report object, entry object, host object, p string) (residentClockObservation, []ResidentCycle) {
	begin := len(r.issues)
	result := residentClockObservation{Sum: new(big.Int), OuterSpan: new(big.Int)}
	c := r.obj(report["clock"], p+".clock", "backend counterApi frequencyApi counterEncoding conversion remainderRule scope support loadStatus counterResolveStatus frequencyResolveStatus initializationStatus initialFrequency finalFrequency verificationStatus frequencyVerified", "frequencyHz initializationError verificationError")
	for k, v := range map[string]string{"backend": "windows-qpc-syscalln-v1", "counterApi": "QueryPerformanceCounter", "frequencyApi": "QueryPerformanceFrequency", "counterEncoding": "signed-int64", "conversion": "floor-ticks-1e9-over-frequency-v1", "remainderRule": "durationNs + remainder/frequencyHz nanoseconds; 0 <= remainder < frequencyHz", "scope": "elapsed interval including blocking/scheduling; not UTC or process CPU; access cost retained; frequency is not calibrated precision"} {
		r.want(c, k, p+".clock", v)
	}
	initial := r.endpoint(c["initialFrequency"], p+".clock.initialFrequency")
	final := r.endpoint(c["finalFrequency"], p+".clock.finalFrequency")
	// Only complete clocks can enter statistics. All failure statuses/raw bytes
	// have already been retained; none can be converted to a statistical retry.
	for _, k := range []string{"loadStatus", "counterResolveStatus", "frequencyResolveStatus", "initializationStatus", "verificationStatus"} {
		r.want(c, k, p+".clock", "ok")
	}
	r.want(c, "support", p+".clock", "supported")
	r.flag(c, "frequencyVerified", p, true)
	r.noError(c, p, "initializationError", "verificationError")
	f := r.sint(c, "frequencyHz", p+".clock")
	result.Frequency = f
	r.number(entry, "stopwatch_frequency", p, uint64(f))
	if f <= 0 || !initial.Valid || !final.Valid || initial.Ticks != f || final.Ticks != f {
		r.invalid(p+".clock", "positive cached/initial/final QPF frequency must agree")
	}
	enc := r.obj(entry["enclosure"], p+".enclosure", "schema format support frequency_hz scope call_status start end", "call_error")
	r.number(enc, "schema", p, 1)
	r.want(enc, "format", p, "resident-qpc-enclosure-v1")
	r.want(enc, "call_status", p, "returned")
	r.noError(enc, p, "call_error")
	r.want(enc, "scope", p, "immediately around unchanged Invoke-ProcessWithTimeout call; includes native launch/wait/drains and post-wall helper work; excludes report validation/disposal/retention; consistency only, not a scoring estimand")
	support := r.obj(enc["support"], p+".support", "status domain counter_api frequency_api endpoint_observation high_resolution frequency_hz framework os architecture powershell corelib_path corelib_sha256 corelib_informational_version timestamp_il frequency_il authority", "")
	for k, v := range map[string]string{"status": "supported", "domain": "same-host-windows-qpc", "counter_api": "QueryPerformanceCounter", "frequency_api": "QueryPerformanceFrequency", "endpoint_observation": "managed return; native BOOL and lastError are not exposed", "architecture": "X64", "corelib_sha256": "8dfc31dfb4783a981179036f492d2690303e6e6571c7f260901b1b9ebc9e64ae", "timestamp_il": "1200E028BD00000626062A", "frequency_il": "1200E028BE00000626062A"} {
		r.want(support, k, p+".support", v)
	}
	r.flag(support, "high_resolution", p, true)
	for _, k := range []string{"framework", "os", "powershell", "corelib_path", "corelib_informational_version", "authority"} {
		if r.str(support, k, p) == "" {
			r.invalid(p+".support."+k, "nonempty managed support binding required")
		}
	}
	for _, k := range []string{"os", "architecture", "powershell"} {
		r.equal(support[k], host[k], p+".host.support."+k)
	}
	if r.sint(enc, "frequency_hz", p) != f || r.sint(support, "frequency_hz", p) != f {
		r.invalid(p, "collector, enclosure and runtime frequencies differ")
	}
	r.equal(enc["support"], host["qpc_support"], p+".enclosure.support")
	aobj := r.obj(enc["start"], p+".outer.start", "status ticks", "bool lastError")
	bobj := r.obj(enc["end"], p+".outer.end", "status ticks", "bool lastError")
	r.absent(aobj, p+".outer.start", "bool", "lastError")
	r.absent(bobj, p+".outer.end", "bool", "lastError")
	r.want(aobj, "status", p, "managed-return")
	r.want(bobj, "status", p, "managed-return")
	a, b := big.NewInt(r.sint(aobj, "ticks", p)), big.NewInt(r.sint(bobj, "ticks", p))
	result.OuterStart, result.OuterEnd = a.Int64(), b.Int64()
	result.OuterSpan.Sub(b, a)
	if result.OuterSpan.Sign() < 0 {
		r.invalid(p, "backwards outer interval")
	}
	low, high := new(big.Int).Sub(a, big.NewInt(1)), new(big.Int).Add(b, big.NewInt(1))
	records := r.arr(report["records"], p+".records", 30)
	var cycles []ResidentCycle
	var previous *big.Int
	for i, raw := range records {
		q := fmtResidentIndex(p+".records", i)
		rec := r.obj(raw, q, "ordinal scenario checkers builders inputState mutation accepted operationStatus start end conversion", "durationNs error")
		start, end := r.endpoint(rec["start"], q+".start"), r.endpoint(rec["end"], q+".end")
		ns, delta := r.conversion(rec, start, end, f, q)
		st, en := big.NewInt(start.Ticks), big.NewInt(end.Ticks)
		if st.Cmp(low) < 0 || st.Cmp(high) > 0 || en.Cmp(low) < 0 || en.Cmp(high) > 0 {
			r.invalid(q, "endpoint outside exact outer enclosure plus one tick")
		}
		if previous != nil && st.Cmp(new(big.Int).Sub(previous, big.NewInt(1))) < 0 {
			r.invalid(q, "serial gap below minus one tick")
		}
		previous = en
		result.TickSpans = append(result.TickSpans, delta)
		result.OneTick = result.OneTick || delta == 1
		result.Sum.Add(result.Sum, big.NewInt(delta))
		cycles = append(cycles, ResidentCycle{i + 1, ns})
	}
	bound := new(big.Int).Add(result.OuterSpan, big.NewInt(int64(len(records)+1)))
	if result.Sum.Cmp(bound) > 0 {
		r.invalid(p, "necessary n+1 aggregate enclosure bound exceeded")
	}
	assessment := r.obj(entry["clock_assessment"], p+".clock_assessment", "status score_authorized suitability calculations_validated frequency_hz outer_span_ticks sum_ticks cycle_count allowance_ticks aggregate_bound_ticks endpoint_order_tolerance_ticks represented_tick_spans zero_tick_count one_tick_count two_tick_count", "error")
	r.want(assessment, "status", p, "consistent")
	r.flag(assessment, "score_authorized", p, false)
	r.flag(assessment, "calculations_validated", p, true)
	r.noError(assessment, p, "error")
	for k, v := range map[string]*big.Int{"frequency_hz": big.NewInt(f), "outer_span_ticks": result.OuterSpan, "sum_ticks": result.Sum, "cycle_count": big.NewInt(int64(len(records))), "allowance_ticks": big.NewInt(int64(len(records) + 1)), "aggregate_bound_ticks": bound, "endpoint_order_tolerance_ticks": big.NewInt(1)} {
		if r.bigNumber(assessment, k, p).Cmp(v) != 0 {
			r.invalid(p+".clock_assessment."+k, "claimed assessment differs from independent raw calculation")
		}
	}
	for value, k := range []string{"zero_tick_count", "one_tick_count", "two_tick_count"} {
		n := uint64(0)
		for _, d := range result.TickSpans {
			if d == int64(value) {
				n++
			}
		}
		r.number(assessment, k, p, n)
	}
	spans, _ := json.Marshal(result.TickSpans)
	r.equal(assessment["represented_tick_spans"], spans, p+".represented_tick_spans")
	suitability := "requires-independent-precision-review"
	if result.OneTick {
		suitability = "unresolved-one-tick"
	}
	r.want(assessment, "suitability", p, suitability)
	result.serialValid = len(r.issues) == begin
	return result, cycles
}
