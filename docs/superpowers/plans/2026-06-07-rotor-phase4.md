# rotor Phase 4 Implementation Plan — Project Layer: CLI, Output Pipeline, Watch, Incremental, Plugins

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make rotor a drop-in `rbxtsc` CLI — the full `build` flag surface, the complete output pipeline (write/cleanup/copy), build watch mode, incremental builds, `.d.ts` emit, and the transformer-plugin sidecar — proven by an end-to-end `rotor build` of randomness whose out/ tree matches rbxtsc's byte-for-byte (header-normalized).

**Source of truth:** `docs/superpowers/research/phase4-project-digest.md` (§1 CLI surface incl. the `rbxts` tsconfig key + exit-code QUIRK, §2 build orchestration back half with the cleanup→copyInclude→copyFiles→compileFiles order, §3 watch with the 100 ms batch + module-level pending sets, §4 incremental — manifest-v1 recommendation, §5 validateCompilerOptions [LANDED in 3c], §6 plugin sidecar protocol, §7 checker-pin recommendation [keep for v1, measure], §8 gap table, §9 risks). Reference (`reference/roblox-ts/src/...`) wins on disagreement. Established conventions apply (TDD, byte/UX parity or documented divergence, oracle technique for output shapes, full gates per commit, quirks verbatim, push every commit, roadmap.md updated per task).

**Already landed during 3c (do NOT re-plan):** include/ emission (`internal/includefiles`, `--noInclude`/`--includePath`), `--type` override, full validateCompilerOptions (known gap: enforced options in an `extends` parent read root-only), minimal `rotor build` write loop, conformance corpus harness (gated off — Phase 5).

---

## Task 1: ProjectOptions + full CLI surface + LogService

Per digest §1: a `ProjectOptions` struct with DEFAULT_PROJECT_OPTIONS defaults; the `rbxts` tsconfig key (raw single-file read, NO extends — quirk verbatim; merge order defaults < rbxts < argv where ABSENT CLI booleans don't clobber); full `rotor build` flags (`-p/--project` with file-path + upward-search resolution, `-w/--watch`, `--usePolling` implies watch, `--verbose`, `--logTruthyChanges`, `--writeOnlyChanged`, `--writeTransformedFiles` → documented no-op/NYS, `--optimizedLoops` default true — WIRE the loops.go:653 gate, `--rojo` (empty string falls through to discovery — quirk), `--luau` default true → PathTranslator ext, `--allowCommentDirectives`); exit-code policy decision: match upstream exit 1 for usage errors (document the change from rotor's current 2); LogService analog (`Compiler Warning:` yellow channel — wire the DROPPED Rojo resolver warnings from project.go, `--verbose` benchmark lines with exact strings: `compiling as X..` two dots, `copy include files ( N ms )`, `N/M compile <rel>` padStart). `--version` prints rotor's version. Tests: option-merge table tests (rbxts key + CLI precedence), flag parsing incl. `--usePolling` without `--watch` error, project-path search.
Commit: `"cli: full build flag surface, ProjectOptions merge, LogService"`

### Task 2: Collect-all diagnostics + comment directives + pretty printing

Per digest §2.7/§2.9 (gaps 12/13/16): CompileProject stops aborting on the FIRST file with diagnostics — port compileFiles' three-gate structure (project-level / post-transformers / post-per-file-loop), collecting ALL files' pre-emit + transform diagnostics before bailing (`emitSkipped`-style result). Wire `DiagNoCommentDirectives` (exists, unwired) via the fileUsesCommentDirectives port: per compiled file, skip when `allowCommentDirectives`; one diagnostic per `SourceFile.CommentDirectives` entry + per `ts-nocheck` pragma (tsgo exposes both — ast.go:2446,2451). Pretty diagnostic printing: ` roblox-ts` string code rendering (`error roblox-ts: <message>`), located diagnostics with `filename:line:col` + color via tsgo's diagnosticwriter (cmd/rotor already has the writer for check — extend to build). Update any diff-harness/compile tests asserting first-error behavior (digest risk 7). Tests: multi-file project where files 2 AND 3 have errors → both reported; comment-directive fixtures (@ts-ignore/@ts-expect-error/@ts-nocheck) with byte-exact message; --allowCommentDirectives suppression.
Commit: `"compile: collect-all diagnostics, comment-directive checks, pretty printing"`

