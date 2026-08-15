# Phase 4 Project-Layer Digest — CLI, build orchestration, watch, incremental, plugins, checker parallelism

Source of truth for porting the roblox-ts 3.0.0 **project layer** (everything outside the
transformer) to Go. All paths relative to `reference/roblox-ts/src/` unless prefixed.
rotor-side citations are absolute repo paths. Builds on `phase3-imports-digest.md` (whose §7/§8
specced the compileFiles FRONT half — Rojo context, runtimeLib validation — already ported in
`internal/compile/project.go`). SCOPE here: the `rbxtsc build` yargs surface, ProjectOptions
plumbing, the compileFiles BACK half (file writing, include copy, non-compiled copy, cleanup,
declaration emit, buildinfo), watch mode, incremental builds, the full validateCompilerOptions,
transformer-plugin loading + the Node sidecar boundary, and the checker-identity pin vs
parallelism question.

Upstream pins: TypeScript **=5.5.3**, chokidar ^3.6.0, yargs ^17.7.2 (`reference/roblox-ts/package.json`
L54-59). `COMPILER_VERSION` = `"3.0.0"` (package.json L3, read at `Shared/constants.ts` L9).

What rotor already has (do NOT re-plan): the compileFiles front half — Program creation,
SanitizeFS, rootDir/outDir validation, ProjectData analog (`projectContext`), RojoResolver +
PathTranslator (full, incl. `GetInputPaths`/`GetOutputDeclarationPath` — `internal/rojo/pathtranslator.go`),
checkRojoConfig/checkFileName, ProjectType inference, runtimeLib validation, per-file pre-emit
diagnostics, macro audit, transform+render (`internal/compile/compile.go`, `project.go`); the
`--!directive` hoisting above the header (`internal/transformer/sourcefile.go` L95-106, ports
`TSTransformer/nodes/transformSourceFile.ts` L232-240); `LogTruthyChanges` plumbing in State
(`state.go` L134-136, consumed `truthiness.go` L57); optimizedLoops gate point identified
(`loops.go` L653-655, currently always-on); `cmd/rotor` with `check` (+poll watch) and minimal
`build`.

---

## §1 CLI surface

### 1.1 Top-level CLI — `CLI/cli.ts`

- yargs app: usage banner `"roblox-ts - A TypeScript-to-Luau Compiler for Roblox"` (L10),
  `--help`/`-h`, `--version`/`-v` (prints COMPILER_VERSION), `.commandDir(out/CLI/commands)`
  (only command: build), `.recommendCommands()`, `.strict()`, `.wrap(terminalWidth)` (L8-26).
- `.fail(str => { process.exitCode = 1; if (str) LogService.fatal(str); })` (L30-35).
  `LogService.fatal` writes the message then `process.exit(1)` (`Shared/classes/LogService.ts`
  L31-34). QUIRK: **usage errors exit 1, not 2** — rotor's current convention (exit 2 for
  usage) diverges; for drop-in parity `rotor build` should exit 1 on bad flags, or we accept
  the divergence deliberately (document either way).
- `parseAsync().catch`: CLIError → `e.log()` (writes formatted diagnostic, exit code already
  set); anything else rethrows (node prints stack, exit 1) (L36-44).
- `CLI/index.ts`: exports Project + COMPILER_VERSION, imports `patchFs` (playground fs no-op
  shims — irrelevant to rotor).
- `CLI/test.ts` is the upstream mocha harness (compiles `tests/` with
  `allowCommentDirectives: true, optimizedLoops: true`); not CLI surface.

### 1.2 `rbxtsc build` flags — `CLI/commands/build.ts` L49-118

Command: `["$0", "build"]` (build is the default command), describe `"Build a project"`.

| flag | alias | type | yargs default | DEFAULT_PROJECT_OPTIONS | hidden | describe |
|---|---|---|---|---|---|---|
| `--project` | `-p` | string | `"."` | — | no | `project path` |
| `--watch` | `-w` | boolean | — | `false` | no | `enable watch mode` |
| `--usePolling` | | boolean (implies: watch) | — | `false` | no | `use polling for watch mode` |
| `--verbose` | | boolean | — | `false` | no | `enable verbose logs` |
| `--noInclude` | | boolean | — | `false` | no | `do not copy include files` |
| `--logTruthyChanges` | | boolean | — | `false` | no | `logs changes to truthiness evaluation from Lua truthiness rules` |
| `--writeOnlyChanged` | | boolean | — | `false` | yes | — |
| `--writeTransformedFiles` | | boolean | — | `false` | yes | `writes resulting TypeScript ASTs after transformers to out directory` |
| `--optimizedLoops` | | boolean | — | `true` | yes | — |
| `--type` | | choices `game`/`model`/`package` | — | `undefined` | no | `override project type` |
| `--includePath` | `-i` | string | — | `""` | no | `folder to copy runtime files to` |
| `--rojo` | | string | — | `undefined` | no | `manually select Rojo project file` |
| `--allowCommentDirectives` | | boolean | — | `false` | yes | — |
| `--luau` | | boolean | — | **`true`** | no | `emit files with .luau extension` |

