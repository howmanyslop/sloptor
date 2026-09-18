# Synthetic Worker Benchmark

The completed reports stay separate because the first 180-source run uses a
different candidate binary. The 4,200-source run covers cold,
declaration-only, and source-map builds only, so it supplements the 180-source
full matrix instead of replacing it.

The first three reports predate a warm-hint repair. The compiler's warm-up hint looked for a
raw TypeScript build-info name while the compiler writes a Rotor-specific infix,
so intended no-change suppression did not engage. That made no-change a
distinct pre-repair condition. The raw reports remain valid historical
regression evidence. The post-repair 180-source cohort below is separate and
is not pooled with them.

## Reproduction and aggregation

`generate-synthetic-worker-fixture.mjs` and `run-synthetic-worker-abba.mjs`
are public copies of the fixture generator and AB/BA runner. The runner is
execution-locked and requires an explicit `--execute` for a controlled run.

`synthetic-worker-summary.mjs` pairs baseline and candidate records by
scenario, repetition, and iteration. It reports baseline and candidate
medians, their delta, and the paired lower/higher/tied count. It does not pool
different binary hashes or treat unavailable counters as zero.

```sh
mise x -- node tools/bench/synthetic-worker-summary.mjs \
  --input tools/bench/synthetic-worker-results/180-first.json \
  --input tools/bench/synthetic-worker-results/180-corrected.json \
  --input tools/bench/synthetic-worker-results/4200-scaling.json \
  --input tools/bench/synthetic-worker-results/180-final-post-repair.json \
  --input tools/bench/synthetic-worker-results/4200-final-post-repair.json \
  --input tools/bench/synthetic-worker-results/180-early-warmup-ablation.json \
  --input tools/bench/synthetic-worker-results/4200-early-warmup-ablation.json
```

The public runner records numeric `stageWorkMs` values in future reports, with
their timing schema, top-level timing total, and stage semantics. Stage values
are aggregate work milliseconds, so concurrent work can make their sum exceed
the elapsed `timingTotalMs`.

For a comparison where both supplied binaries speak the persistent-daemon
protocol, pass `--baseline-daemon true`. It applies the owned-runtime cleanup
and daemon/Node PID reuse checks to the baseline as well as the candidate, and
records that choice in the report. The default remains `false`.

## Completed cohorts

The 520 retained records ran on macOS arm64 with 14 logical CPUs and Node
22.23.2. Ordinary comparisons use this baseline SHA-256:
`bb63d93ce115f33d9f24f417256d51086bcedd713db643efa721bc4ab937e49b`.

The baseline source revision is `ed34b16ec45a0f56edb29aad4b52f4cf779a3ec9`.
Candidate revisions are `26cacaa` for the first run, `6f0a72b` for the second
and first scaling runs, and `ccab040` for the post-repair runs. All binaries
were built with Go 1.26.6 and `go build -trimpath ./cmd/rotor`. The ablation
uses `ccab040` with only the supplied early-warm patch applied through a Go
build overlay; the working source was unchanged.

| Cohort | Baseline SHA-256 | Candidate SHA-256 | Records | Matched pairs | Fixture |
| --- | --- | --- | ---: | ---: | --- |
| First 180-source run | ordinary | `8de13b8100a4362104946ff1cc3b2878c5813e21a6292b5e04753ac3c4e4961f` | 152 | 76 | Metadata was not recorded by the early runner. |
| Second 180-source run | ordinary | `7b764fa4b2a13cc5da9672c23359979949c0f019c42f2bbb7074e5a18ac5074d` | 152 | 76 | 180 source files; TypeScript 6.0.3; checker-query transform. |
| 4,200-source scaling run | ordinary | `7b764fa4b2a13cc5da9672c23359979949c0f019c42f2bbb7074e5a18ac5074d` | 24 | 12 | 4,200 source files; TypeScript 6.0.3; cold, declaration-only, and source-map only. |
| Post-repair 180-source run | ordinary | `52c412dc4ea289044158fd1f460004f2ed7e8e8659f00281385beb7336627466` | 152 | 76 | 180 source files; TypeScript 6.0.3; timing schema 2 stage fields. |
| Post-repair 4,200-source run | ordinary | `52c412dc4ea289044158fd1f460004f2ed7e8e8659f00281385beb7336627466` | 24 | 12 | 4,200 source files; TypeScript 6.0.3; timing schema 2 stage fields. |
| Early-warm ablation, 180 | `5e3f6284d72684088b671ba4fd9926fd7dc41c203938d601b373e3aaa25f009a` | `52c412dc4ea289044158fd1f460004f2ed7e8e8659f00281385beb7336627466` | 8 | 4 | 180 sources; cold only; both daemon controls enabled. |
| Early-warm ablation, 4,200 | `5e3f6284d72684088b671ba4fd9926fd7dc41c203938d601b373e3aaa25f009a` | `52c412dc4ea289044158fd1f460004f2ed7e8e8659f00281385beb7336627466` | 8 | 4 | 4,200 sources; cold only; both daemon controls enabled. |

