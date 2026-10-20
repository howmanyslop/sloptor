# Resident raw reader and statistical core

`perfaudit --resident-plan plan.json --output new-audit.json` independently
reconstructs final launcher schema2 evidence into the reviewed statistical core.
It reads files only; it does not collect data or run a compiler/launcher.
Numerical decisions remain conditional on explicit root admission assertions.
The reader cannot attest physical provenance, clock precision, quietness,
semantics or independence. Every result keeps `promotion_ready=false`.

Authority is the frozen resident policy SHA256
`27053c5e368cd98e22911c87cab9135a0fbc9f40857d38e006a3343301231c0d`.
The existing original and fresh-process resource modes are separate populations;
their duration-ms normalization and 30/100-pair counts are never used here.

## Typed interface

The in-package data and arithmetic interface is:

- `ResidentKey`: comparator, scenario, settings and checked policy role.
- `ResidentBatch`: explicit input label, ID, positive campaign chronology,
  key, purpose, stage and ordered blocks.
- `ResidentBlock`: one-based position and four chronological sessions, ABBA.
- `ResidentSession`: unique ID, launch, pair, AB/BA order, A/B arm, semantic
  admission, all 30 original cycle positions and whole-session native resources.
- `AnalyzeResident([]ResidentBatch) ResidentSummary`: one complete initial,
  follow-up or confirmation, or chronological initial plus follow-up.
- `EvaluateResident(ResidentCampaign) ResidentCampaignResult`: one leaf's
  calibration, comparison, permitted follow-up and independent confirmation.
- `EvaluateResidentCalibration(batches, prerequisites) ResidentTrackResult`:
  calibration initial/follow-up decisions before candidate data exist.
- `EvaluateResidentQuestion(primary, secondary) ResidentQuestionResult`:
  matching predeclared questions, both independently confirmed, without pooling.

These are typed Go data contracts. `invented-synthetic` batches require
`synthetic-assumed` hypothetical prerequisites. The reader labels recorded
batches `admitted-recorded`; their campaign requires `root-declared`
prerequisites. Labels cannot be mixed across a campaign/calibration track.
Neither label certifies collection; `AnalyzeResident` is arithmetic only.
The reader owns its decoded slices, pointers and immutable byte snapshots for
its complete analysis lifetime. Caller-supplied typed durations are never a
substitute for raw files at the CLI boundary.

Stages are `initial`, `follow_up`, `confirmation`; purposes are `calibration`
and `comparison`. Each initial/confirmation has five blocks, ten session pairs,
20 processes and 300 cycles per arm. A follow-up has ten blocks, 20 pairs,
40 processes and 600 cycles per arm. Joint analysis preserves 5+10 strata.
Launch positions are one-based within a batch. Batch chronology describes the
caller's campaign journal. The raw reader additionally checks recorded UTC
ordering and QPC enclosures; it does not attest those observations.

The raw plan's batch ordinals are positive and unique across all supplied
campaigns. A separate ordinal-sorted validation view checks forward,
nonoverlapping recorded UTC intervals; gaps and frozen primary/secondary
interleavings are allowed. Supplied arrays and populations keep their original
order. A declaration cannot override contradictory recorded chronology.
Invocation enclosure endpoints are retained as signed integer ticks. Successive
invocations within a batch, and across batches with identical validated host
snapshot/support bindings and frequency, must have a gap of at least minus one
tick. Endpoint subtraction uses arbitrary precision, including signed extremes.
Unrelated host bindings do not establish a common QPC origin. These consistency
checks establish neither physical host continuity nor calibrated precision.

Arm A is the comparator's reference; arm B is its comparison arm. Each block
launches A, B, B, A, pairing positions 1/2 as AB and 3/4 as BA. Calibration uses
the comparator's identical reference, with the same arm/order bookkeeping.
Session IDs cannot recur across supplied stages, calibration/comparison, or a
primary/secondary question. Reported raw observations are the typed inputs;
the raw reader additionally retains the original raw evidence identities and bytes.

