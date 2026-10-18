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
tail writes, immediate exits, private/inherited-stream descendants, atomic job
ownership on host death, bounded incremental log retention, capture overflow,
strict JSON/duplicate fields, cancellation during validation, cleanup publication,
digest framing, provenance changes, and missing evaluator/wrong-cwd behavior.
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
a 1 ms floor; `ElapsedMS` retains the unrounded observation. Timing starts before
process creation and ends when the root has exited, the owned job has no active
processes, and both redirected streams have completed capture. Children with
private/closed streams must finish within the same deadline. Successful children
are awaited, so their required persistence stays inside the interval. Only failed,
cancelled, or overdue invocations are terminated. Job completion notifications
normally wake the owner immediately. Notifications can be missed; a 25 ms
fallback rechecks authoritative active-process counts, so scheduling/fallback
observation delay remains part of the measured overhead.

Capture uses two dedicated reader threads with 64 KiB buffers and flushes retained
chunks to the OS file cache as they arrive. This capture I/O, scheduling, setup,
and pipe backpressure are honest in-interval overhead. `CaptureSpoolWriteMS`
reports aggregate write/flush API elapsed across both threads; those writes may
overlap, and their sum must not be subtracted from duration. `EvidencePersistenceMS`
reports only final evidence flush/close after timing stops. JSON validation,
output hashing, and journal writes also occur outside timing. Incremental
retention deliberately trades memory-only capture for observable I/O overhead:
calibrate A/A noise and output-volume effects before interpreting small changes.
No durable-disk-flush or physical-write guarantee is implied.

`invocations.ndjson` appends a `started` record before launch and a `finished`
record after evidence retention and result validation. `validation_completed`
starts false and becomes true only after public JSON and output digest validation.
Both records name the raw logs and invocation role; a failed or cancelled finish,
including cancellation during hashing, is explicitly `incomplete`. A successful
invocation followed by a failure remains in the journal. Timeout, nonzero exit,
invalid public JSON, or `ok:false` stops the run and preserves captured stdout,
stderr, records, and `run.json` with before/after identities when observable.
Public results, timings, evaluator verdicts, and fixture JSON metadata use the
same strict UTF-8 JSON guard: one document, no comments/trailing commas, no
duplicate object names, and the required shape for the result kind. The candidate
`--timings` invocation is separately unscored and must succeed.

Windows 10/Server 2016 or newer is required. `CreateProcessW` uses creation-time
`PROC_THREAD_ATTRIBUTE_JOB_LIST` membership and an explicit inherited handle list
containing only the two pipe writers and NUL stdin. Arguments are quoted without
a shell; cwd and inherited environment are explicit/preserved. There is no
suspended-but-unowned or post-launch attachment window. Unsupported/restricted
creation fails without accepting a sample. The non-inherited job handle retains
kill-on-close through validation, so ordinary owner-host death terminates its
members, including children whose root already exited. Failure/cancellation
terminates and joins only owned members; inability to quiesce blocks cleanup.
Cleanup checks absolute ownership and rejects reparse points; refusal preserves
the work tree and records `cleanup_error`. Normal cleanup removes only the
harness's `work-*` copies.

`run.json` starts as `incomplete`. A `.pending` schema-1 manifest is evaluated
only after successful invocations and identity checks. The evaluator runs in an
explicit repository working directory, retaining its stdout/stderr and verdict.
Invalid evidence, retention/overflow failure, or unsafe/failed cleanup leaves the
run incomplete and the manifest pending. The evaluator's real PASS/FAIL remains
available even when cleanup later fails. After valid evaluation and successful
cleanup, publication writes the evaluated status and then renames the manifest
last. Require both `run.json.status == evaluated` and the final manifest, plus its
verdict, as the completion gate; interruption during publication can leave only
one of those artifacts. A completed evaluation publishes
`windows-build-performance-manifest.json` and marks the run `evaluated`, retaining
the actual verdict even when it is `FAIL`; the harness then exits nonzero.
`tools/perfcompare` is unchanged: at least 10 cold and 20 no-change pairs,
**cold median candidate/baseline <= 0.60**, **no-change p95 <= 1.10**. A smaller
improvement or A/A comparison can correctly fail this legacy target. `evaluated`
describes evidence completion, not performance acceptance.

`-MaxCaptureBytes` bounds retained bytes independently per stream (default
16 MiB). Extra output invalidates the invocation and retains only its bounded
prefix; no overflow or retention failure can publish completed/scored evidence.
Validation may allocate from the bounded stdout file. Already-spooled chunks
survive ordinary abrupt host termination; bytes still in pipes or an in-flight
read/write may be lost. A started record without a finished record and an
incomplete run preserve that distinction. No extra supervisor is needed for
this OS-cache retention guarantee. Power loss, storage failure, and simultaneous
loss of the owner and unflushed kernel data remain outside the guarantee. There
is no resume/retry that silently drops failed samples.

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
Tree digests now carry the prefix `sha256-tree-v2:`. SHA-256 hashes the UTF-8
header `rotor-tree-digest-v2` plus NUL, a uint64 file count, then records sorted
by ordinal normalized relative path: uint64 UTF-8 path length, path bytes,
uint64 file length, and the fixed 32-byte SHA-256 content hash. Integers are
little-endian. This framing distinguishes split-file layouts and is deliberately
incompatible with old unframed digests; each manifest records its format identity.
This tool collects no process CPU/peak memory, whole-tree CPU/memory, physical
disk writes, cache/thermal state, noise intervals, or competing-load observations.
The quiet-host lease, sample policy, independent confirmation, and full semantic
acceptance remain the measurement owner's responsibility.