The runner completed output equivalence before publishing each report. Lua and
declaration outputs were byte-equal. Source maps were compared as JSON after
replacing only isolated-project absolute paths in `file`, `sourceRoot`, and
`sources`; mappings, names, source content, relative paths, and external
absolute paths remained part of the comparison.

## Measurement limits

`clientWallMs`, `clientUserSeconds`, `clientSystemSeconds`, and
`clientPeakRssBytes` describe the command process measured by macOS
`/usr/bin/time`: the waited-on process and direct children it waits for. They do
not include a persistent worker that outlives that command as request CPU or as
a total RSS number.

`nodeRequestCpu*` is a request counter only when the sidecar emits it. It omits
program-only warm work and deferred control work, so a lower value is not a
claim of lower total CPU. No-change and declaration-only omit it. Watch values
include readiness and artifact observation, and are not CPU attribution.

The machine was not otherwise isolated and OS caches were uncontrolled. The 20
warm observations come from four primed blocks with five iterations per block.
They are paired observations, but not 20 independent cold starts.

## Client wall-time medians

A positive percentage means the candidate median was higher; lower is
favorable. `lower/higher/tied` is a paired count, not a significance test.

| Cohort | Scenario | Baseline | Candidate | Change | lower/higher/tied |
| --- | --- | ---: | ---: | ---: | --- |
| First 180 | cold | 326.5 ms | 367.5 ms | +12.6% | 0/4/0 |
| First 180 | declaration-only | 212.5 ms | 243.5 ms | +14.6% | 0/4/0 |
| First 180 | missing-output-warm | 221.0 ms | 140.0 ms | -36.7% | 20/0/0 |
| First 180 | no-change† | 33.0 ms | 68.0 ms | +106.1% | 0/20/0 |
| First 180 | one-file-warm | 152.0 ms | 109.0 ms | -28.3% | 20/0/0 |
| First 180 | source-map | 259.5 ms | 486.0 ms | +87.3% | 0/4/0 |
| Second 180 | cold | 296.0 ms | 392.0 ms | +32.4% | 0/4/0 |
| Second 180 | declaration-only | 219.5 ms | 254.0 ms | +15.7% | 0/4/0 |
| Second 180 | missing-output-warm | 237.5 ms | 145.5 ms | -38.7% | 20/0/0 |
| Second 180 | no-change† | 33.5 ms | 72.5 ms | +116.4% | 0/20/0 |
| Second 180 | one-file-warm | 165.0 ms | 118.5 ms | -28.2% | 20/0/0 |
| Second 180 | source-map | 263.0 ms | 413.0 ms | +57.0% | 0/4/0 |
| Scaling 4,200 | cold | 1,992.0 ms | 2,614.5 ms | +31.3% | 0/4/0 |
| Scaling 4,200 | declaration-only | 1,388.5 ms | 961.0 ms | -30.8% | 4/0/0 |
| Scaling 4,200 | source-map | 2,300.0 ms | 3,108.5 ms | +35.2% | 0/4/0 |
| Post-repair 180 | cold | 238.0 ms | 357.0 ms | +50.0% | 0/4/0 |
| Post-repair 180 | declaration-only | 213.5 ms | 247.0 ms | +15.7% | 0/4/0 |
| Post-repair 180 | missing-output-warm | 227.5 ms | 131.0 ms | -42.4% | 20/0/0 |
| Post-repair 180 | no-change | 33.0 ms | 33.0 ms | 0.0% | 8/7/5 |
| Post-repair 180 | one-file-warm | 157.0 ms | 101.5 ms | -35.4% | 20/0/0 |
| Post-repair 180 | source-map | 260.0 ms | 385.5 ms | +48.3% | 0/4/0 |
| Post-repair 4,200 | cold | 2,059.5 ms | 2,637.5 ms | +28.1% | 0/4/0 |
| Post-repair 4,200 | declaration-only | 1,469.5 ms | 958.0 ms | -34.8% | 4/0/0 |
| Post-repair 4,200 | source-map | 2,232.0 ms | 3,023.0 ms | +35.4% | 0/4/0 |
| Early-warm ablation, 180 | cold | 351.0 ms | 348.0 ms | -0.9% | 3/1/0 |
| Early-warm ablation, 4,200 | cold | 2,742.0 ms | 2,621.0 ms | -4.4% | 3/1/0 |

