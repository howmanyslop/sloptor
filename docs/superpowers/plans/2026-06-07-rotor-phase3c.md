# rotor Phase 3c Implementation Plan — JSX, Classes, Spread, Async, Try, Enums

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Break the JSX wall (33 randomness files), land classes + decorators (unlocks the ReactComponent error boundary), spread + logical assignments (7 files), and complete the language surface with async/generators, try rerouting, enums, and namespaces — targeting near-complete randomness byte-parity.

**Source of truth:** the three Phase 3c digests in `docs/superpowers/research/` — `phase3c-jsx-digest.md` (factory-call assembler, attribute spread paths, JsxText fixup port from tsgo, ~20 oracle-verified shapes, fixture-dependency spec), `phase3c-classes-digest.md` (full class/ walkthrough, boilerplate shapes byte-captured, object-spread fast path vs loop, logical assignments, decorators), `phase3c-async-try-enums-digest.md` (TS.async/TS.generator wrappers, the COMPLETE TS.try flow-control rerouting spec with both load-bearing orderings, enum do-block + `_inverse`, namespace `_container`). Reference (`reference/roblox-ts/src/...`) wins on disagreement. Established conventions apply (TDD, byte parity or loud diagnostics, oracle technique, full gates per commit, quirks verbatim, push every commit).

---

## Task 1: JSX

Per jsx digest: add `@rbxts/react@17.3.7-ts.1` (exact pin, same install flavor as the existing lock) + the 3 jsx tsconfig keys (`jsx: "react"`, `jsxFactory: "React.createElement"`, `jsxFragmentFactory: "React.Fragment"`) to `testdata/diff/project` — **all 35 existing goldens must stay byte-unchanged** (verify). Port the 5 transform files + util into `internal/transformer/jsx.go` + `jsxtext.go`: the single factory-call assembler (tag, attrs-map-or-nil, ...children), lowercase tag-name test (`<_Comp/>` → string `"_Comp"` quirk), attributes via MapPointer with inline-map pointer + the two spread paths (`table.clone`+`setmetatable(nil)` fast path vs `_k`/`_v` for-loop), `{}` attr → `true`, children via ensureTransformOrder, JsxText fixup copied from tsgo's unexported `tsgo/transformers/jsxtransforms/jsx.go:810` Go port (backslash-doubling quirk). Factory entity via EmitResolver GetJsxFactoryEntity/GetJsxFragmentFactoryEntity + transformEntityName (all pre-existing). 4 dispatch cases (JsxElement/JsxSelfClosingElement/JsxFragment + expression child handling). JsxText fixup paths are type-illegal to oracle-pin — unit-test against the digest's recorded shapes instead. Oracle-pinned fixture `32_jsx.tsx` (element+props, self-closing, fragment, `.map` children, `&&`/ternary children, key prop, Event/Change-style tables as plain props).
Commit: `"transformer: JSX"`

### Task 2: Classes core

