# Phase 3b Digest — baseUrl→paths, pnpm symlinks, Iterable arity, macro tables, optional chaining

Source of truth for porting the Phase 3b unlock set without re-reading the TS. All upstream
paths relative to `reference/roblox-ts/src/` unless prefixed; tsgo paths relative to repo root.
Every checker/type-API usage is flagged `CHECKER:`. Builds on `phase2-transforms-digest.md`
(P2, esp. §8 calls), `phase2b-transforms-digest.md` (P2b), `phase2-transformstate-digest.md`
(TS-state), `phase3-imports-digest.md` (P3, esp. §3 createImportExpression). SCOPE: (1) the
sanitizer baseUrl→paths rewrite, (2) guessVirtualPath / tsgo symlink cache, (3) the
Iterable-arity checker prerequisite, (4) PROPERTY_CALL_MACROS + CALL_MACROS + IDENTIFIER_MACROS
exhaustively, (5) transformOptionalChain COMPLETE, (6) the macro registration audit, (7)
diagnostics delta. Empirical claims in §1 and §3 were verified 2026-06-06 by compiling a copy
of `testdata/diff/project` through `internal/compile.CompileProject` (probe details inline).

Rotor hook points already in place (do not re-port): `runCallMacro` + the three call inners
(`internal/transformer/call.go:188-458`), `flattenOptionalChain`/`transformChainItem`
(call.go:500-583), MacroManager tables/fallbacks (`macromanager.go`), `SanitizeFS`/
`SanitizeTSConfig` (`internal/compile/sanitize.go`), `getImportParts` (`importexpr.go`).

---

## 1. baseUrl → paths rewrite (SanitizeTSConfig)

### 1.1 Problem

