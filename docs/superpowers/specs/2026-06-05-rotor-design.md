# rotor — Design Document

**Date:** 2026-06-05
**Status:** Approved pending user review

## Summary

`rotor` is a native-code reimplementation of the roblox-ts compiler (`rbxtsc`), written in **Go** on top of **typescript-go** (Microsoft's native port of the TypeScript compiler, shipping as TypeScript 7). The goal is a drop-in replacement: **1:1 output compatibility** with roblox-ts 3.0.0, the same CLI surface, the same `@rbxts/*` package ecosystem via npm, and roughly an order-of-magnitude improvement in startup, cold-build, and watch-rebuild times.

## Motivation

roblox-ts is a Node.js program built on the JS TypeScript compiler API. Startup, cold builds, and watch rebuilds are all slow because every part of the pipeline — boot, parse, bind, typecheck, transform, render — runs in JS, single-threaded.

A pure syntax transpiler (SWC/esbuild-style) cannot replace it: roblox-ts's Luau emit is **type-directed**. Loop emission differs for Array/Map/Set/string/generator iterables; `+` compiles to `+` or `..` by operand type; truthiness guards are emitted based on whether a type can be `0`/`""`/`NaN` (falsy in TS, truthy in Luau); method-vs-function call sites decide `:` vs `.`; macros resolve by checker symbol. Reimplementing the TS checker is a multi-year project (SWC's funded `stc` attempt was abandoned). typescript-go is the only native implementation of the real checker, making it the only viable foundation for a native 1:1 port.

## Research findings the design relies on

(Researched 2026-06-05.)

### typescript-go

