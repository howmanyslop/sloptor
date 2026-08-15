# Diagnostics Plan 2 — TS location plumbing + build/watch frames

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Carry structured source location (file + byte offset + length) through the compile/build pipeline so `rotor build`, watch, and `rotor dev` render TypeScript and transformer/macro errors as `diagframe` code frames, with `--max-errors`, `--json` line/col, and watch transition cues.

**Architecture:** Extend `compile.DiagnosticInfo` with location and resolve it from tsgo `*ast.Diagnostic` and transformer `*ast.Node`. Surface structured diagnostics on `BuildResult` while keeping the existing `[]string` accessors for back-compat. The build CLI groups by file, reads each file's source, and renders via `diagframe.RenderGroups`. Watch reuses that path and adds clear-on-rebuild + a persistent error banner + an optional bell.

**Tech Stack:** Go, tsgo (`tsgo/ast`, `tsgo/scanner`), `internal/compile`, `internal/diagframe` (from Plan 1), `internal/term`.

**Spec:** `docs/superpowers/specs/2026-06-14-unified-code-frame-diagnostics-design.md` (§2, §3, §5). **Depends on Plan 1.**

---

## File structure

- `internal/compile/compile.go` — `DiagnosticInfo` gains `FileName/Offset/Len/Help`; resolvers for node + tsgo diagnostics.
- `internal/compile/project.go` — internal per-file diags become `[]DiagnosticInfo`; structured surface.
- `internal/compile/output.go` — `BuildResult.Diagnostics []DiagnosticInfo`.
- `cmd/rotor/build.go` — `runBuildOnce` returns structured diags; `--max-errors`; `--json` line/col.
- `cmd/rotor/ui.go` — `buildFailure` renders frames via `diagframe`; watch banner states.
- `cmd/rotor/watch.go` — clear-on-rebuild, persistent error banner, `--bell` transitions.

---

## Task 1: `DiagnosticInfo` gains location + resolvers

**Files:**

- Modify: `internal/compile/compile.go:22-26` (struct), add resolvers
- Test: `internal/compile/compile_test.go` (new test)
- [ ] **Step 1: Write the failing test**

```go
func TestTransformerDiagnosticCarriesLocation(t *testing.T) {
	// macros_diag_model has a transformer diagnostic with a known node.
	_, infos, _ := CompileFileDetailed("testdata/macros_diag_model", "src/main.ts")
	if len(infos) == 0 {
		t.Fatal("expected at least one diagnostic")
	}
	d := infos[0]
	if d.FileName == "" {
		t.Errorf("FileName empty")
	}
	if d.Len <= 0 {
		t.Errorf("Len = %d, want > 0", d.Len)
	}
	if d.Offset < 0 {
		t.Errorf("Offset = %d", d.Offset)
	}
}
```

(If `macros_diag_model` does not reliably produce a transformer diagnostic with a node, use `env_diag_model` or `asset_diag_model` — pick the fixture under `internal/compile/testdata/` whose diagnostic has a non-nil node; confirm by reading its `src`.)

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/compile/ -run TestTransformerDiagnosticCarriesLocation -v`
Expected: FAIL — `d.FileName` empty / `Len` is 0 (fields don't exist yet → compile error first).

- [ ] **Step 3: Extend the struct + resolvers**

In `internal/compile/compile.go`, replace the `DiagnosticInfo` struct:

```go
// DiagnosticInfo carries the structured form of a compile diagnostic. Code is
// the upstream-style factory name ("noAny") for transformer diagnostics, or
// "TS####" for TypeScript diagnostics; Message is the primary text. FileName,
// Offset, and Len locate the span for code-frame rendering (Offset/Len are byte
// positions into the file's source; zero Len means "no usable span"). Help holds
// suggestion / more-info lines split off the message for separate rendering.
type DiagnosticInfo struct {
	Code     string
	Message  string
	Warning  bool
	FileName string
	Offset   int
	Len      int
	Help     []string
}
```

Add resolvers (same file). Imports needed: `"rotor/tsgo/scanner"` (ast already imported):

```go
// infoFromNodeDiag builds a located DiagnosticInfo from a transformer
// diagnostic. The token start (leading trivia skipped) and node end give the
// span; the node's source file gives the file name. A nil node yields an
// unlocated info (Offset/Len zero) so rendering falls back to a one-liner.
func infoFromNodeDiag(d transformer.Diagnostic) DiagnosticInfo {
	msg, help := splitHelp(d.Message)
	info := DiagnosticInfo{Code: d.Code, Message: msg, Warning: d.Warning, Help: help}
	if d.Node == nil {
		return info
	}
	sf := ast.GetSourceFileOfNode(d.Node)
	if sf == nil {
		return info
	}
	start := scanner.GetTokenPosOfNode(d.Node, sf, false)
	end := d.Node.End()
	info.FileName = sf.FileName()
	info.Offset = start
	if end > start {
		info.Len = end - start
	}
	return info
}