Cycle nanoseconds are positive `uint64`. Required resource pointers distinguish
absent values from measured zero. User and kernel CPU are native 100 ns ticks;
their arbitrary-size sum is divided by exactly 10,000 for ms/session. Peak is
simultaneous job committed bytes. The six IO slots are read/write/other
operations, then read/write/other bytes. They cover complete owned sessions,
including startup, setup, warm, mutation, report and cleanup. They exclude the
outer launcher's own costs. No resource is divided by 30 or interpreted as
isolated cycle CPU/IO, RSS, retained heap or durable IO.

Invalid counts/order/positions, zero cycles, duplicate identities, unknown
inventory or unexpected semantic labels reject without partial statistics.
Missing required native counters are `incomplete-unmeasured`; malformed typed
evidence takes precedence. Oversized input counts are rejected before traversal.
All supplied inputs remain in the returned report, including invalid later data.
The functions read their inputs without mutation; callers must treat retained
slices and resource pointers as immutable while using the results.

## Estimates and draws

One SplitMix64 stream, seed 20261018, supplies exactly 20,000 replicates per
analysis. It rejects words below `2^64 mod n` before modulo. The seed resets for
each analysis. Every selected block carries both pairs, all matched positions
and all four process resource records. Joint draws are replicate-major, initial
stratum first, then follow-up, at their original block counts.

Reports retain the SHA256 of local uint32 little-endian selected block indices
and the first two draws with global block offsets. One multiplicity table feeds
all metrics, full/AB/BA populations and reversals. There is no per-cycle sampling,
per-metric stream, independent order resampling or variable production draw count.

Benefit is `exp(median(log(candidate_ns/reference_ns)))`. Its point and type-7
central 95% log endpoints are exponentiated. Symmetric reversal negates the same
log draws. This estimator deliberately uses approximate float transcendental
arithmetic, with the declared inequalities and no epsilon or boundary snapping.

Named marginal cycle median/p95 rows use all selected original cycle positions.
Resource rows use complete original session observations. Means are arithmetic,
even medians average the two central observations and p95 is nearest rank.
Every session retains its own original distribution. Full/AB/BA estimates and
controls are reported; full-population guards decide the leaf. Order strata are
diagnostic, and their log points/endpoints participate in calibration noise.

Integers and rationals control all marginal decisions. The core reuses the
reviewed `resourceExactSamples.direction`, rational type-7 interpolation and
exact-payload control helpers from the dependency. Means carry `exact_mean`;
median/p95 rows carry `exact_marginal`. The fractions remain authoritative after
JSON marshal/unmarshal; display floats cannot replace them. Reverse marginal
draws are formed before quantiles, not by inverting forward endpoints.

Any point or draw denominator of zero suppresses the entire ratio, including
percentage change. Zero-draw counts and all difference draws remain. An
available absolute route uses the complete difference distribution. There are
no discarded draws or pseudocounts. Totals are retained for additive quantities;
peak memory has no total or mean gate.

## Guard units and decision routes

| Population/statistic | Relative limit | Absolute allowance |
| --- | ---: | ---: |
| Cycle median | 1.05 | 50,000 ns/cycle |
| Ordinary edited cycle p95 | 1.10 | 100,000 ns/cycle |
| Rewrite/Invalidate/EmptyDrain/mixed no-mutation p95 | 1.10 | None |
| Session CPU mean and median | 1.05 | 125/8 ms/session |
| Peak job commit median | 1.10 | 2 MiB/session |
| Peak job commit p95 | 1.10 | 4 MiB/session |
| Each IO operation counter mean and median | 1.10 | 16 ops/session |
| Read/other bytes mean and median | 1.10 | 64 KiB/session |
| Write bytes mean and median | 1.10 | 4 KiB/session |

Pass when the ratio upper endpoint meets its limit OR the difference upper
endpoint meets its allowance. Fail only when both lower endpoints strictly
exceed their budgets; otherwise remain inconclusive. A suppressed ratio uses
the absolute-only route. Ratio-only rows use inclusive upper pass and strict
lower failure. Decisive routes and percentage changes disclose accepted costs
through absolute floors. These floors are permitted costs, not clock resolution.

