# rotor watch v2 + QOL + hardening — design

Date: 2026-06-11. Status: approved under the standing autonomous-execution grant.

## Goal

Post-v1.1.0 quality-of-life pass with three thrusts:

1. **Watch mode v2** — make `rotor build -w` / `rotor check -w` dramatically cheaper
   while idle, faster to react, and correct around mid-build edits.
2. **CLI/logging QOL** — batched change reporting, rebuild stats, a build-time
   sparkline, and a new `rotor doctor` environment checker.
3. **Security hardening** — output-path containment, private sidecar cache
   permissions, and a `govulncheck` job in CI.

Byte-parity contract is untouched: no transformer, logservice, or emit changes.

## 1. Watch engine v2 (`cmd/rotor/watch.go`)

### Problems with v1

- `snapshotProjectTree` walks the **entire** project tree every 250 ms — including
  `node_modules` (tens of thousands of `os.Stat` calls per tick on real projects).
- One `os.Stat` per file on top of the `os.ReadDir` that already enumerated it.
  On Windows, `DirEntry.Info()` is free (populated from the FindFirstFile data);
  on Linux it is one syscall instead of two.
- The post-build snapshot is taken **after** the build, so a file saved while a
  build runs is silently absorbed into the new baseline — the edit never triggers
  a rebuild (lost-update bug).
- Editor junk (`*.swp`, `*~`, vim's `4913`, emacs `.#*`) triggers full rebuilds.
- A change fires a rebuild immediately, so an editor "save all" of N files causes
  a rebuild against a half-written tree; only the first file is reported.

### Design

**Approaches considered:**

- *(a) Native FS events via fsnotify* — best latency, but a new dependency, no
  cross-platform recursive watch (per-directory watch management with
  create/rename races), and divergent platform semantics. Deferred.
- *(b) Optimized polling* (chosen) — `os.ReadDir` + `entry.Info()` walks with
  pruned trees make the idle cost so small (sub-millisecond on a typical `src/`)
  that polling at 100 ms beats v1's 250 ms latency while using orders of
  magnitude less CPU. No new dependency; identical behavior on all six release
  targets; `--usePolling` stays a compatible no-op (polling is the engine).

**Mechanics:**

- `snapshotProjectTree` v2: `os.ReadDir` + `entry.Info()`; prune directories:
  `.git`, `node_modules`, any dot-directory, the resolved **output dir**, and the
  resolved **include dir** (the include dir is rotor-written each build — without
  pruning it, the pre-build baseline would self-trigger an infinite rebuild loop).
  Ignore junk files by basename: `*.swp`, `*.swo`, `*.tmp`, `*~`, `4913`, `.#*`,
  `.DS_Store`, `Thumbs.db`.
  - `npm install` no longer trips the watcher via `node_modules` churn, but the
    root lockfiles (`package-lock.json`, `bun.lock*`, `pnpm-lock.yaml`,
    `yarn.lock`) and `package.json` are in the walk, so installs still rebuild.
- **Adaptive interval**: poll every `clamp(10 × walkCost, 100ms, 1s)` — snappy on
  small trees, self-throttling on pathological ones.
- **Debounce/batch**: when a tick detects changes, keep re-snapshotting on a
  short quiet timer (50 ms) until a snapshot adds nothing new (capped at 500 ms),
  accumulate the full changed set, then rebuild once and report **all** paths.
- **Mid-build edits**: the baseline snapshot is taken **before** the build starts;
  after the build, the next tick diffs against that pre-build baseline (with
  output/include pruned, build writes don't show up), so a save during the build
  triggers the next rebuild instead of vanishing.
- `rotor check -w` keeps its parsed-file-list poll but gains the same debounce,
  batched reporting, and 100 ms cadence.

## 2. CLI/logging QOL

- `ui.watchChanges(paths)`: one line per rebuild — `3 files changed · a.ts, b.ts, +1 more`
  (up to 3 basenames shown).
- Watch idle line gains rebuild stats: `◷ watching · rebuild #4 · last 142 ms ▂▁▇▂ · Ctrl+C to exit`
  — the sparkline (`term.Spark`, unicode ramp with ASCII fallback) shows the last
  12 build durations. This is the "special touch": you can *see* incremental
  builds getting faster.
- `rotor doctor` — a new command that diagnoses the environment and project setup
  with ✓ / ! / ✗ rows and actionable hints; exit 1 only on hard failures:
  - tsconfig.json found (upward search, same as build)
  - package.json + node_modules present
  - `typescript`, `@rbxts/compiler-types`, `@rbxts/types` resolved with versions
  - Node.js on PATH + version (hard requirement only when transformer plugins are
    configured; informational otherwise)
  - transformer plugins listed in tsconfig resolve in node_modules; embedded
    sidecar extraction works
  - a `*.project.json` Rojo project exists (warn if absent); `rojo` on PATH (info)
- `usage()` documents `doctor`.

`internal/logservice` stays byte-stable (differential-test channel). All chrome
lives in `cmd/rotor/ui.go` + `internal/term`.

## 3. Security hardening

- **Output containment**: `BuildProjectWithOptions` rejects any compiled output
  whose project-relative path is not local (`filepath.IsLocal`) before joining it
  to the project dir — defense-in-depth against a hostile/buggy path mapping
  writing outside the project.
- **Sidecar cache**: extraction dirs/files tightened to `0o700`/`0o600` (the
  worker JS is executed by Node — keep it user-private on Unix; no-op on Windows).
- **CI**: a separate `vuln` job runs a pinned `govulncheck ./...` so known-vuln
  advisories in rotor's module graph fail CI loudly. Validated locally first.

## Testing

- watch: prune rules (node_modules/dot-dirs/out/include), junk-file ignore,
  added/removed/modified detection (existing tests updated), debounce batching
  via injected snapshot/sleep functions, adaptive-interval clamp.
- ui/term: `Spark` ramp + fallback, changed-file line truncation.
- doctor: package-version reading, plugin listing from tsconfig, missing
  node_modules paths — against temp fixtures.
- output containment: non-local rel path errors, normal paths unaffected.
- Gates: gofmt, `go vet`, full `go test ./...` (differential + conformance +
  transformer fixtures), self code-review before commit.

## Out of scope

Native FS events (fsnotify), compile-pipeline perf beyond watch (already
profiled to FS churn and fixed in the cachedvfs pass), `--writeTransformedFiles`,
log JSON output.