// infoFromTSDiag builds a located DiagnosticInfo from a tsgo diagnostic.
func infoFromTSDiag(d *ast.Diagnostic) DiagnosticInfo {
	info := DiagnosticInfo{
		Code:    fmt.Sprintf("TS%d", d.Code()),
		Message: d.String(),
		Warning: d.Category() == diagnostics.CategoryWarning,
	}
	if f := d.File(); f != nil {
		info.FileName = f.FileName()
		info.Offset = d.Pos()
		info.Len = d.Len()
	}
	return info
}

// splitHelp separates suggestion / more-info tail lines (transformer messages
// join parts with "\n"; the first line is the primary message, "Suggestion:" and
// "More information:" lines become help). Reconstructable: strings.Join of
// [msg]+help with "\n" equals the original.
func splitHelp(message string) (msg string, help []string) {
	parts := strings.Split(message, "\n")
	msg = parts[0]
	for _, p := range parts[1:] {
		if strings.HasPrefix(p, "Suggestion: ") || strings.HasPrefix(p, "More information: ") {
			help = append(help, p)
		} else {
			// Non-suggestion continuation lines stay on the primary message.
			msg += "\n" + p
		}
	}
	return msg, help
}
```

Add the `diagnostics` import (`"rotor/tsgo/diagnostics"`) if not present.

Now use the resolvers. Replace `tsDiagnosticInfos` body:

```go
func tsDiagnosticInfos(diags []*ast.Diagnostic) []DiagnosticInfo {
	out := make([]DiagnosticInfo, len(diags))
	for i, d := range diags {
		out[i] = infoFromTSDiag(d)
	}
	return out
}
```

Replace the transformer-diag loop in `transformAndRenderDetailed` (compile.go:138-144):

```go
	for _, d := range state.Diags.Flush() {
		diags = append(diags, infoFromNodeDiag(d))
	}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./internal/compile/ -run TestTransformerDiagnosticCarriesLocation -v`
Expected: PASS.

- [ ] **Step 5: Confirm message-only accessor unchanged + commit**

`diagnosticInfoMessages` must still return the same strings existing tests expect. Because `splitHelp` keeps suggestion lines *out* of `Message`, the flattened message changes for diagnostics that had suggestions. To preserve byte-identical message output, make `diagnosticInfoMessages` re-join help:

```go
func diagnosticInfoMessages(diags []DiagnosticInfo) []string {
	out := make([]string, len(diags))
	for i, d := range diags {
		msg := d.Message
		for _, h := range d.Help {
			msg += "\n" + h
		}
		out[i] = msg
	}
	return out
}
```

Run: `go test ./internal/compile/ ./internal/conformance/ ./internal/diff/`
Expected: PASS (messages/codes unchanged).

```bash
git add internal/compile/compile.go internal/compile/compile_test.go
git commit -m "feat(compile): DiagnosticInfo carries source location + help split"
```

---

## Task 2: structured diags through the project path

**Files:**

- Modify: `internal/compile/project.go` (`compiledProjectSourceFile`, `compileProjectSourceFiles`, `compileProjectSourceFile`, `compileProjectProgram`)
- Modify: `internal/compile/output.go` (`BuildResult`, `BuildProjectWithOptions`)
- Test: `internal/compile/project_test.go` (new)
- [ ] **Step 1: Write the failing test**

```go
func TestBuildResultCarriesStructuredDiagnostics(t *testing.T) {
	// a project fixture with a transformer diagnostic
	res, _, err := BuildProjectWithOptions("testdata/macros_diag_model", ProjectOptions{})
	_ = err // build returns an error when diagnostics are present
	if res == nil {
		t.Fatal("nil BuildResult")
	}
	if len(res.Diagnostics) == 0 {
		t.Fatal("BuildResult.Diagnostics empty")
	}
	if res.Diagnostics[0].FileName == "" {
		t.Error("structured diagnostic missing FileName")
	}
}
```

NOTE: `BuildProjectWithOptions` currently returns `nil` BuildResult on diagnostics (it returns the `[]string` and error). This task changes it to always return a `*BuildResult` whose `Diagnostics` is populated even on failure. Adapt the assertion once the signature is confirmed; if changing the nil-on-error contract is too broad, instead add `BuildProjectDetailed` returning `(*BuildResult, error)` and assert on that. Prefer extending `BuildResult` + always returning it.

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/compile/ -run TestBuildResultCarriesStructuredDiagnostics -v`
Expected: FAIL — `res.Diagnostics` undefined / nil result.

