# Optional resource/control mode

Run post-collection analysis with a new output path:

```powershell
perfaudit.exe --resource-plan plan.json --output new-resource-audit.json
```

This mode implements resource-control-policy.md SHA256
`3c234c019869f91a6bb811b04b20ce15d5b4d52fd96f6e2e8a984afd8dda6072`.
It accepts no manifest-mode policy flags. The original command without
`--resource-plan` retains its original behavior and report. Neither mode runs
benchmarks, changes evidence, or grants promotion: `promotion_ready` is always
false. Exit codes remain 0 for numeric criterion-met, 1 for another evidence or
policy outcome, 2 for CLI/I/O failure. Interrupted output is retained incomplete.

## Plan schema and recipes

Use strict UTF-8 JSON, schema 1, exact field names and no unknown plan fields.
Paths are absolute or relative to the plan's directory. Each batch supplies all
three original files from one completed harness evidence directory:

The supplied directory must be the recorded absolute original location. Copying
unchanged files elsewhere, including on the same OS, is unsupported: the journal
and stable-state paths must still match their recorded locations. Separator/case
normalization is supported; portable mount mapping is outside this mode. Do not
rewrite original paths to make relocated evidence pass.

```json
{
  "schema": 1,
  "fixture": "predeclared-fixture-name",
  "mode": "calibration",
  "role": "primary",
  "primary_phase": "cold",
  "initial": {
    "manifest": "aa/windows-build-performance-manifest.json",
    "resource_journal": "aa/invocations.ndjson",
    "run_metadata": "aa/run.json"
  }
}
```

`mode` is `calibration` or `candidate`. `role` is `primary`, `secondary` or
`negative-control`; `primary_phase` is `cold` or `no_change`. Fixture and role
are declarations: their pre-registration and real-world meaning stay external.
Ordinary initial/confirmation batches require exactly 30 cold/100 no-change
pairs. A `follow_up` requires exactly 60/200. Both phases and every resource
metric travel together. This frozen mode does not accept alternative thresholds
or counts; synthetic smaller vectors exercise its underlying functions in tests.

- **Individual batch:** the plan above evaluates A/A equivalence and noise.
  Change mode to `candidate` to evaluate a candidate batch's conditional gates.
  Without admitted calibration its campaign remains unverified; without required
  confirmation a numerically admitted primary/secondary remains unverified.
- **Initial plus follow-up:** add `follow_up` with the same three path fields.
  The output retains initial, standalone doubled and batch-stratified joint
  summaries. Only an initially inconclusive fixture permits this one follow-up.
  Any initial fail defers; both doubled and joint must pass every gate.
- **Candidate with calibration:** use mode `candidate` and add
  `"calibration": {"initial": { ...three paths... }}`. Its optional `follow_up`
  follows the same rule. A/A arms must identify the identical binary. Candidate
  baseline, fixture identities, stable host/environment configuration, capture,
  format and project-mode labels must match the calibration. Its admitted maximum
  noise envelope is used for candidate benefit gates.
- **Confirmation:** add `confirmation` with three paths to a primary/secondary
  candidate plan. It requires a separate 30/100 batch passing independently;
  failure or uncertainty defers without rescue or pooling. Controls have no extra
  confirmation or benefit obligation. Calibration plans have no confirmation.

Later batches must have nonoverlapping recorded timestamps in the declared order.
The exact same journal cannot stand for two batches. Identical numeric manifests
can occur in separate runs. These checks do not prove independent collection,
honest timestamps or quiet hosts. Supplied forbidden later stages remain visible
and cannot rescue an earlier failure; their malformed evidence still invalidates
the supplied campaign.

## Admission and measured scope

Admission checks all raw JSON/NDJSON for syntax, duplicate decoded fields, Unicode
scalar correctness, recognized case shadows and numeric types before ignoring
evidence extensions. Original input bytes and SHA256s, every journal line and
exact integers remain recoverable in the output. It requires evaluated run state,
final published manifest filename, final format IDs, matching before/after
identities, record counts and ordered phase/pair/arm correspondence, complete
started/finished invocations and matching elapsed/exit/digest records. A legacy
FAIL is retained as FAIL; it does not mean incomplete collection.

Optional native `cleanup_error`, `scope` and `tools` fields are recognized and
type checked, including case shadows. An evaluated run permits absent or null
cleanup error only. True future evidence fields remain retained after strict JSON
validation. Within A/A, optional supplied source, typed observed build and build
setting hashes must be present for both arms or neither, and agree when present.
Absent assertions stay unverified. The raw `go version -m` transcript hash also
needs matching presence, but equality is required only for identical recorded path
spellings: the transcript includes that path. This rule applies across batches
too; comparable typed build fields always compare even for copied executables.

The fixed resource format accepts its final native descriptors and these exact
semantic shorthand aliases from the original synthetic recipes: boundary
`post-wall`; CPU `100ns ticks`; memory `job peak bytes, not RSS` or
`job committed peak bytes`; I/O `OS I/O, not filesystem-only` or
`OS counters, not filesystem-only`. Other descriptions reject; descriptors cannot
redefine tick units or OS I/O as durable filesystem writes. Available native
structures require supported fixed C# sequential layouts: JobAccounting 96 bytes;
JobMemory 112 or 144 bytes for a 32- or 64-bit collector. These derive from the
49f2 field definitions: accounting 4*8+4*4+6*8; extended limits BasicLimits
(48/64)+IoCounters (48)+four UIntPtr (16/32). Child compiler GOARCH does not select
the collector layout. Both peak fields must fit the layout's native UIntPtr:
0..MaxUint32 for 112 bytes, 0..MaxUint64 for 144 bytes. Widening to UInt64 in
the emitter does not expand the native range. Every available JobMemory snapshot
in one declared run, including the trailing diagnostic, must use the same layout.
Failed/unsupported/unavailable snapshots with null Values make no layout claim.
Separate batches may use different supported layouts. Matching sizes remain
assertions, not proof of collector architecture or collection.
A snapshot with any available component
requires nonnull nonnegative getter duration. Genuine missing snapshots may have
null duration. Duration lexemes must be mathematically nonnegative before float
conversion; explicit mathematical negative zero is allowed, and native fractional
values retain ties-even/max-one elapsed rounding.