### Task 3: Output pipeline — write phase, cleanup, copyFiles

Per digest §2.4-2.7: move file writing INTO the compile result path (`outputFileSync` mkdir-p semantics, `--writeOnlyChanged` byte-compare skip, emittedFiles list); `cleanup(pathTranslator)` orphan removal (`.git` dirs skipped entirely, children before parent, isOutputFileOrphaned's 4 rules incl. buildinfo protection — wire the real buildinfo output path into createPathTranslator, replacing the `""` at project.go:369); `copyFiles`/`copyItem` non-compiled passthrough (rootDirs mirrored minus `.ts`/`.tsx`, `.d.ts` gated on declaration mode, writeOnlyChanged content compare, `dereference: true` symlink materialization); `--luau=false` → `.lua` extension end-to-end; the non-watch order VERBATIM: cleanup → copyInclude → copyFiles → compileFiles. Tests: temp-dir project trees — orphan cleanup table tests (stale outputs removed, live kept, .git preserved, buildinfo preserved), copyItem filter table (lua/json copied, ts skipped, d.ts gated), writeOnlyChanged mtime-preservation, `.lua` ext run.
Commit: `"build: output pipeline — write phase, orphan cleanup, file passthrough"`

### Task 4: Declaration emit for Package projects

Per digest §2.7: per queued file in declaration mode, tsgo `Program.Emit(EmitOptions{TargetSourceFile, EmitOnly: EmitOnlyDts, WriteFile})`; the typeReferenceDirectives rewrite (`/// <reference types="types" />` → `types="@rbxts/types"`) as a text post-process in the WriteFile callback (digest option a); transformPaths SKIPPED when baseUrl/paths absent (sanitizer strips baseUrl — verify the interaction, document paths-alias projects as a v1 limitation); declaration emit runs even when the Luau write was writeOnlyChanged-skipped (quirk). PathTranslator.declaration wiring. Test: a Package-shaped fixture project compiled with declaration on — d.ts emitted at the PathTranslator path with the typeRef rewrite applied; note in the test that TS7-vs-5.5 d.ts textual fidelity is a Phase 5 differential item (digest risk 2).
Commit: `"build: declaration emit for Package projects"`

### Task 5: Incremental v1 — manifest + changed-file selection

Per digest §4.2's HONEST ASSESSMENT (manifest-v1, NOT tsgo snapshot integration): persist rotor's own manifest at the buildinfo path (salt = version + type + isPackage + plugins JSON + raw Rojo config contents, per-file content hash, per-file import list recorded during transform); on build, salt mismatch or missing manifest → full compile; else recompile changed files + transitive dependents (port the getChangedFilePaths reverse-reference walk over rotor's import graph); gated on the project's own `incremental` option (rbxtsc forces nothing — quirk); manifest written ONLY on fully successful compiles (quirk: error builds don't advance state). cleanup/copyFiles stay non-incremental (upstream behavior). Tests: temp project — edit one file → only it + importers recompile (verify via emittedFiles); salt change → full rebuild; failed build → manifest not advanced; non-incremental project → always full.
Commit: `"build: incremental builds via salted manifest"`

### Task 6: Watch mode for build

Per digest §3: `rotor build -w` — tsgo `fswatch` recursive watching of rootDirs (add/change/delete derived from Update/Delete via fileNamesSet+stat heuristic, paths fixSlashes'd + canonicalized); the 100 ms fixed batch window after `File change detected. Starting incremental compilation...`; pending sets module-level-equivalent, NOT cleared on failure (retry next cycle — quirk); initial compile = full pipeline, `initialCompileCompleted` only on success (failed initial → next event reruns cleanup+copies — quirk); per-batch: additions walk dirs for compilables + checkFileName for non-compilables, changes → filesToCompile/filesToCopy, removals → tryRemoveOutput on output + declaration output; NEW program per cycle (reuse via the Task 5 manifest); file selection per digest §3.5 (incremental → manifest changed-set; non-incremental → batched-paths hint mode); emitSkipped → return BEFORE clean/copy (half-updated-out-dir guard); status lines byte-matching tsc's (`Starting compilation in watch mode...`, `Found N error(s). Watching for file changes.` — tsgo has these strings); no screen clearing (code-0 quirk); tsconfig/Rojo config NOT watched (upstream parity — document). `--usePolling` → polling fallback. Tests: watch loop unit-tested with an injected event source (batch window, pending-set retry, add/delete flows); a manual smoke documented (not CI-flaky fs-event tests).
Commit: `"build: watch mode with incremental recompilation"`

### Task 7: Transformer-plugin Node sidecar

Per digest §6.2: `tools/sidecar/` Node script (bundled JS, pinned typescript@5.5.3) implementing getPluginConfigs (raw read + extends recursion, own-first append order) → createTransformerList (resolve relative to projectPath, export `config.import ?? "default"`, factory-per-type, transformerNotFound WARNING continues) → `ts.transformNodes` with the after-THEN-before-THEN-afterDeclarations flatten QUIRK verbatim → printFile per transformed source. JSON-over-stdio protocol per the digest spec (protocol 1, compileFileNames, changedFiles deltas; response diagnostics + transformed texts). rotor side: spawn on `compilerOptions.plugins.length > 0` only (no-plugin projects NEVER spawn node), overlay Program (second tsgo program whose host serves transformed texts) feeding transform + declaration emit, sidecar kept warm in watch with `.d.ts` updateFile routing. Hard ProjectError when node is missing but plugins configured. Tests: a trivial test transformer plugin fixture (e.g. renames an identifier) — end-to-end through the sidecar produces the transformed Luau; no-plugins project verified to not spawn node; transformerNotFound warning path.
Commit: `"build: transformer-plugin sidecar"`

### Task 8: E2E acceptance + perf measurement + merge

1. **End-to-end acceptance**: `rotor build` over a COPY of randomness vs `rbxtsc build` over the same copy — compare the ENTIRE out tree (compiled files header-normalized, copied files, include/) byte-for-byte; document any divergence honestly. Watch-mode manual smoke on randomness (edit → rebuild correctness).
2. **Checker-pin measurement** (digest §7.3): time check-vs-transform on randomness with Checkers=1 vs 4 for the CHECK phase only; record results in the digest or a perf note — NO unpinning.
3. **Extends-gap**: fix the root-only raw-config read for the sanitizer + validateCompilerOptions + rbxts key if cheap (resolve the extends chain manually); else document precisely.
4. Benchmarks: cold `rotor build` vs `rbxtsc` on randomness — the README headline numbers.
5. README (Phase 4 ✅ rows, real CLI docs, benchmark numbers) + roadmap.md + memory update. Final whole-branch review (opus) → fixes → merge → push.
Commit: `"Phase 4 complete: full build CLI, watch, incremental, plugins"`

---

## Parallelization guide (per the Phase 3c worktree pattern)

- Task 1 first, solo (everything consumes ProjectOptions).
- Task 2 second, solo (changes CompileProject's contract; Tasks 3-5 build on it).
- Tasks 3, 4, 5 in parallel worktrees (output pipeline / declaration emit / incremental manifest — disjoint files; all touch project.go lightly → small merge conflicts acceptable).
- Task 6 after 3+5 merge (watch consumes both). Task 7 in parallel with Task 6 (disjoint).
- Task 8 last, solo.

## Done criteria

1. `rotor build` on randomness produces an out tree byte-matching rbxtsc's (header-normalized), including copied files and include/.
2. `rotor build -w` rebuilds incrementally with upstream-parity status lines and pipeline order; `--usePolling` works.
3. Incremental manifest: warm builds recompile only changed files + dependents; salt invalidation correct.
4. A real transformer plugin runs through the sidecar; pluginless projects never spawn node.
5. All flags of §1.2 parsed with upstream semantics (or documented divergence); exit codes match.
6. 43/43 fixtures still byte-identical; all gates green; checker-pin measurement recorded.