- [ ] **Step 3: Change internal diag type to structured**

In `internal/compile/project.go`:

Change `compiledProjectSourceFile.diags` from `[]string` to `[]DiagnosticInfo`:

```go
type compiledProjectSourceFile struct {
	relOut string
	text   string
	diags  []DiagnosticInfo
	err    error
}
```

In `compileProjectSourceFile`, use the detailed transform and the structured macro-missing path:

```go
		if missing := state.Macros().Missing(); len(missing) > 0 {
			result.diags = stringDiagnostics(missing) // already []DiagnosticInfo (Message-only)
			result.err = errors.New("compile: macro registration failure")
			return
		}
		// ...
		text, diags, err := transformAndRenderDetailed(state)
		if err != nil {
			result.err = err
			return
		}
		if len(diags) > 0 {
			result.diags = diags
```

In `compileProjectSourceFiles`, the precheck surfacing must carry location. Replace the early returns that used `diagnosticStrings(...)`:

```go
	for _, precheck := range prechecks {
		if len(precheck.tsDiags) > 0 {
			return nil, tsDiagnosticInfos(precheck.tsDiags), errors.New("compile: TypeScript diagnostics")
		}
		if len(precheck.commentDiags) > 0 {
			return nil, stringDiagnostics(precheck.commentDiags), errors.New("compile: comment directive diagnostics")
		}
	}
	if tsDiags := program.GetGlobalDiagnostics(ctx); len(tsDiags) > 0 {
		return nil, tsDiagnosticInfos(tsDiags), errors.New("compile: TypeScript diagnostics")
	}
```

This requires `compileProjectSourceFiles` to return `[]DiagnosticInfo` instead of `[]string`. Change its signature and the aggregation:

```go
func compileProjectSourceFiles(...) (map[string]string, []DiagnosticInfo, error) {
	// program option diagnostics:
	if tsDiags := program.GetProgramDiagnostics(); len(tsDiags) > 0 {
		return nil, tsDiagnosticInfos(tsDiags), errors.New("compile: TypeScript diagnostics")
	}
	// ... per-file results:
	for _, result := range results {
		if result.err != nil {
			return nil, result.diags, result.err
		}
		outputs[result.relOut] = result.text
	}
	return outputs, nil, nil
}
```

Propagate the type change up through `compileProjectProgram` and `CompileProjectWithOptions` internals: the **public** `CompileProjectWithOptions`/`CompileProject` keep returning `[]string` by flattening with `diagnosticInfoMessages` at the boundary:

```go
func CompileProjectWithOptions(projectDir string, opts ProjectOptions) (map[string]string, []string, error) {
	outputs, infos, err := compileProjectDetailed(projectDir, opts)
	return outputs, diagnosticInfoMessages(infos), err
}

// compileProjectDetailed is the structured-diagnostic core.
func compileProjectDetailed(projectDir string, opts ProjectOptions) (map[string]string, []DiagnosticInfo, error) {
	dir, program, diags, err := newProjectProgram(projectDir, opts.TsConfigPath)
	if err != nil {
		return nil, stringDiagnostics(diags), err
	}
	if err := maybeCopyInclude(dir, opts); err != nil {
		return nil, nil, err
	}
	return compileProjectProgramDetailed(dir, program, opts)
}
```