- TypeScript 7.0 is in **beta** (`@typescript/native-preview`, `7.0.0-dev.*`); stable expected ~late June 2026. ~10x faster than TS 6, parallel checker workers. TS 6 (JS) and TS 7 (Go) are semantically aligned by design.
- **All Go packages live under `internal/`** — not importable by third parties, and the TS team does not plan a public Go API near-term. Proven workarounds exist: `go:linkname` shims (typescript-eslint's `tsgolint`, forked and shipped by oxc for Oxlint type-aware linting) and **mirror-forks with rewritten import paths** (`buke/typescript-go-internal`).
- License: **Apache-2.0** — vendoring/mirroring is legal with attribution and statement of changes.
- The internal API surface matches what roblox-ts needs: `tsoptions.GetParsedCommandLineOfConfigFile` → `compiler.NewCompilerHost` → `compiler.NewProgram` → `Program.GetTypeChecker(ctx)` → `checker.GetTypeAtLocation / GetSymbolAtLocation / ResolveName / GetSignaturesOfType / GetPropertiesOfType / TypeToString`, etc. Internal APIs roblox-ts abuses via `ts-expose-internals` (emit resolver, `skipAlias`) exist in tsgo because it is a faithful port.

### roblox-ts 3.0.0 (port surface ≈ 17k LOC TS)

| Component | LOC | Notes |
|---|---|---|
| `TSTransformer` | 12,320 | One transform per `ts.SyntaxKind`; central `TransformState` with **prereq statement stack** (linearizes expression semantics into Luau statements); type-predicate combinators (`isDefinitelyType`/`isPossiblyType`); hoisting; try/catch control-flow routing via `TS.try` flags |
| Macros | 1,196 | Symbol-keyed tables resolved against `@rbxts/compiler-types` ambient declarations via `checker.resolveName`. Identifier (`Promise`), call (`assert`, `typeOf`, `typeIs`, `classIs`, `identity`, `$range`, `$tuple`, `$getModuleTree`), constructor (`WeakMap` etc.), property-call (`Array.map`, `string.format`, `Set.has`, vector math operators, …) |
| `luau-ast` + renderer | 2,460 | 40 Luau node kinds, factories, doubly-linked statement lists, renderer with scope-aware temp-identifier collision solver |
| Rojo resolver + path translator | 643 | FS path ↔ Roblox instance path; `index.*` ↔ `init.*` |
| Project orchestration | 1,827 | Builder-program incrementality (tsbuildinfo), watch (chokidar), transformer plugins, `.d.ts` emit |
| CLI + shared | ~1,000 | yargs CLI, ~70 diagnostic factories |
| `RuntimeLib.lua` + `Promise.lua` | 258 + 2,068 (Lua) | **Reused verbatim — no port** |

- Checker access is funneled: `state.getType()` (cached `getTypeAtLocation`, 63 sites), `getSymbolAtLocation` (~28 sites), plus contextual type, signatures, exports.
- Tests: **no golden-output corpus upstream** — instead 486 behavioral TestEZ cases (~7k LOC) compiled, rojo-built, and executed under Lune; plus 87 diagnostic-expectation files (filename ↔ expected diagnostic ID).
- Plugins: roblox-ts runs JS transformer plugins via `ts.transformNodes`, then **reprints transformed files to text** and re-adds them through a LanguageService proxy program (transformed nodes lack type info). The plugin boundary is effectively text→text.

## Architecture

```text
rotor (single static Go binary)
├── cmd/rotor               CLI — rbxtsc-compatible flags (build, -w, --type, --luau, …)
├── internal/project        program creation, watch, incremental, file orchestration
├── internal/transformer    port of TSTransformer
│   ├── expressions/  statements/  jsx/  classes/  binding/
│   └── macros/             symbol-keyed macro tables
├── internal/luau           Luau AST (40 node kinds) + renderer + temp-id solver
├── internal/rojo           RojoResolver + PathTranslator ports
├── sidecar/                Node helper for transformer plugins (spawned only if configured)
└── include/                RuntimeLib.lua, Promise.lua — verbatim from roblox-ts
        ▲ imports
typescript-go (parser, binder, full checker) via pinned mirror
```

### Consuming typescript-go

We maintain **our own automated mirror**: a vendoring script that snapshots a pinned typescript-go commit and rewrites `internal/...` import paths to importable ones (the `buke/typescript-go-internal` technique). Rationale vs alternatives:

- vs **buke's mirror**: we control the pin and update cadence; no third-party freshness risk.
- vs **`go:linkname` shims**: proven (tsgolint/oxc) but brittle and unwieldy at rotor's scale of checker access (~90 call sites across many types).

Apache-2.0 obligations: retain LICENSE/NOTICE, state changes.

### Compatibility target

- Emit semantics: **roblox-ts 3.0.0**, `@rbxts/compiler-types` 3.0.
- Language semantics: TS 6/7 (aligned by Microsoft; the user's project is on TS 6.0.3).
- Output: byte-identical Luau modulo the `-- Compiled with` version header.
- Ecosystem: same npm/`node_modules/@rbxts` layout; RuntimeLib's `TS.getModule` resolution unchanged.

### Transformer plugins (Node sidecar)

When `compilerOptions.plugins` is configured, rotor spawns a bundled Node helper that:

1. loads plugins with the real JS `typescript` package and builds the JS-side `ts.Program` they expect (full `TypeChecker` access — `rbxts-transformer-flamework`-class plugins work unmodified);
2. applies transformers and prints transformed TS source text;
3. streams transformed text back to rotor, which compiles it through the native pipeline.

Projects without plugins never spawn Node and get full native speed. Plugin projects keep the JS program warm in watch mode; their cold builds are dominated by the JS checker pass (unavoidable — plugins are JS programs demanding the JS compiler API).

### Error handling

- Port roblox-ts's ~70 custom diagnostics with identical messages and IDs; same `getPreEmitDiagnostics`-equivalent gating per file.
- TS syntax/semantic errors surface from the tsgo checker with standard TS codes, matching rbxtsc behavior.
- Unsupported v1 features (see Scope) fail fast with a clear named error, never silently mis-emit.

## Scope

### In (v1)

Full transformer and macro surface; JSX (`@rbxts/react`/Roact factories); async/await, generators, try/catch control-flow routing; classes and decorators; destructuring/binding patterns; Game/Model/Package project types; node_modules package imports; `.d.ts` emit for packages; watch mode (native fs events, debounced batching); incremental builds; `.lua`/`.luau` output; rbxtsc CLI flag surface including `--logTruthyChanges`, `--allowCommentDirectives`, `--writeOnlyChanged`, `--optimizedLoops`; transformer plugins via Node sidecar; comment-directive hoisting (`--!strict` above header).

### Out (v1)

- **Playground/VirtualProject** (powers roblox-ts.com; irrelevant to CLI use)
- **`--writeTransformedFiles`, `devlink`** (upstream repo-dev tooling)

## Testing strategy (the 1:1 proof)

1. **Differential golden testing (backbone):** harness runs `rbxtsc` 3.0.0 and `rotor` over the same corpus and byte-diffs every emitted file (header-normalized). Corpus: roblox-ts's `tests/src` (~7k LOC exercising every feature) **plus the user's real `randomness` project**. Runs in CI from the first transformer commit; any divergence fails.
2. **Behavioral suite:** the 486 upstream TestEZ cases compiled by rotor → `rojo build` → executed under **Lune**. Proves output runs correctly, not merely that it matches.
3. **Diagnostics corpus:** the 87 expected-error files; rotor must report the same diagnostic IDs at the same locations.
4. **Acceptance:** `rotor build` on `randomness` is byte-identical to `rbxtsc` and the game runs.

Development is TDD: each ported transform gets differential fixtures first (red), then the Go port (green). Phase 1's pure Luau AST/renderer is unit-tested conventionally.

## Phasing

- **Phase 0 — Foundation:** repo scaffolding; tsgo mirror + vendoring script; spike that parses + typechecks a file and queries `GetTypeAtLocation` from Go. *De-risks the core bet immediately.*
- **Phase 1 — Luau AST + renderer:** pure code, no checker; includes temp-identifier solver. Fully unit-tested.
- **Phase 2 — Transformer core:** `TransformState` + prereq stack; expressions/statements in dependency order; differential harness live from first emit.
- **Phase 3 — Type-directed layer:** type-predicate combinators; for-of shapes; truthiness; all macro tables; JSX; classes; async/try/generators.
- **Phase 4 — Project layer:** Rojo resolver, path translator, import emission, CLI, watch, incremental, plugin sidecar.
- **Phase 5 — Conformance:** full behavioral suite under Lune; diagnostics corpus; `randomness` differential; fix divergences to zero.

## Risks

| Risk | Mitigation |
|---|---|
| tsgo internals churn under our mirror | Pin a commit; vendoring script makes rebasing deliberate; TS 7.0 stable (~weeks away) will slow churn |
| TS 5.5.3 (rbxtsc's pin) vs TS 6/7 checker behavioral differences create emit divergence | User's project already compiles on TS 6.0.3; differential harness surfaces any divergence immediately; judge divergences case-by-case (some will be TS-version, not rotor, bugs) |
| Subtle semantics in prereq-stack ordering / temp-id naming break byte-identity | Byte-diff harness from day one; port the temp-id solver faithfully rather than "improving" it |
| Internal APIs (`getEmitResolver`, `skipAlias`) shaped differently in tsgo | Verified to exist; Phase 0 spike exercises them before committing to the port |
| Sidecar plugin fidelity (Flamework) | Sidecar uses the real JS `typescript` package — same code path roblox-ts uses today |
| Scale: ~17k LOC of dense porting | Phased delivery; differential testing means progress is measurable and never silently wrong |

## Success criteria

1. Byte-identical output (header-normalized) vs rbxtsc 3.0.0 on the upstream test corpus and `randomness`.
2. All 486 behavioral cases pass under Lune; all 87 diagnostic expectations match.
3. `rotor build` and `rotor -w` work as drop-in `rbxtsc` replacements in `randomness` with the same npm packages.
4. Measured wins on `randomness`: startup, cold build, and watch-rebuild times each substantially faster than rbxtsc (target: ≥5x cold, near-instant watch rebuilds).