rbxtsc projects set `"baseUrl": "src"` so `import ... from "shared/foo"` resolves to
`src/shared/foo.ts`. tsgo (TS7) REMOVED baseUrl: `tsgo/compiler/program.go:791-803` emits a
hard "removed option" diagnostic, so the sanitizer deletes the key (sanitize.go:82) — which
silently breaks every non-relative project-internal import (34 randomness files:
`Cannot find module 'shared/...'`, a tsgo SEMANTIC diagnostic surfaced by
`preEmitDiagnostics`, NOT rotor's `noModuleSpecifierFile`).

### 1.2 TS7's own replacement (authoritative)

tsgo's removal diagnostic SUGGESTS the exact rewrite (program.go:791-803):

```go
if options.BaseUrl != "" {
    relative := tspath.GetRelativePathFromFile(configFilePath(), options.BaseUrl, ...)
    if !(strings.HasPrefix(relative, "./") || strings.HasPrefix(relative, "../")) {
        relative = "./" + relative
    }
    suggestion := tspath.CombinePaths(relative, "*")
    useInstead = fmt.Sprintf(`"paths": {"*": [%s]}`, ...)
}
```

i.e. for `baseUrl: "src"` → **`"paths": { "*": ["./src/*"] }`** in the same config file.

### 1.3 Why it is exactly equivalent (tsgo resolver mechanics)

- **paths base directory**: `core.CompilerOptions.GetPathsBasePath`
  (tsgo/core/compileroptions.go:363-371) returns `options.PathsBasePath`, which
  `tsgo/tsoptions/tsconfigparsing.go:1061-1067` sets to the directory of the config file that
  DECLARES `paths` ("Since 'paths' can be inherited from an extended config in another
  directory ... store it here"). TS5 resolved baseUrl relative to the declaring config's dir
  too — same anchor. (`parsinghelpers.go:460` also accepts an explicit `pathsBasePath` key.)
- **eligibility**: `tryLoadModuleUsingPathsIfEligible` (tsgo/module/resolver.go:1244-1264)
  runs for any NON-relative specifier when `Paths.Size() > 0`; substitutions are resolved
  `tspath.CombinePaths(baseDirectory, strings.Replace(subst, "*", matchedStar, 1))` and loaded
  via `nodeLoadModuleByRelativeName(..., considerPackageJson=true)` — full extension probing
  - directory/index resolution, identical to what a baseUrl candidate got in TS5.
- **fallthrough**: `tryLoadModuleUsingPaths` (resolver.go:1266-1302) returns
  `continueSearching()` when the matched pattern's substitutions all fail;
  `resolveNodeLikeWorker` (resolver.go:540-580) then proceeds to
  `loadModuleFromNearestNodeModulesDirectory` + `resolveFromTypeRoot`. So
  `import "@rbxts/foo"` matches `"*"`, fails `./src/@rbxts/foo`, and still resolves through
  node_modules — same net behavior as TS5's baseUrl-miss→node_modules walk.
  (Divergence note, harmless: TS5 paths used SearchResult semantics where a matched-but-failed
  pattern could STOP resolution; tsgo always falls through. Only affects diagnostics of
  already-broken imports.)
- It also runs under `moduleResolution: "bundler"` (the sanitizer's choice):
  `tryLoadModuleUsingOptionalResolutionSettings` (resolver.go:1221-1232) is called
  unconditionally from `resolveNodeLikeWorker` (resolver.go:541), which serves bundler mode.
  Its comment "No more tryLoadModuleUsingBaseUrl" marks exactly the removed TS5 branch.

### 1.4 EMPIRICAL PROOF (CompileProject probe)

Copy of `testdata/diff/project` + `src/shared/scratchmod.ts` + `src/26_paths.ts`
(`import { sharedFn, SHARED_VALUE } from "shared/scratchmod";`) + tsconfig gaining
`"paths": { "*": ["./src/*"] }` (no baseUrl). `CompileProject` → zero diagnostics, and:

```lua
local _scratchmod = TS.import(script, script.Parent, "shared", "scratchmod")
```

byte-shape-identical to rbxtsc's emission for baseUrl projects. Fallthrough proof: a scratch
`node_modules/@rbxts/scratchpkg` (package.json main/types + init.lua + index.d.ts) imported as
`"@rbxts/scratchpkg"` with the wildcard ACTIVE still resolves and emits
`TS.import(script, script.Parent, "node_modules", "@rbxts", "scratchpkg").scratch` — the
wildcard does not shadow node_modules packages.

### 1.5 Sanitizer rewrite spec (sanitize.go SanitizeTSConfig)

When deleting `baseUrl` with value `B` (string; read BEFORE `delete(co, "baseUrl")`):

1. Normalize `B` to a `./`-anchored slash path with trailing `/*`:
   `wild := "./" + strings.TrimSuffix(strings.TrimPrefix(filepath.ToSlash(B), "./"), "/") + "/*"`
   (`B == "."` → `./*`). baseUrl is relative to the config file; the injected substitution is
   resolved against `PathsBasePath` = the same config's dir, so no path math is needed.
2. If `co["paths"]` absent → `co["paths"] = map[string]any{"*": []any{wild}}`.
3. If `co["paths"]` present (rare in rbxts projects), reproduce TS5's
   paths-relative-to-baseUrl + baseUrl-fallback semantics:
   - every RELATIVE substitution `s` in every pattern is rewritten to `./B/s` (TS5 resolved
     substitutions against baseUrl; tsgo resolves against the config dir);
   - append `"./B/<pattern>"` (pattern text with its `*` kept) to each pattern's substitution
     list — this restores TS5's "specific pattern failed → baseUrl lookup of the FULL name"
     fallback, because tsgo's `MatchPatternOrExact` picks only the single longest-prefix
     pattern and never retries `"*"`;
   - add the `"*"` entry per (2) if absent, else append `wild` to it.
4. Order within a substitution list is significant (tried in order) — user entries first,
   injected baseUrl fallbacks last, matching TS5's paths-before-baseUrl priority.

Known sanitizer gap (pre-existing, now load-bearing): `isTSConfigPath` (sanitize.go:30-32)
only intercepts files NAMED `tsconfig.json`; a `"extends": "./tsconfig.base.json"` carrying
baseUrl is not sanitized → tsgo still hard-errors on the removed option. Acceptable for 3b
(randomness keeps baseUrl in the root tsconfig.json); note in code.

### 1.6 Flow into import emission — rbxPath UNAFFECTED (confirmed)

`getImportParts` (importexpr.go:229-268) starts from
`getSourceFileFromModuleSpecifier` → CHECKER: `GetSymbolAtLocation(moduleSpecifier)` /
`ResolveExternalModuleName` → the resolved module's SourceFile. How the resolver FOUND that
file (paths vs old baseUrl) is invisible here: the same `moduleFile.FileName()` feeds
`PathTranslator.GetImportPath` → `RojoResolver.GetRbxPathFromFilePath`, producing the same
rbxPath and the same `TS.import(...)` argument list. Verified by the §1.4 probe output.

---

## 2. pnpm symlinks — guessVirtualPath

### 2.1 Symptom

tsgo realpaths node_modules resolutions: `createResolvedModuleHandlingSymlink`
(tsgo/module/resolver.go:1153-1166) — for an `isExternalLibraryImport` (resolved path contains
`/node_modules/`) of a non-relative name with `PreserveSymlinks != true`, it swaps
`resolved.path` for `realPath(path)` and stores the symlink-side path in
`resolved.originalPath` (`getOriginalAndResolvedFileName`, resolver.go:1207-1219). Under pnpm,
`node_modules/@rbxts/foo` is a symlink into `node_modules/.pnpm/@rbxts+foo@.../node_modules/
@rbxts/foo`, so the program's SourceFile.FileName() is the `.pnpm` realpath. In
`getNodeModulesImportParts` (importexpr.go:131-146) `moduleScope :=
path.relative(nodeModulesPath, moduleOutPath)[0]` then yields `".pnpm"` → no `"@"` prefix →
`DiagNoUnscopedModule` ("You cannot use modules directly under node_modules") — the 30
randomness failures. (A pnpm store OUTSIDE the project hits `validateModule` →
`noInvalidModule` instead.)

### 2.2 Upstream algorithm — TransformState.ts:365-384 (verbatim)

```ts
/** attempts to reverse symlink lookup */
public guessVirtualPath(fsPath: string) {
    const reverseSymlinkMap = this.program.getSymlinkCache?.().getSymlinkedDirectoriesByRealpath();
    if (!reverseSymlinkMap) return;
    const original = fsPath;
    while (true) {
        // reverseSymlinkMap always has trailing slashes
        // as it is constructed from `SymlinkedDirectory.real`
        const parent = ts.ensureTrailingDirectorySeparator(path.dirname(fsPath));
        if (fsPath === parent) break;
        fsPath = parent;
        const symlink = reverseSymlinkMap.get(
            ts.toPath(fsPath, this.program.getCurrentDirectory(), getCanonicalFileName),
        )?.[0];
        if (symlink) {
            return path.join(symlink, path.relative(fsPath, original));
        }
    }
}
```

Walk the realpath's ancestor directories from innermost out; the first ancestor present in
the realpath→symlink reverse map rebases the file onto the symlink side. Consumption —
`createImportExpression.ts:191`:
`const virtualPath = state.guessVirtualPath(moduleFile.fileName) || moduleFile.fileName;`
then `isInsideNodeModules(virtualPath)` / `nodeModulesPathMapping.get(canonical(virtualPath))`
/ `pathTranslator.getImportPath(virtualPath)` all operate on the VIRTUAL path. Rotor's
insertion point is exactly the `TODO(phase-3 symlinks)` at importexpr.go:244-249.

### 2.3 What tsgo offers

`Program.GetSymlinkCache()` (tsgo/compiler/program.go:1991-2038), memoized once per program:

- builds `symlinks.NewKnownSymlink(cwd, useCaseSensitiveFileNames)` and seeds it via
  `SetSymlinksFromResolutions(p.ForEachResolvedModule, p.ForEachResolvedTypeReferenceDirective)`
  (tsgo/symlinks/knownsymlinks.go:81-92): every resolution with a non-empty `OriginalPath`
  runs `ProcessResolution(originalPath, resolvedFileName)` (knownsymlinks.go:93-112) —
  `SetFile` plus `guessDirectorySymlink` (knownsymlinks.go:114-130: strip equal trailing
  components from both paths, stopping at `node_modules`/`@scope` components) → `SetDirectory`.
- additionally scans emitted files' package.json runtime dependencies and
  `ResolvePackageDirectory`s them (program.go:2000-2036) — strada parity.
- the strada `getSymlinkedDirectoriesByRealpath()` equivalent is
  **`DirectoriesByRealpath()`** (knownsymlinks.go:41-43):
  `*collections.SyncMap[tspath.Path, *collections.SyncSet[string]]`. Keys are
  `KnownDirectoryLink.RealPath` values — canonical, **always trailing-separator**
  (`EnsureTrailingDirectorySeparator`, knownsymlinks.go:55-63), matching the upstream comment.
  Values are the symlink-side ORIGINAL-CASING paths (`commonOriginal`, no canonicalization).

DIVERGENCE: strada's map value is an insertion-ordered array (upstream takes `[0]`); tsgo's
`SyncSet` iterates in NONDETERMINISTIC order. In practice each pnpm realpath dir has exactly
one symlink; for the >1 case the Go port must pick deterministically — spec: collect and take
the lexicographic minimum, comment the divergence.

### 2.4 Go port spec

`State.GuessVirtualPath(fsPath string) string` (transformer; "" = no remap), called from
importexpr.go in place of the TODO:

```go
virtualPath := moduleFile.FileName()
if guessed := s.GuessVirtualPath(virtualPath); guessed != "" { virtualPath = guessed }
```

Implementation (mirrors §2.2 1:1; s.Program may be nil in unit tests → return ""):

1. `cache := s.Program.GetSymlinkCache()`; `byReal := cache.DirectoriesByRealpath()`.
2. `original := fsPath` (slash-separated; SourceFile names already are).
3. Loop: `parent := tspath.EnsureTrailingDirectorySeparator(tspath.GetDirectoryPath(fsPath))`;
   `if fsPath == parent { break }`; `fsPath = parent`;
   `key := tspath.ToPath(fsPath, s.Program.GetCurrentDirectory(), useCaseSensitive)` —
   CHECKER-ADJACENT: use the program host's case sensitivity, same canonicalization the cache
   used. NOTE `ToPath` of a trailing-separator string keeps the separator, matching the
   cache's `EnsureTrailingDirectorySeparator` keys.
4. `if set, ok := byReal.Load(key); ok` → pick deterministic element `symlink` (§2.3) →
   `return path-join(symlink, relative(fsPath, original))` (slash join; `original` minus the
   `fsPath` prefix appended to `symlink`).
5. Loop exhausts → return "".

Interplay already correct elsewhere: `createNodeModulesPathMapping`
(internal/compile/project.go:231-270) reads typeRoot dir entries through the SYMLINK side
(os.ReadDir of `node_modules/@rbxts` + os.ReadFile follows links), so its canonical keys are
virtual-path-keyed — exactly what the post-guess `nodeModulesPathMapping.get(...)` lookup
needs. `validateModule`/`moduleScope` (importexpr.go:108-146) also become correct since
`moduleOutPath` now derives from the virtual path.

Watch mode (Phase 4): strada invalidates the symlink cache per program; tsgo memoizes per
`*Program` — fine while rotor builds one Program per pass.

---

## 3. Iterable-arity prerequisite (tsgo checker vs @rbxts/compiler-types)

### 3.1 Root cause (exact mechanics)

- tsgo resolves the iteration globals at **arity 3** — checker.go:1081-1087:
  `getGlobalIteratorType/getGlobalIterableType/getGlobalIterableTypeChecked/
  getGlobalIterableIteratorType(+Checked)/getGlobalIteratorObjectType/getGlobalGeneratorType`
  all `getGlobalTypeResolver(name, 3, ...)` (TS5.6+ "strict builtin iterator types" lib shape
  `Iterable<T, TReturn = any, TNext = any>`). rbxtsc pins TS 5.5.3 (fixture package.json)
  where the checker resolved arity **1** — why upstream never hit this.
- `getGlobalType` (checker.go:1204-1224): symbol found but
  `len(t.AsInterfaceType().TypeParameters()) != arity` → returns **`c.emptyGenericType`**
  (errors reported only by the `*Checked` reportErrors variants).
- @rbxts/compiler-types `types/Iterable.d.ts` declares: `Iterator<Yields, Returns = void,
  Next = undefined>` (arity 3 ✓), `Generator<Yields = unknown, Returns = void, Next = unknown>`
  (3 ✓), `AsyncIterator`/`AsyncGenerator` (3 ✓), but **`Iterable<T>`, `IterableIterator<T>`,
  `AsyncIterable<T>`, `AsyncIterableIterator<T>` at arity 1** → those four resolve to
  emptyGenericType. (`IteratorObject` is not declared at all — non-reporting resolver, fine;
  TS5 had none either.)
- THE gate — `getIteratedTypeOrElementType`, checker.go:6088:
  `iterableExists := c.getGlobalIterableType() != c.emptyGenericType` is **false**, so the
  entire iteration-protocol branch (6090-6120) is skipped for for-of/spread/destructuring/
  yield*, falling to the array-like fallback; `Set<T>`/`Map<K,V>`/`Generator<...>` are not
  array-like → error at 6159-6162 via `getIterationDiagnosticDetails` (checker.go:6688-6701):
  the structural probe `getIterationTypeOfIterable(use, Yield, inputType, nil)` SUCCEEDS
  (compiler-types' `[Symbol.iterator]` slow path works — checker.go:6434-6453), so branch 1
  picks `Type_0_can_only_be_iterated_through_when_using_the_downlevelIteration_flag_or_with_a_
  target_of_es2015_or_higher` — the exact message tolerated in forof_test.go:28-46. And the
  sanitizer MUST strip `downlevelIteration` (program.go:844 removed-option error), so users
  cannot work around it.
- Secondary damage sites if only the diagnostic were suppressed:
  - checker.go:6092-6120: `iterationTypes.yieldType` never reached → for-of/spread element
    types degrade to `anyType` (`checkIteratedTypeOrElementType`, 6069-6078) → downstream
    type-dependent emit (e.g. rotor's loop-builder classification, addOneIfArrayType) sees
    wrong types;
  - checker.go:6449 (`getIterationTypesOfIterableSlow`): error path
    `checkTypeAssignableToEx(t, r.getGlobalIterableTypeChecked(), ...)` → spurious
    "Global type 'Iterable' must have 3 type parameter(s)" on the compiler-types declaration;
  - createGeneratorType (checker.go:20295-20311): OK today (Generator is arity 3) but its
    IterableIterator fallback is dead;
  - createIterableType (checker.go:24564-24566; caller 17836 = empty/rest-only array binding
    patterns): `createTypeFromGenericGlobalType(emptyGenericType, ...)` degrades to
    emptyObjectType.

### 3.2 Options evaluated

- **(a) checker shim via tools/mirror overlay — REJECTED.** The overlay system
  (tools/mirror/main.go + tools/mirror/overlay/) is purely ADDITIVE: it drops new files (e.g.
  `tsgo/checker/rotor_exports.go` from `overlay/checker/rotor_exports.go.tmpl`) next to the
  mirrored sources. Relaxing the arity check means EDITING `NewChecker`'s resolver
  registrations (checker.go:1081-1087) or `getGlobalType` — a patch to vendored code that
  re-mirroring (pinned 1f955e97 today, future bumps planned) would silently drop or conflict.
  Worse, accepting an arity-1 global while `createTypeFromGenericGlobalType` instantiates it
  with 3 type arguments is an instantiation-arity hazard deep in checker internals.
- **(c) suppress the diagnostic — REJECTED.** Leaves every §3.1 secondary site broken: the
  gate still short-circuits, element types become `any` (which ALSO trips rotor's
  `validateNotAnyType`/noAny machinery differently than rbxtsc), 6449's arity error still
  fires, and filtering one diagnostic code from `preEmitDiagnostics` is a per-message hack.
- **(b) compiler-types d.ts overlay via the SanitizeFS-style vfs layer — RECOMMENDED,
  EMPIRICALLY VALIDATED.** Rewrite the four arity-1 interfaces to the TS5.6 lib shape with
  permissive defaults. Pure data fix: no mirror drift, no checker fork, applies to every
  project compiled through rotor, and `.d.ts` text has zero emit impact.

### 3.3 Validation

Probe project (§1.4 copy) + `src/27_iter.ts` (for-of over `Set<string>`, destructured for-of
over `Map<string, number>`, for-of over a generator call). Before: 3 diagnostics
(`Type 'Set<string>' can only be iterated through ...`, same for `Map<string, number>` and
`Generator<1 | 2, void, unknown>`). After rewriting the temp copy's Iterable.d.ts per §3.4:
**all three checker diagnostics gone**; the compile proceeds into the transformer, which
correctly reports its own Phase-3 NYS items (for-of over Set/Map/generators) — proving the
checker now computes real iteration types and no new diagnostics appear on the other 27
fixture files.

### 3.4 Implementation spec

Extend `SanitizeFS` (sanitize.go) — same wrapvfs.Wrap ReadFile interception:

- Match: slash-path contains `/node_modules/@rbxts/compiler-types/` and ends `.d.ts` (don't
  hardcode `types/Iterable.d.ts`; declarations may move across compiler-types versions).
- Transform (textual, idempotent, anchored to `interface <Name><T>` with a single type
  parameter named exactly `T` — current shape across compiler-types 2.x/3.x):
  - `interface Iterable<T>` → `interface Iterable<T, TReturn = any, TNext = any>`
  - `interface IterableIterator<T>` → `interface IterableIterator<T, TReturn = any, TNext = any>`
  - `interface AsyncIterable<T>` → `interface AsyncIterable<T, TReturn = any, TNext = any>`
  - `interface AsyncIterableIterator<T>` → `interface AsyncIterableIterator<T, TReturn = any, TNext = any>`
  (regex `\binterface (Iterable|IterableIterator|AsyncIterable|AsyncIterableIterator)<T>` →
  `interface $1<T, TReturn = any, TNext = any>`; leave bodies, `extends` clauses, and every
  REFERENCE `Iterable<T>` untouched — defaulted parameters keep 1-arg references legal, and
  `getGlobalType`'s arity check counts ALL type parameters including defaulted ones, so the
  rewritten interfaces resolve at arity 3.)
- Why defaults `= any`: maximally permissive — `TReturn`/`TNext` only feed assignability
  checks (`Cannot iterate value because the next method ... expects type ...`,
  checker.go:6092-6110) that rbxtsc/TS5.5 never performed; `any` keeps them vacuous, so no
  NEW diagnostics on code rbxtsc accepts. Do NOT copy the lib's strict
  `BuiltinIteratorReturn` shape.
- Declaration merging is NOT an alternative: merged interface declarations must repeat
  identical type parameter lists — an augmenting `interface Iterable<T, TReturn, TNext>`
  alongside the original arity-1 declaration is itself an error. In-place rewrite only.
- Tests: a sanitize_test asserting the rewrite (and idempotence); a forof_test flip — the
  `buildStateTolerating` workaround (forof_test.go:40-47) becomes a strict no-diagnostic
  expectation once the vfs used by buildState applies the overlay; a CompileProject fixture
  with Set/Map/generator iteration once the Phase-3 loop builders land.
- This is a PREREQUISITE for: for-of over Set/Map/string/generator/IterableFunction/$range's
  `Iterable<number>`, array spread of iterables (`getAddIterableToArrayBuilder`), array
  destructuring of LuaTuple-less iterables, and `yield*`.

---

## 4. PROPERTY_CALL_MACROS + CALL_MACROS + IDENTIFIER_MACROS — full inventory

Files: `macros/propertyCallMacros.ts` (1001 L), `macros/callMacros.ts` (61 L),
`macros/identifierMacros.ts` (5 L), `macros/types.ts` (23 L), `classes/MacroManager.ts`
(202 L). Signatures (types.ts): `PropertyCallMacro(state, node /*CallExpression whose
.expression is PropertyAccess|ElementAccess*/, expression /*transformed base*/, args
/*transformed call args*/) → luau.Expression`; CallMacro identical minus the node refinement.
Rotor types exist (macromanager.go:50-78, entry structs carry Name for diagnostics).

Dispatch (already ported — call.go:308-318/351-361/417-427): CHECKER:
`GetNonOptionalType(GetType(call.Expression))` → `GetFirstDefinedSymbol` →
`Macros().GetCallMacro` / `GetPropertyCallMacro` → `runCallMacro(entry.Macro, s, node,
expression|baseExpression, nodeArguments)` (call.go:188-265 — P2 §8.4; handles tuple-spread
expansion, arg/base pushToVar-if-might-mutate, wrapReturnIfLuaTuple). Porting a macro =
replace the table's nil with the function; the `rotorNotYetSupported` branch dies naturally.

Registration (MacroManager.ts:119-144, ported macromanager.go:213-241): per class name →
CHECKER: `resolveName(className, undefined, SymbolFlags.Interface, false)` → for EVERY
interface declaration of the symbol, for every `MethodSignature` member with identifier name:
key = CHECKER: `getTypeAtLocation(skipUpwards(member)).symbol` (the method's function-type
symbol — exactly what `GetFirstDefinedSymbol` yields at call sites). Missing method →
ProjectError (§6).

### 4.1 wrapComments — propertyCallMacros.ts:941-1001 (applies to EVERY property-call macro)

Every table entry is wrapped at module init
(`macroList[methodName] = wrapComments("ClassName.methodName", macro)`, L997-1001). Behavior:

```ts
function wrapComments(methodName: string, callback: PropertyCallMacro): PropertyCallMacro {
    return (state, callNode, callExp, args) => {
        const [expression, prereqs] = state.capture(() => callback(state, callNode, callExp, args));
        let size = luau.list.size(prereqs);
        if (size > 0) {
            // detect the case of `expression = state.pushToVarIfComplex(expression, "exp");` and put header after
            const wasPushed = wasExpressionPushed(prereqs, callExp);
            let pushStatement: luau.Statement | undefined;
            if (wasPushed) { pushStatement = luau.list.shift(prereqs); size--; }
            if (size > 1) {
                luau.list.unshift(prereqs, header(methodName));                 // `-- ▼ X ▼`
                if (wasPushed && pushStatement) luau.list.unshift(prereqs, pushStatement);
                luau.list.push(prereqs, footer(methodName));                    // `-- ▲ X ▲`
            } else if (wasPushed && pushStatement) {
                luau.list.unshift(prereqs, pushStatement);
            }
        }
        state.prereqList(prereqs);
        return expression;
    };
}
```

- `header/footer` = `luau.comment(" ▼ ReadonlyArray.reduce ▼")` etc. (L943-949) — rendered
  `-- ▼ ReadonlyArray.reduce ▼` (rotor: `luau.NewComment`).
- `wasExpressionPushed` (L951-963): first prereq is a VariableDeclaration whose `left` is a
  single TemporaryIdentifier and whose `right` is **pointer-identical** to the INCOMING
  `callExp` — i.e. the macro began with `pushToVarIfComplex(expression, "exp")` on a complex
  base. That `local _exp = <base>` line stays ABOVE the header comment.
- Comments appear only when the macro produced **≥2 prereq statements** after excluding the
  base push (`size > 1`); single-statement macros (e.g. most Array methods on simple bases)
  emit no markers. Byte parity depends on this exactly.
- Go port: wrap when building `propertyCallMacroTable` values (or in NewMacroManager when
  installing entries). Rotor's existing math macros are pure-expression (no prereqs) — wrap
  them too; zero-prereq invocations are unaffected, keeping current goldens byte-identical.
- CALL_MACROS / constructor / identifier macros are NOT comment-wrapped.

### 4.2 Shared helpers in macro bodies

- `state.pushToVarIfComplex(exp, "exp")` / `pushToVar(exp, hint)` / `pushToVarIfNonId(args[0],
  "callback")` → rotor `PushToVarIfComplex/PushToVar/PushToVarIfNonID` (state.go:300-332).
- `luau.tempId("k"|"v"|"i"|"result")` → `luau.TempID`. `offset(exp, ±1)` → rotor `offsetExpr`
  (access.go:77; folds literals and `x ± n` right operands — P2). `valueToIdStr` ✓,
  `isUsedAsStatement(node)` ✓ (conditional.go:16), `convertToIndexableExpression` ✓.
- `luau.globals.string.X` / `table.X` / `next` / `tostring` / `error` / `assert` / `typeof` /
  `type` → `luau.NewPropertyAccess(luau.GlobalID("string"), "byte")` etc. / `luau.GlobalID`.
- `luau.strings[", "]` = preallocated `luau.string(", ")` (reference/luau-ast strings.ts:20,
  "used for ReadonlyArray.join()") → just `luau.Str(", ")`.
- **`argumentsWithDefaults(state, args, defaults)`** (L126-157) — currently used ONLY by
  `ReadonlyArray.join`:
  - for each PROVIDED arg that is not `luau.isSimplePrimitive`: push to var (hint
    `valueToIdStr(arg) || "arg<i>"`), then prereq `if argN == nil then argN = defaults[i] end`;
  - for each MISSING trailing arg: `args[j] = defaults[j]` literally.
- Loop emission inventory (which macros build loops — these are plain luau AST loops, NOT the
  TS for-of transform): **generic `for k, v in exp do`** (`luau.SyntaxKind.ForStatement`,
  rotor `luau.NewFor`): every/some, forEach (all 3 variants), map, mapFiltered, filter, find,
  findIndex, join's tostring pre-pass, filterUndefined pass 1 (ids = `_i` only),
  Set/Map shared `size` (ids = one valueless tempId). **numeric `for i = a, b[, s] do`**
  (`NumericForStatement`, rotor `luau.NewNumericFor`): filterUndefined pass 2, reduce.

### 4.3 Math classes (PORTED — propertycallmacros.go)

`makeMathMethod(op)`: `luau.binary(expression, op, rhs)`, rhs parenthesized unless
`luau.isSimple`. Rows: CFrame `+ - *`; UDim/UDim2 `+ -`; Vector2/Vector3 `+ - * / //`;
Vector2int16/Vector3int16 `+ - * /`; Number `//` → names via OPERATOR_TO_NAME_MAP
(add/sub/mul/div/idiv). NOTE these are declared by @rbxts/types macro_math.d.ts (merged
declarations) — the audit (§6) must therefore distinguish the two packages.

### 4.4 String — STRING_CALLBACKS (13 entries, L46-61)

| method | emit |
|---|---|
| size | `#expression` (`luau.unary("#", expression)`) |
| byte, find, format, gmatch, gsub, lower, match, rep, reverse, split, sub, upper | `makeStringCallback(luau.globals.string.X)` = `string.X(expression, ...args)` — plain call, expression FIRST, args appended verbatim |

No prereqs, no wrapComments output ever (size 0). NOTE upstream relies on the checker having
already added +1 offsets etc. at the ARGUMENT level only where the d.ts says so — the macro
itself does no offsetting (Luau string API is 1-based and the rbxts String d.ts exposes the
raw API).

### 4.5 ArrayLike — ARRAY_LIKE_METHODS (1)

`size: #expression`. (Registered for the `ArrayLike` interface — covers
ReadonlyArray/Array size through interface inheritance of the symbol tables; String.size and
Set/Map size are separate entries.)

### 4.6 ReadonlyArray — READONLY_ARRAY_METHODS (15, L163-585)

- **isEmpty** — `#expression == 0`.
- **join(separator?)** — `args = argumentsWithDefaults(state, args, [luau.strings[", "]])`.
  CHECKER: `indexType = typeChecker.getIndexTypeOfType(state.getType(node.expression.expression),
  ts.IndexKind.Number)` (the ELEMENT type of the array — note `node.expression.expression`,
  the base, not the call); if `indexType && !isDefinitelyType(indexType, isStringType,
  isNumberType)`: base → `pushToVarIfComplex(exp,"exp")`, `local _result =
  table.create(#_exp)` (pushToVar hint "result"), generic-for `_result[_k] = tostring(_v)`,
  then concat the COPY. Always returns `table.concat(expression, args[0])`. tsgo CHECKER
  equivalent: `chk.GetIndexTypeOfType(t, chk.NumberType())`-style lookup — rotor already
  exposes index-info queries used by addOneIfArrayType; reuse `IsDefinitelyType` +
  `IsStringType/IsNumberType` (types.go).
- **move(sourceStart, sourceEnd, destination, target?)** — `table.move(expression,
  offset(args[0],1), offset(args[1],1), offset(args[2],1)[, args[3]])` (args[3] NOT offset —
  it's the target array).
- **includes(value, fromIndex?)** — `table.find(expression, args[0][, offset(args[1],1)]) ~= nil`.
- **indexOf(value, fromIndex?)** — `offset((table.find(findArgs) or 0), -1)`; note the `or 0`
  BinaryExpression is built raw so the offset folds onto the `0` → renders
  `(table.find(exp, v) or 0) - 1`.
- **every(cb)** / **some(cb)** — `makeEveryOrSomeMethod(argsMaker, initialState)` (L63-124):
  base → pushToVarIfComplex "exp"; `local _result = true|false` (pushToVar bool initialState,
  hint "result"); cb → pushToVarIfNonId "callback"; generic-for `_k, _v in exp`:
  `if not cb(v, k-1, exp) then` (every) / `if cb(v, k-1, exp) then` (some) → `_result =
  false|true; break`. argsMaker = `[valueId, offset(keyId,-1), expression]`. Returns _result.
- **forEach(cb)** — base pushToVarIfComplex "exp"; cb pushToVarIfNonId "callback";
  generic-for `_k, _v`: CallStatement `cb(v, k-1, exp)`. Returns
  `!isUsedAsStatement(node) ? luau.nil() : luau.none()` — i.e. `nil` when the value is
  consumed, NOTHING when the call is a statement.
- **map(cb)** — base push "exp"; `local _newValue = table.create(#exp)` (hint "newValue");
  cb pushToVarIfNonId; generic-for: `_newValue[_k] = cb(v, k-1, exp)`. Returns _newValue.
- **mapFiltered(cb)** (L289-331) — verbatim shape:

  ```lua
  local _newValue = {}            -- pushToVar(luau.array(), "newValue")
  local _callback = ...           -- pushToVarIfNonId(args[0], "callback")
  local _length = 0               -- pushToVar(number 0, "length")
  for _k, _v in _exp do
      local _result = _callback(_v, _k - 1, _exp)
      if _result ~= nil then
          _length += 1
          _newValue[_length] = _result
      end
  end
  ```

  returns _newValue. (Declaration order: newValue, callback, length — byte-relevant.)
- **filterUndefined()** (L333-401) — two passes:

  ```lua
  local _length = 0                       -- pushToVar(0, "length")
  for _i in _exp do                        -- generic for, ONE id
      if _i > _length then _length = _i end
  end
  local _result = {}                      -- pushToVar(array, "result")
  local _resultLength = 0                 -- pushToVar(0, "resultLength")
  for _i = 1, _length do                  -- numeric for, no step
      local _v = _exp[_i]                 -- ComputedIndex(convertToIndexableExpression(exp))
      if _v ~= nil then
          _resultLength += 1
          _result[_resultLength] = _v
      end
  end
  ```

  returns _result. Base pushToVarIfComplex FIRST (header-exempt push per §4.1).
- **filter(cb)** (L403-444) — like mapFiltered but condition `cb(v, k-1, exp) == true`
  (explicit `== luau.bool(true)` comparison!) and assigns `_newValue[_length] = _v`.
  Declaration order: newValue, callback, length.
- **reduce(cb, initialValue?)** (L446-512) — verbatim TS:

  ```ts
  expression = state.pushToVarIfComplex(expression, "exp");
  let start: luau.Expression = luau.number(1);
  const end = luau.unary("#", expression);
  const step = 1;
  const lengthExp = luau.unary("#", expression);
  let resultId;
  if (args.length < 2) {     // no initialValue
      state.prereq(/* if #exp == 0 then error("Attempted to call `ReadonlyArray.reduce()` on an empty array without an initialValue.") end */);
      resultId = state.pushToVar(/* exp[1] (ComputedIndex over convertToIndexableExpression) */, "result");
      start = offset(start, step);                       // 2
  } else {
      resultId = state.pushToVar(args[1], "result");
  }
  const callbackId = state.pushToVar(args[0], "callback");   // pushToVar, NOT IfNonId!
  // numeric for: for _i = start, #exp do  _result = _callback(_result, exp[_i], _i - 1, exp) end
  ```

  returns _result. Note: callback pushed AFTER result (byte order), with unconditional
  pushToVar; `step === 1` → step omitted in the NumericForStatement.
- **find(cb)** (L514-548) — base push "exp"; cb pushToVarIfNonId; `local _result`
  (pushToVar(undefined, "result") — VALUELESS declaration, rotor PushToVar(nil, "result"));
  generic-for `_i, _v`: `if cb(v, i-1, exp) == true then _result = _v; break end`. Returns
  _result.
- **findIndex(cb)** (L550-584) — same but `local _result = -1` and assignment
  `_result = _i - 1` (offset(loopId,-1)).

### 4.7 Array — ARRAY_METHODS (9, L587-734)

- **push(...items)** — `args.length === 0` → return `#expression` ("always emit luau.unary so
  the call doesn't disappear in emit" — even as a statement). Else base pushToVarIfComplex
  "exp"; per arg a CallStatement prereq `table.insert(exp, arg)`; returns
  `!isUsedAsStatement ? #exp : none`.
- **pop()** — base pushToVarIfComplex "exp"; `returnValueIsUsed = !isUsedAsStatement(node)`;
  if used: `local _length = #exp` (pushToVar hint "length"), `local _result = _exp[_length]`
  (hint "result"); prereq `exp[lengthExp] = nil` (lengthExp is the temp when used, else the
  raw `#exp` unary); return _result or none. Statement case emits the one-liner
  `exp[#exp] = nil`.
- **shift()** — `table.remove(expression, 1)` (plain expression; works as statement too).
- **unshift(...items)** — base pushToVarIfComplex "exp"; iterate args in REVERSE:
  CallStatement `table.insert(exp, 1, arg)`; returns `!isUsedAsStatement ? #exp : none`.
- **insert(index, value)** — `table.insert(expression, offset(args[0],1), args[1])`.
- **remove(index)** — `table.remove(expression, offset(args[0],1))`.
- **unorderedRemove(index)** (L662-707) — `local _index = <args[0]+1>` via
  pushToVarIfComplex(offset(args[0],1), "index") — NOTE: index temp BEFORE the base temp
  (upstream order); base pushToVarIfComplex "exp"; `local _length = #exp` (pushToVar,
  "length"); `valueIsUsed = !isUsedAsStatement`; `local _value = exp[_index]` (pushToVar,
  "value" — created UNCONDITIONALLY, even when unused!); prereq:

  ```lua
  if _value ~= nil then
      _exp[_index] = _exp[_length]
      _exp[_length] = nil
  end
  ```

  returns _value or none.
- **sort(compareFn?)** — `valueIsUsed = !isUsedAsStatement`; if used base→pushToVarIfComplex
  "exp"; `args.unshift(expression)`; CallStatement prereq `table.sort(exp[, cb])`; returns
  expression or none.
- **clear()** — CallStatement prereq `table.clear(expression)`; returns
  `!isUsedAsStatement ? luau.nil() : none`.

### 4.8 Set/Map — shared + specific (L736-908)

READONLY_SET_MAP_SHARED (spread into both readonly tables):

- **isEmpty** — `next(expression) == nil`.
- **size** — `local _size = 0` (pushToVar, "size"); generic-for with ONE valueless tempId
  (`ids: [luau.tempId()]` — renders `for _ in exp do`): `_size += 1`. Returns _size.
- **has(key)** — `expression[args[0]] ~= nil` (ComputedIndex over
  convertToIndexableExpression).

SET_MAP_SHARED (spread into both mutable tables):

- **delete(key)** (L767-798) — `local _value = args[0]` via pushToVarIfComplex(args[0],
  "value"); `valueIsUsed = !isUsedAsStatement`; if used: base → **pushToVarIfNonId** (exp,
  "exp") (NonId, not IfComplex!), `local _valueExisted = exp[_value] ~= nil` (pushToVar,
  "valueExisted"); prereq `exp[_value] = nil`; return _valueExisted or none.
- **clear()** — identical to Array.clear (`table.clear` CallStatement; nil/none).

READONLY_SET_METHODS = shared + **forEach(cb)**: base pushToVarIfComplex "exp"; cb
pushToVarIfNonId "callback"; generic-for ONE id `_v`: CallStatement `cb(_v, _v, exp)` (value
passed TWICE — JS Set.forEach signature); returns nil/none.

SET_METHODS = mutable-shared + **add(value)**: `valueIsUsed = !isUsedAsStatement`; if used
base→pushToVarIfComplex "exp"; prereq `exp[args[0]] = true`; returns expression (the SET, for
chaining) or none.

READONLY_MAP_METHODS = shared + **forEach(cb)**: generic-for `_k, _v`: CallStatement
`cb(_v, _k, exp)` (VALUE first — JS Map.forEach order); returns nil/none. + **get(key)**:
`expression[args[0]]` (plain ComputedIndex, no prereqs).

MAP_METHODS = mutable-shared + **set(key, value)**: like Set.add with
`exp[keyExp] = valueExp`; returns expression or none.

(WeakSet/WeakMap have no extra methods — they reuse Set/Map entries through the same
interfaces; only CONSTRUCTORS differ, already ported.)

### 4.9 Promise — PROMISE_METHODS (1)

**then(...)** — `luau.create(MethodCallExpression, { expression: convertToIndexable(exp),
name: "andThen", args })` → `exp:andThen(...)`. No prereqs. (Everything else on Promise is a
REAL method on the runtime Promise class.)

### 4.10 Table rows (PROPERTY_CALL_MACROS, L919-939)

`CFrame UDim UDim2 Vector2 Vector2int16 Vector3 Vector3int16 Number` (math, ported) +
`String→STRING_CALLBACKS, ArrayLike→ARRAY_LIKE, ReadonlyArray, Array, ReadonlySet, Set,
ReadonlyMap, Map, Promise`. Counts: String 13, ArrayLike 1, ReadonlyArray 15, Array 9,
ReadonlySet 4 (isEmpty/size/has/forEach), Set 7 (4 + delete/clear/add — spreads make
ReadonlySet's and SET_MAP_SHARED's entries distinct registrations on the Set interface's OWN
method symbols), ReadonlyMap 5, Map 8, Promise 1.

### 4.11 CALL_MACROS (callMacros.ts:22-61, 8 entries)

All reached via `transformCallExpressionInner`'s GetCallMacro hook (call.go:310). Registered
by name with CHECKER: `resolveName(name, undefined, SymbolFlags.Function, false)`.

- **assert(value, message?)** — `args[0] = createTruthinessChecks(state, args[0],
  node.arguments[0])` (rotor `CreateTruthinessChecks`, truthiness.go:31 — NOTE upstream here
  passes the TS node and lets getType derive the type; rotor's signature takes the type —
  pass `s.GetType(node.Arguments()[0])`), then `luau.call(luau.globals.assert, args)`.
  The truthiness wrap means `assert(x)` on `number?` emits `assert(x ~= 0 and x == x and
  x ~= nil)` etc. — full 0/NaN/""/nil expansion.
- **typeOf(...)** — `typeof(args...)`.
- **typeIs(value, typeStr)** — `typeFunc = (isStringLiteral(typeStr) &&
  PRIMITIVE_LUAU_TYPES.has(value)) ? luau.globals.type : luau.globals.typeof`;
  `typeFunc(value) == typeStr`. PRIMITIVE_LUAU_TYPES = {nil, boolean, string, number, table,
  userdata, function, thread, vector, buffer} (L9-20).
- **classIs(instance, className)** — `convertToIndexable(value).ClassName == typeStr`.
- **identity(v)** — `args[0]` verbatim.
- **$range(...)** — outside for-of: `DiagnosticService.addDiagnostic(
  errors.noRangeMacroOutsideForOf(node.expression))` + `luau.none()`. (For-of intercepts
  $range BEFORE expression transform — rotor forof.go:126-165 already does; the CALL macro is
  the error path only. Symbol identity: the for-of check is
  `state.services.macroManager.getCallMacro(symbol) === CALL_MACROS.$range` upstream —
  rotor compares `entry.Name == "$range"`.)
- **$tuple(...)** — outside return: `errors.noTupleMacroOutsideReturn(node)` + none. (Return
  statements intercept $tuple — P2b functions digest.)
- **$getModuleTree(specifier)** — `const parts = getImportParts(state, node.getSourceFile(),
  node.arguments[0]); return luau.array([parts.shift()!, luau.array(parts)])` → emits
  `{ root, { "rest", "of", "path" } }`. Reuses importexpr.go `getImportParts` directly. NOTE
  prerequisite: `getSourceFileFromModuleSpecifier`'s fallback (its L25-32:
  `ts.resolveModuleName(specifier.text, sourceFile.path, compilerOptions, ts.sys)` when the
  checker has no symbol — modules never imported normally) is still a TODO in rotor
  (importexpr.go:43-47); tsgo equivalent `s.Program.ResolveModuleName(text,
  sourceFile.FileName(), mode)` (tsgo/compiler/program.go:2040-2043) — port it WITH this
  macro.

### 4.12 IDENTIFIER_MACROS (identifierMacros.ts, 1 entry)

**Promise** → `state.TS(node, "Promise")` → rotor `s.RuntimeLib(node, "Promise")` (sets
UsesRuntimeLib; emits `TS.Promise`). Hook: `GetIdentifierMacro` at the transformIdentifier
site; registered with SymbolFlags.Variable. After implementing, NARROW rotor's fallback
(macromanager.go:303-311) per its own comment: real table + the upstream guards
(noConstructorMacroWithoutNew / noMacroExtends / noIndexWithoutCall — transformIdentifier.ts
L137-159, diagnostics already exist).

---

## 5. transformOptionalChain — COMPLETE (nodes/transformOptionalChain.ts, 356 L)

Entry (L350-356): `flattenOptionalChain` then
`transformOptionalChainInner(state, chain, transformExpression(state, expression))`. Rotor has
flatten + the non-optional fold (call.go:500-603); this section specs the optional path that
replaces call.go:590-603's NYS branch.

### 5.1 chainItem fields — the "eager type snapshot" is VESTIGIAL

Upstream items carry `type: state.getType(node.expression)` (and compound items
`callType: state.getType(node)`) captured at flatten time (L67-131). **Verified: neither
field is read anywhere** — `transformOptionalChainInner` re-queries the checker live (L263
`state.getType(item.expression.expression)`, L280
`state.typeChecker.getNonOptionalType(state.getType(item.node.expression))`), and
`flattenOptionalChain` has no other consumers (repo grep). The snapshots are dead weight from
an older design; since `state.getType` is pure/memoized, eager vs lazy is unobservable.
**Port decision: keep rotor's snapshot-free chainItem (call.go:483-495) — add nothing.**
Fields that ARE consumed: `kind, node, optional, name | argumentExpression, expression,
callOptional, args`.

`isCompoundCall` (L227-229): PropertyCall | ElementCall.

### 5.2 Helpers

```ts
function createOrSetTempId(state, tempId, expression, node) {           // L192-217
    if (tempId === undefined) {
        tempId = state.pushToVar(expression,
            node.parent && ts.isVariableDeclaration(node.parent) && ts.isIdentifier(node.parent.name)
                ? node.parent.name.text : "result");
    } else if (tempId !== expression) {
        state.prereq(/* tempId = expression */);
    }
    return tempId;
}
function createNilCheck(tempId, statements) {                            // L219-225
    return /* if tempId ~= nil then <statements> end */;
}
```

`node` passed to createOrSetTempId is ALWAYS `chain[chain.length - 1].node` — the OUTERMOST
expression; so `const foo = a?.b?.c` names the temp `_foo`(via parent VariableDeclaration),
otherwise `_result`. The `tempId !== expression` guard avoids self-assignment when the chain
result already lives in the temp.

### 5.3 transformOptionalChainInner (L231-348) — verbatim semantics

```ts
function transformOptionalChainInner(state, chain, baseExpression, tempId = undefined, index = 0) {
    if (index >= chain.length) return baseExpression;
    const item = chain[index];
    if (item.optional || (isCompoundCall(item) && item.callOptional)) {
        let isMethodCall = false, isSuperCall = false, selfParam;
        if (isCompoundCall(item)) {
            isMethodCall = isMethod(state, item.expression);             // CHECKER (P2 isMethod)
            isSuperCall = ts.isSuperProperty(item.expression);
            if (item.callOptional && isMethodCall && !isSuperCall) {
                selfParam = state.pushToVar(baseExpression, "self");     // BEFORE any nil check
                baseExpression = selfParam;
            }
            if (item.optional) {                                          // a?.b(...)
                tempId = createOrSetTempId(state, tempId, baseExpression, chain[chain.length - 1].node);
                baseExpression = tempId;
            }
            if (item.callOptional) {                                      // a.b?.(...)
                if (item.kind === OptionalChainItemKind.PropertyCall) {
                    baseExpression = luau.property(convertToIndexableExpression(baseExpression), item.name);
                } else {
                    const expType = state.getType(item.expression.expression);    // CHECKER
                    baseExpression = /* base[addOneIfArrayType(state, expType,
                                        transformExpression(state, item.argumentExpression))] */;
                }
            }
        }
        // capture so we can wrap later if necessary
        const [result, prereqStatements] = state.capture(() => {
            tempId = createOrSetTempId(state, tempId, baseExpression, chain[chain.length - 1].node);
            const [newValue, ifStatements] = state.capture(() => {
                let newExpression;
                if (isCompoundCall(item) && item.callOptional) {
                    const expType = state.typeChecker.getNonOptionalType(state.getType(item.node.expression)); // CHECKER
                    const symbol = getFirstDefinedSymbol(state, expType);          // CHECKER
                    if (symbol && state.services.macroManager.getPropertyCallMacro(symbol)) {
                        DiagnosticService.addDiagnostic(errors.noOptionalMacroCall(item.node));
                        return luau.none();
                    }
                    const args = ensureTransformOrder(state, item.args);
                    if (isMethodCall) args.unshift(isSuperCall ? luau.globals.self : selfParam!);
                    newExpression = wrapReturnIfLuaTuple(state, item.node, luau.call(tempId!, args));
                } else {
                    newExpression = transformChainItem(state, tempId!, item);
                }
                return transformOptionalChainInner(state, chain, newExpression, tempId, index + 1);
            });
            const isUsed = !luau.isNone(newValue) && !isUsedAsStatement(item.node);
            if (tempId !== newValue && isUsed) {
                luau.list.push(ifStatements, /* tempId = newValue */);
            } else if (luau.isCall(newValue)) {
                luau.list.push(ifStatements, /* CallStatement newValue */);
            }
            state.prereq(createNilCheck(tempId, ifStatements));
            return isUsed ? tempId : luau.none();
        });
        if (isCompoundCall(item) && item.optional && item.callOptional) {
            state.prereq(createNilCheck(tempId!, prereqStatements));      // SECOND nil check wraps everything
        } else {
            state.prereqList(prereqStatements);
        }
        return result;
    }
    return transformOptionalChainInner(state, chain, transformChainItem(state, baseExpression, item), tempId, index + 1);
}
```

Key rules, in order:

1. **Temp reuse**: ONE temp threads the whole chain — created at the first optional link
   (named from the outermost node's VariableDeclaration parent or "result"), then REASSIGNED
   (`tempId = newValue` appended inside each nil-check block). Subsequent recursion levels
   receive the same `tempId`.
2. **Nil-check block shape**: each optional link contributes
   `if _result ~= nil then <rest of chain folded into assignments> end`. The RECURSION happens
   inside the inner `capture`, so deeper links nest INSIDE the current `ifStatements`.
3. **Result usage**: `isUsed = !isNone(newValue) && !isUsedAsStatement(item.node)`. Used →
   trailing `tempId = newValue` assignment in the if-block and the temp is the expression
   result. Unused (expression-statement position) → a bare `CallStatement` if newValue is a
   call (so side effects survive), and the inner returns `luau.none()` — NOTE
   `isUsedAsStatement(item.node)` is checked per-LINK node; the none propagates outward
   through `!luau.isNone(newValue)` at enclosing links.
4. **`a.b?.()` (callOptional, methodish)**: when CHECKER `isMethod(state, item.expression)`
   and not a super property — `local _self = <base>` FIRST (outside all nil checks), base
   becomes `_self`, the callee is then `_self.b` stored in the temp, nil-checked, and called
   `_temp(_self, ...args)` — explicit self, never `:` sugar. Super property → `self` global
   unshifted instead, no selfParam temp.
5. **`a?.b()` (optional access, non-optional call)**: no special call handling — the link
   recurses through `transformChainItem` → `transformPropertyCallExpressionInner` with
   `tempId` as base; property-call MACROS run normally here (Set's `s?.add(x)` works; emitted
   inside the nil-check block).
6. **`a.b?.()` where `b` is a macro method**: `noOptionalMacroCall` diagnostic
   ("Macro methods can not be optionally called!" + none). CHECKER:
   `getNonOptionalType(getType(item.node.expression))` → `getFirstDefinedSymbol` →
   `getPropertyCallMacro` — note this fires for entries with nil Macro too (registration is
   what matters), so rotor must check `entry != nil`, not `entry.Macro != nil`.
7. **`a?.b?.()` (optional && callOptional)**: the OUTER capture's statements (which include
   the inner nil check for the call) are wrapped in a SECOND `createNilCheck(tempId, ...)` —
   producing the nested
   `if _r ~= nil then _r = _r.b; if _r ~= nil then ... end end` shape. All other cases just
   `prereqList` the captured statements.
8. **ElementCall index**: `addOneIfArrayType(state, getType(item.expression.expression),
   transformExpression(argumentExpression))` — CHECKER; the +1 rule for array bases.
9. **wrapReturnIfLuaTuple** applies to the optional-call result (L298) — LuaTuple-returning
   `f?.()` packs `{ _temp(...) }` under the standard P2b context rules.

Worked block shapes (ground truth from upstream tests; `local _result` hint per §5.2):

```lua
-- a?.b               local _result = a; if _result ~= nil then _result = _result.b; end
-- a?.b()             ... if _result ~= nil then _result = _result.b(); end          (b non-method)
-- a.b?.()            local _result = a.b; if _result ~= nil then _result = _result(); end
-- a.b?.() method b   local _self = a; local _result = _self.b; if _result ~= nil then _result = _result(_self); end
-- a?.pop() (macro)   local _result = a; if _result ~= nil then <pop emit using _result> ... end
```

Rotor wiring: replace call.go:590-603's loop with the recursive inner (or an iterative
equivalent preserving capture nesting EXACTLY — the two-level capture determines statement
order and is byte-load-bearing); keep flattenOptionalChain as-is; `luau.IsNone` /
`luau.IsCall` / `s.Capture` / `s.Prereq*` all exist.

---

## 6. Macro registration audit — spec

Upstream contract (MacroManager.ts:64-153): construction THROWS `ProjectError` the moment any
registration fails — texts (TYPES_NOTICE = `"\nYou may need to update your
@rbxts/compiler-types!"`):

- `MacroManager could not find symbol for ${name}` + NOTICE (identifier/call macro names and
  every SYMBOL_NAMES entry, L78/L151);
- `MacroManager could not find constructor for ${interfaceName}` + NOTICE (L88);
- `MacroManager could not find method for ${className}.${methodName}` + NOTICE (L138-141);
- `getFirstDeclarationOrThrow` → `ProjectError("")` (empty message, L70 — interface symbol
  with no InterfaceDeclaration).

Rotor deliberately diverged (macromanager.go header + L159: "Unresolvable names are skipped")
so checker-light transformer tests work — but the final Phase-3a review flagged the cost: a
failed `ResolveName` silently regresses macros to method calls (the damage-numbers.ts bug
class: `v.add(w)` → `v:add(w)` wrong output, no diagnostic).

Spec:

1. `NewMacroManager` additionally records `missing []string` — one entry per failed
   registration, formatted with the EXACT upstream texts above (including the NOTICE suffix
   and the empty-string case). Failure points: identifier/call `ResolveName` nil
   (macromanager.go:174/180), constructor `ResolveName` nil or no interface declaration or
   `interfaceConstructSymbol` nil (L186-200), property-call class `ResolveName` nil (record
   `could not find symbol for ${className}`) or per-method `methodMap[methodName]` nil
   (L237), and — once SYMBOL_NAMES moves to eager registration or keeps lazy `Symbol()` —
   any SYMBOL_NAMES name that fails to resolve. Expose `func (m *MacroManager) Missing()
   []string` (sorted for determinism — Go map iteration).
2. Gate condition ("while @rbxts/types + compiler-types are present"): compute once in
   `NewMacroManager`: compiler-types present ⇔ CHECKER:
   `chk.ResolveName("LuaTuple", nil, ast.SymbolFlagsAll, false) != nil` (its declaration lives
   only in compiler-types); @rbxts/types present ⇔ `chk.ResolveName("CFrame", nil,
   ast.SymbolFlagsInterface, false) != nil`. Record both booleans. `Missing()` returns nil
   when EITHER package is absent for entries that belong to it — i.e. partition: math-class
   rows (CFrame/UDim/UDim2/Vector*/Number — declared by @rbxts/types macro_math.d.ts) audit
   under types-present; everything else under compiler-types-present. (Upstream throws
   unconditionally; the partition is rotor's test-friendly refinement — a project genuinely
   missing the packages already dies earlier in noLib/global resolution.)
3. Enforcement point: `CompileProject` and `CompileFile` (internal/compile), immediately
   after the first `NewState` constructs the pass MacroManager (state.go:190-192): if
   `multi.macroManager.Missing()` non-empty → return the strings as diagnostics + hard error
   (`errors.New("compile: macro registration failure")`), mirroring upstream's
   ProjectError-before-any-emit. Transformer-level unit tests (buildState) never call the
   audit — existing tests unaffected.
4. After the §4 tables land, also tighten the fallbacks to upstream shape: implement
   `isMacroOnlyClass` (MACRO_ONLY_CLASSES = ReadonlyArray, Array, ReadonlyMap, WeakMap, Map,
   ReadonlySet, WeakSet, Set, String; membership ALSO requires `symbols.get(symbol.name) ===
   symbol` — the REGISTERED global symbol, not a same-named user type) and upstream's
   `getPropertyCallMacro` assert (L190-201): registered macro-only parent + unregistered
   method → hard error `Macro ${parent}.${name}() is not implemented!` (upstream
   `assert(false, ...)`; rotor: panic→error boundary or diagnostic). The compiler-types
   fallback (GetPropertyCallMacro/GetCallMacro/GetIdentifierMacro) then SHRINKS to exactly
   this assert + the audit, removing the Phase-2 "any compiler-types symbol is a macro"
   over-approximation.

---

## 7. Diagnostics delta

No NEW DiagnosticService diagnostics are introduced by this whole phase — rotor's set
(diagnostics.go) already contains every one referenced: `noOptionalMacroCall`,
`noVarArgsMacroSpread`, `noRangeMacroOutsideForOf`, `noTupleMacroOutsideReturn`,
`noModuleSpecifierFile`, `noUnscopedModule`, `noInvalidModule`, `noNonModuleImport`,
`noIsolatedImport`, `noServerImport`, `noRojoData`, `noPackageImportWithoutScope`,
`noConstructorMacroWithoutNew`, `noMacroExtends`, `noIndexWithoutCall`.

New NON-diagnostic failure texts: the §6 ProjectError strings (plain-text hard errors, like
the four compileFiles emit failures). Removed failure modes: tsgo's `Cannot find module
'shared/...'` semantic diags (§1), `You cannot use modules directly under node_modules` on
pnpm projects (§2), `Type '...' can only be iterated through when using the
'--downlevelIteration' flag ...` + the latent `Global type 'Iterable' must have 3 type
parameter(s)` (§3). The forof_test.go:40-47 tolerance for the iteration message must be
DELETED with §3 so regressions resurface.

Runtime (not compile-time) string for byte parity: ReadonlyArray.reduce's
`"Attempted to call \`ReadonlyArray.reduce()\` on an empty array without an initialValue."`.

---

## 8. Port inventory (Phase 3b execution checklist)

| # | Item | Where | Depends on |
|---|---|---|---|
| 1 | baseUrl→paths rewrite (§1.5) + sanitize tests | internal/compile/sanitize.go | — |
| 2 | compiler-types Iterable arity-3 overlay (§3.4) + forof_test tolerance removal | internal/compile/sanitize.go (same FS wrapper) | — |
| 3 | GuessVirtualPath (§2.4) + wire importexpr.go:244 | internal/transformer | tsgo GetSymlinkCache (vendored ✓) |
| 4 | wrapComments + header-exempt push detection (§4.1) | internal/transformer/propertycallmacros.go | — |
| 5 | argumentsWithDefaults (§4.2) | propertycallmacros.go | — |
| 6 | STRING_CALLBACKS 13 (§4.4) + ArrayLike.size (§4.5) | propertycallmacros.go | 4 |
| 7 | READONLY_ARRAY 15 (§4.6) — join needs CHECKER index-type query | propertycallmacros.go | 4,5 |
| 8 | ARRAY 9 (§4.7) | propertycallmacros.go | 4 |
| 9 | Set/Map tables (§4.8) + Promise.then (§4.9) | propertycallmacros.go | 4 |
| 10 | CALL_MACROS 8 (§4.11) — $getModuleTree needs the resolveModuleName fallback in getSourceFileFromModuleSpecifier | callmacros (new file) + importexpr.go | §1 (paths) for non-relative args |
| 11 | IDENTIFIER_MACROS Promise (§4.12) + fallback narrowing + identifier guards | macromanager.go, identifier.go | 13 |
| 12 | transformOptionalChainInner full port (§5) | call.go | 9 (macro interplay tests) |
| 13 | Registration audit (§6) + isMacroOnlyClass + macro-only assert | macromanager.go, compile | 6-11 |
| 14 | Oracle fixtures: every macro × statement/expression position × simple/complex base (wrapComments thresholds!), optional-chain shapes (§5.3 five forms), baseUrl fixture, Set/Map for-of post-§3 | testdata/diff | all |

CHECKER calls introduced in this phase (beyond already-ported ones): `getIndexTypeOfType`
(join), `resolveName` per registration (exists), `getPropertyCallMacro` inside optional chain
(exists), `isMethod` on the inner access of compound optional calls (exists),
`Program.ResolveModuleName` fallback ($getModuleTree), `Program.GetSymlinkCache` (§2).