(Rename the current `compileProjectProgram` internals to `…Detailed` returning `[]DiagnosticInfo`; keep `newProjectContext`/`newProjectProgram` returning `[]string` and wrap with `stringDiagnostics` where they surface — those are config/validation diagnostics without a node and render fine as one-liners.)

- [ ] **Step 4: Surface on BuildResult**

In `internal/compile/output.go`, add to `BuildResult`:

```go
	// Diagnostics holds the structured diagnostics from this build (populated
	// even when the build fails). Empty on success.
	Diagnostics []DiagnosticInfo
```

In `BuildProjectWithOptions`, capture the detailed diags and always return a `*BuildResult` carrying them. Locate where it calls the project compile and the emit; populate `result.Diagnostics = infos` and, on diagnostics, return `(result, diagnosticInfoMessages(infos), err)` instead of `(nil, …)`.

- [ ] **Step 5: Run test + regression**

Run: `go test ./internal/compile/ ./internal/conformance/ ./internal/diff/ ./cmd/rotor/`
Expected: PASS. (Generated `.luau` and message strings unchanged; only diagnostics gain structure.)

```bash
git add internal/compile/project.go internal/compile/output.go internal/compile/project_test.go
git commit -m "feat(compile): thread structured diagnostics through build; BuildResult.Diagnostics"
```

---

## Task 3: build CLI renders frames

**Files:**

- Modify: `cmd/rotor/build.go` (`runBuildOnce`, build command flow, `--json`, `--max-errors`)
- Modify: `cmd/rotor/ui.go` (`buildFailure`)
- Test: `cmd/rotor/build_test.go`
- [ ] **Step 1: Write the failing test**

```go
func TestBuild_TransformerErrorShowsFrame(t *testing.T) {
	dir := copyFixture(t, "testdata/macros_diag_model") // adapt to existing fixture-copy helper
	var stderr bytes.Buffer
	code := runBuildCmd([]string{dir}, &stderr) // adapt to actual build entry/IO seam
	if code == 0 {
		t.Fatal("expected non-zero exit")
	}
	out := stderr.String()
	if !strings.Contains(out, "-->") || !strings.Contains(out, "^") {
		t.Errorf("expected a code frame:\n%s", out)
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./cmd/rotor/ -run TestBuild_TransformerErrorShowsFrame -v`
Expected: FAIL — output is bare indented messages, no `-->`/caret.

- [ ] **Step 3: Render structured diags**

Add a `diagFrames` helper in `cmd/rotor/ui.go` and rewrite `buildFailure` to accept structured diagnostics:

```go
// buildFailure prints the failure headline followed by a code frame per file.
func (u *ui) buildFailure(headline string, diags []compile.DiagnosticInfo, maxErrors int) {
	g := u.s.Glyphs()
	fmt.Fprintf(u.w, "  %s  %s\n", u.s.ErrorBold(g.Cross), u.s.Bold(headline))
	if len(diags) == 0 {
		fmt.Fprintln(u.w)
		return
	}
	fmt.Fprintln(u.w)
	fmt.Fprint(u.w, renderDiagFrames(u.w, diags, maxErrors))
	fmt.Fprintln(u.w)
}

// renderDiagFrames groups located diagnostics by file, reads each file's source,
// and renders frames; diagnostics without a usable file fall back to one-liners.
func renderDiagFrames(w io.Writer, diags []compile.DiagnosticInfo, maxErrors int) string {
	color := term.ColorEnabled(w)
	byFile := map[string][]diagframe.Spot{}
	var order []string
	var loose []string
	for _, d := range diags {
		sev := diagframe.Error
		if d.Warning {
			sev = diagframe.Warning
		}
		if d.FileName == "" || d.Len == 0 {
			loose = append(loose, fmt.Sprintf("%s: %s", d.Code, d.Message))
			continue
		}
		if _, ok := byFile[d.FileName]; !ok {
			order = append(order, d.FileName)
		}
		byFile[d.FileName] = append(byFile[d.FileName], diagframe.Spot{
			Offset: d.Offset, Len: d.Len, Severity: sev, Code: d.Code, Message: d.Message, Help: d.Help,
		})
	}
	var groups []diagframe.Group
	for _, fn := range order {
		src, _ := os.ReadFile(fn)
		groups = append(groups, diagframe.Group{
			Path: relForDisplay(fn), Source: string(src), Lang: diagframe.TypeScript, Spots: byFile[fn],
		})
	}
	out := diagframe.RenderGroups(groups, diagframe.Options{Color: color, Link: color}, maxErrors)
	for _, l := range loose {
		out += "    " + l + "\n"
	}
	return out
}

// relForDisplay shows a path relative to the working directory when possible.
func relForDisplay(p string) string {
	if cwd, err := os.Getwd(); err == nil {
		if rel, err := filepath.Rel(cwd, p); err == nil {
			return filepath.ToSlash(rel)
		}
	}
	return p
}
```

