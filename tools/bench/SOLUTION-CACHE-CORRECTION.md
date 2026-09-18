# Solution-cache correction benchmark

This compares `9aecb049a5ea9feba23d1a9bf6145f44fa86c8f8` with
`d27f8849b2435325f09eecfff845b502919e73e2`. It isolates the final
directory-membership freshness correction. It does not replace the committed
44-observation report for the earlier `ed34b16` to `9aecb04` cache change.

The [sanitized raw records](pr59-solution-cache-pinned-node22-raw.json) have
96 timed records: a four-pair primary cohort and an independent eight-pair
confirmation cohort, with alternating AB/BA order. Each fixture has 14 projects
with 300 sources each. Every timed invocation received an isolated fixture copy;
no-change and one-file builds were primed first.

The earlier 32-row exploratory cohort is retained as local-only audit evidence,
not a repository artifact. Its nested compiler subprocesses used Node 26.8.1
through an inherited path, so it is excluded from the medians below.

The reported binaries used Go 1.26.6 with `-trimpath -buildvcs=false`.
Their SHA-256 values are `5854d7e1ae20fb0de52c9f51a60aacdf3549d40c997e95f073162d7f7a45c3fa`
for 9aecb04 and `da049788edad7099acb3f1ebfd23c47c492da0a4bd17ac74dac90f58363d0acc`
for d27f884. Every run that started a sidecar reported Node 22.23.2; no-change
runs made no sidecar request and therefore recorded no Node version. Both
fixtures used project TypeScript 6.0.2. The
measurements ran on macOS arm64 with 14 logical CPUs. `/usr/bin/time -lp`
supplied wall, CPU, and peak-RSS values.

## Pinned-runtime medians

Medians use all 12 pairs per scenario and average the two middle values.
Negative deltas favor d27f884.

| Scenario | Wall (ms) | CPU (s) | Peak RSS (MiB) |
| --- | --- | --- | --- |
| Cold | 2162.5 to 1944.0 (-10.1%) | 9.535 to 8.700 (-8.8%) | 208.1 to 192.7 (-7.4%) |
| No change | 579.0 to 612.5 (+5.8%) | 2.760 to 2.700 (-2.2%) | 108.7 to 110.5 (+1.6%) |
| One file | 687.5 to 613.5 (-10.8%) | 2.885 to 2.200 (-23.7%) | 111.6 to 115.2 (+3.2%) |
| Declaration only | 1472.0 to 1398.5 (-5.0%) | 5.685 to 5.045 (-11.3%) | 175.3 to 157.8 (-10.0%) |

All 48 baseline/candidate scenario pairs matched their emitted-output digest,
runtime-library digest, diagnostic digest, compiled-file count, and output-file
count. No-change counters also matched: 4,200 total sources; zero selected
sources, emitted entries, writes, sidecar spawns, and prepared directories; and
325 parse-cache hits with 25 misses.

## No-change regression investigation

The no-change wall median is an observed regression: +5.8% over all 12 pairs.
The primary four-pair cohort was near flat (+0.9%, candidate slower in 1/4),
while the independent confirmation cohort was +9.9% (candidate slower in 7/8).
The corresponding CPU and RSS medians disagree across those cohorts: primary
CPU was -28.4% and RSS +0.6%; confirmation CPU was +23.9% and RSS +6.0%.
That spread, together with uncontrolled OS caches and ordinary host scheduling,
means the samples establish the wall result but do not isolate a stable CPU or
RSS regression.

The most specific stage signal is incremental selection: its combined no-change
median rose from 175.0 to 424.5 ms. This stage includes Rojo-cache loading, but
also the rest of incremental selection, so it cannot assign the increase to one
operation.

The only production code delta between these two commits adds a direct
directory-membership digest to Rojo resolver-cache freshness validation. The
new validation calls `os.ReadDir` and hashes each walked directory's names and
types whenever a resolver snapshot is checked. It is therefore a plausible
source of extra no-change work, but this benchmark does not prove causation.
The correctness reason is concrete: the accompanying regression test keeps a
predecessor output directory's mtime fixed after new entries appear; the
dependent must reject the stale shared Rojo snapshot and use the new mapping.

This is a macOS-only performance result. OS caches were deliberately neither
flushed nor called cold. Linux and Windows CI establish correctness there; they
do not establish performance on those platforms.

## Reproduce

Prerequisites:

- macOS, because the harness uses `/usr/bin/time -lp` and rejects other hosts.
- Clean source worktrees at the two commits, with Go 1.26.6 available to build
  both binaries.
- Node 22.23.2 before any other `node` in `PATH`. The compiler starts Node as a
  child process, so choosing the harness executable alone is insufficient.
- A TypeScript 6.0.2 installation directory, shared by the two fixtures.

Set the paths for an isolated scratch directory and the two source worktrees:

```sh
ROOT=/path/to/solution-cache-benchmark
BASELINE_SOURCE=/path/to/rotor-9aecb04
CANDIDATE_SOURCE=/path/to/rotor-d27f884
NODE=/path/to/node-v22.23.2/bin/node
TYPESCRIPT_DIR="$CANDIDATE_SOURCE/node_modules/typescript"
export PATH="$(dirname "$NODE"):$PATH"

mkdir -p "$ROOT/bin" "$ROOT/results"
(cd "$BASELINE_SOURCE" && go build -trimpath -buildvcs=false -o "$ROOT/bin/sloptor-9aecb04" ./cmd/rotor)
(cd "$CANDIDATE_SOURCE" && go build -trimpath -buildvcs=false -o "$ROOT/bin/sloptor-d27f884" ./cmd/rotor)

"$NODE" "$CANDIDATE_SOURCE/tools/bench/synthetic-solution.mjs" \
  --root "$ROOT/fixture" \
  --typescript "$TYPESCRIPT_DIR"

(cd "$CANDIDATE_SOURCE" && "$NODE" tools/bench/solution-build-performance.mjs \
  --template "$ROOT/fixture" \
  --baseline "$ROOT/bin/sloptor-9aecb04" \
  --candidate "$ROOT/bin/sloptor-d27f884" \
  --baseline-source 9aecb049a5ea9feba23d1a9bf6145f44fa86c8f8 \
  --candidate-source d27f8849b2435325f09eecfff845b502919e73e2 \
  --output "$ROOT/results/primary" \
  --repetitions 4 \
  --scenarios cold,no-change,one-file,declaration-only)
```

Run the final command again with a fresh output directory and `--repetitions 8`
for the independent confirmation cohort.
