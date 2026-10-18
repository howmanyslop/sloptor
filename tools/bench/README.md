# Windows Build Benchmark Harness

`windows-build-performance.ps1` compares prebuilt baseline and candidate binaries
on two isolated copies of one canonical fixture. Each invocation is a fresh
`build --project <work-copy> --json` process. It covers single-project builds;
solution builds, watch sessions, and resident sidecar reuse need separate tools.

## Preparation checks

Run the portable self-test with Go on PATH:

```powershell
pwsh -NoProfile -File tools/bench/windows-build-performance.ps1 -ValidateOnly
```

It checks AB/BA order, the cold-cleanup allowlist, fixture immutability,
environment equality, and schema compatibility. Its fabricated two-cold-pair
manifest must be rejected with `cold pairs = 2, want at least 10`.

Run the adversarial process tests on Windows with Go and Git:

```powershell
pwsh -NoProfile -File tools/bench/windows-build-performance.tests.ps1
```

These prebuild a small Go stub, then test compiler persistence versus delayed
evidence writes, inherited-stream completion, timeout/cancellation tree ownership, nonzero
exits, invalid JSON, launch failures, missing evaluator prerequisites, caller
cwd, binary/fixture/clean-HEAD changes, and cleanup traversal/junction guards.
They also check the unchanged evaluator using fabricated threshold fixtures.
They run no Rotor performance scores. The printed evidence directory retains
the synthetic logs and identities; optional `-EvidenceRoot` must name a new path.

## Controlled measurement

Finish builds, installs, tests, and profiling before the measurement owner grants
a quiet host window. Keep the fixture and input Git worktrees clean, prebuild
both binaries with recorded flags, and select a fresh, empty evidence directory
outside the repository, fixture, and binary input roots. Go and the evaluator
sources must exist before compiler invocations begin.
Use a regular-file fixture tree: input/evidence path junctions and fixture
reparse points are rejected so copies and snapshots stay inside their roots.

```powershell
pwsh -NoProfile -File tools/bench/windows-build-performance.ps1 `
  -BaselineExe C:\rotor-bench\baseline\rotor.exe `
  -CandidateExe C:\rotor-bench\candidate\rotor.exe `
  -Fixture C:\rotor-bench\fixture `
  -ColdPairs 10 `
  -NoChangeRuns 20 `
  -SidecarTimeout 300s `
  -EvidenceDir C:\rotor-bench\evidence-new
```

Optional `-BaselineSourceSha` and `-CandidateSourceSha` record full source SHAs
independently; they must match embedded Go VCS revisions when available. A and B
may come from different revisions. `-FixtureIdentity` records a nonsecret fixture
label or provenance reference. Provided identities are caller claims when the
artifact has no embedded VCS metadata. Keep build commands/flags in the external
experiment record: the harness observes Go build settings and stores hashes of
their values, exposing only public toolchain/VCS/target fields. It also records
PowerShell/Go/Git/available Node versions, fixture package/lock/config hashes,
selected installed package versions, repository HEAD/status, canonical fixture
digest, and executable hashes before and after the run. Snapshots detect changes
across the run; they cannot attest that an input was never changed and restored
between snapshots. Environment values are hashed; secret values and raw build
flags are not recorded. Compiler stdout/stderr are retained verbatim, so use
fixtures whose diagnostics are appropriate to retain.

## Timing and evidence

The existing schema-1 integer `duration_ms` remains rounded to milliseconds with
a 1 ms floor. It starts immediately before process launch and stops after both
process exit and redirected stdout/stderr completion. This includes the
compiler's own output persistence and process startup, plus capture/ownership
overhead. It excludes harness evidence-log writes, JSON parsing, output hashing,
and journal writes. Invocation results separately report `EvidencePersistenceMS`
for writing captured stdout/stderr; this is elapsed API write time, without a
durable-disk-flush guarantee. A descendant retaining an inherited stream keeps
the invocation open until completion or its timeout. Timeouts are unscored;
their recorded duration ends at detection and excludes termination/drain cleanup.

`invocations.ndjson` appends a `started` record before launch and a `finished`
record after evidence persistence and result validation. Both name the raw logs
and invocation role; a failed finish is explicitly `incomplete`. A successful
invocation followed by a failure remains in the journal. Timeout, nonzero exit,
invalid public JSON, or `ok:false` stops the run and preserves captured stdout,
stderr, records, and `run.json` with before/after identities when observable.
The candidate `--timings` invocation is separately marked unscored and must
succeed. On Windows, timeouts terminate only that invocation's owned job,
including job members whose parent already exited. Job attachment happens
immediately after launch, rather than using suspended process creation; a child
created before attachment can escape membership. Assignment failure invalidates
the invocation. Cleanup checks absolute
ownership and rejects reparse points; refusal preserves the work tree and records
`cleanup_error`. Normal cleanup removes only the harness's `work-*` copies.

`run.json` starts as `incomplete`. A `.pending` schema-1 manifest is evaluated
only after successful invocations and identity checks. The evaluator runs in an
explicit repository working directory, retaining its stdout/stderr and verdict.
Invalid evidence or evaluator failure leaves the run incomplete and never
publishes a final manifest. A valid evaluation publishes
`windows-build-performance-manifest.json` and marks the run `evaluated`, retaining
the actual verdict even when it is `FAIL`; the harness then exits nonzero.
`tools/perfcompare` is unchanged: at least 10 cold and 20 no-change pairs,
**cold median candidate/baseline <= 0.60**, **no-change p95 <= 1.10**. A smaller
improvement or A/A comparison can correctly fail this legacy target. `evaluated`
describes evidence completion, not performance acceptance.

Output is buffered in memory until process/stream completion or handled failure.
Abrupt host termination or power loss can leave a `started` record without a
finish and lose that invocation's buffered output; the run remains incomplete.
There is no resume/retry that silently drops failed samples. Completed journal
appends close their file handles, but do not promise survival of power loss.

## Scope limits

Cold cleanup removes only `out`, `out-test`, `out-tsc`, and eligible
`*.rbxtsc.tsbuildinfo` outside `node_modules`, `.rotor`, and `.git`. Dependencies,
`.rotor`, OS caches, Defender/indexer/power settings, `UV_THREADPOOL_SIZE`, and
other inherited environment values are preserved. Only `ROTOR_SIDECAR_TIMEOUT`
is set to the requested value and restored afterward. Cold means output/state
reset with warm OS cache. No-change builds reuse the final cold build's state.

The canonical fixture digest covers file paths and bytes except `.git`. The
output comparison digest covers only the allowlisted output directories and
build-info files; it excludes include files, custom output paths, dependencies,
`.rotor`, mtimes, and write counts. It cannot establish write-only-changed
behavior. Add fixture-specific oracles separately when those contracts matter.
This tool collects no process CPU/peak memory, whole-tree CPU/memory, physical
disk writes, cache/thermal state, noise intervals, or competing-load observations.
The quiet-host lease, sample policy, independent confirmation, and full semantic
acceptance remain the measurement owner's responsibility.