Imports for `ui.go`: `"io"`, `"os"`, `"path/filepath"`, `"rotor/internal/compile"`, `"rotor/internal/diagframe"`, `"rotor/internal/term"`.

- [ ] **Step 4: Return structured diags from `runBuildOnce` + thread `--max-errors`**

Change `runBuildOnce` to return `[]compile.DiagnosticInfo` (from `result.Diagnostics`) instead of `[]string`:

```go
func runBuildOnce(dir, tsConfigPath string, opts projectOptions) (*compile.BuildResult, []compile.DiagnosticInfo, time.Duration, error) {
	start := time.Now()
	result, _, err := compile.BuildProjectWithOptions(dir, compile.ProjectOptions{ /* unchanged */ })
	var diags []compile.DiagnosticInfo
	if result != nil {
		diags = result.Diagnostics
	}
	return result, diags, time.Since(start), err
}
```

Update the two call sites (one-shot build flow and `reportBuildPass` in watch.go) to pass `diags` (now structured) into `buildFailure(err.Error(), diags, maxErrors)`. Add a `--max-errors N` flag parsed in the build command (default 50, 0 = unlimited); store on `projectOptions` or a local and pass through.

- [ ] **Step 5: `--json` real line/col**

In the `--json` path, replace the message-only loop. For each `compile.DiagnosticInfo` with a `FileName`, compute line/col from the file source (reuse a small helper or read the file and count newlines to `Offset`) and populate `jsonDiagnostic{File, Line, Col, Message, Severity}`. Drop the outdated comment at build.go:322-326.

- [ ] **Step 6: Run tests + commit**

Run: `go test ./cmd/rotor/ -run 'TestBuild' -v` then `go test ./cmd/rotor/`
Expected: PASS (update any changed build/json goldens deliberately).

```bash
git add cmd/rotor/build.go cmd/rotor/ui.go cmd/rotor/build_test.go
git commit -m "feat(build): code frames for TS + transformer errors; --max-errors; --json line/col"
```

---

## Task 4: watch frames + transition cues

**Files:**

- Modify: `cmd/rotor/watch.go` (`reportBuildPass`, rebuild entry, flags)
- Modify: `cmd/rotor/ui.go` (watch banner states)
- Test: `cmd/rotor/watch_test.go`
- [ ] **Step 1: Write the failing test**

```go
func TestWatch_PersistentErrorBannerAndTransition(t *testing.T) {
	u, buf := newTestUI() // adapt to existing ui test constructor
	stats := &watchStats{}
	// failing build -> banner shows error count
	reportBuildPass(u, nil, []compile.DiagnosticInfo{{Code: "noAny", Message: "x", FileName: "", Len: 0}}, 0, errors.New("compile: TypeScript diagnostics"), stats)
	if !strings.Contains(buf.String(), "watching for changes") {
		t.Error("missing idle line")
	}
	if !strings.Contains(buf.String(), "error") {
		t.Error("expected error count in banner after failure")
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./cmd/rotor/ -run TestWatch_PersistentErrorBannerAndTransition -v`
Expected: FAIL — banner has no error state.

