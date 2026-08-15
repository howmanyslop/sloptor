# rotor Phase 3a Implementation Plan — Imports, Module Resolution, NewExpression

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** rotor compiles multi-file projects — import/export-from statements, node_modules package imports, Rojo-aware require chains, runtime-lib injection, and `new X(...)` — unblocking 96% of the real-world files the Phase 2b smoke identified.

**Architecture:** Port from `docs/superpowers/research/phase3-imports-digest.md` (source of truth; `reference/` wins on disagreement — including the two newly-vendored resolver repos). The differential harness gains multi-file fixtures; `CompileFile` grows into a project-aware compile per digest §7. Established conventions apply (TDD; byte parity or loud diagnostics; quirks verbatim; oracle technique; full gates per commit).

**Out of scope (Phase 3b):** JSX, property-call/call macros beyond constructor macros (MacroManager centralization HAPPENS here as a refactor, the full macro tables don't), optional chaining, Map/Set for-of builders + the Iterable-arity overlay fix, classes, async/try, watch/concurrency refactors.

---

## Task 1: Vendor resolver references + port RojoResolver/PathTranslator

1. Vendor `github.com/roblox-ts/rojo-resolver` @ v1.1.0 and `github.com/roblox-ts/path-translator` @ v1.1.0 into `reference/rojo-resolver/` and `reference/path-translator/` (same recipe as Phase 0 Task 2: clone at the version tag — verify package.json version 1.1.0, fall back to the matching commit; strip .git; record SHAs in `reference/VERSIONS.md`; LICENSE files must exist).
2. Port both to Go: `internal/rojo/resolver.go` + `internal/rojo/pathtranslator.go` (package `rojo`) per the vendored TS sources, cross-checked against digest §4's behavioral extraction (partitions LIFO, init/index renames, sub-extensions, isolated/network containers, `Relative`, `GetImportPath(isNodeModule)` no-rebase quirk, `RojoResolver.Synthetic`). TDD with table-driven tests over: the diff fixture's default.project.json, a game-shaped project json (ServerScriptService/ReplicatedStorage tree), synthetic, and path-translation cases (init.luau, .d.ts skips, sub-extensions).
Commit: `"rojo: RojoResolver and PathTranslator ports"`

### Task 2: MacroManager centralization (refactor) + constructor macros

1. `internal/transformer/macromanager.go`: one type owning ALL macro identification — move/absorb the scattered stand-ins (isSizeMacro in loops.go, findRangeMacro in forof.go, the call.go isCallMacroSymbol/isPropertyCallMacroSymbol/compiler-types checks, identifier.go, access.go). Same stand-in SEMANTICS (compiler-types detection), one implementation, with the real MacroManager's resolution structure (symbol→macro lookup maps per digest §6 + the Phase 2 transformstate digest's MacroManager section) so Phase 3b drops in tables without touching call sites. All existing tests must stay green (pure refactor at the behavior level).
2. CONSTRUCTOR_MACROS per digest §6: `new Array(n)` → table.create path, `new Set/Map` literal-vs-loop paths (decided on the TRANSFORMED luau AST per digest), `new WeakSet/WeakMap` → `__mode = "k"` setmetatable wrap. These are real implementations (not stand-ins) — they're self-contained.
3. `transformNewExpression` per digest §6: construct-signature-symbol macro lookup → constructor macro or fallback `X.new(args)` (covers `new Instance("Part")`). Wire NewExpression in dispatch.
Oracle-pin unit tests: each constructor macro (incl. literal vs loop Set/Map variants), `new Instance("Part")`, user-class-shaped `new X()` against a declared `.new`-style interface if expressible without classes (else document deferral to Phase 3b classes).
Commit: `"transformer: MacroManager skeleton, constructor macros, new expressions"`

### Task 3: Project-aware compile + runtime-lib emission

Per digest §5 + §7:

1. `internal/compile`: `CompileProject(projectDir) (map[relOutPath]string, diags, error)` — one Program; per-file State sharing MultiState; module-specifier→SourceFile resolution via the tsgo equivalents the digest names (ResolveExternalModuleName etc.); PathTranslator out-paths. Keep `CompileFile` as a thin wrapper for existing tests (single file, same machinery).
2. Runtime-lib emission replaces the `ErrRuntimeLibNotSupported` sentinel: Model-type `local TS = require(script.Parent...RuntimeLib)` relative chains (the diff fixture is Model — digest §5 has the verbatim 2-file ground truth), Game absolute `GetService`/`WaitForChild` chains, Package `_G[script]`. RuntimeLibRbxPath validation diagnostics (the 4 plain-text emit failures digest §8 flags as new — add them).
3. The TransformState gains the Rojo context (RojoResolver, PathTranslator, ProjectType, runtimeLibRbxPath) per the reference TransformState fields deferred since Phase 2.
Commit: `"compile: project-aware compilation and runtime library emission"`

