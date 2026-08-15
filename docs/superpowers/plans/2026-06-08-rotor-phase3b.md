# rotor Phase 3b Implementation Plan — Resolution Gaps, Macro Tables, Optional Chaining, Iteration

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the two module-resolution gaps blocking 64 randomness files, fix the Iterable-arity divergence, land the complete property-call/call macro tables with the registration audit, port optional chaining, and complete the for-of builders — leaving JSX/classes/async as Phase 3c.

**Source of truth:** `docs/superpowers/research/phase3b-digest.md` (956 lines; §1 baseUrl→paths with empirical proof, §2 symlinks via tsgo GetSymlinkCache, §3 Iterable-arity d.ts rewrite RECOMMENDED+VALIDATED, §4 the complete macro tables with verbatim emit logic + wrapComments machinery, §5 optional chaining with the vestigial-snapshot finding, §6 registration audit spec). Reference (`reference/roblox-ts/src/...`) wins on disagreement. Established conventions apply (TDD, byte parity or loud diagnostics, oracle technique, full gates per commit, quirks verbatim).

---

## Task 1: Resolution fixes — baseUrl→paths, Iterable-arity overlay, registration audit

Per digest §1 (sanitizer rewrite `baseUrl: "src"` → `paths: {"*": ["./src/*"]}` with the existing-paths merge rule; the `extends` gap documented), §3 (SanitizeFS-style d.ts rewrite of the 4 compiler-types interfaces to `<T, TReturn = any, TNext = any>` — apply via the same vfs layer, scoped to `node_modules/@rbxts/compiler-types/types/*.d.ts` reads), §6 (NewMacroManager.Missing() + CompileProject/CompileFile failure with upstream ProjectError texts, sentinel-gated). Tests: the digest's empirical scenarios become regression tests (non-relative import through a baseUrl project; Set/Map/Generator for-of reaching transformer NYS instead of checker diags; audit failure on a broken types package). Full suite green; 29/29 unchanged.
Commit: `"compile: baseUrl paths rewrite, Iterable arity overlay, macro registration audit"`

### Task 2: pnpm symlinks (guessVirtualPath)

Per digest §2: port the TransformState.ts:365-384 ancestor walk over tsgo's `Program.GetSymlinkCache().DirectoriesByRealpath()` (lexicographic-min for the unordered set divergence), wired at the importexpr.go:244 TODO. Test: a constructed symlink/junction fixture (Windows junctions work without admin) reproducing the pnpm layout — realpathed module under `.pnpm` resolves back to the virtual `node_modules/@rbxts/...` path and emits the correct TS.getModule. **Then the intermediate measurement**: re-run the randomness first-blocker tally (per-file CompileFile, throwaway program) — report the new byte-identical count (expect a jump from 28 as the 64 resolution-blocked files unlock into either success or their NEXT blocker). Don't commit the tally.
Commit: `"transformer: pnpm symlink resolution for package imports"`

### Task 3: Macro infrastructure + String/ArrayLike tables

Per digest §4: the `wrapComments` machinery (`-- ▼ X ▼`/`-- ▲ X ▲` markers incl. the push-exempt header rule and ≥2-prereq threshold), `argumentsWithDefaults`, then the String table (13 macros: size/byte/find/format/gmatch/gsub/lower/match/rep/reverse/split/sub/upper — mostly direct `string.X` mappings per digest) + ArrayLike.size. Register via MacroManager (real Macro funcs replacing nil sentinels). Oracle-pinned fixture `26_stringmacros`.
Commit: `"transformer: macro infrastructure, String and ArrayLike macros"`

### Task 4: Array tables

ReadonlyArray (15: isEmpty/join/move/includes/indexOf/every/some/forEach/map/mapFiltered/filterUndefined/filter/reduce/find/findIndex) + Array (9: push/pop/shift/unshift/insert/remove/unorderedRemove/sort/clear) per digest §4 verbatim emit logic (the loop shapes, reduce's noReduceEmptyArray runtime error, sort's table.sort). Oracle-pinned fixture `27_arraymacros` (+unit fixtures for the long tail). This is the biggest single task — split the Go across `arraymacros.go`/`arraymacros2.go` if >400 lines each.
Commit: `"transformer: ReadonlyArray and Array macros"`

### Task 5: Set/Map/Promise + call macros

Set/Map families (incl. shared READONLY_SET_MAP/SET_MAP methods: size/has/delete/isEmpty/clear/forEach/get/set/add) + `Promise.then`→`andThen` + the call macros (assert/typeOf/typeIs/classIs/identity/$tuple — $range already NYS-guarded in for-of, now implement; $getModuleTree per digest or NYS with marker) + Promise identifier macro (TS.Promise). Oracle-pinned fixture `28_collectionmacros`.
Commit: `"transformer: Set, Map, Promise, and call macros"`

### Task 6: for-of builders completion

Per the Phase 2b digest §3 table + 3b arity fix: Set/Map/string/IterableFunction/IterableFunctionLuaTuple/generator-object builders + the inline-destructure fast paths (Map `[k,v]`). The binding-accessor NYS entries in binding.go get their real accessors too (same table). Oracle-pinned fixture `29_iteration`.
Commit: `"transformer: complete for-of builders and binding accessors"`

### Task 7: Optional chaining

Per digest §5: transformOptionalChainInner full port (snapshot-free chainItem — the digest proved upstream's type fields vestigial), double nil-check nesting, `_self` method rule, temp reuse, noOptionalMacroCall interplay, the 5 worked block shapes as oracle-pinned tests. Fixture `30_optional`.
Commit: `"transformer: optional chaining"`

### Task 8: Conformance + re-smoke + merge

Adversarial fixture `31_mixed3b` (macros chained through optional calls inside for-of over Maps, etc.). Full randomness re-smoke (expect the count to approach the JSX wall: ~36 .tsx + classes/async stragglers remain). README roadmap (3b ✅, 3c JSX/classes/async 🚧). Memory update. Final whole-branch review (opus) → fixes → merge → push.
Commit: `"Phase 3b complete: macros, optional chaining, full iteration"`

---

## Done criteria

1. All fixtures byte-identical (target ~36+ entries incl. 26-31).
2. randomness blocked-file table shows resolution gaps GONE; count materially above 28 (measure, report honestly).
3. No nil-Macro sentinel reachable for any upstream-implemented macro (registration audit enforces).
4. Optional chaining + full iteration live; remaining NYS: JSX, classes, enums, async/generators, try, namespaces ($getModuleTree if deferred).
