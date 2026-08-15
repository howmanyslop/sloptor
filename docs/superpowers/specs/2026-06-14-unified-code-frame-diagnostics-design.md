# rotor diagnostics & DX improvements — design

Date: 2026-06-14. Status: approved under the standing autonomous-execution grant.

Centerpiece: a unified **code-frame renderer** so a failed compile shows the
offending source (line, caret, highlighted keywords) everywhere errors surface.
Around it, a set of diagnostics-polish and developer-experience improvements
selected during brainstorming.

## Goal

When user code fails to compile, rotor should **show the offending source** — the
line(s) of code, a caret/underline pointing at the span, and reserved keywords
highlighted — instead of a bare `path:line:col: message` one-liner. This must
hold in one-shot builds **and in watch / `rotor dev`**.

Today only `rotor check` shows source context (via tsgo's `diagnosticwriter`).
Every other error path prints a flat message:

- `rotor build` / watch — both TypeScript pre-emit errors **and** rotor's own
  transformer/macro diagnostics (`noAny`, `rotorNotYetSupported`, `$env`/`$asset`
  errors, …) surface as message-only strings. The build pipeline flattens
  diagnostics to `[]string` and the transformer→`DiagnosticInfo` conversion
  **drops the `Node`**, so position is lost before it reaches the CLI
  (`internal/compile/compile.go` `transformAndRenderDetailed`,
  `internal/compile/project.go` `compiledProjectSourceFile.diags []string`).
  Watch and one-shot build share `runBuildOnce` → `reportBuildPass` →
  `ui.buildFailure`, which prints `"    %s\n"` per diagnostic
  (`cmd/rotor/ui.go:70`) — so fixing `buildFailure` fixes watch too.
- `rotor bundle` / `rotor minify` / `rotor pack` — Luau parse/lex errors
  (`cst.Diagnostic{Pos{Offset,Line,Col}, Message}`) print as
  `path:line:col: message` (`cmd/rotor/minify.go:75`,
  `internal/bundle/bundle.go:86`, `internal/pack/luau.go`).

## Decisions (settled during brainstorming)

1. **Unified rotor frame for both TypeScript and Luau** — one renderer, one
   visual style across the toolchain (not tsgo-style for TS + rotor-style for
   Luau).
2. **Highlight tier: keywords + caret** — source line(s) with a gutter and a
   severity-colored caret/underline, reserved keywords colored. *Not* a full
   tokenizer-driven syntax highlighter.
3. **Watch shows frames** — the same renderer drives one-shot build, watch, and
   `rotor dev`.
4. **Diagnostics polish, all four**: error summary + per-file grouping;
   clickable `file:line:col` links (OSC 8); distinct `help:` lines for
   suggestion/more-info text; truncation of long lists with `--max-errors`.
5. **DX**: `rotor init` adopts an existing project (config-only); `rotor doctor`
   flags a missing/invalid `rotor.toml` and points at `rotor init`; watch gets
   transition cues (clear-on-rebuild, persistent error banner, optional bell).
6. **Deferred**: an editor/Studio diagnostics feed (`diagnostics.json` / Studio
   plugin push) — separate follow-up.

## Scope & non-goals

**In scope:** the `internal/diagframe` renderer; structured-location plumbing
through the compile/build path; wiring into `build`, watch/`dev`, `bundle`,
`minify`, `pack`; the four diagnostics-polish items; init-adopt; doctor↔init
synergy; watch transition cues.

**Explicit non-goals:**

- **Changing `rotor check`'s output.** It keeps tsgo's `diagnosticwriter`; the
  conformance/differential harness asserts on tsgo's exact formatting and the
  byte-parity contract makes this the high-risk path to leave alone.
- **Changing diagnostic message or code text.** The frame wraps the existing
  message; `noAny` says exactly what it says today (the diff/conformance harness
  asserts codes and messages). Splitting out `help:` lines is presentational and
  reconstructs the original text byte-for-byte when uncolored.
- **Changing generated `.luau` bytes.** Error presentation on stderr only.
- **Full syntax highlighting** (strings/numbers/comments). Keywords only.
- **Config-file errors** (`internal/config/load.go`'s `path:line:col` lines) get
  framing only incidentally via the doctor work; not a focus.
- **Editor/Studio diagnostics feed.** Deferred to a follow-up plan.

## Architecture

### 1. `internal/diagframe` — the renderer

Pure, no filesystem I/O. The caller supplies the source text; the renderer
slices and decorates it. Uniform interface keyed on **byte offset** (both Luau
`cst.Diagnostic.Pos.Offset` and tsgo node/diagnostic positions are byte offsets
into their source text, so one shape serves both languages):

```go
type Severity int        // Error, Warning
type Language int         // Luau, TypeScript

// Spot is one diagnostic anchored to a byte span of the source.
type Spot struct {
    Offset   int          // byte offset of the span start into Source
    Len      int          // span length in bytes; rendered caret count is max(1, …)
    Severity Severity
    Code     string       // optional, shown after message: "TS2322", "noAny", ""
    Message  string       // primary message (without suggestion/more-info tail)
    Help     []string     // suggestion / "More information:" lines, rendered as help:
}

type Options struct {
    Context int           // context lines above/below (default 1)
    Color   bool          // caller sets from term.ColorEnabled(w)
    Link    bool          // emit OSC 8 hyperlink on the locator (caller: Color && supported)
}

// Render returns the framed (and, when o.Color, ANSI-colored) block for one
// file's spots. Pure: no I/O, the caller writes the result where it wants.
func Render(path, source string, lang Language, spots []Spot, o Options) string
```

Behavior:

- **Position math.** Line/col computed from `Offset` by counting newlines in
  `Source` (1-based line, 1-based byte column). No dependency on tsgo's position
  APIs — one code path resolves Luau and TS spots.
- **Layout** (matches the approved mock):

  ```text
  error: expected ')'
    --> entry.luau:3:18
     |
   2 | local function f()
   3 |   return print(1 2)
     |                  ^ expected ')'
     |   help: add the missing ')'
  ```

  Severity headline (`error:` red / `warning:` yellow), `-->` locator, gutter
  with right-aligned line numbers, one context line above/below the primary
  line, a caret/underline line in the severity color, then any `help:` lines.
- **Keyword highlighting.** Word-boundary scan (`[A-Za-z_][A-Za-z0-9_]*`) over
  each visible source line colors words in the language's reserved set. Both
  sets live here (Luau: the 21 reserved words; TS/JS: the standard reserved
  set). The keywords-only tier may color a keyword inside a string/comment on
  the shown line — an accepted cosmetic limitation.
- **Clickable links.** When `o.Link`, the locator path is wrapped in an OSC 8
  hyperlink (`term.Styler.Hyperlink`) pointing at a `file://…` URI with the line
  fragment, so supporting terminals jump to the spot. No-op when color/links are
  off; never affects the uncolored byte output.
- **Help lines.** `Spot.Help` entries render as a muted/cyan `help:` line under
  the frame. Callers populate it by splitting the existing multi-line message
  (transformer messages join `suggestion(...)` and `issue(...)` with `\n`); the
  uncolored rendering still contains the exact original text.
- **Color gating.** Caller passes `o.Color` from `term.ColorEnabled(w)`
  (honors `NO_COLOR`/`FORCE_COLOR`/TTY). Color off → plain ASCII, byte-stable.
- **Edge cases.** Tabs expanded for caret alignment; a multi-line span
  underlines only through the first line's end; `Len==0` → single caret; empty
  or out-of-range `Source` degrades to `path:line:col: message`; never panics.

A drift-guard test imports `internal/luau` and asserts diagframe's Luau keyword
set equals the canonical set behind `internal/luau/validate.go`. `internal/luau`
exposes `IsReservedKeyword(string) bool` to enable it.

### 2. Summary, grouping & truncation

A thin presentation layer over `Render`, used by build/watch and the Luau
commands:

- **Grouping.** Diagnostics are bucketed by file; each file renders its frames
  together under its path, files in stable sorted order.
- **Summary footer.** After the frames: `✗ 3 errors in 2 files` (and
  `· 1 warning` when present), in rotor chrome colors. Replaces the current
  `buildFailure` headline-only behavior; warnings that don't fail the build
  still get a frame + count.
- **Truncation.** A `--max-errors N` flag (default e.g. 50; `0` = unlimited)
  caps rendered frames; the footer notes `…and 12 more` so silent truncation
  never reads as "all clear". The cap is per-invocation across all files.

### 3. TypeScript / compile plumbing

The renderer needs `(path, source, offset, len)` per diagnostic; the compile
layer currently throws location away. Changes:

- **`DiagnosticInfo`** (`internal/compile/compile.go`) gains `FileName string`,
  `Offset int`, `Len int` (alongside `Code`, `Message`, `Warning`).
- **`transformAndRenderDetailed`** populates location from `d.Node`: token start
  (skipping leading trivia) and end give `Offset`/`Len`; the node's source file
  gives `FileName` and source text.
- **`tsDiagnosticInfos`** populates from each `*ast.Diagnostic`
  (`File()`/`Pos()`/`Len()`).
- **Project path** (`internal/compile/project.go`): `compiledProjectSourceFile`
  carries structured per-file diagnostics plus the file's source text; a
  detailed collector surfaces these through `BuildResult`. The existing
  message-only accessors (`diagnosticInfoMessages` and the `[]string`-returning
  `CompileFile`/`CompileProject`/`BuildProjectWithOptions`) stay unchanged, so
  conformance/diff tests are untouched; the structured output is additive.
- **`cmd/rotor/build.go`**: `runBuildOnce` returns structured diagnostics;
  `--json` (`jsonDiagnostic`) can now emit real `file`/`line`/`col` instead of
  the message-only placeholder it documents today.

Source text for the frame is captured at collection time (we hold the
`*ast.SourceFile`), so the renderer never re-reads disk and virtual/in-memory
test sources work unchanged.

### 4. Luau plumbing

`cmd/rotor/minify.go`, `cmd/rotor/bundle.go` (+ the error surface in
`internal/bundle`), and `internal/pack/luau.go` already hold the source string
and `[]cst.Diagnostic`. Each maps `cst.Diagnostic` → `diagframe.Spot` (`Offset`
from `Pos.Offset`, `Message`, `Severity=Error`) and renders with
`diagframe.Luau`. The `internal/bundle` parse-error path returns structured
context (path, source, diagnostic) so the CLI frames it, keeping `internal/bundle`
free of presentation concerns.

### 5. Watch wiring & transition cues

- **Frames in watch.** `reportBuildPass`/`ui.buildFailure` render via the
  grouping layer (§2). Because watch and one-shot build share this path, no
  watch-specific rendering code is needed.
- **Transition cues** (`cmd/rotor/watch.go`, `cmd/rotor/ui.go`):
  - Clear the screen (or print a separator) at the start of each rebuild so the
    latest result is unambiguous (respects non-TTY: no escape codes when piped).
  - A persistent error banner: while the last build failed, the idle "watching"
    line shows `✗ N errors — watching for changes`; on the next passing build it
    flips to the success line.
  - Optional terminal bell (`\a`) on a pass→fail or fail→pass *transition*
    (not every rebuild), gated behind a `--bell` flag (off by default; suppressed
    when not a TTY).

### 6. `rotor init` — adopt an existing project

Today `cmdInit` refuses when `package.json`/`default.project.json` exist
(`init.go:89-97`). New behavior:

- When run in a directory that **is already a project** (has `package.json`,
  `tsconfig.json`, or `default.project.json`) **and has no `rotor.toml`**, switch
  to **adopt mode** instead of refusing: write only the missing rotor pieces —
  `rotor.toml`, `rotor.schema.json`, and `rotor-env.d.ts` (if absent) — and never
  overwrite an existing file (each pre-existing target is reported as
  `· path (exists, kept)`).
- **Template detection** picks the `rotor.toml` skeleton: `plain` when there's a
  `default.project.json` but no `tsconfig.json`; `package` when `tsconfig.json`
  has `"declaration": true` (or the Rojo tree points at a model, not a
  DataModel); `game` otherwise. Detection only chooses which commented skeleton
  to emit; it writes no source files.
- When `rotor.toml` already exists, adopt mode reports "already configured" and
  exits 0 (idempotent), suggesting `rotor doctor`.
- A `--config` flag forces config-only adopt mode explicitly (useful in scripts
  / when the heuristics are ambiguous). Greenfield scaffolding (empty dir) is
  unchanged.

Refactor: `writeInitFiles` already skips nothing; adopt mode reuses `scaffold`'s
config-file producers (`rotorTOML`, schema, env types) via a small
`adoptFiles(opts)` that returns only those, and a write path that skips existing
targets rather than failing.

### 7. `rotor doctor` ↔ `init` synergy

`runDoctor` (`cmd/rotor/doctor.go`) gains a `rotor.toml` check:

- `config.Load(dir)` → `ErrNotFound`: a `warn` row, "no rotor.toml — run
  `rotor init` to add one" (asset/deploy features need it).
- Load error (parse/validation): a `fail` row carrying the message (framed via
  diagframe when the error has a position — `internal/config/load.go` already has
  `path:line:col`).
- Valid: an OK row, "rotor.toml" with the resolved path; surface any unknown-key
  warnings (`load.go:79`) as warn rows.

## Data flow

```text
Luau:  source + []cst.Diagnostic ─┐
                                  ├─► []diagframe.Spot ─► group by file ─► Render ─► stderr
TS:    *ast.Diagnostic ───────────┤        (per file, with that file's source)
       transformer.Diagnostic ────┘                              └─► summary footer
       (Node → offset/len/source via tsgo AST)
```

## Error handling & degradation

- No color (pipe/redirect/`NO_COLOR`) → plain framed text, ASCII, byte-stable;
  no hyperlinks, no bell, no screen clear.
- Missing/empty source or out-of-range offset → `path:line:col: message`
  fallback; renderer never panics.
- Warnings render in the warning color, counted separately, and do not change
  exit codes.
- adopt mode never overwrites; a write failure on one file aborts with a clear
  message and leaves others as-is.

## Testing

- **Renderer unit/golden** (`internal/diagframe`): error, warning, multi-spot,
  color vs `NO_COLOR`, tab expansion, multi-line span, `Len==0`, offset at file
  start/end, keyword coloring (both langs), help lines, OSC 8 link on/off.
- **Keyword drift**: diagframe Luau set == `internal/luau` canonical set.
- **Grouping/summary/truncation**: footer counts, sort order, `--max-errors`
  truncation note.
- **Integration goldens**: `rotor minify`/`bundle`/`build` error stderr (change
  by design); watch failure→success transition; `--bell`/`--max-errors`.
- **init-adopt**: existing game/package/plain project → correct skeleton, no
  clobber, idempotent re-run, `--config`; greenfield path unchanged.
- **doctor**: missing / invalid / valid `rotor.toml` rows.
- **Regression**: conformance/diff harness passes (codes/messages and generated
  `.luau` unchanged); `rotor check` output unchanged.

## Files

| File | Change |
|---|---|
| `internal/diagframe/diagframe.go` | new — renderer + `Spot`/`Severity`/`Language`/`Options` + keyword sets |
| `internal/diagframe/group.go` | new — group-by-file, summary footer, truncation |
| `internal/diagframe/*_test.go` | new — golden + edge-case + drift tests |
| `internal/luau/validate.go` | export `IsReservedKeyword` (drift-guard hook) |
| `internal/compile/compile.go` | `DiagnosticInfo` gains location; populate it |
| `internal/compile/project.go` | structured per-file diagnostics + source through `BuildResult` |
| `cmd/rotor/build.go` | structured diags; frames; fill `--json` line/col; `--max-errors` |
| `cmd/rotor/ui.go` | `buildFailure` → grouping layer; summary footer; watch banner states |
| `cmd/rotor/watch.go` | clear-on-rebuild; persistent error banner; `--bell` transitions |
| `cmd/rotor/minify.go` | frame Luau diagnostics |
| `cmd/rotor/bundle.go` / `internal/bundle/bundle.go` | structured parse-error context; frame |
| `internal/pack/luau.go` | frame Luau parse errors |
| `cmd/rotor/init.go` | adopt mode (`--config`, template detection, no-clobber) |
| `cmd/rotor/doctor.go` | `rotor.toml` check |

## Decomposition (implementation plans)

One spec, executed as ordered plans so each lands self-contained and reviewable:

1. **Renderer + Luau wiring** — `diagframe` (render + group/summary/truncate),
   keyword drift guard, then `minify`/`bundle`/`pack` (source already in hand).
   Self-contained; smallest blast radius.
2. **TS plumbing + build/watch** — structured location through compile/build,
   `buildFailure` frames, `--json` line/col, watch frames + transition cues +
   `--max-errors`/`--bell`.
3. **init-adopt + doctor synergy** — independent of diagnostics; can run in
   parallel with (2). Adopt mode, template detection, doctor `rotor.toml` row.

Roadmap + measured-number upkeep per the standing convention after each plan.