DEFAULT_PROJECT_OPTIONS: `Shared/constants.ts` L41-55. ProjectOptions shape:
`Shared/types.ts` L4-18. QUIRK: only `--project` has a yargs default ("DO NOT PROVIDE
DEFAULTS BELOW HERE" comment, build.ts L62) — this is load-bearing for the merge in 1.3.
yargs 17 boolean-negation means `--no-luau`, `--no-optimizedLoops` etc. work; `--luau=false`
also. `usePolling` `implies: "watch"` → yargs errors if given without `--watch`.

### 1.3 ProjectOptions merge + the `rbxts` tsconfig key — build.ts L120-136

```ts
const tsConfigPath = findTsConfigPath(argv.project);
const projectOptions = Object.assign({}, DEFAULT_PROJECT_OPTIONS,
                                     getTsConfigProjectOptions(tsConfigPath), argv);
```

- `getTsConfigProjectOptions` (L22-29): reads tsConfigPath raw, JSON-parses via
  `ts.parseConfigFileTextToJson`, returns the top-level **`rbxts`** key — an undocumented
  way to persist any Partial\<ProjectOptions\> in tsconfig.json. Merge order: defaults <
  tsconfig `rbxts` < CLI argv. Booleans not passed on the CLI are ABSENT from argv (no yargs
  default), so they don't clobber `rbxts` values. QUIRK: `extends` is NOT followed for the
  `rbxts` key (raw single-file read).
- `findTsConfigPath` (L31-40): `path.resolve(argv.project)`; if that's not an existing FILE,
  `ts.findConfigFile(dir, fileExists)` — which walks UP parent directories looking for
  `tsconfig.json`. Failure: `throw new CLIError("Unable to find tsconfig.json!")`. Result
  re-resolved against cwd.
- `LogService.verbose = projectOptions.verbose === true` (L132).
- Flag consumers: `type`/`rojo`/`includePath`/`noInclude` → createProjectData + copyInclude +
  inferProjectType override; `luau` → PathTranslator ext; `writeOnlyChanged` → compileFiles
  write loop + copyItem; `optimizedLoops` → transformForStatement; `logTruthyChanges` →
  truthiness warnings; `allowCommentDirectives` → fileUsesCommentDirectives;
  `writeTransformedFiles` → plugin debug output (OUT of v1 scope per design doc §Out);
  `watch`/`usePolling` → setupProjectWatchProgram.

### 1.4 Non-watch handler flow — build.ts L134-157

```ts
const diagnosticReporter = ts.createDiagnosticReporter(ts.sys, /*pretty*/ true);
const data = createProjectData(tsConfigPath, projectOptions);
if (watch) setupProjectWatchProgram(data, usePolling);
else {
  const program = createProjectProgram(data);
  const pathTranslator = createPathTranslator(program, data);
  cleanup(pathTranslator);
  copyInclude(data);
  copyFiles(data, pathTranslator, new Set(getRootDirs(program.getCompilerOptions())));
  const emitResult = compileFiles(program.getProgram(), data, pathTranslator,
                                  getChangedSourceFiles(program));
  for (const d of emitResult.diagnostics) diagnosticReporter(d);
  if (hasErrors(emitResult.diagnostics)) process.exitCode = 1;
}
```

Order matters: **cleanup → copyInclude → copyFiles → compileFiles**. Error diags → exit 1;
`hasErrors` = any `DiagnosticCategory.Error` (`Shared/util/hasErrors.ts`). catch: exit 1 +
`LoggableError.log()` (formatted via `ts.formatDiagnosticsWithColorAndContext`,
`Shared/util/formatDiagnostics.ts` L15-17) or rethrow (L158-166).

### 1.5 Console output formats (byte-relevant)

- **Diagnostic printing** (non-watch + watch): `ts.createDiagnosticReporter(ts.sys, true)` —
  pretty (color + context + `filename:line:col`) UNCONDITIONALLY, no TTY check.
- **Custom roblox-ts diagnostics** carry `code: " roblox-ts"` (a string crammed into the code
  field — `Shared/util/createTextDiagnostic.ts` L9), so pretty print renders
  `error roblox-ts: <message>` instead of `error TS1234:`. Located diagnostics (from
  `Shared/diagnostics.ts`) follow the same scheme with `roblox-ts` suffixed ids.
- **LogService** (`Shared/classes/LogService.ts`): `warn` = `kleur.yellow("Compiler Warning:")
  - " " + msg`; `writeLineIfVerbose` gated on `--verbose`; `write` tracks partial lines (a
  partial benchmark line gets a `\n` injected before any writeLine).
- **Verbose benchmark lines** (`Shared/util/benchmark.ts`): `LogService.write(name)` then
  `` ` ( ${Date.now()-start} ms )\n` `` appended — i.e. `copy include files ( 12 ms )`.
  There is **no "Compiled in X.XXXs" line in rbxtsc 3.0.0** — total-time reporting does not
  exist; per-step verbose benchmarks and the watch "Found N errors" line are all there is.
- **Verbose strings** (exact): `compiling as ${projectType}..` (compileFiles.ts L102, note
  TWO dots), `running transformers..` (L110), `copy include files` (copyInclude.ts L12),
  `copy non-compiled files` (copyFiles.ts L7), `writing compiled files` (L189),
  `${i+1}/${total} compile ${path.relative(cwd, fileName)}` with the progress fraction
  right-padStart'ed to `len("N/N")` (L105, L154-155), `remove ${outPath}`
  (tryRemoveOutput.ts L27).
- **Exit codes**: 0 = clean; 1 = compile errors / project errors / usage errors. Watch mode
  never exits (exitCode remains 0 until killed).

---

## §2 Full build orchestration (the back half rotor lacks)

### 2.1 createProjectData — `Project/functions/createProjectData.ts`

Already ported into `projectContext` EXCEPT the option-driven bits:

- `projectOptions.includePath = path.resolve(includePath || join(projectPath, "include"))`
  (L29 — `||` intentionally catches `""`); rotor hardcodes the fallback.
- `--rojo` override: truthy `projectOptions.rojo` → `path.resolve(rojo)`; else
  `RojoResolver.findRojoConfigFilePath(projectPath)` with warnings → `LogService.warn`
  (L33-43). QUIRK: `--rojo ""` (empty string) falls through to auto-discovery.
- Rojo resolver warnings (`rojoResolver.getWarnings()` → `LogService.warn`, compileFiles.ts
  L65-67) are currently DROPPED by rotor (project.go L135-138) — Phase 4 needs the warning
  channel.

### 2.2 Program creation + buildinfo hash salt — `createProjectProgram.ts`, `createProgramFactory.ts`

- `createProjectProgram` = getParsedCommandLine → createProgramFactory → createProgram
  (fileNames, options, host?).
- `createProgramFactory` (L28-38) returns a `ts.CreateProgram<EmitAndSemanticDiagnosticsBuilderProgram>`:
  host = `ts.createIncrementalCompilerHost(options)` with **createHash salted** (L8-26):
  `contentsToHash = "version=3.0.0," + "type=" + String(projectOptions.type) + "," +
  "isPackage=" + String(isPackage) + "," + "plugins=" + JSON.stringify(compilerOptions.plugins ?? []) + ","
  - <raw rojo config file contents if any>`, prepended to every hashed datum
  (`host.createHash = data => origCreateHash(contentsToHash + data)`). Effect: changing the
  compiler version, `--type`, package-ness, plugin config, or the Rojo project file
  invalidates EVERY file signature in `.tsbuildinfo` → full rebuild. This is the entire
  invalidation story beyond TS's own file-version tracking.
- `oldProgram = ts.readBuilderProgram(options, createReadBuildProgramHost())` — reads
  `.tsbuildinfo` from DISK on every program creation (both cold builds and every watch
  rebuild). `createReadBuildProgramHost` (`Project/util/createReadBuildProgramHost.ts`):
  cwd/readFile/useCaseSensitiveFileNames from ts.sys.
- `getParsedCommandLine` (`getParsedCommandLine.ts`): `ts.getParsedCommandLineOfConfigFile`
  (errors → DiagnosticError); dev-mode guard (global `RBXTSC_DEV` or attached inspector)
  force-disables `incremental` + `tsBuildInfoFile` (L29-32 — dev-only, skip); then
  `validateCompilerOptions` (§5).

### 2.3 getChangedSourceFiles / getChangedFilePaths — the file-selection contract

`getChangedSourceFiles(program, pathHints?)` (`getChangedSourceFiles.ts`): map
`getChangedFilePaths` results through `program.getSourceFile`, dropping declaration files
and JSON source files (L8).

`getChangedFilePaths(program, pathHints?)` (`getChangedFilePaths.ts` L12-54):

- Builds `reversedReferencedMap` from `program.getState().referencedMap` (importer→imported,
  reversed to imported→importers) (L18-28).
- `search(filePath)`: add file + transitively all files that import it; recursion stops at
  direct dependents if `assumeChangesOnlyAffectDirectDependencies === true` (L32-42).
- With `pathHints` (watch non-incremental path): seed search with
  `getCanonicalFileName(hint)` (lowercased on case-insensitive fs —
  `Shared/util/getCanonicalFileName.ts`).
- Without hints: seed from `buildState.changedFilesSet`, then **`changedFilesSet.clear()`**
  (L49-50) — consuming the builder's pending set. Fresh build with no/invalid buildinfo: every
  file is in changedFilesSet → full compile. Valid buildinfo: only files TS considers changed
  → **non-watch builds are already incremental when `incremental: true`**.

QUIRK: this leans on TS internal builder state (`program.getState()`); tsgo analogs in §4.

### 2.4 cleanup — orphaned-output removal (`cleanup.ts`, `tryRemoveOutput.ts`)

- `cleanup(pathTranslator)`: if `outDir` exists, recurse. For each entry of each dir:
  directories named `.git` skipped entirely; directories recursed FIRST (children cleaned
  before parent), then `tryRemoveOutput(pathTranslator, itemPath)` runs on EVERY entry —
  files AND directories (cleanup.ts L6-19).
- `tryRemoveOutput` → `isOutputFileOrphaned` (tryRemoveOutput.ts L6-22):
  1. `.d.ts` output while `!pathTranslator.declaration` → orphaned (stale declarations are
     purged when declaration emit is off).
  2. any input path of `getInputPaths(outPath)` exists → NOT orphaned. (`GetInputPaths` is
     already ported 1:1 — `internal/rojo/pathtranslator.go` L169. For a directory the last
     branch maps `out/foo` → `src/foo`, so an out-dir whose source dir is gone is removed
     wholesale via `fs.removeSync`.)
  3. `pathTranslator.buildInfoOutputPath === filePath` → NOT orphaned (protects
     `.tsbuildinfo` living inside outDir). Note createPathTranslator normalizes the
     buildInfo path (`createPathTranslator.ts` L12-15); rotor currently passes `""` (project.go
     L369) — must wire `ts.getTsBuildInfoEmitOutputFilePath` analog (tsgo:
     `ParsedCommandLine.GetBuildInfoFileName()` / `outputpaths`).
  4. else orphaned → `fs.removeSync` + verbose `remove ${outPath}`.
- Watch incremental also calls `tryRemoveOutput` for unlinked inputs on BOTH
  `getOutputPath(fsPath)` and (if declaration) `getOutputDeclarationPath(fsPath)`
  (setupProjectWatchProgram.ts L153-158).

### 2.5 copyInclude — `copyInclude.ts`

```ts
if (!noInclude && projectOptions.type !== ProjectType.Package
    && !(projectOptions.type === undefined && data.isPackage))
  fs.copySync(INCLUDE_PATH, data.projectOptions.includePath, { dereference: true });
```

- `INCLUDE_PATH` = `<rbxtsc package root>/include` (`Shared/constants.ts` L5) — contains
  exactly `RuntimeLib.lua` (6018 B) and `Promise.lua` (60896 B) in 3.0.0
  (`reference/roblox-ts/include/`). rotor has NO include/ assets yet → Phase 4 must vendor
  them (e.g. `go:embed`) and write to `includePath`.
- QUIRK: the skip condition uses `projectOptions.type` (the CLI override), NOT the inferred
  projectType — `--type game` on a scoped-name package still copies include; no `--type` +
  scoped name skips. Full recursive copy each build (no mtime check; `writeOnlyChanged`
  does NOT apply here).

### 2.6 copyFiles / copyItem — non-compiled file passthrough

- `copyFiles(data, pathTranslator, sources)` with `sources = new Set(getRootDirs(options))`
  (build.ts L144): copyItem over each rootDir, under verbose benchmark
  `copy non-compiled files` (copyFiles.ts L6-12).
- `copyItem` (copyItem.ts L7-27): `fs.copySync(item, pathTranslator.getOutputPath(item),
  { filter, dereference: true })`. Filter, per src:
  1. `writeOnlyChanged` && dest exists && src not a directory && contents equal → skip
     (byte-compare both sides).
  2. `src.endsWith(".d.ts")` → copy only if `pathTranslator.declaration`.
  3. else `!isCompilableFile(src)` — directories return true (recurse); `.ts`/`.tsx`
     (non-`.d.ts`) excluded; **everything else copies**: `.lua`, `.luau`, `.json`, `.txt`,
     model files, etc. (`Project/util/isCompilableFile.ts`: dir → false; `.d.ts` → false;
     `.ts`/`.tsx` → true).
  So: the entire rootDir tree is mirrored into outDir minus compiled sources, with `.d.ts`
  gated on declaration mode. Note the dir is mapped through `getOutputPath` (no extension →
  identity path mapping rootDir→outDir).
- QUIRK: `dereference: true` everywhere — symlinked content is materialized.

### 2.7 compileFiles back half — `compileFiles.ts` L100-213

Front half (L27-100) is ported. The rest:

- L100/L146/L185: `DiagnosticService.hasErrors()` early-returns
  `{ emitSkipped: true, diagnostics: flush() }` at three gates (project-level checks, after
  transformers, after per-file transforms). Note the per-file loop CONTINUES past a file
  with errors (the `benchmarkIfVerbose` callback `return`s but the loop proceeds) — all
  pre-emit/transform diagnostics across all files are collected before the L185 bail. rotor
  currently aborts on the FIRST file with diagnostics (compile.go / project.go) — divergence
  to fix in Phase 4 for multi-error parity.
- L104-105: `fileWriteQueue: {sourceFile, source}[]`; progress padding.
- L107-144: plugin transformers (§6).
- L148-183: per-file: `getPreEmitDiagnostics(proxyProgram, sourceFile)` +
  `getCustomPreEmitDiagnostics(data, sourceFile)` (= `fileUsesCommentDirectives`, §2.9);
  TransformState; transformSourceFile; renderAST; queue.
- **L187-208 write phase** (verbose benchmark `writing compiled files`):

  ```ts
  const afterDeclarations = compilerOptions.declaration
    ? [transformTypeReferenceDirectives, transformPathsTransformer(program, {})] : undefined;
  for (const { sourceFile, source } of fileWriteQueue) {
    const outPath = pathTranslator.getOutputPath(sourceFile.fileName);
    if (!writeOnlyChanged || !fs.pathExistsSync(outPath)
        || fs.readFileSync(outPath).toString() !== source) {
      fs.outputFileSync(outPath, source);     // mkdir -p semantics
      emittedFiles.push(outPath);
    }
    if (compilerOptions.declaration)
      proxyProgram.emit(sourceFile, ts.sys.writeFile, undefined,
                        /*emitOnlyDtsFiles*/ true, { afterDeclarations });
  }
  ```

  - `--writeOnlyChanged`: byte-compare existing output, skip identical writes (keeps Rojo
    live-sync from re-syncing untouched files).
  - **Declaration emit (Package projects)**: per queued file, TS's own d.ts-only emit
    through the (possibly plugin-proxied) program. Output path comes from TS outputpaths
    (outDir mirror, `.d.ts`) — same shape as `pathTranslator.getOutputDeclarationPath`.
    QUIRK: declaration emit runs even when the Luau write was skipped by writeOnlyChanged,
    and is NOT content-gated.
  - afterDeclarations transformers (declaration mode only):
    - `transformTypeReferenceDirectives` (`Project/transformers/builtin/transformTypeReferenceDirectives.ts`):
      rewrites `/// <reference types="types" />` → `types="@rbxts/types"` by MUTATING
      `sourceFile.typeReferenceDirectives[i].fileName` in place (and bumping `.end` by 7).
    - `transformPathsTransformer(program, {})` (`builtin/transformPaths.ts`, vendored
      typescript-transform-paths): rewrites `compilerOptions.paths`/`baseUrl` aliases in
      declaration import/export/import-type specifiers to relative paths (no-op when neither
      baseUrl nor paths configured — L95).
    - tsgo port note: tsgo `Program.Emit(ctx, EmitOptions{TargetSourceFile, EmitOnly:
      EmitOnlyDts, WriteFile})` exists (`tsgo/compiler/program.go` L1583-1602,
      `emitter.go` L24-29) but has NO custom-transformer hook
      (`getDeclarationTransformers` hardcodes the standard transformer, emitter.go L54-57).
      Options: (a) post-process emitted d.ts TEXT via the WriteFile callback — the
      typeRef rewrite is a trivial string substitution (`/// <reference types="types" />`),
      and skip transformPaths when baseUrl/paths absent (SanitizeFS already strips baseUrl —
      check interaction!); (b) patch a hook into the vendored emitter. Flag (a) as default,
      with paths-alias projects an honest v1 limitation.
- **L210 `program.emitBuildInfo()`** — writes `.tsbuildinfo` (no-op unless incremental).
  Runs even after successful compile only (the error paths returned earlier). tsgo analog §4.
- L212 returns `{ emittedFiles, emitSkipped: false, diagnostics: flush() }`.

### 2.8 checkFileName scope QUIRK

compileFiles L71-75 runs `checkFileName` for every program source file outside
`data.nodeModulesPath`; watch additionally runs it for non-compilable ADDED files (`init.*.d.ts`
copies — setupProjectWatchProgram.ts L111-113). Both ported/known (`project.go` L156-163);
watch path needs the addition hook.

### 2.9 Comment directives — `preEmitDiagnostics/fileUsesCommentDirectives.ts`

Per compiled file (via `getCustomPreEmitDiagnostics`, `Project/util/getCustomPreEmitDiagnostics.ts`
— a one-entry checker list):

- skip entirely if `projectOptions.allowCommentDirectives` (L6-8);
- one `errors.noCommentDirectives` per `sourceFile.commentDirectives` entry (the
  `@ts-ignore`/`@ts-expect-error` ranges TS's scanner collected) at the directive's range
  (L12-19);
- plus per `ts-nocheck` pragma (`sourceFile.pragmas.get("ts-nocheck")`, single-or-array)
  (L21-31).
- Message (Shared/diagnostics.ts L162-166): ``Usage of `@ts-ignore`, `@ts-expect-error`, and
  `@ts-nocheck` are not supported!`` + `roblox-ts needs type and symbol info to compile
  correctly.` + suggestion ``Consider using type assertions or `declare` statements.``
- rotor: `DiagNoCommentDirectives` already exists (`internal/transformer/diagnostics.go`
  L247) but is UNWIRED. tsgo exposes both `SourceFile.CommentDirectives` and
  `SourceFile.Pragmas` (`tsgo/ast/ast.go` L2446, L2451) — direct port. (The `--!strict`
  hoisting is a DIFFERENT feature, already done — it's about Luau directive comments in
  OUTPUT, this is about TS suppression directives in INPUT.)

---

## §3 Watch mode — `Project/functions/setupProjectWatchProgram.ts`

### 3.1 Watcher setup (L24-31, L223-235)

chokidar over `getRootDirs(options)` with:

```ts
{ awaitWriteFinish: { pollInterval: 10, stabilityThreshold: 50 },
  ignoreInitial: true, disableGlobbing: true, usePolling }
```

Events: `add`+`addDir` → collectAddEvent; `change` → collectChangeEvent;
`unlink`+`unlinkDir` → collectDeleteEvent; `once("ready")` → report
`"Starting compilation in watch mode..."` then run the initial compile. All event paths are
`fixSlashes`'d (backslash→slash, L33-35).

QUIRK: only rootDirs are watched — tsconfig.json, the Rojo project file, package.json, and
node_modules are NOT watched; config changes require a manual restart. There is no
full-restart-on-config-change logic at all.

### 3.2 Debounce/batching (L195-221)

Three pending sets (filesToAdd/Change/Delete). First event opens a collection window:
`reportText("File change detected. Starting incremental compilation...")` then
`setTimeout(closeEventCollection, 100)` — a fixed **100 ms** batch window (not sliding);
events landing inside the window join the batch. closeEventCollection runs the compile and
reports the emit result.

### 3.3 Status reporting (L47-71)

- `watchReporter = ts.createWatchStatusReporter(ts.sys, /*pretty*/ true)`;
  `reportText` wraps a message-category diagnostic with **code 0**.
  TS 5.5 pretty watch-status format: `[<locale time string, grey>] <message>\n\n`.
  QUIRK: TS only clears the screen for its own codes 6031/6032
  (`screenStartingMessageCodes`); rbxtsc's code-0 messages therefore **never clear the
  screen** (and `preserveWatchOutput` is irrelevant). (Verify against TS 5.5 output when
  implementing; the no-clear behavior is observable rbxtsc UX.)
- `reportEmitResult`: print every diagnostic via the pretty diagnostic reporter, then
  `Found ${n} error${n===1?"":"s"}. Watching for file changes.` (L65-71).
- Exact watch strings: `Starting compilation in watch mode...`,
  `File change detected. Starting incremental compilation...`,
  `Found N errors. Watching for file changes.` — same wording as tsc's own messages; tsgo
  already has them as `diagnostics.Starting_compilation_in_watch_mode` etc.
  (`tsgo/execute/watcher.go` L122, L144, L193-196).

### 3.4 Initial compile (L81-93)

`refreshProgram()` (create builder program over `[...fileNamesSet]` + fresh PathTranslator)
→ `cleanup` → `copyInclude` → `copyFiles` → `compileFiles(program, data, pathTranslator,
getChangedSourceFiles(program))`. `initialCompileCompleted` only flips on
`!emitResult.emitSkipped` — i.e. a failed initial compile makes the NEXT event run the FULL
initial pipeline again (cleanup + copies included).

### 3.5 Incremental compile (L98-168)

For the batched additions/changes/removals:

- **additions**: directory → `walkDirectorySync` collecting compilable descendants into
  `fileNamesSet` + `filesToCompile`; compilable file → both sets; non-compilable →
  `checkFileName` + `filesToCopy` (L99-115).
- **changes**: compilable → `filesToCompile`; else if `.d.ts` and a transformerWatcher
  exists → `transformerWatcher.updateFile(fsPath, ts.sys.readFile(fsPath))` (keeps the
  plugin LanguageService program in sync — d.ts files never flow through compileFiles);
  always also `filesToCopy` (L117-137).
- **removals**: drop from `fileNamesSet`, add to `filesToClean` (L139-142).
- `refreshProgram()` — a NEW builder program every cycle (rootNames = updated fileNamesSet),
  chaining to `ts.readBuilderProgram` (disk buildinfo) as oldProgram. **Program reuse
  between rebuilds is entirely via .tsbuildinfo on disk**, not in-memory builder chaining.
- File selection: `getChangedSourceFiles(program, options.incremental ? undefined :
  [...filesToCompile])` (L146) — incremental projects trust the builder's changedFilesSet;
  non-incremental projects seed dependency search with the batched paths (hint mode finds
  importers via the new program's referencedMap).
- `compileFiles(...)`; on emitSkipped, RETURN before clean/copy ("exit before copying to
  prevent half-updated out directory", L148-151). QUIRK: the pending filesToCompile/Copy/
  Clean sets are module-level and NOT cleared on failure — the next cycle retries them
  (cleared only after success, L163-165).
- Post-emit: `tryRemoveOutput` per cleaned file (output + declaration output);
  `copyItem` per copy (L153-161).
- DiagnosticError thrown anywhere inside → caught as emitSkipped result (L183-192).

### 3.6 rotor port mapping

- Native watcher: `tsgo/fswatch` — recursive `WatchDirectory` with batched callbacks,
  per-platform backends (inotify/FSEvents/kqueue/Windows), built-in debounce (50 ms min /
  500 ms max — `fswatch/debounce.go` L8-11) and create+delete coalescing
  (`fswatch/event.go`). Events are only Update/Delete (`EventKind`); rbxtsc's add-vs-change
  distinction must be derived (stat: path in fileNamesSet already? directory?). The polling
  fallback for `--usePolling` can reuse `tsgo/vfs/vfswatch` (polling FileWatcher used by
  tsgo's own tsc --watch) or rotor's existing cmd/rotor/watch.go stamp-polling.
- rotor's current `check -w` (cmd/rotor/watch.go) is 250 ms full-recheck polling — fine for
  check, but `build -w` should follow the rbxtsc pipeline above.

---

## §4 Incremental builds — tsbuildinfo

### 4.1 What rbxtsc persists/invalidates (summary of §2.2/§2.3/§2.7)

- Standard TS `.tsbuildinfo` via `createEmitAndSemanticDiagnosticsBuilderProgram` +
  `program.emitBuildInfo()`; enabled by the project's own `incremental`/`tsBuildInfoFile`
  compilerOptions (rbxtsc forces nothing — `init game` templates ship `incremental: true`).
- Read path: `ts.readBuilderProgram` from disk at every program creation.
- Invalidation: TS file-version hashing, SALTED with compiler version + `--type` +
  isPackage + plugins JSON + raw Rojo config contents (§2.2). Changing any of those =
  full rebuild. Note the salt means rotor's buildinfo is inherently incompatible with
  rbxtsc's (different version string at minimum) — a rotor build after an rbxtsc build
  full-rebuilds, which is correct behavior.
- Consumption: `changedFilesSet` (+ referencedMap dependents) selects which files get
  re-transformed (§2.3); cleanup/copy phases are NOT incremental (they re-run fully each
  non-watch build).
- QUIRK: `emitBuildInfo` runs only on fully successful compiles (early returns skip it), so
  error builds don't advance the persisted state.

### 4.2 tsgo native analogs

tsgo does NOT expose `EmitAndSemanticDiagnosticsBuilderProgram`; its incremental machinery
lives in `tsgo/execute/incremental`:

- `incremental.Program` wraps `compiler.Program` + a `snapshot` holding `fileInfos`
  (version/signature), `referencedMap`, `semanticDiagnosticsPerFile`, `changedFilesSet`,
  `affectedFilesPendingEmit`, `emitSignatures` (`incremental/snapshot.go` L300-341) — the
  direct port of TS's builder state, serialized to/from `.tsbuildinfo`
  (`buildInfo.go`, `snapshottobuildinfo.go`, `buildinfotosnapshot.go`).
- Read path: `incremental.NewBuildInfoReader(host)` + `incremental.ReadBuildInfoProgram(config,
  reader, host)` (validates version + `IsIncremental()`) (`incremental/incremental.go` L44-56);
  buildinfo filename from `config.GetBuildInfoFileName()`.
- Chain: `incremental.NewProgram(compiler.NewProgram(...), oldIncrementalProgram, host,
  testing)` — exactly how tsgo's own watcher does it (`execute/watcher.go` L112, L172-175,
  which also demonstrates config-modification detection via `reflect.DeepEqual(ParsedConfig)`
  and `ExtendedSourceFiles` watching — useful if rotor watch wants to go beyond rbxtsc and
  handle tsconfig edits).
- Gaps to bridge for rbxtsc parity:
  1. **Hash salting**: tsgo computes file signatures internally; injecting the rbxtsc salt
     needs either a wrapper at the snapshot layer, our own salt-in-buildinfo field, or
     simplest: store the salt as a separate sidecar value in the buildinfo path and
     hard-invalidate (ignore buildinfo) when it differs. Recommend the sidecar/field
     approach — byte-format parity of `.tsbuildinfo` with rbxtsc is NOT required (different
     TS versions already make them incompatible).
  2. **changedFilesSet + referencedMap access**: snapshot fields are unexported; rotor needs
     a small exported accessor patch in the vendored tree (MIRROR.md precedent) or to
     recompute changed files itself (compare fileInfos hashes old-vs-new) — the
     `getChangedFilePaths` reverse-reference walk (§2.3) then runs over either tsgo's
     `referencedMap` or rotor's own import graph (the transformer already resolves imports;
     a per-file dependency list is cheap to record during transform).
  3. **emitBuildInfo**: `incremental.Program` writes buildinfo as part of its emit flow
     (`emitfileshandler.go`); rotor bypasses tsgo emit, so it must invoke the
     snapshot→buildinfo serialization directly after its own write phase.
- HONEST ASSESSMENT: full builder-fidelity incremental (signature-based "did the d.ts shape
  change" cutoffs) is the most tsgo-coupled Phase 4 item. A correct, simpler v1: persist
  rotor's own manifest (file path → content hash + salt + per-file import list), recompile
  changed files + transitive dependents, ignore TS signature optimization. That matches
  rbxtsc's OBSERVABLE behavior in all but the "edit a file without changing its public
  shape" optimization (rbxtsc recompiles dependents there too unless
  assumeChangesOnlyAffectDirectDependencies — TS's emit-signature cutoff mainly helps
  semantic-diagnostic reuse, which rotor recomputes anyway). Recommend manifest-v1, tsgo
  snapshot integration as a follow-up if cold-start profiling demands it.

---

## §5 validateCompilerOptions — full port (`Project/functions/validateCompilerOptions.ts`)

rotor has only rootDir/outDir (project.go L380-398). Full list — each failure pushes a
bullet; all collected then thrown as one ProjectError (L107-115):

```text
Invalid "tsconfig.json" configuration!
https://roblox-ts.com/docs/quick-start#project-folder-setup
- <error1>\n- <error2>\n...
```

(each error line ends with `\n` including the last — already byte-matched in rotor.)

Checks, in source order (kleur.yellow() = `y`, applied to the quoted fragments):

1. `opts.noLib !== true` → `` `"noLib"` must be `true` `` (L37-39).
2. `opts.strict !== true` → `` `"strict"` must be `true` `` (L41-43).
3. target check is **commented out** upstream (L45-47) — any target passes. Do not enforce.
4. `opts.module !== ts.ModuleKind.CommonJS` → `` `"module"` must be `commonjs` `` (L49-51).
5. `opts.moduleDetection !== ts.ModuleDetectionKind.Force` → `` `"moduleDetection"` must be `"force"` `` (L53-55).
6. `opts.moduleResolution !== ts.ModuleResolutionKind.Node10` → `` `"moduleResolution"` must be `"Node"` `` (L57-59).
7. `opts.allowSyntheticDefaultImports !== true` → `` `"allowSyntheticDefaultImports"` must be `true` `` (L61-63).
8. typeRoots: `path.join(projectPath, "node_modules", "@rbxts")` must be in
   `opts.typeRoots` (each side `path.resolve`'d) → else
   `` `"typeRoots"` must contain `<abs path>` `` (L65-68, helper L23-31). The yellow'd path
   is the ABSOLUTE `<projectPath>/node_modules/@rbxts` (platform separators).
9. For each `opts.types` entry: must exist under SOME typeRoot (fallback typeRoots
   `["node_modules/@rbxts"]` when undefined), testing `resolve(projectPath, typeRoot,
   typesLocation)` and `+ ".d.ts"` existence → else `` `"types"` `<loc>` were not found. Make
   sure the path is relative to `typeRoots` `` (L70-86).
10. `rootDir`/`rootDirs` (ported). 11. `outDir` (ported) (L89-95).
11. `opts.importsNotUsedAsValues !== undefined` → `` `"importsNotUsedAsValues"` is no longer
    supported, use `"verbatimModuleSyntax": <true|false>` instead `` — suggested value `true`
    iff the old value was `Preserve` (L98-104).

tsgo port notes: option enums live in `tsgo/core` (`core.ModuleKindCommonJS`,
`core.ModuleDetectionKindForce`, `core.ModuleResolutionKindNode10` — but **SanitizeFS rewrites
moduleResolution node→bundler-compatible values for tsgo**; the validation must run against
the USER's raw config, not the sanitized one. Recommend: parse the raw tsconfig once for
validation (or have SanitizeFS record original values) so checks 4-7 see what the user wrote.
Same caution for `noLib`: tsgo still resolves @rbxts types with noLib; verify the sanitizer
isn't touching it. Color: kleur.yellow = `\x1b[33m...\x1b[39m`; gate on NO_COLOR/TTY like
cmd/rotor's useColor.

---

## §6 Transformer plugins + the Node sidecar boundary

### 6.1 Upstream mechanics

- Trigger: `compilerOptions.plugins?.length > 0` (compileFiles.ts L109), verbose benchmark
  `running transformers..`.
- `getPluginConfigs(tsConfigPath)` (`Project/transformers/getPluginConfigs.ts`): re-reads the
  config file raw, collects `compilerOptions.plugins[]` entries having a string `transform`,
  then RECURSES into `extends` (require.resolve relative to the tsconfig dir) and APPENDS
  parent configs (own plugins first, then extended — L23-28).
- Config shape (`Shared/types.ts` L35-65): `{ transform?: string; import?: string;
  type?: "program"|"config"|"checker"|"raw"|"compilerOptions"; after?: boolean;
  afterDeclarations?: boolean; [k: string]: unknown }`.
- `createTransformerList(program, configs, projectPath)` (`createTransformerList.ts` L86-128):
  resolve.sync each `transform` relative to projectPath, `require` the module, pick export
  `config.import ?? "default"` (function-module = default), call the factory per `type`
  (default/`program` → `factory(program, manualConfig, { ts })`; `checker` →
  `(program.getTypeChecker(), cfg)`; `compilerOptions`; `config`; `raw` — L39-61);
  result placed in before/after/afterDeclarations buckets (a bare function goes to `before`
  unless `after`/`afterDeclarations` flags). `manualConfig` = the config minus
  after/afterDeclarations/type keys. Failures → `warnings.transformerNotFound(name, err)`:
  `` Transformer `<name>` was not found! `` + `More info: <err>` + suggestion
  `Did you forget to install the package?` (diagnostics.ts L240-245) — a WARNING, build
  continues without the plugin.
- `flattenIntoTransformers` order QUIRK (L74-84): **after, then before, then
  afterDeclarations** — yes, `after` transformers run FIRST in the single
  `ts.transformNodes` pass. Verbatim-port this oddity.
- Application (compileFiles.ts L114-142): one `ts.transformNodes(undefined, undefined,
  ts.factory, compilerOptions, sourceFiles, transformers, false)` over the to-compile set;
  diagnostics added; each transformed SourceFile is **reprinted to text**
  (`ts.createPrinter().printFile`) because transformed nodes lack symbol/type info; text fed
  into `data.transformerWatcher ??= createTransformerWatcher(program)` — a LanguageService
  over a versioned in-memory overlay (`createTransformerWatcher.ts`: file→version map,
  `updateFile` bumps version, reads fall back to ts.sys) — and `proxyProgram =
  service.getProgram()`. All downstream type queries + declaration emit use proxyProgram.
  `--writeTransformedFiles` dumps the printed text to
  `getOutputTransformedPath` (out of v1 scope).
- Watch integration: `.d.ts` changes routed to `transformerWatcher.updateFile` (§3.5);
  the watcher persists on ProjectData across rebuilds.

### 6.2 Sidecar protocol spec (per design doc §Transformer plugins, rotor-side)

Boundary: text→text per compilation pass. Projects without `plugins` never spawn the sidecar.

- **Spawn**: `node <bundled sidecar.js>`, once per build (kept alive in watch). Locate node
  from PATH; failure → hard ProjectError naming the missing prerequisite.
- **Request** (JSON over stdio, one message per compile pass):

  ```jsonc
  {
    "protocol": 1,
    "tsConfigPath": "<abs>",        // sidecar runs getPluginConfigs + program itself
    "projectDir": "<abs>",
    "compileFileNames": ["<abs>.ts", ...],   // the sourceFiles list rotor will compile
    "changedFiles": [{ "fileName": "<abs>", "text": "..." }]  // watch overlay updates (d.ts etc.)
  }
  ```

  The sidecar (real `typescript` npm package) builds/reuses its own ts.Program (full
  TypeChecker — flamework-class plugins work), runs getPluginConfigs → createTransformerList
  → ts.transformNodes → printFile, mirroring §6.1 exactly including the after/before/
  afterDeclarations flatten order.
- **Response**:

  ```jsonc
  {
    "diagnostics": [{ "category": "error|warning", "code": "...", "file": "...",
                      "start": 0, "length": 0, "message": "..." }],
    "transformed": [{ "fileName": "<abs>.ts", "text": "<printed TS source>" }]
  }
  ```

  rotor then re-parses each transformed text through tsgo as an overlay (the analog of
  proxyProgram: a second tsgo Program whose host serves overridden file contents) and
  compiles from THAT program/checker. Declaration emit (Package + plugins) also runs over
  the overlay program.
- Watch: sidecar stays warm; rotor streams `changedFiles` deltas (including the `.d.ts`
  update path of §3.5). One sidecar per project.
- Plugin-visible fidelity: because the sidecar uses the real JS typescript at rbxtsc's
  pinned major, plugin behavior is bit-compatible; the only rotor-side risk is the overlay
  re-parse (tsgo TS-version differences on the PRINTED output — printed output is plain TS,
  low risk).

---

## §7 Checker parallelism — the CHECKER-IDENTITY PIN

### 7.1 Current state

`internal/compile/project.go` L70-89: `parsed.CompilerOptions().Checkers = &one` before
NewProgram. The pin exists because:

- tsgo's pool (`tsgo/compiler/checkerpool.go`): default 4 checkers (L41), files assigned
  ROUND-ROBIN `fileAssociations[file] = checkers[i%checkerCount]` (L116);
  `Program.GetTypeChecker(ctx)` returns checkers[0] while
  `GetSemanticDiagnostics(ctx, file)` checks with the file-associated checker
  (`program.go` L435-440, L452-466, L541).
- `EmitResolver.IsReferencedAliasDeclaration` (`tsgo/checker/emitresolver.go` L688-699) reads
  `c.aliasSymbolLinks` — per-checker-INSTANCE LinkStore (`checker.go` L676) populated during
  that instance's semantic pass. Import elision read from the wrong checker = spurious
  elision (proven: TestCompileProjectImportsModel breaks without the pin).
- `checker.mergedSymbols` is per-instance (`checker.go` L661, L966): global/ambient merged
  symbols have DIFFERENT `*ast.Symbol` identities per checker. Everything rotor keys by
  symbol pointer is therefore checker-scoped when the symbol is checker-created:
  - MacroManager maps (`internal/transformer/macromanager.go` L200-205): keys come from
    `chk.ResolveName` (globals → merged symbols) and `chk.GetTypeAtLocation(...).Symbol()`
    (method-type symbols — checker-created) (L243-345).
  - MultiState caches (`state.go` L26-32): six `map[*ast.Symbol]...` shared across files.
  - Per-file State maps (IsHoisted, SymbolToID, moduleIDBySymbol) — file-scoped, safe.

### 7.2 What unpinning would require

1. **Per-file checker affinity**: every State must use
   `Program.GetTypeCheckerForFileExclusive(ctx, file)` (program.go L461) so transform-time
   queries hit the checker whose semantic pass populated `aliasSymbolLinks` for that file.
   The semantic-diagnostics call already routes there; rotor's single
   `GetTypeChecker(ctx)` grab (project.go L306) must become per-file.
2. **Per-checker MacroManager**: one instance per pool checker, built lazily on first file
   assigned to it (registration cost ×4; ~milliseconds). `Missing()` audit runs once (any
   instance).
3. **Per-checker MultiState shards** for the six symbol caches. Subtlety: caches whose keys
   are BINDER symbols (user-project declarations) are identity-stable across checkers, but
   `GetModuleExportsCache`/`GetModuleExportsAliasMapCache` store checker-DERIVED symbol
   slices, and the `IsReportedBy*` dedup caches change DIAGNOSTIC counts if sharded (same
   symbol diagnosed via two checkers → duplicate warning vs upstream's single). Exact-parity
   dedup would need a checker-independent key (symbol's declaration node pointer) — doable
   but new invariants.
4. **Scheduling**: transform must hold the file's checker exclusively (checker query APIs are
   not concurrent-safe per instance); the natural shape is tsgo's own
   `forEachCheckerGroupDo` pattern (checkerpool.go L148-165) — N goroutines, each serially
   transforming its checker's files. Output (write queue, diagnostics, progress lines) must
   be re-serialized deterministically (sort by file order) to keep byte/UX parity.
5. **MultiState non-symbol state** (e.g. anything order-dependent) audited for cross-file
   determinism under reordered execution.

### 7.3 Recommendation

**Keep the pin for v1.** Rationale:

- Correctness surface is wide (items 1-5), and the dedup-cache divergence (item 3) directly
  threatens the diagnostics-corpus parity gate of Phase 5.
- The win is bounded: with the pin, the semantic pass is single-checker (≈ tsc-equivalent
  speed — still massively faster than rbxtsc's JS checker, which is rotor's actual
  comparison baseline); transform itself is a small fraction of wall time.
- The pin costs ONE line and is fully understood; unpinning is an isolated perf project.
Phase 4 should include a measurement task (time check-vs-transform on `randomness` and the
tests corpus with checkers=1 vs 4 for the CHECK phase only) so the post-v1 decision is
data-driven. If measurements show the checker pass dominating cold builds badly, the §7.2
plan is the spec.

---

## §8 Gap sweep — everything compileFiles/CLI does that rotor lacks today

| # | Capability | Upstream cite | rotor status |
|---|---|---|---|
| 1 | yargs-equivalent `build` flag surface (§1.2) + `rbxts` tsconfig key | build.ts L49-136 | `build [path]` only |
| 2 | `--project` resolution incl. upward tsconfig search + file paths | build.ts L31-40 | dir-only, no search |
| 3 | ProjectOptions plumbing into transformer (optimizedLoops gate, logTruthyChanges, allowCommentDirectives) | constants.ts L41-55 | fields exist; no wiring from CLI |
| 4 | `--type` override of inferProjectType + copyInclude skip logic | compileFiles.ts L80, copyInclude.ts L7-11 | inference only |
| 5 | `--rojo` / `--includePath` overrides | createProjectData.ts L29-43 | hardcoded discovery |
| 6 | `--luau=false` → `.lua` extension | createPathTranslator.ts L17 | hardcoded `.luau` (project.go L369) |
| 7 | cleanup of orphaned outputs (+ `.git` skip, buildinfo protection) | cleanup.ts, tryRemoveOutput.ts | none |
| 8 | include/ copy (RuntimeLib.lua + Promise.lua assets) | copyInclude.ts; `include/` | no assets in repo |
| 9 | non-compiled file passthrough (lua/json/d.ts policy, writeOnlyChanged) | copyItem.ts | none |
| 10 | output writing inside the compile result path + `--writeOnlyChanged` + emittedFiles | compileFiles.ts L187-208 | cmd-level naive write |
| 11 | `.d.ts` emit for Package projects + afterDeclarations transforms | compileFiles.ts L190-205 | none (tsgo EmitOnlyDts available) |
| 12 | comment-directive pre-emit diagnostics (`@ts-ignore` etc.) | fileUsesCommentDirectives.ts | diagnostic exists, unwired |
| 13 | collect-all-files diagnostics before bailing (vs first-file abort) | compileFiles.ts L151-185 | first-error abort |
| 14 | Rojo-resolver/config warnings via `Compiler Warning:` channel | compileFiles.ts L65-67, createProjectData.ts L41 | dropped |
| 15 | progress/verbose output (`N/M compile`, benchmarks, `compiling as X..`) | compileFiles.ts L102-155, benchmark.ts | none |
| 16 | pretty diagnostic printing with `roblox-ts` code + located ids | createTextDiagnostic.ts, diagnostics.ts | plain strings |
| 17 | exit-code parity (1 for usage errors too) | cli.ts L30-35 | uses 2 |
| 18 | watch mode for build (events, 100 ms batch, incremental sets, statuses) | setupProjectWatchProgram.ts | check-only poll watch |
| 19 | incremental builds (.tsbuildinfo + salt + changed-file selection) | createProgramFactory.ts, getChangedFilePaths.ts | none |
| 20 | transformer plugins (sidecar) | compileFiles.ts L109-144, transformers/ | none |
| 21 | `getChangedSourceFiles` JSON-file exclusion symmetry | getChangedSourceFiles.ts L8 | equivalent filter exists (project.go L316-318) |
| 22 | `--watch` + `--usePolling` flag semantics for build | build.ts L63-72 | n/a |

Out of v1 scope (design doc §Out, confirm in plan): `--writeTransformedFiles`, playground
VirtualProject/VirtualFileSystem, devlink.

---

## §9 Open questions / risks

1. **SanitizeFS vs validateCompilerOptions**: rotor's sanitizer rewrites the very options
   (moduleResolution, downlevelIteration, baseUrl) the validator must check from the USER's
   perspective. Validation must read the raw config (or sanitizer must report originals).
   Also: does SanitizeFS stripping `baseUrl` break transformPaths-relevant projects silently?
2. **tsgo declaration emit fidelity**: tsgo's DeclarationTransformer is TS7-based; rbxtsc
   emits with TS 5.5.3. Package-project d.ts output may differ textually (ordering,
   `/// <reference>` synthesis). Phase 5 differential should include a Package fixture's
   d.ts. afterDeclarations gap handled per §2.7 (text post-process); paths-alias rewriting
   in declarations is the weakest spot — decide: text-level rewrite vs documented limitation.
3. **Incremental depth** (§4.2): manifest-v1 vs vendored-snapshot integration — pick after
   profiling cold vs warm builds on `randomness`. Unexported snapshot fields require a
   mirror patch if we go deep.
4. **Watch UX parity**: locale time strings (`[1:23:45 PM]`) — match Node's
   `toLocaleTimeString()` or Go's equivalent; harmless divergence but worth a deliberate
   choice. Confirm no-screen-clear behavior against real rbxtsc.
5. **chokidar vs fswatch semantics**: awaitWriteFinish (50 ms stability) suppresses
   partial-write compiles; fswatch's debounce is similar but not identical — watch tests
   should cover rapid-save editors. add/addDir/change derivation from EventUpdate needs the
   fileNamesSet+stat heuristic (§3.6).
6. **Plugin sidecar TS version**: bundle which typescript? rbxtsc pins =5.5.3; flamework
   ecosystems track it. Pin the sidecar to 5.5.3 for fidelity.
7. **Diagnostics-at-once (gap #13)** changes CompileProject's public contract (collect
   per-file diagnostics and continue) — diff-harness tests asserting first-error behavior
   may need updating.
8. **Windows paths**: upstream fixSlashes + getCanonicalFileName (case-insensitive lowering)
   — rotor must canonicalize watch paths against program file names consistently (tsgo
   `tspath` helpers exist).

### Suggested task breakdown (for the Phase 4 plan)

1. **ProjectOptions + CLI surface**: options struct w/ defaults, `rbxts` tsconfig key, full
   flag parsing for `rotor build` (incl. `--project` search), exit-code policy, LogService
   analog (verbose/warn/benchmark). (§1)
2. **Output pipeline**: write phase in CompileProject (outputFileSync semantics,
   writeOnlyChanged, emittedFiles), cleanup, copyInclude (embed include/ assets), copyFiles/
   copyItem, buildinfo-path wiring in createPathTranslator. (§2.4-2.7)
3. **validateCompilerOptions full port** + raw-config sourcing + kleur-yellow formatting. (§5)
4. **Comment-directive pre-emit check** + collect-all-diagnostics refactor + pretty
   diagnostic printer (` roblox-ts` code rendering). (§2.7/2.9, gaps 12/13/16)
5. **Declaration emit for Packages**: EmitOnlyDts + typeRef text rewrite; Package fixture in
   diff harness. (§2.7)
6. **Watch mode**: fswatch integration, 100 ms batching, add/change/delete sets, incremental
   recompile loop, status lines, `--usePolling` fallback. (§3)
7. **Incremental v1**: manifest (salt + content hashes + import graph) + changed-file
   selection (getChangedFilePaths port); measure; decide on tsgo-snapshot deep integration. (§4)
8. **Plugin sidecar**: Node helper (getPluginConfigs/createTransformerList/transformNodes/
   printFile), JSON protocol, overlay Program on the rotor side, watch warm-keeping. (§6)
9. **Perf measurement task** for the checker pin (no unpinning in v1). (§7)