The separate trailing candidate `--timings` diagnostic is explicitly classified
and retained. Its observations never enter scored elapsed/resource populations.
Malformed syntax/shape, negative/overflow counters, mismatched provenance/order
or incomplete invocations are invalid evidence. Truthful failed/unsupported
required components yield `incomplete-unmeasured`, with causes and all rows
retained; no missing metric becomes measured zero. A failed optional
`OtherOwnedCpu` leaves available raw job measurements eligible. Affected required
metric populations are suppressed wholesale rather than filtered to successes.

Required counters are Windows owned-job user+kernel CPU ticks, job peak committed
memory and six read/write/other operation/byte counters. CPU is their exact tick
sum divided by 10000 ms. CPU and each I/O counter have separate arithmetic mean
AND median guards. Job peak uses median/p95, with no sum of peaks. Raw job/root
CPU, other-owned subtraction, individual peak, faults/process counts and getter
duration remain descriptive. RSS, Node-only attribution, physical filesystem
operations and durable flush are unmeasured. Aggregate OS write bytes are not
application output writes; fresh-process peaks imply no retained-heap/watch gain.

## Statistics and decisions

All gates use 20,000 whole AB/BA block draws, SplitMix64 seed 20261018 restarted
per phase, unbiased rejection before modulo, arithmetic even medians,
nearest-rank p95 and central 95% type7 percentile endpoints. Joint draws sample
initial then doubled strata at their original block counts within every draw.
Draw-index hashes encode each local zero-based index as four little-endian bytes.
Both arms, reversals, metrics and full/AB/BA populations use the same block stream.

Resource/control rows use the named **marginal** statistic's ratio and difference.
Pass means ratio upper <= limit OR difference upper <= allowance; fail requires
both lower bounds exceeding their budgets. Otherwise uncertainty is inconclusive.
If the actual or any draw denominator is zero, the entire row's ratio is null;
the zero-draw count is reported and every difference draw remains. Absolute-only
upper <= allowance passes, lower > allowance fails, otherwise inconclusive.
The JSON includes exact limits, route, percentage/absolute changes, both-arm
mean/median/p95/range, additive totals, raw pairs and block/order/batch counts.

All marginal decisions in this optional mode use exact admitted integers and
rational arithmetic. CPU uses the exact user+kernel tick sum divided by 10000;
I/O and peak memory use uint64 counters. Elapsed uses the admitted positive int64
manifest `duration_ms` (<=MaxInt64/2), matching journal `DurationMS` after validated
collector ties-even/max-one normalization. Fractional `ElapsedMS` remains raw
correspondence evidence; it is never substituted into elapsed marginal statistics.
CPU component signed-native bounds and the existing unsigned counter ranges still
apply. Arbitrary-size sums avoid overflow, even medians retain central halves,
and p95 selects nearest rank ceil(.95*n). Ratios/differences and type7 endpoints
at exactly 1/40 and 39/40 remain rational through exact decimal limit comparison.

Mean directions retain `exact_mean`; median/p95 directions add `exact_marginal`,
each containing point/interval fraction strings. Only one is emitted per direction.
These fields determine decisions and ratio availability, including after JSON
roundtrip; float ratio/difference/percentage and descriptive summaries are displays.
Displays can coincide on opposite sides of a limit. Whole-row zero-denominator
fallback retains every draw and its exact zero count. Reverse ratios are computed
before percentile interpolation, using the same selected blocks. Additive totals
remain separate from non-additive peaks. Journal-free internal synthetic helpers
interpret shortest decimal spellings of their supplied finite floats exactly;
the CLI uses admitted raw integers. Original paired-log effect/noise conventions,
primary/legacy classifiers and original manifest-only behavior remain unchanged.
The optional mode's candidate no-change p95 guard also uses the exact marginal
ratio-only rule; its existing gate name and 1.10 threshold are retained.

A/A applies elapsed guards to every fixture and all resource guards in both
directions using reversed draw statistics, including ratio-only no-change p95
<=1.10. Candidate-benefit and legacy cold<=0.60 rules do not gate A/A; their
actual separate results are retained. Named candidate controls receive cold
median/p95 guards. Every candidate retains the original no-change p95 ratio-only
guard; primary/secondary benefit retains point<=0.95 and CI upper<1.

Calibration N is the maximum absolute full/AB/BA log point estimate and interval
endpoint over initial and, when used, doubled and joint summaries. A biased
zero-width interval retains its nonzero center. Candidate benefit additionally
requires paired-ratio CI upper < exp(-N); lower >= that cutoff fails, otherwise
it is inconclusive. Later narrower A/A evidence cannot erase initial noise.

Source correctness, full output/diagnostic/write/mtime contracts, actual runtime
provenance, host quietness, independent collection, pre-registration and exact-SHA
independent evaluator review remain external/unverified. Per-row percentile
intervals imply neither exact nor simultaneous family-wide coverage. Reported
labels and numeric passes do not establish any of these external obligations.