†A valid historical pre-repair condition. Do not pool it with the post-repair
no-change cohort.

On the post-repair 180-source run, watch initial readiness was +66.4% and
rebuild was +12.6%, with the candidate higher in all four pairs for each
measure.

## Early-warm ablation

The ablation compares the candidate from source revision
`ccab040ccadef451fb4bbab97dada40a8ac9338d` with a temporary baseline overlay
that changes only `startPersistentSidecarWarmupIfCold` to `return nil`.
The exact overlay is [early-warmup-ablation.patch](early-warmup-ablation.patch).
It is a disposable benchmark input, not a product flag or a committed product
change. Both ablation runs used `--baseline-daemon true`.

Cold wall medians changed by -0.9% at 180 sources and -4.4% at 4,200 sources,
with the candidate lower in three of four pairs in each run. This small ablation
does not account for the ordinary post-repair cold regression. It also does not
separate the other persistence work or establish a total CPU change.

## Post-repair stage attribution

Stages are aggregate work measurements and do not prove a causal split of
wall time. They do identify the stages that rose with the post-repair cold and
source-map regressions:

| Scenario | Stage | Baseline median | Candidate median | Change | lower/higher/tied |
| --- | --- | ---: | ---: | ---: | --- |
| cold | sidecar round trip | 163.5 ms | 260.0 ms | +59.0% | 0/4/0 |
| cold | sidecar preparation | 4.0 ms | 17.0 ms | +325.0% | 0/4/0 |
| cold | response decode | 1.0 ms | 4.0 ms | +300.0% | 0/4/0 |
| cold | overlay program | 1.0 ms | 5.0 ms | +400.0% | 0/4/0 |
| source-map | sidecar round trip | 170.0 ms | 273.0 ms | +60.6% | 0/4/0 |
| source-map | sidecar preparation | 3.5 ms | 17.5 ms | +400.0% | 0/4/0 |
| source-map | response decode | 1.0 ms | 4.0 ms | +300.0% | 0/4/0 |
| source-map | overlay program | 2.0 ms | 5.0 ms | +150.0% | 0/4/0 |
| 4,200 cold | initial program | 69.5 ms | 136.0 ms | +95.7% | 2/2/0 |
| 4,200 cold | overlay program | 91.0 ms | 172.0 ms | +89.0% | 0/4/0 |
| 4,200 cold | response decode | 36.5 ms | 85.5 ms | +134.2% | 0/4/0 |
| 4,200 source-map | overlay program | 91.5 ms | 173.5 ms | +89.6% | 0/4/0 |
| 4,200 source-map | response decode | 36.0 ms | 116.5 ms | +223.6% | 0/4/0 |

## Post-repair medians above 5%

| Scenario | Components with higher candidate median |
| --- | --- |
| cold | wall +50.0% (0/4/0); client user CPU +100.0% (0/4/0); client system CPU +11.8% (0/4/0); client RSS +18.3% (0/4/0); timing total +54.8% (0/4/0); request system CPU +65.4% (0/4/0) |
| declaration-only | wall +15.7% (0/4/0); client user CPU +100.0% (0/4/0); client system CPU +9.7% (0/4/0); client RSS +17.2% (0/4/0); timing total +17.3% (0/4/0) |
| missing-output-warm | client user CPU +100.0% (0/20/0); client system CPU +11.8% (2/15/3); client RSS +15.7% (0/20/0) |
| one-file-warm | client user CPU +200.0% (0/20/0); client system CPU +12.5% (3/13/4); client RSS +21.0% (0/20/0) |
| source-map | wall +48.3% (0/4/0); client user CPU +100.0% (0/4/0); client system CPU +5.6% (1/3/0); client RSS +13.8% (0/4/0); timing total +52.6% (0/4/0); request user CPU +19.1% (0/4/0); request system CPU +54.0% (1/3/0) |
| watch | initial readiness +66.4% (0/4/0); rebuild +12.6% (0/4/0) |
| 4,200 cold | wall +28.1% (0/4/0); client user CPU +10.1% (0/4/0); timing total +32.4% (0/4/0); request system CPU +37.1% (0/4/0) |
| 4,200 source-map | wall +35.4% (0/4/0); client user CPU +12.2% (0/4/0); client RSS +29.4% (0/4/0); timing total +36.6% (0/4/0); request system CPU +40.6% (0/4/0) |

The summary emits all timing components and stage summaries. Keep the
post-repair cohort alongside, rather than pooled with, the historical
no-change data.
