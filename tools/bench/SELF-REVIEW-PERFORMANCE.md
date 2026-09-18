# Protocol-3 worker self-review

I kept these two awake cohorts separate. They compare the persistent-worker
implementation at `ccab040` with protocol-3 candidates, and do not compare
either variant with the original non-persistent worker. They are useful
regression evidence for this synthetic fixture, not a general performance
claim.

| Cohort | Baseline source / SHA-256 | Candidate source / SHA-256 | Records / pairs |
| --- | --- | --- | ---: |
| Initial protocol-3 review | `ccab040` / `52c412dc…7336627466` | `7bd55b5` / `19d9102c…1205eec23df` | 152 / 76 |
| Path-memoized protocol-3 review | `ccab040` / `52c412dc…7336627466` | `e6703d8` / `b0192374…82ae93c4d0` | 152 / 76 |

The complete binary hashes and every raw observation are retained in
[the initial report](synthetic-worker-results/180-self-review-initial.json) and
[the path-memoized report](synthetic-worker-results/180-self-review-final.json).
Both used macOS arm64, 14 logical CPUs, Node 22.23.2, and the same 180-source
fixture. Each has four AB/BA repetitions; cold, declaration-only, source-map,
and watch have four pairs, while each warm scenario has four primed blocks with
five iterations, or 20 pairs. Those 20 warm measurements are paired
observations, not 20 independent cold starts.

`--baseline-daemon true` gave both binaries isolated daemon runtimes,
owned-daemon shutdown verification, and daemon/Node PID reuse checks in warm
blocks. Lua and declaration output matched byte-for-byte. Source maps matched
after normalizing only fixture-contained absolute paths in `file`,
`sourceRoot`, and `sources`; mappings, names, source content, relative
paths, and other absolute paths remained compared. The missing-output probe
also required restored outputs, the prime selected-source set, a sidecar
request, and worker PID reuse.

Two sleep-affected self-review attempts are intentionally absent. The retained
cohorts ran under a scoped wake lock, and the corresponding power-event logs
have no sleep or wake event during either benchmark interval.

## Wall-time medians

A positive value means the candidate median was higher. The paired count is
descriptive rather than a significance test.
Parentheses show candidate-lower, candidate-higher, and tied pairs.

| Scenario | Initial protocol-3 review | Path-memoized protocol-3 review |
| --- | --- | --- |
| cold | 344.5 → 351.5 ms, +2.0% (1/2/1) | 383.5 → 358.5 ms, -6.5% (4/0/0) |
| declaration-only | 234.5 → 236.5 ms, +0.9% (1/3/0) | 253.0 → 253.0 ms, 0.0% (2/2/0) |
| no-change | 32.0 → 32.0 ms, 0.0% (4/12/4) | 35.0 → 34.0 ms, -2.9% (14/3/3) |
| one-file warm | 96.0 → 102.0 ms, +6.3% (0/20/0) | 103.0 → 105.5 ms, +2.4% (7/13/0) |
| missing-output warm | 125.0 → 132.0 ms, +5.6% (0/19/1) | 136.5 → 136.5 ms, 0.0% (9/10/1) |
| source-map | 368.5 → 375.5 ms, +1.9% (0/3/1) | 399.0 → 390.0 ms, -2.3% (4/0/0) |
| watch initial readiness | 374.0 → 398.5 ms, +6.6% (0/4/0) | 405.0 → 405.0 ms, 0.0% (3/1/0) |
| watch rebuild | 226.5 → 210.0 ms, -7.3% (4/0/0) | 211.5 → 219.0 ms, +3.5% (2/1/1) |

The path-resolution memoization review no longer has a wall-time regression
above five percent. Its cold improvement and smaller warm deltas are confined
to this cohort. The two reports have different candidate hashes and are not
pooled, so this is not a causal microbenchmark of the memoization by itself.

The initial cohort's one-file and missing-output wall regressions moved with
sidecar round-trip medians: +5.0 ms (+12.2%) and +6.5 ms (+14.4%), respectively.
In the path-memoized cohort, those changes were +3.0 ms (+6.8%) and +1.5 ms
(+3.1%). Aggregate stage work may overlap, so it is not an additive or causal
wall-time breakdown.

## Payload, CPU, RSS, and residency

Protocol 3 sends the config-parse snapshot with transform and validation
requests. The payload-bearing scenarios selected the same source counts in
both protocols, while the protocol-3 request medians grew modestly:

| Scenario | Selected sources | Request bytes, protocol 2 → 3 | Response bytes, protocol 2 → 3 |
| --- | ---: | ---: | ---: |
| cold | 180 | 75,076 → 77,459 (+3.2%) | 62,163 → 62,343 (+0.3%) |
| one-file warm | 1 | 58,470 → 60,693 (+3.8%) | 742 → 743–744 (+0.1–0.3%) |
| missing-output warm | 180 | 83,386 → 85,799 (+2.9%) | 64,859 → 65,039 (+0.3%) |
| source-map | 180 | 101,511 → 104,086 (+2.5%) | 200,217–218 → 200,938 (+0.4%) |

No-change selected zero transforms and emitted no sidecar payload. The
declaration-only path does not emit these payload counters. The added snapshot
and request-size changes are real, but the data does not establish that either
one alone causes any elapsed-time change.

The macOS client user/system CPU and peak-RSS values describe the compiler
client and direct child work it waits for. They do not include a persistent
worker detached from that command. `nodeRequestCpu*` values are only
per-request counters; they omit program-only warm work and deferred control
work, and absent values stay absent rather than becoming zero. They therefore
do not support a total CPU claim.

The initial warm observations had higher request CPU counters, while the
path-memoized review narrowed them: one-file user/system medians changed from
7.8/3.3 ms to 10.5/6.8 ms initially, then 8.8/3.8 ms to 9.8/5.8 ms after the
memoization. Missing-output changed from 24.6/2.7 ms to 30.1/6.9 ms initially,
then 27.3/3.4 ms to 28.5/3.8 ms. This is request-level attribution only.

All warm blocks preserved their daemon and Node worker PIDs. Both reports
observed a Node worker for every non-watch scenario and both watch phases.
The sampled Node worker RSS falls around 116–143 MiB across scenarios, with
daemon samples around 22–28 MiB. These are instantaneous post-run samples,
not per-request RSS or a daemon-wide total. The fixture has one project and
does not exercise an idle-worker retention or eviction policy.

OS caches and unrelated machine activity were not controlled. Keep the two
source-hash cohorts alongside the existing historical benchmark records; none
of the historical 520 records are replaced by this review.