Calibration applies all full-population guards in both directions using shared
draws. Noise N retains the maximum absolute log point or endpoint over
full/AB/BA and every supplied initial/follow-up/joint calibration summary.
A biased zero-width interval retains its center.

Practical benefit precedence is: lower >= 1 fails; else upper >= 1 is
inconclusive; else point > .95 fails; else passes. Noise cutoff is `exp(-N)`:
upper strictly below passes, lower >= cutoff fails, otherwise inconclusive.
Every benefit and control gate must pass; required failures dominate uncertainty.

Only a valid inconclusive initial can authorize one whole doubled follow-up.
Standalone AND stratified joint must pass. Initial failure has no rescue;
independent ten-pair confirmation has no retry or pooling. Invalid/incomplete
later evidence dominates routing. A supplied follow-up after an initial pass is
invalid protocol evidence. Unverified/failed prerequisites never authorize a
statistical retry. `synthetic-assumed` prerequisites are explicit hypothetical
assumptions only; external gates remain unverified even on a numerical pass.

## Inventory and remaining gates

B:S contains the original 18 leaves; B:C and S:C contain all 30. B comparisons
retain the predefined original and mixed stale API leaves as semantic-cost
reports, with no equal-output benefit/regression status. API_X1/API_X10 remain
same-output controls. B:S has four semantic-cost leaves; B:C has eight; S:C zero.

The raw report folds outcomes in this order: `invalid-evidence`,
`correctness-failure`, `incomplete-unmeasured`, `external-unverified`,
`criterion-not-met`, `inconclusive`, `semantic-cost`, then `criterion-met`.
Absent or unrecognized internal outcome values are invalid evidence, never an
implicit pass. Semantic cost remains descriptive at the top level and in nested
estimates; it returns exit1 and requires root adjudication. It is neither
acceptance nor an equal-output criterion failure. Recorded successful track
actions name admitted recorded data conditional on root declarations; invented
tracks retain the original hypothetical synthetic wording. Neither permits
promotion or changes the numerical transition rules.

B:S/B:C primary and secondary are Chain32/C1B1 and Chain32/C4B4. S:C primary is
MixedAPI_M1/C1B1; secondary is MixedAPI_M0/C4B4. Other leaves are controls or
semantic costs. Control-only leaves need no benefit or confirmation. A question
requires its own primary AND secondary decisions and independent confirmations;
all other required matrix leaves remain a separate campaign-wide obligation.

RP-R1-P01 is enforced by excluding all supplemental benchmark means from this
numerical interface. The 81 mean populations require root's separate signed
leaf/metric dispositions, including adverse mean/median/order/concentration
signals. No mean-only pass, resampling, follow-up or promotion route exists.
RP-R1-P02 is implemented by the benefit precedence and stage transitions above.

The reader implements raw schema2 consistency, strict JSON, native layouts,
QPC/remainder/enclosure reconstruction and explicit CLI dispatch. Physical
source-to-binary provenance, actual collection, effect precision, quiet-host
admission, empirical A/A, semantics/writes/mtimes, supplemental and semantic-cost
adjudication, independent reader review and final acceptance remain external.
Raw clock invalidity and one-tick readiness cannot become statistical retry
permission. The six retained EmptyDrain diagnostics remain diagnostic input,
including their distinct `__MISE_SESSION` hashes; they cannot supply A/A or score
populations. Finite clock-readiness closure does not establish future effect
precision or literal environment equivalence.

## Frozen raw interface

The adapter binds launcher `bcdfcfbc558f31e3b74999d6420f466f6e5aefa8`, collector
`518a2f4b0bc3ea1cf021ad4ba281e210abc64fd9` and capture helper
`49f2b69d43af4610ca2bac55480cfec331f1512d`. Recorded evidence must match the
reviewed B/S/C binaries, eight collector sources, collector interface, policy
and corresponding production revisions. Source/build/overlay/fixture/dependency
metadata proves internal consistency only. It does not prove what physically
executed or independently reconstruct every compiler dependency graph.

