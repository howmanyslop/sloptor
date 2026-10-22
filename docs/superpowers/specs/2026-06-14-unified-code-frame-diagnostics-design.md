# Compiler diagnostics architecture

Retained compiler sections from the original diagnostics design.

## 1. `internal/diagframe` — the renderer

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