### Task 4: Import declarations

Per digest §1: transformImportDeclaration (type-only early-out; lazy TS.import deferral — fully-elided imports emit NOTHING; use-counting → tempId(cleanModuleName(lastSegment)) for >1 use; default-import real-`default`-export check via getModuleExports else synthetic-default whole-module table; namespace imports never elided; named elements propertyName??name; side-effect + verbatimModuleSyntax CallStatement fallback) + transformImportEqualsDeclaration (eager, no .default). Elision via tsgo's `IsReferencedAliasDeclaration` (checker/emitresolver.go:688 — check export status; if unexported, add an overlay shim via tools/mirror/overlay like GetTypeOfAssignmentPattern; note the `canCollectSymbolAliasAccessibilityData` gate and the requirement that GetSemanticDiagnostics ran first — rotor's pipeline already does). createImportExpression per digest §3 — the full pipeline (node_modules scope/typeRoots checks → TS.getModule for Package... for the Model fixture: node_modules-in-rbxPath; project paths: non-ModuleScript ban, Game network/isolation FileRelation → absolute vs relative; dynamic import() → NYS-or-TS.Promise per digest — implement what the digest specs, defer what it defers (guessVirtualPath symlinks)). Wire ImportDeclaration/ImportEqualsDeclaration in dispatch.
Commit: `"transformer: import declarations and module-to-Luau resolution"`

### Task 5: Export-from + export assignment

Per digest §2: transformExportDeclaration (`export {x} from` per-statement exports.X assignments; `export * from` → `for _k, _v in importExp or {} do exports[_k] = _v end` star loop; hasExportFrom forces exports-table shape — already wired in ChooseExportShape) + transformExportAssignment (`export =`). The star/named-from ordering confinement the digest proved (statement position, never exportPairs) resolves the exports.go TODO — update that comment. Wire both kinds.
Commit: `"transformer: export-from and export assignment"`

### Task 6: Multi-file differential fixtures

1. New fixtures: `21_imports/` — the harness must learn multi-file: fixture sources `21a_util.ts` (exports consts + functions) + `21b_main.ts` (named/default/namespace imports of 21a, uses them) — flat files in the same src/ work fine (relative `./21a_util` imports). `22_exportfrom.ts` + `22a_base.ts` (star + named re-exports). `23_new.ts` (constructor macros + Instance.new). Adjust the diff harness: goldens already exist per-file; `CompileFile` per fixture still works IF imports resolve project-wide — switch the harness to CompileProject and compare EVERY produced out-file against its golden (manifest entries become out-file names; simplest: keep per-file enablement, harness compiles the project once and diffs enabled files).
2. Oracle regen; existing 20 goldens MUST be byte-unchanged (imports in new files don't affect old outputs — verify).
3. Enable; iterate to byte parity. The 2-file ground truth in digest §5 predicts the shapes (temp `__scratch_util`-style module temps, `.default` unwrap, runtime-lib require lines).
Commit: `"diff: multi-file import fixtures byte-identical"`

### Task 7: Conformance + randomness re-smoke + final gates + merge

1. All fixtures green; full suite; vet; gofmt.
2. Adversarial fixture `24_mixed3a.ts(+deps)`: diamond imports (two files importing the same util), import-only-for-types (elision proof), re-export chains, new-in-loop with closure.
3. **Randomness re-smoke** (the payoff measurement): re-run the per-file tally — report how many of the 95 files now compile byte-identical vs before (14), and the new blocker frequency table (expect JSX/macros/optional-chaining to surface as the next wall). NOTE: cross-file imports of files that themselves fail may cascade — tally first-blocker per file honestly.
4. README roadmap (split Phase 3 row: 3a imports/new ✅ — 3b JSX/macros/classes 🚧), memory update.
5. Final whole-branch review (opus) → fixes → merge to master → push.
Commit: `"Phase 3a complete: multi-file projects compile"`

---

## Done criteria

1. Multi-file fixtures (21-24) byte-identical including runtime-lib require lines and the star-export loop.
2. RojoResolver/PathTranslator ported with the vendored references in-repo.
3. MacroManager is ONE component; constructor macros + NewExpression live.
4. Randomness re-smoke shows a step-change in byte-identical file count; the new blocker table recorded for Phase 3b.
