# Recorded-pair performance audit

`perfaudit` evaluates schema1 manifests containing fresh-process cold and
no-change AB/BA pairs. It reports recorded correctness, effect sizes and
uncertainty. `criterion-met` means the manifest's statistical criterion was met;
it does not mean a candidate is ready for promotion. Every report marks external
gates `unverified` and `promotion_ready` false.

## Workflow

1. Before collecting data, record the primary workload, primary phase, practical
   threshold, exact batch counts and independent confirmation plan in the
   experiment declaration. Review balanced-path A/A calibration and require the
   practical effect to exceed its measured noise floor. The default audit policy
   uses 30 cold and 100 no-change pairs and a 5% point improvement.
2. Collect evidence under the root's quiet-host lease with prebuilt binaries.
   Keep the manifest, run status, invocation journal, raw logs and identities
   together. Review the measurement boundary, source/fixture/dependency/binary
   identity and contract-specific correctness evidence.
3. For the corrected Windows harness, require **both** companion `run.json` with
   `status: "evaluated"` and the final published manifest, plus the actual legacy
   verdict. A pending manifest, incomplete run or cleanup failure is unfinished
   evidence even if a saved evaluator verdict says PASS. A complete run may have
   an actual legacy FAIL. This CLI deliberately does not verify companion files;
   `harness_completion` stays unverified even when manifest statistics qualify.
4. Run the unchanged legacy evaluator and retain its actual JSON and exit code:

   ```powershell
   go run ./tools/perfcompare --input manifest.json --output legacy-verdict.json
   ```

   Its cold median candidate/baseline limit remains 0.60 and its no-change p95
   limit remains 1.10, with at least 10/20 pairs. An audit success never converts
   its FAIL into PASS. A/A and a real 5% improvement normally fail the legacy
   cold threshold; report that result explicitly.