Per classes digest §2: `internal/transformer/classes.go` (+ split file if >400 lines) — class declaration vs expression, the setmetatable boilerplate byte-verbatim, `.new` returning `self:constructor(...) or self`, default constructor synthesis, property initializers vs constructor-body order, parameter properties, methods (type-driven isMethod — static methods emit as COLON methods quirk: `Animal:create()`), static fields/methods/blocks, inheritance (`super()` → the 3 super-call arms in call.go, `super.X`, abstract-no-extends bare `{}` no-`__index` quirk, `__tostring` wrapper re-emitted per subclass inheriting a toString METHOD), class expressions + temp naming, computed member names, hoisting (both class carve-outs already in rotor's hoisting — verify), `#field` diagnostic spanning the WHOLE class. Dispatch: ClassDeclaration/ClassExpression/ThisKeyword/SuperKeyword. getAllSuperTypeNodes reimplemented locally (3 lines, unexported in tsgo/ls). Constructor must NOT get a `self` param from transformParameters (verify isMethod(ConstructorDeclaration)=false). All 12 class diagnostics already in diagnostics.go — wire them. Oracle-pinned fixture `33_classes.ts`.
Commit: `"transformer: classes"`

### Task 3: Decorators

Per classes digest decorators section: legacy experimental decorators as rbxtsc 3.0 implements them (class/method/property decorators, the evaluation order, decorator key-pinning in functions.go the digest flags). Acceptance shape: `@ReactComponent export class ErrorBoundary extends React.Component` (randomness `client/ui/error-boundary.tsx` — no JSX syntax in the file). Oracle-pinned fixture `34_decorators.ts`.
Commit: `"transformer: decorators"`

### Task 4: Object/array spread + logical assignments

Per classes digest §3-4: object-literal spread — `table.clone` + `setmetatable(_object, nil)` fast path ONLY when spread hits an empty inline map, otherwise generalized-iteration copy loop, if-wrapped when not definitely-object; spread arm in literals.go. Destructuring rest stays BANNED — wire the upstream diagnostic byte-text (`"Operator \`...\` is not supported for destructuring!"`) at the sites rotor can reach. Array-literal spread (`[...a, b]` — killfeed-state.ts:43 blocker): port from `reference/roblox-ts/src/TSTransformer/nodes/expressions/transformArrayLiteralExpression.ts` spread handling directly (not digest-covered — reference is authority; oracle-pin every shape). Call-argument spread only if it shares the same machinery upstream, else leave NYS. Logical assignments per digest §4: `??=` pure `== nil` if-wrap (no temps for id-rooted targets); `&&=`/`||=` capture `_condition` temp, truthiness-test the ORIGINAL writable, unconditionally write back; BOTH dispatch points (statements.go:247 statement position, binary.go:126 expression position). Oracle-pinned fixture `35_spread.ts`.
Commit: `"transformer: spread and logical assignments"`

### Task 5: async + generators

Per async digest §1-2: `TS.async(function(...) end)` at the 3 sites — async DECLARATIONS become `local f = TS.async(...)` (or `f = ...` when hoisted), never function statements; async methods drop the colon shape (`!isAsync` gate), generator methods keep it; `await x` → `TS.await(skipDownwards(x))` one-liner. Generators: `wrapStatementsAsGenerator` body swap → `return TS.generator(function() <body> end)`; `yield v` → `coroutine.yield(v)` (bare yield zero-arg); `yield*` lowers to generic for over `inner.next` re-yielding with `_returnValue` capture when used as a value. Oracle-pinned fixture `36_async.ts`.
Commit: `"transformer: async functions and generators"`

### Task 6: try/catch/finally + flow-control rerouting

Per async digest §3 (the COMPLETE spec): `TS.try(tryFn, catchFn|nil, finallyFn?)` returning `exitType, returns`; flags `TS.TRY_RETURN=1/TRY_BREAK=2/TRY_CONTINUE=3`; producers are ONLY the break/continue/return transforms (`return TS.TRY_BREAK` / `return TS.TRY_RETURN, { vals }`); only transformTryStatement consumes. Blocked checks: nearest-of try-vs-function (return), try-vs-loop-or-switch (break); breakBlocked ⟹ returnBlocked. The two load-bearing orderings VERBATIM: (1) `_exitType`/`_returns` temps created BEFORE body transform (nested-try numbering parity — digest oracle f8), (2) `popTryUsesStack()` BEFORE transformFlowControl so propagation marks hit the OUTER try. collapseFlowControlCases replaces the LAST case's condition with bare `if _exitType then`. Loops/switch need ZERO changes (oracle-verified incl. break-in-try-in-switch). This retires the Phase 2 TRY_* rerouting no-op. Oracle-pinned fixture `37_try.ts` covering every rerouting shape from the digest's worked examples.
Commit: `"transformer: try statements and flow-control rerouting"`

### Task 7: Enums + namespaces

Per async digest §4-5: enums — string-only → plain map; otherwise `local E` + do-block with `_inverse` + `setmetatable({}, { __index = _inverse })`, interleaved member/inverse assignments; checker folds constant expressions (tsgo GetConstantValue equivalent — rotor's getConstantValueLiteral already inlines const-enum access); unfoldable computed members spill to `local _value`; const enums emit NOTHING; declare enum skipped. Namespaces — `local X = {}` + do-block with `local _container = X`; non-mutable value exports via the statement-list ExportInfo mapping (live in rotor); `export let` excluded (SetModuleIDBySymbol(containerId) + existing export-let indirection); dotted `namespace A.B` = nested ModuleDeclarations (implicit reparsed export modifier); merging BANNED via new ~10-line hasMultipleDefinitions util + noEnumMerging/noNamespaceMerging diagnostics (texts already byte-exact in diagnostics.go) once-per-symbol via AddDiagnosticWithCache. Oracle-pinned fixture `38_enums_namespaces.ts`.
Commit: `"transformer: enums and namespaces"`

### Task 8: Conformance + re-smoke + merge

Adversarial fixture `39_mixed3c.tsx` (JSX trees with class components and decorated classes, spread props feeding macros, async handlers with try/await inside loops with break, enum-keyed Maps iterated in JSX children, etc. — interactions, not isolated features). Full randomness re-smoke: expect a step change above 54/95 (the 41 blocked files' first blockers are all Phase 3c features, but cascaded second blockers may surface — tally honestly, report the new table). README roadmap (3c ✅ reworded to what landed, Phase 4 next) + "Try it today" numbers. Memory update. Final whole-branch review (opus) → fixes → merge → push.
Commit: `"Phase 3c complete: JSX, classes, async, try, enums"`

---

## Done criteria

1. All fixtures byte-identical (target ~43 entries incl. 32-39).
2. randomness count materially above 54 — every Phase 3c first-blocker construct compiles; remaining blockers (if any cascade out) tabled for the next phase, measured honestly.
3. JSX/classes/async/try/enums/namespaces produce byte-identical output or loud diagnostics — no silent wrong output; the Phase 2 TRY_* no-op is retired.
4. Remaining NYS surface explicitly enumerated in the final report (expected: call-spread if deferred, `$getModuleTree` if still deferred, watch/incremental = Phase 4).