The outer JSON plan is separate from the launcher's plan. All referenced paths
are original absolute local paths; relocation or timestamp/environment
normalization is unsupported. A minimal calibration-only shape is:

```json
{
  "schema": 1,
  "evidence_kind": "recorded-evidence",
  "policy_sha256": "27053c5e368cd98e22911c87cab9135a0fbc9f40857d38e006a3343301231c0d",
  "admission": {"path": "D:/evidence/root-admission.json", "sha256": "<exact SHA256>"},
  "campaigns": [{
    "key": {
      "comparator": "B:C", "scenario": "Body_L32F32",
      "settings": "C1B1", "role": "control"
    },
    "calibration": [{
      "directory": "D:/evidence/AA-initial",
      "plan": {
        "path": "D:/plans/AA-initial.json", "sha256": "<exact SHA256>"
      },
      "chronology": 1
    }],
    "comparison": []
  }]
}
```

`calibration` contains initial and at most one follow-up. `comparison` contains
initial, at most one follow-up, then required independent confirmation for
benefit leaves. Raw stages map `followup` to typed `follow_up`; calibration raw
stages have the `calibration-` prefix. Two campaigns represent the predefined
primary then secondary question; they are evaluated independently and ANDead.
Reusing A/A between separate analyses provides shared nuisance evidence only,
not independent replication. Candidate and calibration must match exact
reference path/binding, fixture, host/environment and declared conditions.

Admission is a separate frozen JSON file with `schema:1`,
`kind:"root-resident-admission-v1"`, `policy_sha256`, RFC3339 `declared_utc`,
`prerequisites`, and `batches`. The seven prerequisite keys are
`clock-suitability`, `effect-precision`, `quiet-host`,
`source-build-fixture-dependencies`, `semantic-write-mtime-persistence`,
`independent-sessions`, and `frozen-protocol-chronology`. Each explicitly states
`admitted`, `unverified`, or `failed`. Every supplied batch needs one row with
`plan_path`, `plan_sha256`, `batch`, positive `chronology`, `host_sha256`,
`fixture_sha256`, and RFC3339 `authorized_utc`. Both declaration and authorization
must precede that batch; authorization cannot precede the declaration. These
are bindings of root's assertions, not independent proof of physical facts or
of root's actual review. Actual stage authorization remains root's obligation.

Missing/failed/unverified prerequisites or a one-tick cycle produce
`external-unverified` and cannot authorize statistical follow-up. Malformed or
contradictory evidence is `invalid-evidence`; absent required native counters
are `incomplete-unmeasured`. Neither condition selects successful sessions.
Optional producer error fields must be absent or actual empty JSON strings:
report `error`/`firstFailure`, cycle `error`, clock
`initializationError`/`verificationError`, enclosure `call_error`, and
clock-assessment `error`. Explicit null is invalid at these string boundaries,
as are nonstring production-manifest entries. Literal manifest string lines,
including allowed empty lines, retain exact hashing. This follows the pinned
collector string declarations and launcher text/manifest checks. Native C#
`Error`/`CompletionError` and retention error fields have separate schemas;
their required successful null values remain valid.
Diagnostic copies use deterministic path/kind/message ordering. This does not
reorder raw journals, snapshots or observations. The report retains every
supplied raw stage, native original serialization,
outer partial annotation, raw snapshots, exact reconstructed observations,
per-stage decisions, shared draws and unchecked external gates.

`invented-protocol` requires an explicit `INVENTED SYNTHETIC PROTOCOL` source
marker and `kind:"invented-root-assertions-v1"` declaration. It uses the same
raw joins and numerical formulas but allows clearly invented source descriptors;
it cannot establish anything about recorded execution. It must never relabel
collected diagnostic data as synthetic. Tests build files directly and invoke
no collector/compiler/launcher. A reproducible invented recipe is:

```powershell
$env:GOPROXY = 'off'
$env:GOTOOLCHAIN = 'local'
$env:RESIDENT_READER_ARTIFACT_DIR = 'D:/new-empty-evidence/invented'
go test ./tools/perfaudit -run '^TestResidentReaderRawPositive$' -count=1 -timeout=220s
# Use the printed/test-created TestResidentReaderRawPositive-*/reader.json:
# perfaudit.exe --resident-plan <reader.json> --output <new-outside-batch-audit.json>
```

The embedded test policy is an exact compressed copy checked against its frozen
SHA256; it is policy text, not collected observations. Optional environment
paths enable read-only tests of retained diagnostics/stubs; ordinary portable
invented tests need no external evidence or installed compiler dependencies.
Real analysis uses the same CLI only after root separately constructs the plan
and declarations from admitted complete collections. No real recipe is executed
by this implementation.

Output uses exclusive creation. Existing outputs, input aliases and output
inside retained batch directories are refused. Write/close failures report exit2
and preserve any partial output. Exit0 means the supplied numerical question or
calibration met its conditional criteria; exit1 reports an evidence/non-pass
result; usage or output failures exit2. Original manifest and `--resource-plan`
flags cannot be mixed with `--resident-plan`.

## Source-level bounds and interpretation

A valid leaf campaign contains at most 35 blocks across calibration and
comparison, 140 sessions and 4,200 cycles. One analysis has at most 15 blocks,
60 sessions and 1,800 cycles (900 per arm). Reports retain inputs, per-session
distributions, stage summaries and joint membership, with repeated typed data.
Typed entry points retain caller-owned input. Their valid-population counts do
not bound the raw reader's rejected-input retention below.

The draw table is 20,000 arrays of 15 bytes. Marginal observations are sorted
once and weighted ranks reproduce expanded block samples. Each row retains
20,000 exact numerator pairs temporarily, plus rational sorting buffers and a
cache bounded by the 20,000 multiplicity patterns. Log medians use the same
weights. Work scales with draws, source observations, metrics and order strata;
there is no sampling until favorable or unbounded convergence loop. These are
source bounds, not measured peak-memory, throughput or OOM guarantees.

Intervals are approximate per-row finite-block evidence, not exact coverage or
family-wide guarantees. Five initial blocks cannot establish unseen rare tails,
stationarity, long-run retention, cancellation promptness or watcher latency.
Test execution times establish only bounded checks and imply no compiler gain.

Raw snapshot bounds are 16MiB per nonbinary file, 256MiB per binary and 512MiB
total unique captured bytes. A valid raw plan has at most two campaigns and ten
batches, conservatively bounded by 400 invocation clock observations and ten
declared host/frequency domains. The parser permits up to three calibration and
three comparison references per campaign. A third calibration reference records
an invalid-evidence issue while reconstruction retains every supplied reference.
Thus malformed plans can retain up to twelve batches, 480 clock observations
and twelve declared domains before rejection, without numerical analysis.
Arrays exceeding the parser's caps are rejected before batch reconstruction.
Per-batch bounds include 512 retained rows, 128 input rows and 30 cycles per
session; journal traversal is bounded by the planned 20 or 40 invocations.

Deep JSON is limited by the existing strict scanner. Original bytes are held
once per path, with separate raw report encoding copies; binary contents stay
private and output retains their hash/length. The unique-snapshot byte bound
does not include decoded objects, repeated report populations or exact
arithmetic buffers. These allocations add memory beyond those byte bounds;
neither the valid nor rejected-input count bounds establish an OS memory cap or
a measured OOM guarantee. Oversized or changing snapshots, links/reparse
ancestors, nonlocal/device paths and ambiguous native numeric lexemes are
rejected. Snapshot ownership establishes consistency during this analysis, not
hostile-owner isolation or historical physical authentication.

The two-campaign question wrapper performs at most 28 bounded stage analyses
(including its independent question evaluation); it never extends the fixed
20,000-draw stream or samples until a favorable result.