- [ ] **Step 3: Track last-build state + banner**

Add a `lastFailed bool` (and `lastErrCount int`) to `watchStats`. In `reportBuildPass`, set them, and make `watchIdle` render `✗ N errors — watching for changes` when `stats.lastFailed`, else the normal idle line. On a fail→pass or pass→fail flip, when `--bell` is set and output is a TTY, write `"\a"`.

```go
func reportBuildPass(u *ui, result *compile.BuildResult, diags []compile.DiagnosticInfo, elapsed time.Duration, err error, stats *watchStats) {
	failed := err != nil
	if failed != stats.lastFailed && stats.bell && isTerminal(os.Stderr) {
		fmt.Fprint(os.Stderr, "\a")
	}
	stats.lastFailed = failed
	if failed {
		stats.lastErrCount = len(diags)
		newUI(os.Stderr).buildFailure(err.Error(), diags, stats.maxErrors)
	} else if result != nil {
		// ... existing success note-lines + buildSuccess ...
	}
	u.watchIdle(stats)
}
```

- [ ] **Step 4: Clear-on-rebuild (TTY only)**

At the start of each rebuild (the rebuild entry in watch.go, before `runBuildOnce`), when stdout is a TTY and not disabled, print the clear sequence; otherwise print a separator. Gate behind a `--no-clear` opt-out:

```go
func clearForRebuild(w io.Writer) {
	if term.ColorEnabled(w) { // proxy for "interactive TTY"
		fmt.Fprint(w, "\x1b[2J\x1b[H")
	}
}
```

- [ ] **Step 5: Flags**

Parse `--bell` and `--no-clear` in the watch/dev command setup; store `bell`, `maxErrors`, `noClear` on `watchStats` (or a small watch-config struct threaded into `reportBuildPass`). Default: bell off, clear on, maxErrors 50.

- [ ] **Step 6: Run tests + commit**

Run: `go test ./cmd/rotor/ -run 'TestWatch' -v` then `go test ./cmd/rotor/`
Expected: PASS (update watch goldens deliberately).

```bash
git add cmd/rotor/watch.go cmd/rotor/ui.go cmd/rotor/watch_test.go
git commit -m "feat(watch): code frames + persistent error banner + clear-on-rebuild + --bell"
```

---

## Task 5: full regression + roadmap

- [ ] **Step 1: Whole suite**

Run: `go test ./...`
Expected: PASS. Validate via the golang Docker container per the project's CI-validation note (native Windows agent-ci is broken).

- [ ] **Step 2: Manual smoke**

```bash
go run ./cmd/rotor build internal/compile/testdata/macros_diag_model
```

Expected: a framed TS/transformer error with `-->`, source line, caret, and a `✗ N errors in M files` footer.

- [ ] **Step 3: Roadmap**

Tick the build/watch diagnostics items in `roadmap.md`; record Plan 2 complete with measured test counts.

```bash
git add roadmap.md
git commit -m "docs(roadmap): TS build/watch code frames complete (plan 2)"
```

---

## Self-review notes (author)

- Spec §3 (TS plumbing) → Tasks 1–2; §2 (max-errors/summary in build) → Task 3; §5 (watch frames + cues) → Task 4.
- Type consistency: `compile.DiagnosticInfo{Code,Message,Warning,FileName,Offset,Len,Help}`; `BuildResult.Diagnostics`; `runBuildOnce(...) (*BuildResult, []DiagnosticInfo, time.Duration, error)`; `buildFailure(headline string, diags []compile.DiagnosticInfo, maxErrors int)`; `renderDiagFrames(w, diags, maxErrors)`. These align with Plan 1's `diagframe.Spot/Group/RenderGroups/Options`.
- Byte-parity guard: public `CompileFile`/`CompileProject*` keep `[]string` returns via `diagnosticInfoMessages`, which re-joins `Help` so messages are byte-identical; conformance/diff suites are the gate (Task 2 Step 5, Task 5 Step 1).
- "adapt to existing harness" notes in Tasks 1/3/4 depend on unseen test-helper shapes (`copyFixture`, `runBuildCmd`, `newTestUI`); the implementer mirrors the existing `cmd/rotor` test seams.