5. Run this separate audit with a new output path:

   ```powershell
   go run ./tools/perfaudit --input manifest.json --output audit.json `
     --primary cold --max-primary-ratio 0.95
   ```

   Both policy flags are required, including the explicit `0.95`. For a justified
   preregistered study, exact counts can be supplied with `--cold-pairs 10
   --no-change-pairs 20`. Counts must be even and at least the legacy minima;
   defaults require exactly 30/100. Extra samples are rejected along with missing
   ones so a report cannot silently change the declared batch. Both phases are
   required. Smaller ratio thresholds must be justified before scoring outside
   the tool; absolute-millisecond practical criteria are not implemented here.
6. Review every issue, both phase summaries and both order strata. Finish the
   external gates: full semantic/artifact/write parity, A/A noise, independent
   primary and second-workload confirmation, CPU/memory/I/O regressions and
   independent statistical/evidence review of the exact evaluator SHA. Apply the
   root's predeclared fixed follow-up policy, keeping original and follow-up
   batches visible. This CLI combines no batches and performs no optional retry.

`go run` is for post-collection analysis only. It is never a scored invocation.

## Input validity and recorded correctness

The input is the existing `tools/perfcompare/schema.go` schema1 shape. Required
fields use exact spelling and types. The entire document must be strict UTF-8
JSON with one value, no comments, no trailing content, no duplicate object names
(including escaped names) and no case shadows of recognized fields. Nested JSON
is checked to depth 128. Unknown extension fields are retained, not interpreted.

Machine metadata, environment and both binary revision/command labels must be
present. `machine.run_order` must contain exactly AB and BA. Available `sha256:`
revision labels and `sha256-tree-v2:` digests must have 64 hexadecimal digits.
An optional `tree_digest_format` must agree with every output digest prefix.
The corrected harness's `measurement_boundary` and format metadata are retained;
labels alone cannot establish their real-world truth.

Each record needs a positive pair ID, supported phase, AB/BA order, recognized
arm, integer duration in milliseconds, explicit integer exit code and nonempty
diagnostic/output digests. Duration must be positive and at most `MaxInt64/2`,
as in the legacy validator. Fractional, infinite, NaN and overflowing values
are rejected. Every adjacent two records must describe one unique pair in the
declared command order. Each phase must be contiguous, balanced and alternating;
either starting order is accepted. Adjacent opposite-order pairs form complete
four-command blocks. Records are never sorted into a preferred order.

Both exits must be zero for these successful workloads. Both diagnostic digests
and both output digests must match within **every** pair. Nonzero exits and digest
mismatches are correctness failures, distinct from a performance threshold miss.
Expected-failure/recovery workloads need a different external correctness design.
The limited output digest does not prove complete artifacts, write counts, mtimes
or semantic equivalence. No sample or outlier is dropped.

Validation visits every decodable record and reports all discoverable issues.
A syntax error can prevent decoding the rest of the document. Invalid evidence
suppresses all statistics rather than computing on a surviving subset.

## Statistics and uncertainty

For each phase the JSON reports pair/block counts and the following estimates
with central 95% block-bootstrap intervals:

- Baseline and candidate marginal medians, using the middle value or arithmetic
  mean of the two middle values, as in the legacy evaluator.
- Baseline and candidate marginal p95, using sorted rank `ceil(0.95*n)`, as in
  the legacy evaluator; their candidate/baseline ratio is the tail guardrail.
- Median paired difference `candidate_ms - baseline_ms`.
- Median paired log ratio `median(log(candidate_ms/baseline_ms))`, and its
  exponential, the paired ratio. This differs from a ratio of marginal medians
  and from an arithmetic median of ratios for even counts.

AB and BA strata have separate counts and the same point statistics. The full
phase's intervals resample intact blocks, preserving both orders together;
stratum intervals are not independently fitted.

Each phase restarts **SplitMix64 at seed 20261018**. There are **20,000**
replicates. Each replicate uniformly draws the original number of blocks with
replacement. The same selected four-command blocks supply both arms and the
marginal p95 ratio. Arms and order strata are never resampled independently.
Unsigned 64-bit arithmetic wraps. Uniform indices use rejection of stream values
below `(-bound) % bound` before reduction modulo the block count. See
`splitMix64.next` for the complete stream definition, tested against reference
values; this does not depend on Go's standard-library RNG version.

For bootstrap endpoints, sort the replicate statistics. At probability `p`, use
zero-based `h=(20000-1)*p` and linearly interpolate between `floor(h)` and
`ceil(h)` (Hyndman-Fan type 7), at `p=0.025` and `p=0.975`. Paired-ratio
endpoints exponentiate the interpolated **log-median** endpoints. This preserves
the inverse relationship under label reversal. Difference intervals reverse
sign with swapped endpoints. Constant distributions deterministically give equal
endpoints; constant A/A gives ratio 1 and interval `[1,1]`, with no improvement.

The bootstrap assumes blocks reasonably capture short-range dependence.
Long-range drift/dependence and path effects remain possible. Finite block counts
and empirical p95 tails limit coverage; these are not exact-coverage promises.
Integer-ms rounding limits small-effect analysis. Statistics use float64, which
can additionally lose precision for very large integer durations. A zero-width
interval does not establish zero real-world noise.

## Decision and output contract

`schema: 1` identifies this audit report, independently of the manifest schema.
The JSON includes the complete decision inputs, fixed bootstrap method, issues,
phase statistics, primary and guardrail decisions, external gates and limits.
`input.sha256` hashes the exact input bytes. Base64-decoding
`input.raw_json_base64` recovers those exact bytes, including every raw record,
extension, whitespace and malformed input. Input is read only and output is
created exclusively; existing files, including aliased input paths, are preserved.
Interrupted writes may leave partial output, which must be retained as incomplete.

The primary rule is paired ratio point estimate `<=` the declared limit **and**
95% CI upper `< 1`. A CI lower `>= 1` is criterion-not-met. A CI including 1
with lower `< 1` is inconclusive. Once the interval excludes no improvement, a
point estimate missing the practical threshold is criterion-not-met. Thus
`0.96 [0.94,0.99]` misses the default; `0.94 [0.90,1.00]` is inconclusive.

The no-change p95 guardrail meets the criterion when CI upper `<= 1.10`.
CI lower `> 1.10` demonstrates failure; other intervals crossing 1.10 are
inconclusive, including a lower endpoint equal to 1.10 with a larger upper.
Overall outcomes use this precedence:

1. `invalid-evidence`: any structural/metadata/sample issue; correctness issues
   remain visible, statistics and decisions are omitted.
2. `correctness-failure`: structurally valid records have failed exits or unequal
   digests. Descriptive statistics/conditional performance decisions remain
   visible, and cannot override correctness failure.
3. `criterion-not-met`: either performance decision demonstrates failure as
   defined above, even if the other is inconclusive.
4. `inconclusive`: neither fails, and either needs more uncertainty resolution.
5. `criterion-met`: both statistical decisions meet their rules.

Human summary goes to stdout. JSON is written to `--output`. Exit 0 means
criterion-met, exit 1 means another audit outcome, and exit 2 means a CLI or I/O
error (no complete report promised). `--help` exits 0 without evaluating evidence.
The actual legacy verdict is intentionally not inferred from this audit's
statistics: run `tools/perfcompare` and preserve its own result separately.
