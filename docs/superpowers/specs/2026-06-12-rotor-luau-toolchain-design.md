# rotor Luau toolchain — bundler, minifier, `rotor dev` — design

Date: 2026-06-12. Status: approved under the standing autonomous-execution grant.

## Goal

Expand rotor from a drop-in `rbxtsc` (TypeScript → Luau) compiler into an
**all-in-one Luau/Roblox toolchain**. On top of the existing v1 compiler we add:

1. A **Luau front-end** (lexer + faithful, trivia-preserving CST + parser +
   generators) — the foundation that lets rotor *read* and *rewrite* arbitrary
   hand-written / vendored Luau, not just emit its own.
2. A **bundler** (`rotor bundle`) that resolves `require` graphs and inlines them
   into a single **still-runnable** Luau file (darklua's headline capability).
3. A **minifier** (`rotor minify`) — local renaming, comment/whitespace stripping,
   index→field, dense output.
4. A **`rotor dev`** command that watches the cwd, incrementally compiles, and
   serves the result to Roblox Studio (via Rojo) for the live edit loop.

**Reference:** [darklua](https://darklua.com/docs/) (Sea of Voices) for the Luau
processing model, and [full_moon](https://github.com/Kampfkarren/full-moon) for
the trivia-preserving parser design.

**The byte-parity contract of the existing TS→Luau compiler is untouched.** None
of this touches `internal/transformer`, `internal/luau` (emit AST), or
`internal/luau/render`. The new Luau layer is additive.

## Scope & non-goals

**In scope (this effort, across four sub-projects):** a Luau front-end, a
bundler, a minifier, and `rotor dev`.

**Explicit non-goals for v1:**

- **A native Rojo serve implementation.** `rotor dev` shells out to an installed
  `rojo serve`. Rojo's wire protocol moved to WebSocket + msgpack in 7.7 and is
  not a stable documented spec; re-implementing it (project-format encoding,
  instance/property serialization, patch diffing, Studio-plugin compatibility) is
  a large, fragile, separate project. Deferred, possibly indefinitely.
- **A full darklua-compatible rule/config engine** (`.darklua.json`, the ~40 rule
  catalog, data-file loaders, downlevel-to-5.1 rules). We build exactly the rules
  the bundler and minifier need. A general `rotor process` config surface can come
  later if there is demand.
- **Cross-module tree-shaking / aggressive dead-code elimination.** The minifier is
  scope-local (unused locals at most), matching darklua.
- **Source maps for the bundled/minified output.** The `retain_lines` generator
  preserves original line numbers within a file; cross-file bundle source maps are
  out of scope for v1.

## Decomposition

Four sub-projects, each getting its own implementation plan (and, for the larger
ones, its own detailed spec) after this umbrella design is approved:

| | Sub-project | New code | Depends on |
|---|---|---|---|
| **A** | Luau front-end | `internal/luau/lex`, `internal/luau/cst`, `internal/luau/gen` | — (critical path) |
| **B** | `rotor dev` | `cmd/rotor/dev.go` | existing watch-v2 (independent of A) |
| **C** | Minifier | `internal/luaproc`, `cmd/rotor/minify.go` | A |
| **D** | Bundler | `internal/bundle`, `cmd/rotor/bundle.go` | A + existing `internal/rojo` |

**Build order:** A is the critical path and is built first. B is independent of A
(pure orchestration over existing watch + a supervised `rojo serve`) and can be
built first or in parallel as an early, low-risk user-visible win. C then D follow
A. D is the most complex and lands last.

This document specifies **A in full detail** (it is what we build first) and gives
**B/C/D at an architecture level** sufficient to validate the overall shape; each
gets its own detailed spec when its turn comes.

## Cross-cutting decisions

1. **Separate CST, not the emit AST (`internal/luau`).** The emit AST is wrong for
   parsing: it has no source positions and no trivia, and it splits a table
   constructor `{...}` into `Array`/`Map`/`Set`/`MixedTable` by *inferred* kind —
   a parser only sees a field list. The new CST mirrors **surface syntax** and
   attaches trivia. The two ASTs coexist; neither is forced to serve the other's
   job.
2. **Trivia model: Roslyn-style, ported clean-room from full_moon.** Every
   significant token is a `Token` carrying leading trivia + the core lexeme +
   trailing trivia, where trivia = whitespace and comments. A token owns trailing
   trivia up to and including its line's newline; subsequent whitespace/comments
   become the *leading* trivia of the next token. This gives byte-exact roundtrip.
   full_moon is MPL-2.0 — we reimplement the *design*, copying no source.
3. **Generators mirror darklua:** `readable` (pretty), `dense` (minified), and
   `retain_lines` (default for transforms — preserves original line numbers so
   runtime stack traces still point at source lines).
4. **Cyclic requires: Roblox-faithful runtime error, deferred to runtime — not
   darklua's build-time hard-error and not a silent partial value.** Real Roblox
   ModuleScripts raise *"Requested module was required recursively"* when a require
   cycle is actually hit; roblox-ts output that compiles and runs therefore has no
   live value cycles. The bundle shim mirrors this: it tracks a `loading` set and
   `error()`s if a module is required while already loading. The build itself never
   fails on a (possibly type-only / never-exercised) cycle — only a cycle that
   actually executes errors, exactly as on Roblox. This is more faithful than
   darklua (which refuses to build) and safer than registering a partial `nil`
   (which would mask the bug).
5. **`rotor dev` shells out to `rojo serve`** (see non-goals).
6. **Grammar authority = `luau-lang/luau` `Ast/`.** Full Luau is supported:
   type annotations, generic functions, string interpolation (backtick), compound
   assignment (`+=` …), floor division (`//`), `if-then-else` expressions,
   `continue`, function attributes (`@native`/`@checked`), numeric separators and
   `0b`/`0x` literals.

---

## Sub-project A — Luau front-end (detailed)

The foundation: parse arbitrary Luau into a faithful CST, mutate it, and serialize
it back. Three packages.

### A.1 `internal/luau/lex` — tokenizer

- **Output:** a flat `[]Token`. Each `Token` has: `Kind` (Whitespace, Comment,
  Symbol, Name, Number, String, InterpStringChunk, Eof, plus keyword sub-kinds),
  the raw lexeme text, and a `Position` (byte offset + line + column for both
  start and end). Whitespace and comment tokens are **trivia** and are kept (they
  are not discarded), so the parser can attach them.
- **Luau lexical features:** long strings/comments `[[ ]]` / `[==[ ]==]`, `--`
  line comments, backtick interpolated strings with `{expr}` holes and `\{`
  escapes, number literals with `_` separators / `0x` / `0b` / hex floats / `e`
  exponents, all Luau symbols including `+=` `-=` `*=` `/=` `//=` `%=` `^=` `..=`
  `//` `::` `->` `...`. Keywords are recognized as `Name` tokens with a keyword
  flag (Luau has contextual keywords like `type`, `continue`).
- **Error policy:** the lexer never panics on bad input; an unterminated string or
  malformed number produces a diagnostic and a best-effort token so the parser can
  continue (error recovery).

### A.2 `internal/luau/cst` — concrete syntax tree + parser

- **Trivia attachment (`TokenRef`):** the parser consumes the trivia stream and
  produces `TokenRef{Leading []Trivia, Token Token, Trailing []Trivia}` wrappers.
  Every leaf in the CST references a `TokenRef`; concatenating every `TokenRef`'s
  leading + lexeme + trailing in source order reproduces the input **byte-for-byte**.
  This roundtrip property is the package's core invariant and is fuzz-tested.
- **Node taxonomy** (surface-faithful; one struct per grammar production):
  - *Block* = `[]Stmt` + optional trailing `LastStmt` (`return`/`break`/`continue`).
  - *Statements:* local-assign (`local a, b = …` with optional `: type`
    annotations and `<attribute>`), assignment (n targets = n exprs), call-stmt,
    do, while, repeat-until, numeric-for, generic-for, if/elseif/else,
    function-decl (`function a.b:c() … end`), local-function, `type`/`export type`
    alias, compound-assign (`a += b`).
  - *Expressions:* nil/true/false/number/string/vararg literals, interpolated
    string, name, `(expr)`, table constructor (a flat `[]Field` where each field
    is positional / `[k] = v` / `name = v` — **no** Array/Map/Set classification),
    binary (full operator set incl. `..` `//` `and` `or`), unary (`-` `not` `#`),
    `function() … end`, index `a.b` / `a[b]`, call `f(...)` / `f"s"` / `f{...}`,
    method call `a:b(...)`, `if-then-else` expression, type assertion `e :: T`.
  - *Types* (Luau type syntax): name types with generics, table types, function
    types, unions/intersections, `typeof(...)`, variadic `...T`. Parsed into a
    `TypeNode` subtree so `remove_types` (future) and faithful roundtrip both work;
    rotor itself never needs to *understand* types, only preserve/strip them.
  - Every node embeds a `base` with a parent pointer and exposes its bounding
    `TokenRef`s, mirroring the existing emit-AST conventions.
- **Parser:** a hand-written recursive-descent / Pratt parser (operator precedence
  table per the Luau grammar). Hand-written (not goyacc) because trivia attachment
  and error recovery need fine control. Error recovery: on an unexpected token,
  emit a diagnostic and synchronize to the next statement boundary, producing a
  partial tree (so tooling/`dev` doesn't die on one bad file).
- **Mutation + traversal:** nodes are mutable; a `Visit(node, pre, post)` walker
  (mirroring `internal/luau/render/visit.go`) supports the rule passes. Helpers to
  insert/replace statements and to mint fresh trivia (e.g. a single space) for
  synthesized nodes.

### A.3 `internal/luau/gen` — generators (serializers)

One `Generator` interface, three implementations:

- **`readable`** — pretty-prints from structure, ignoring original trivia
  (canonical formatting; tabs, newlines). Reuses the formatting *rules* of the
  existing emit renderer where practical.
- **`retain_lines`** (default) — replays attached trivia faithfully, only
  adjusting where the tree was mutated, so unmodified regions keep their exact
  bytes and original line numbers. This is what bundle/transform passes use so
  stack traces stay meaningful.
- **`dense`** — minimal whitespace: one space only where required to separate
  tokens (`local x` , `a and b`), no newlines except where semantically required,
  no comments. Inserts `;` only to disambiguate (the same ambiguity cases the emit
  renderer's `ending.go` already handles: a statement starting with `(` after a
  call). This is the minifier's serializer.

### A.4 Testing strategy for A

- **Roundtrip goldens + fuzz:** for a corpus of `.luau` files, assert
  `gen.RetainLines(Parse(src)) == src` byte-for-byte. Fuzz the lexer+parser+
  retain_lines roundtrip (Go native fuzzing) to harden trivia handling.
- **Corpus sources:** the vendored `reference/` Luau, the `@rbxts` RuntimeLib and
  Promise (already in `include/`), rotor's own compiled `out/` from the acceptance
  project, and a hand-written edge-case suite (long strings with `]==]`, nested
  interpolation, comment placement around every node kind, CRLF, trailing-comment
  attachment, no-trailing-newline files).
- **`dense`/`readable` semantic-equivalence:** parse → dense-print → re-parse →
  assert structural equality (idempotence of the round, ignoring trivia).
- **Diagnostics:** malformed inputs produce stable, useful messages and recover.

---

## Sub-project B — `rotor dev` (architecture)

The inner-loop command: `rotor dev [-p project] [--rojo project.json] [--no-serve]`.

- **Behavior:** run the existing watch-v2 build loop (incremental TS→Luau on every
  change, writing `out/`) **and** supervise a child `rojo serve <project>` so
  Roblox Studio live-syncs the fresh output. One Ctrl-C tears down both.
- **Reuse:** `cmd/rotor/watch.go` (watch-v2 engine), `cmd/rotor/build.go`
  (incremental build), `internal/term`/`ui.go` (output), and `rotor doctor`'s
  existing Rojo discovery (`ResolveSidecarDir` is sidecar; Rojo project discovery
  lives in the resolver/doctor). New code is mostly process supervision +
  interleaving rotor's and Rojo's log streams.
- **Rojo discovery:** find `rojo` on PATH (and `aftman`/`rokit` shims); find the
  Rojo project file (`default.project.json` or `--rojo`). If Rojo is missing,
  `dev` still watches+builds and prints a `doctor`-style hint instead of failing
  hard (`--no-serve` makes that explicit).
- **Process model:** start `rojo serve` as a child, stream its stdout/stderr
  through `internal/term` with a `rojo` prefix, restart it if it exits unexpectedly
  (bounded), and propagate signals. rotor's build is authoritative for `out/`;
  Rojo only serves files.
- **Non-goal:** rotor does not speak the Rojo protocol itself (see non-goals).

## Sub-project C — minifier (architecture)

`rotor minify <input.luau> [-o out.luau]` (and `--minify` shared by `bundle`).

- **Pipeline:** `cst.Parse` → ordered rule passes → `gen.Dense`.
- **Rules (built on the CST visitor), matching darklua's minify set:**
  - `remove_comments` — drop comment trivia.
  - `dense` serialization handles whitespace removal (no separate `remove_spaces`
    pass needed — the generator owns it).
  - `rename_variables` — scope-aware shortest-name renaming of **locals and
    function parameters only** (never globals/fields/`self`). Requires a lexical
    scope pass: walk blocks, track `local`/param declarations and their references,
    assign minimal fresh names (`a`, `b`, … skipping keywords and in-scope
    globals). This is the one non-trivial pass; correctness (no capture/shadowing
    bugs) is the priority and is property-tested by behavioral equivalence under
    Lune.
  - `convert_index_to_field` — `t["abc"]` → `t.abc` when the key is a valid ident.
  - `group_local_assignment` — merge adjacent `local` statements (size win).
- **Correctness gate:** minified output must be behaviorally identical. Test by
  running representative scripts (and the vendored behavioral suite where feasible)
  through minify and executing under Lune, comparing results.

## Sub-project D — bundler (architecture)

`rotor bundle <entry.luau> [-o bundle.luau] [--rojo project.json]`.

- **Require graph:** from the entry file, resolve each `require(...)`, recurse,
  and collect the module set. A `RequireResolver` interface abstracts resolution:
  - **path mode** (default for raw-Luau projects): `./`/`../` relative, `/`
    absolute, first-segment aliases from `.luaurc`/a `sources` map; tail resolution
    tries `+.luau`, `+.lua`, `<dir>/init.luau`, `<dir>/init.lua` (Rojo's `init`
    convention). `excludes` globs (Roblox services, `@lune/**`) stay as runtime
    requires.
  - **rojo mode**: resolve Roblox instance-path requires
    (`require(game.ReplicatedStorage.X)`) back to files via the existing
    `internal/rojo` resolver + a `rojo sourcemap`. This is what lets rotor bundle a
    Rojo/roblox-ts project. (roblox-ts's own `TS.import(...)` RuntimeLib shape is a
    documented later increment, not v1-D core.)
- **Runtime model (darklua-equivalent table+closure, Roblox-faithful cycle
  detection):** emit one module table:

  ```lua
  local __ROTOR_BUNDLE = { cache = {}, loading = {} }
  do
      local function impl_<id>(...) --[[ original module body ]] end
      function __ROTOR_BUNDLE.load_<id>()
          local cached = __ROTOR_BUNDLE.cache[<id>]
          if cached ~= nil then return cached.value end
          if __ROTOR_BUNDLE.loading[<id>] then
              error("Requested module was required recursively", 2)
          end
          __ROTOR_BUNDLE.loading[<id>] = true
          local value = impl_<id>()
          __ROTOR_BUNDLE.loading[<id>] = nil
          __ROTOR_BUNDLE.cache[<id>] = { value = value }
          return value
      end
  end
  ```

  Each `require(...)` in a module body is rewritten to `__ROTOR_BUNDLE.load_<id>()`.
  The entry module runs last. The `loading` guard reproduces Roblox's recursive-
  require error rather than recursing forever or silently returning a partial
  value. The table identifier is configurable.
- **Caching semantics:** a module body runs at most once; subsequent
  `load_<id>()` calls return the cached value (including a legitimately `nil`/`false`
  export, distinguished by the `{ value = … }` wrapper's presence vs a `nil` cache
  slot). This replicates Roblox's run-once-and-cache ModuleScript contract.
- **Serialization:** `retain_lines` by default (debuggable); `--minify` swaps in the
  `dense` generator + minify rules.
- **Diagnostics:** unresolved require → clear error with the require text and the
  resolver chain tried. A statically detectable require cycle is **reported as a
  build warning** with the cycle path (side-effect ordering may differ, and an
  exercised cycle will error at runtime per decision #4); the build still succeeds.

## CLI surface (summary)

| Command | Purpose | Status |
|---|---|---|
| `rotor build` / `check` / `doctor` | existing TS→Luau compiler | unchanged |
| `rotor dev` | watch + incremental build + supervise `rojo serve` | new (B) |
| `rotor minify <in> [-o out]` | minify Luau | new (C) |
| `rotor bundle <entry> [-o out]` | bundle a require graph into one file | new (D) |

`--minify` is shared by `bundle`. Global flags (`-p/--project`, `--verbose`,
color env vars) follow the existing conventions.

## Risks & mitigations

- **Parser correctness / roundtrip completeness** is the biggest risk. Mitigation:
  hand-written parser against the canonical `luau-lang/luau` grammar, byte-exact
  roundtrip goldens + native fuzzing, and a broad real-world corpus (RuntimeLib,
  Promise, `@rbxts` Packages, rotor's own output) before C/D depend on it.
- **`rename_variables` capture bugs** would silently break runtime behavior.
  Mitigation: a dedicated scope-resolution pass with property tests and Lune
  behavioral-equivalence checks; conservative (skip rename) when scope is uncertain.
- **Bundle require-resolution edge cases** (aliases, `init`, sub-extensions,
  cross-partition Rojo paths). Mitigation: reuse the battle-tested `internal/rojo`
  resolver for rojo mode; extensive path-mode unit tests mirroring darklua's.
- **Rojo version drift** for `rotor dev`. Mitigation: we only *launch* `rojo`, never
  speak its protocol, so its internal WebSocket/msgpack churn doesn't affect us.

## Milestones

1. **A** — front-end: lex → cst/parser → gen, roundtrip-green on the full corpus.
2. **B** — `rotor dev` (parallelizable with A; can ship first as an early win).
3. **C** — `rotor minify` on top of A.
4. **D** — `rotor bundle` on top of A + `internal/rojo`.

Each milestone updates `roadmap.md` (new "Luau toolchain" section) and ships with
its tests green, per the project's standing autonomous-execution + roadmap-upkeep
practice.
