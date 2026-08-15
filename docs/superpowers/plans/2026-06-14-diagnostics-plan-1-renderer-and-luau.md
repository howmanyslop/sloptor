# Diagnostics Plan 1 — `diagframe` renderer + Luau wiring

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the language-agnostic code-frame renderer (`internal/diagframe`) and wire it into the Luau commands (`minify`, `bundle`, `pack`) so their parse/lex errors show source context with a caret and highlighted keywords.

**Architecture:** A pure renderer takes a source string + byte-offset "spots" + a `Language` and returns a framed, optionally-colored block. A thin grouping layer adds per-file grouping, a summary footer, and `--max-errors` truncation. Luau callers already hold the source string and `[]cst.Diagnostic`, so they map directly to `Spot`s.

**Tech Stack:** Go, `internal/term` (color gating + glyphs + OSC 8 links), `internal/luau/cst` + `internal/luau/lex` (Luau diagnostics), Go `testing`.

**Spec:** `docs/superpowers/specs/2026-06-14-unified-code-frame-diagnostics-design.md` (§1, §2, §4).

---

## File structure

- `internal/diagframe/diagframe.go` — `Spot`, `Severity`, `Language`, `Options`, `Render`, keyword sets, position math.
- `internal/diagframe/group.go` — `Group` (per-file buckets), `RenderGroups`, summary footer, truncation.
- `internal/diagframe/diagframe_test.go` — renderer golden + edge-case tests.
- `internal/diagframe/group_test.go` — grouping/summary/truncation tests.
- `internal/diagframe/keywords_drift_test.go` — Luau keyword-set drift guard.
- `internal/luau/validate.go` — export `IsReservedKeyword`.
- `cmd/rotor/minify.go` — render frames for `cst.Minify` diagnostics.
- `cmd/rotor/bundle.go` + `internal/bundle/bundle.go` — structured parse-error context, frame at CLI.
- `internal/pack/luau.go` — frame Luau parse errors in the failure message.

---

## Task 1: `diagframe` core types + position math

**Files:**

- Create: `internal/diagframe/diagframe.go`
- Test: `internal/diagframe/diagframe_test.go`
- [ ] **Step 1: Write the failing test**

```go
package diagframe

import "testing"

func TestLineColAt(t *testing.T) {
	src := "ab\ncde\nf"
	cases := []struct {
		off, wantLine, wantCol int
	}{
		{0, 1, 1},  // 'a'
		{1, 1, 2},  // 'b'
		{3, 2, 1},  // 'c'
		{6, 2, 4},  // newline after "cde" -> col past 'e'
		{7, 3, 1},  // 'f'
		{99, 3, 2}, // clamp past end
	}
	for _, c := range cases {
		line, col, _ := lineColAt(src, c.off)
		if line != c.wantLine || col != c.wantCol {
			t.Errorf("offset %d: got %d:%d, want %d:%d", c.off, line, col, c.wantLine, c.wantCol)
		}
	}
}

func TestLineTextStripsCR(t *testing.T) {
	src := "x = 1\r\ny = 2\r\n"
	got := lineText(src, 1)
	if got != "x = 1" {
		t.Errorf("lineText line 1 = %q, want %q", got, "x = 1")
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/diagframe/ -run 'TestLineColAt|TestLineTextStripsCR' -v`
Expected: FAIL — `undefined: lineColAt`, `undefined: lineText`.

- [ ] **Step 3: Write the types + position helpers**

```go
// Package diagframe renders a compiler/lexer diagnostic as a source "code
// frame": the offending line(s) with a gutter, a severity-colored
// caret/underline, and reserved keywords highlighted. It is language-agnostic —
// callers pass byte offsets into the source plus a Language selecting the
// reserved-keyword set. Output is plain ASCII (byte-stable) unless Options.Color.
package diagframe

import (
	"strings"
)

// Severity selects the headline word and caret color.
type Severity int

const (
	Error Severity = iota
	Warning
)

// Language selects which reserved-keyword set is highlighted.
type Language int

const (
	Luau Language = iota
	TypeScript
)

// Spot is one diagnostic anchored to a byte span of the source.
type Spot struct {
	Offset   int      // byte offset of the span start into the source
	Len      int      // span length in bytes; the caret count is max(1, Len-on-line)
	Severity Severity
	Code     string   // optional, shown after the message: "TS2322", "noAny", ""
	Message  string   // primary message (no suggestion/more-info tail)
	Help     []string // suggestion / "More information:" lines, rendered as `help:`
}

// Options controls presentation. Color and Link are decided by the caller from
// the destination writer (term.ColorEnabled); with Color false the output is
// plain ASCII and byte-stable.
type Options struct {
	Context int  // context lines above/below the primary line (default 1)
	Color   bool // emit ANSI styling
	Link    bool // emit OSC 8 hyperlink on the locator (caller: Color && wanted)
}

// lineColAt returns the 1-based line and column (byte column) for a byte offset,
// plus the 0-based index of the line's first byte. The offset is clamped to
// [0, len(src)].
func lineColAt(src string, offset int) (line, col, lineStart int) {
	if offset < 0 {
		offset = 0
	}
	if offset > len(src) {
		offset = len(src)
	}
	line = 1
	lineStart = 0
	for i := 0; i < offset; i++ {
		if src[i] == '\n' {
			line++
			lineStart = i + 1
		}
	}
	return line, offset - lineStart + 1, lineStart
}

// lineText returns the text of the given 1-based line, with any trailing CR
// stripped (so CRLF sources render cleanly). Returns "" when line is out of
// range.
func lineText(src string, line int) string {
	if line < 1 {
		return ""
	}
	cur := 1
	start := 0
	for i := 0; i < len(src); i++ {
		if cur == line {
			break
		}
		if src[i] == '\n' {
			cur++
			start = i + 1
		}
	}
	if cur != line {
		return ""
	}
	end := start
	for end < len(src) && src[end] != '\n' {
		end++
	}
	return strings.TrimSuffix(src[start:end], "\r")
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./internal/diagframe/ -run 'TestLineColAt|TestLineTextStripsCR' -v`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add internal/diagframe/diagframe.go internal/diagframe/diagframe_test.go
git commit -m "feat(diagframe): core types + line/col position math"
```

---

## Task 2: keyword sets + highlighting

**Files:**

- Modify: `internal/diagframe/diagframe.go`
- Test: `internal/diagframe/diagframe_test.go`
- [ ] **Step 1: Write the failing test**

```go
func TestHighlightKeywords_Luau(t *testing.T) {
	got := highlightKeywords("local function f()", Luau, styler(true))
	// `local` and `function` are colored; `f` is not.
	if !strings.Contains(got, "\x1b[") {
		t.Fatalf("expected ANSI codes, got %q", got)
	}
	if strings.Contains(stripANSI(got), "local function f()") == false {
		t.Errorf("visible text changed: %q", stripANSI(got))
	}
}

func TestHighlightKeywords_NoColorIsIdentity(t *testing.T) {
	in := "const x = 1"
	if got := highlightKeywords(in, TypeScript, styler(false)); got != in {
		t.Errorf("no-color highlight changed text: %q", got)
	}
}

func TestKeywordSets(t *testing.T) {
	if !isKeyword("function", Luau) || !isKeyword("function", TypeScript) {
		t.Error("`function` should be a keyword in both")
	}
	if isKeyword("print", Luau) {
		t.Error("`print` is a global, not a reserved keyword")
	}
	if !isKeyword("const", TypeScript) || isKeyword("const", Luau) {
		t.Error("`const` is TS-only")
	}
}
```

Add test helpers at the bottom of the test file:

```go
func styler(color bool) *term.Styler { // term import added to test file
	if color {
		return term.For(forceColorWriter{})
	}
	return term.For(plainWriter{})
}

type forceColorWriter struct{}

func (forceColorWriter) Write(p []byte) (int, error) { return len(p), nil }

type plainWriter struct{}

func (plainWriter) Write(p []byte) (int, error) { return len(p), nil }

func stripANSI(s string) string {
	var b strings.Builder
	inEsc := false
	for _, r := range s {
		switch {
		case inEsc:
			if r == 'm' {
				inEsc = false
			}
		case r == '\x1b':
			inEsc = true
		default:
			b.WriteRune(r)
		}
	}
	return b.String()
}
```

NOTE: `term.For` decides color from the writer, not a bool. To force color on/off deterministically in tests, do not rely on `term.For`; instead construct the styler via a test seam. Replace the `styler` helper with direct construction once Step 3 exposes `stylerFor(color bool)`:

```go
func styler(color bool) *term.Styler { return stylerFor(color) }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/diagframe/ -run 'TestHighlightKeywords|TestKeywordSets' -v`
Expected: FAIL — `undefined: highlightKeywords`, `isKeyword`, `stylerFor`.

- [ ] **Step 3: Implement keyword sets + highlighting**

Append to `internal/diagframe/diagframe.go`:

```go
import "rotor/internal/term" // add to the import block

// luauKeywords is the 21 Luau reserved words (mirrors internal/luau's canonical
// set; the keywords_drift_test guards against divergence).
var luauKeywords = map[string]struct{}{
	"and": {}, "break": {}, "do": {}, "else": {}, "elseif": {}, "end": {},
	"false": {}, "for": {}, "function": {}, "if": {}, "in": {}, "local": {},
	"nil": {}, "not": {}, "or": {}, "repeat": {}, "return": {}, "then": {},
	"true": {}, "until": {}, "while": {},
}

// tsKeywords is the TypeScript/JavaScript reserved + contextual keyword set used
// for highlighting (presentation only — not a parser).
var tsKeywords = map[string]struct{}{
	"abstract": {}, "any": {}, "as": {}, "asserts": {}, "async": {}, "await": {},
	"boolean": {}, "break": {}, "case": {}, "catch": {}, "class": {}, "const": {},
	"continue": {}, "debugger": {}, "declare": {}, "default": {}, "delete": {},
	"do": {}, "else": {}, "enum": {}, "export": {}, "extends": {}, "false": {},
	"finally": {}, "for": {}, "from": {}, "function": {}, "if": {}, "implements": {},
	"import": {}, "in": {}, "infer": {}, "instanceof": {}, "interface": {},
	"keyof": {}, "let": {}, "namespace": {}, "never": {}, "new": {}, "null": {},
	"number": {}, "object": {}, "of": {}, "private": {}, "protected": {},
	"public": {}, "readonly": {}, "return": {}, "static": {}, "string": {},
	"super": {}, "switch": {}, "this": {}, "throw": {}, "true": {}, "try": {},
	"type": {}, "typeof": {}, "undefined": {}, "unknown": {}, "var": {}, "void": {},
	"while": {}, "yield": {},
}

func isKeyword(word string, lang Language) bool {
	if lang == TypeScript {
		_, ok := tsKeywords[word]
		return ok
	}
	_, ok := luauKeywords[word]
	return ok
}

func isIdentByte(c byte) bool {
	return c == '_' || (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') || (c >= '0' && c <= '9')
}

// highlightKeywords colors reserved words in one line of source. A no-color
// styler returns the line unchanged (identity), so uncolored output is
// byte-stable. The scan is word-boundary based (keywords inside strings/comments
// may be colored — accepted for the keywords-only tier).
func highlightKeywords(line string, lang Language, s *term.Styler) string {
	if !s.Color() {
		return line
	}
	var b strings.Builder
	i := 0
	for i < len(line) {
		c := line[i]
		if c == '_' || (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') {
			j := i + 1
			for j < len(line) && isIdentByte(line[j]) {
				j++
			}
			word := line[i:j]
			if isKeyword(word, lang) {
				b.WriteString(s.Accent(word))
			} else {
				b.WriteString(word)
			}
			i = j
			continue
		}
		b.WriteByte(c)
		i++
	}
	return b.String()
}

// stylerFor builds a Styler with color forced on or off (test seam + the
// renderer's single source of truth for the Options.Color decision).
func stylerFor(color bool) *term.Styler {
	if color {
		return term.For(term.ForceColorWriter{})
	}
	return term.For(term.PlainWriter{})
}
```

Then add the two seam writers to `internal/term/term.go` (exported so other packages can force a styler):

```go
// ForceColorWriter / PlainWriter are zero-cost writers used to construct a
// Styler with color forced on or off without consulting the environment. They
// discard all writes; only their identity matters to ColorEnabled.
type ForceColorWriter struct{}

func (ForceColorWriter) Write(p []byte) (int, error) { return len(p), nil }

type PlainWriter struct{}

func (PlainWriter) Write(p []byte) (int, error) { return len(p), nil }
```

And make `ColorEnabled` treat them specially at the top:

```go
func ColorEnabled(w io.Writer) bool {
	switch w.(type) {
	case ForceColorWriter:
		return true
	case PlainWriter:
		return false
	}
	// ... existing NO_COLOR / FORCE_COLOR / char-device logic unchanged ...
}
```

Remove the duplicate `styler`/writer/`stripANSI` helpers from the test that Step 1 added if they now conflict; keep only `styler` delegating to `stylerFor`, and keep `stripANSI`.

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./internal/diagframe/ ./internal/term/ -run 'TestHighlightKeywords|TestKeywordSets|TestColor' -v`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add internal/diagframe/diagframe.go internal/diagframe/diagframe_test.go internal/term/term.go
git commit -m "feat(diagframe): Luau/TS keyword sets + keyword highlighting"
```

---

## Task 3: `Render` — the frame

**Files:**

- Modify: `internal/diagframe/diagframe.go`
- Test: `internal/diagframe/diagframe_test.go`
- [ ] **Step 1: Write the failing test**

```go
func TestRender_BasicNoColor(t *testing.T) {
	src := "local function f()\n  return print(1 2)\n"
	// offset of the space between `1` and `2` on line 2:
	off := strings.Index(src, "1 2") + 1
	got := Render("entry.luau", src, Luau, []Spot{{
		Offset: off, Len: 1, Severity: Error, Message: "expected ')'",
	}}, Options{Context: 1, Color: false})

	want := "" +
		"error: expected ')'\n" +
		"  --> entry.luau:2:16\n" +
		"   |\n" +
		" 1 | local function f()\n" +
		" 2 |   return print(1 2)\n" +
		"   |                ^ expected ')'\n"
	if got != want {
		t.Errorf("frame mismatch:\n--- got ---\n%s\n--- want ---\n%s", got, want)
	}
}

func TestRender_HelpLines(t *testing.T) {
	src := "x = 1\n"
	got := Render("a.luau", src, Luau, []Spot{{
		Offset: 0, Len: 1, Severity: Error, Message: "bad", Help: []string{"do this instead"},
	}}, Options{Color: false})
	if !strings.Contains(got, "help: do this instead") {
		t.Errorf("missing help line:\n%s", got)
	}
}

func TestRender_EmptySourceFallsBack(t *testing.T) {
	got := Render("a.luau", "", Luau, []Spot{{Offset: 0, Message: "boom", Severity: Error}}, Options{})
	if got != "a.luau:1:1: boom\n" {
		t.Errorf("fallback = %q", got)
	}
}

func TestRender_TabExpansionCaretAligns(t *testing.T) {
	src := "\tbad\n" // one leading tab
	got := Render("a.luau", src, Luau, []Spot{{Offset: 1, Len: 3, Severity: Error, Message: "x"}}, Options{Color: false})
	// tab expands to 4 spaces; caret sits under `bad` => 4 spaces after the gutter pipe + space.
	if !strings.Contains(got, "   |     ^^^ x") {
		t.Errorf("caret not aligned after tab expansion:\n%s", got)
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/diagframe/ -run TestRender -v`
Expected: FAIL — `undefined: Render`.

- [ ] **Step 3: Implement `Render`**

Append to `internal/diagframe/diagframe.go`:

```go
import (
	"fmt"
	"strconv"
	// strings already imported; term already imported
)

const tabWidth = 4

// Render returns the framed block for one file's spots. Pure: no I/O.
func Render(path, source string, lang Language, spots []Spot, o Options) string {
	if o.Context == 0 {
		o.Context = 1
	}
	s := stylerFor(o.Color)
	var b strings.Builder
	for _, sp := range spots {
		renderSpot(&b, path, source, lang, sp, o, s)
	}
	return b.String()
}

func renderSpot(b *strings.Builder, path, source string, lang Language, sp Spot, o Options, s *term.Styler) {
	line, col, _ := lineColAt(source, sp.Offset)
	primary := lineText(source, line)
	// Fallback: no usable source line -> one-liner.
	if source == "" || primary == "" && line > countLines(source) {
		fmt.Fprintf(b, "%s:%d:%d: %s\n", path, line, col, sp.Message)
		return
	}

	sevWord, paint := "error", s.Error
	if sp.Severity == Warning {
		sevWord, paint = "warning", s.Warn
	}

	// Headline.
	head := sevWord + ": " + sp.Message
	if sp.Code != "" {
		head = sevWord + "[" + sp.Code + "]: " + sp.Message
	}
	fmt.Fprintf(b, "%s\n", paint(head))

	// Locator (optionally an OSC 8 link).
	loc := fmt.Sprintf("%s:%d:%d", path, line, col)
	if o.Link {
		loc = s.Hyperlink(fmt.Sprintf("file://%s", path), loc)
	}
	fmt.Fprintf(b, "  %s %s\n", s.Muted("-->"), loc)

	gutterW := len(strconv.Itoa(line + o.Context))
	pad := strings.Repeat(" ", gutterW)
	bar := s.Muted("|")
	fmt.Fprintf(b, "%s %s\n", pad, bar)

	for ln := line - o.Context; ln <= line+o.Context; ln++ {
		if ln < 1 {
			continue
		}
		txt := lineText(source, ln)
		if ln > countLines(source) {
			break
		}
		expanded := expandTabs(txt)
		num := s.Muted(fmt.Sprintf("%*d", gutterW, ln))
		fmt.Fprintf(b, "%s %s %s\n", num, bar, highlightKeywords(expanded, lang, s))
		if ln == line {
			caretCol := expandedCol(txt, col) // 1-based visual column
			n := sp.Len
			if n < 1 {
				n = 1
			}
			// Cap caret run at end of the visible line.
			if maxN := len([]rune(expanded)) - (caretCol - 1); n > maxN && maxN > 0 {
				n = maxN
			}
			caret := strings.Repeat(" ", caretCol-1) + paint(strings.Repeat("^", n))
			fmt.Fprintf(b, "%s %s %s %s\n", pad, bar, caret, paint(""))
			// The trailing paint("") is empty; message rides on the caret line:
		}
	}
	// Actually render the caret line's trailing message + help below (see Step 3b).
}
```

NOTE: the inline message on the caret line is added in Step 3b to keep the caret-building readable. Add helpers:

```go
func countLines(src string) int {
	n := 1
	for i := 0; i < len(src); i++ {
		if src[i] == '\n' {
			n++
		}
	}
	// A trailing newline does not create a real extra line for display.
	if strings.HasSuffix(src, "\n") {
		n--
	}
	if n < 1 {
		n = 1
	}
	return n
}

func expandTabs(s string) string {
	if !strings.ContainsRune(s, '\t') {
		return s
	}
	var b strings.Builder
	col := 0
	for _, r := range s {
		if r == '\t' {
			spaces := tabWidth - (col % tabWidth)
			b.WriteString(strings.Repeat(" ", spaces))
			col += spaces
			continue
		}
		b.WriteRune(r)
		col++
	}
	return b.String()
}

// expandedCol maps a 1-based byte column on the raw line to a 1-based visual
// column on the tab-expanded line.
func expandedCol(raw string, byteCol int) int {
	visual := 0
	for i := 0; i < byteCol-1 && i < len(raw); i++ {
		if raw[i] == '\t' {
			visual += tabWidth - (visual % tabWidth)
		} else {
			visual++
		}
	}
	return visual + 1
}
```

- [ ] **Step 3b: Put the message on the caret line and add help lines**

Replace the caret block inside `renderSpot` (the `if ln == line { ... }` and the stray trailing `paint("")`) with:

```go
		if ln == line {
			caretCol := expandedCol(txt, col)
			n := sp.Len
			if n < 1 {
				n = 1
			}
			if maxN := len([]rune(expanded)) - (caretCol - 1); maxN >= 1 && n > maxN {
				n = maxN
			}
			carets := paint(strings.Repeat("^", n))
			line := fmt.Sprintf("%s %s %s%s %s", pad, bar,
				strings.Repeat(" ", caretCol-1), carets, paint(sp.Message))
			fmt.Fprintln(b, strings.TrimRight(line, " "))
		}
```

After the loop (still inside `renderSpot`), add help lines:

```go
	for _, h := range sp.Help {
		fmt.Fprintf(b, "%s %s %s\n", pad, bar, s.Info("help: "+h))
	}
```

Also delete the earlier `fmt.Fprintf(b, "%s %s %s %s\n", ...)` caret draft and its trailing comment from Step 3 so only the Step 3b version remains.

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./internal/diagframe/ -run TestRender -v`
Expected: PASS (all four).

- [ ] **Step 5: Commit**

```bash
git add internal/diagframe/diagframe.go internal/diagframe/diagframe_test.go
git commit -m "feat(diagframe): Render frame with gutter, caret, tabs, help lines, fallback"
```

---

## Task 4: keyword-set drift guard

**Files:**

- Modify: `internal/luau/validate.go`
- Create: `internal/diagframe/keywords_drift_test.go`
- [ ] **Step 1: Export the canonical predicate**

In `internal/luau/validate.go`, add below the `luauReservedKeywords` var:

```go
// IsReservedKeyword reports whether id is a Luau reserved keyword (the canonical
// set behind IsValidIdentifier). Exposed so presentation layers can mirror the
// set without duplicating the source of truth.
func IsReservedKeyword(id string) bool {
	_, ok := luauReservedKeywords[id]
	return ok
}
```

- [ ] **Step 2: Write the drift test**

```go
package diagframe

import (
	"testing"

	"rotor/internal/luau"
)

func TestLuauKeywordSetMatchesCanonical(t *testing.T) {
	for w := range luauKeywords {
		if !luau.IsReservedKeyword(w) {
			t.Errorf("diagframe lists %q but internal/luau does not", w)
		}
	}
	// And the reverse: every canonical keyword must be in diagframe's set.
	for _, w := range []string{
		"and", "break", "do", "else", "elseif", "end", "false", "for",
		"function", "if", "in", "local", "nil", "not", "or", "repeat",
		"return", "then", "true", "until", "while",
	} {
		if _, ok := luauKeywords[w]; !ok {
			t.Errorf("canonical keyword %q missing from diagframe", w)
		}
		if !luau.IsReservedKeyword(w) {
			t.Errorf("test list has %q but internal/luau rejects it", w)
		}
	}
	if len(luauKeywords) != 21 {
		t.Errorf("expected 21 Luau keywords, got %d", len(luauKeywords))
	}
}
```

- [ ] **Step 3: Run test to verify it passes**

Run: `go test ./internal/diagframe/ -run TestLuauKeywordSetMatchesCanonical -v`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add internal/luau/validate.go internal/diagframe/keywords_drift_test.go
git commit -m "test(diagframe): guard Luau keyword set against internal/luau drift"
```

---

## Task 5: grouping + summary + truncation

**Files:**

- Create: `internal/diagframe/group.go`
- Test: `internal/diagframe/group_test.go`
- [ ] **Step 1: Write the failing test**

```go
package diagframe

import (
	"strings"
	"testing"
)

func TestRenderGroups_SummaryAndOrder(t *testing.T) {
	groups := []Group{
		{Path: "b.luau", Source: "y = 2\n", Lang: Luau, Spots: []Spot{{Offset: 0, Len: 1, Severity: Error, Message: "b err"}}},
		{Path: "a.luau", Source: "x = 1\n", Lang: Luau, Spots: []Spot{
			{Offset: 0, Len: 1, Severity: Error, Message: "a err"},
			{Offset: 0, Len: 1, Severity: Warning, Message: "a warn"},
		}},
	}
	out := RenderGroups(groups, Options{Color: false}, 0)
	// a.luau sorts before b.luau.
	if strings.Index(out, "a.luau") > strings.Index(out, "b.luau") {
		t.Error("files not sorted")
	}
	if !strings.Contains(out, "2 errors") || !strings.Contains(out, "1 warning") {
		t.Errorf("summary wrong:\n%s", out)
	}
	if !strings.Contains(out, "in 2 files") {
		t.Errorf("file count wrong:\n%s", out)
	}
}

func TestRenderGroups_Truncation(t *testing.T) {
	var spots []Spot
	for i := 0; i < 5; i++ {
		spots = append(spots, Spot{Offset: 0, Len: 1, Severity: Error, Message: "e"})
	}
	groups := []Group{{Path: "a.luau", Source: "x = 1\n", Lang: Luau, Spots: spots}}
	out := RenderGroups(groups, Options{Color: false}, 2)
	if !strings.Contains(out, "and 3 more") {
		t.Errorf("expected truncation note:\n%s", out)
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/diagframe/ -run TestRenderGroups -v`
Expected: FAIL — `undefined: Group`, `RenderGroups`.

- [ ] **Step 3: Implement grouping**

```go
package diagframe

import (
	"fmt"
	"sort"
	"strings"
)

// Group is one file's diagnostics ready to render.
type Group struct {
	Path   string
	Source string
	Lang   Language
	Spots  []Spot
}

// RenderGroups renders all groups (files sorted by path) followed by a summary
// footer. maxErrors caps the number of rendered frames across all groups (0 =
// unlimited); when capped, the footer notes how many were hidden.
func RenderGroups(groups []Group, o Options, maxErrors int) string {
	sorted := append([]Group(nil), groups...)
	sort.Slice(sorted, func(i, j int) bool { return sorted[i].Path < sorted[j].Path })

	var b strings.Builder
	var errs, warns, shown, total int
	for _, g := range sorted {
		var render []Spot
		for _, sp := range g.Spots {
			total++
			if sp.Severity == Warning {
				warns++
			} else {
				errs++
			}
			if maxErrors > 0 && shown >= maxErrors {
				continue
			}
			render = append(render, sp)
			shown++
		}
		if len(render) > 0 {
			b.WriteString(Render(g.Path, g.Source, g.Lang, render, o))
			b.WriteString("\n")
		}
	}

	s := stylerFor(o.Color)
	if hidden := total - shown; hidden > 0 {
		fmt.Fprintf(&b, "  %s\n", s.Muted(fmt.Sprintf("…and %d more", hidden)))
	}
	b.WriteString(summaryLine(s, errs, warns))
	return b.String()
}

func summaryLine(s *term.Styler, errs, warns int) string {
	files := "" // computed by caller context; recomputed here from counts is not possible
	_ = files
	parts := []string{}
	if errs > 0 {
		parts = append(parts, s.Error(plural(errs, "error")))
	}
	if warns > 0 {
		parts = append(parts, s.Warn(plural(warns, "warning")))
	}
	if len(parts) == 0 {
		return ""
	}
	return fmt.Sprintf("  %s %s\n", s.ErrorBold("✗"), strings.Join(parts, " · "))
}

func plural(n int, word string) string {
	if n == 1 {
		return fmt.Sprintf("%d %s", n, word)
	}
	return fmt.Sprintf("%d %ss", n, word)
}
```

NOTE: the test wants `in N files`. Thread the file count through `summaryLine`. Replace `summaryLine`'s signature/body:

```go
func summaryLine(s *term.Styler, errs, warns, files int) string {
	parts := []string{}
	if errs > 0 {
		parts = append(parts, s.Error(plural(errs, "error")))
	}
	if warns > 0 {
		parts = append(parts, s.Warn(plural(warns, "warning")))
	}
	if len(parts) == 0 {
		return ""
	}
	return fmt.Sprintf("  %s %s %s\n", s.ErrorBold("✗"),
		strings.Join(parts, " · "), s.Muted(fmt.Sprintf("in %s", plural(files, "file"))))
}
```

And in `RenderGroups`, count files with ≥1 spot and pass it:

```go
	filesWithDiags := 0
	for _, g := range sorted {
		if len(g.Spots) > 0 {
			filesWithDiags++
		}
	}
	// ... after the loop:
	b.WriteString(summaryLine(s, errs, warns, filesWithDiags))
```

Add the `term` import to `group.go`.

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./internal/diagframe/ -run TestRenderGroups -v`
Expected: PASS.

- [ ] **Step 5: Run the whole package + commit**

Run: `go test ./internal/diagframe/ ./internal/luau/ ./internal/term/`
Expected: PASS.

```bash
git add internal/diagframe/group.go internal/diagframe/group_test.go
git commit -m "feat(diagframe): group by file, summary footer, --max-errors truncation"
```

---

## Task 6: wire `rotor minify`

**Files:**

- Modify: `cmd/rotor/minify.go:71-78`
- Test: `cmd/rotor/minify_test.go`
- [ ] **Step 1: Write/extend the failing test**

Add to `cmd/rotor/minify_test.go` (follow the file's existing harness for invoking the command and capturing stderr; adapt names to the existing helpers):

```go
func TestMinify_SyntaxErrorShowsFrame(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "bad.luau")
	// `return print(1 2)` is a syntax error (missing separator).
	if err := os.WriteFile(path, []byte("local x = 1\nreturn print(1 2)\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	var stderr bytes.Buffer
	code := runMinify([]string{path}, &stderr) // adapt to actual minify entry/IO seam
	if code == 0 {
		t.Fatal("expected non-zero exit")
	}
	out := stderr.String()
	if !strings.Contains(out, "return print(1 2)") {
		t.Errorf("frame missing source line:\n%s", out)
	}
	if !strings.Contains(out, "^") {
		t.Errorf("frame missing caret:\n%s", out)
	}
}
```

NOTE: if `cmdMinify` reads/writes globals (`os.Stderr`) directly, add a minimal seam (a package-level `var minifyErr io.Writer = os.Stderr`, or pass a writer) as part of this task so the test can capture output. Keep the change small and mirror how other `cmd/rotor` tests inject IO.

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./cmd/rotor/ -run TestMinify_SyntaxErrorShowsFrame -v`
Expected: FAIL — output is the old `path:line:col: message` form, no caret/source line.

- [ ] **Step 3: Replace the diagnostic print with a frame**

In `cmd/rotor/minify.go`, replace lines 72-78:

```go
	minified, diags := cst.Minify(string(src))
	if len(diags) != 0 {
		errUI.failLine(fmt.Sprintf("rotor minify: %s has %s", input, plural(len(diags), "syntax error")))
		spots := make([]diagframe.Spot, len(diags))
		for i, d := range diags {
			spots[i] = diagframe.Spot{Offset: d.Pos.Offset, Len: 1, Severity: diagframe.Error, Message: d.Message}
		}
		fmt.Fprint(os.Stderr, diagframe.RenderGroups(
			[]diagframe.Group{{Path: input, Source: string(src), Lang: diagframe.Luau, Spots: spots}},
			diagframe.Options{Color: term.ColorEnabled(os.Stderr), Link: term.ColorEnabled(os.Stderr)},
			0,
		))
		return 1
	}
```

Add imports `"rotor/internal/diagframe"` and `"rotor/internal/term"` to `cmd/rotor/minify.go`.

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./cmd/rotor/ -run TestMinify_SyntaxErrorShowsFrame -v`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add cmd/rotor/minify.go cmd/rotor/minify_test.go
git commit -m "feat(minify): show code frame for Luau syntax errors"
```

---

## Task 7: wire `rotor bundle` (+ `internal/bundle`)

**Files:**

- Modify: `internal/bundle/bundle.go:83-87`
- Modify: `cmd/rotor/bundle.go:81-90`
- Test: `internal/bundle/bundle_test.go`, `cmd/rotor/bundle_test.go`
- [ ] **Step 1: Write the failing test (structured parse error)**

The bundler currently returns a formatted `error` string. Give it a typed error carrying the location so the CLI can frame it. Add to `internal/bundle/bundle_test.go`:

```go
func TestBundle_ParseErrorIsStructured(t *testing.T) {
	// build a temp project with an entry that has a syntax error, then:
	_, err := BundleWith(entry, Options{})
	var pe *ParseError
	if !errors.As(err, &pe) {
		t.Fatalf("want *ParseError, got %T: %v", err, err)
	}
	if pe.Path == "" || pe.Source == "" || pe.Diag.Message == "" {
		t.Errorf("ParseError missing context: %+v", pe)
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/bundle/ -run TestBundle_ParseErrorIsStructured -v`
Expected: FAIL — `undefined: ParseError`.

- [ ] **Step 3: Add the typed error and return it**

In `internal/bundle/bundle.go`, add:

```go
// ParseError is a Luau parse failure with enough context to render a code
// frame. It is returned by Bundle/BundleWith so the CLI can frame it.
type ParseError struct {
	Path   string
	Source string
	Diag   cst.Diagnostic
}

func (e *ParseError) Error() string {
	return fmt.Sprintf("%s:%d:%d: %s", e.Path, e.Diag.Pos.Line, e.Diag.Pos.Col, e.Diag.Message)
}
```

Replace lines 83-87:

```go
		file, diags := cst.Parse(src)
		if len(diags) != 0 {
			return nil, &ParseError{Path: absPath, Source: src, Diag: diags[0]}
		}
```

`Error()` preserves the old one-line string, so any caller that just prints `err` is unchanged.

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./internal/bundle/ -run TestBundle_ParseErrorIsStructured -v`
Expected: PASS.

- [ ] **Step 5: Frame it in the CLI**

In `cmd/rotor/bundle.go`, replace the `if err != nil` block (lines 82-84):

```go
	out, err := bundle.BundleWith(entry, bundle.Options{Exclude: exclude})
	if err != nil {
		var pe *bundle.ParseError
		if errors.As(err, &pe) {
			errUI.failLine("rotor bundle: syntax error")
			fmt.Fprint(os.Stderr, diagframe.RenderGroups(
				[]diagframe.Group{{Path: pe.Path, Source: pe.Source, Lang: diagframe.Luau,
					Spots: []diagframe.Spot{{Offset: pe.Diag.Pos.Offset, Len: 1, Severity: diagframe.Error, Message: pe.Diag.Message}}}},
				diagframe.Options{Color: term.ColorEnabled(os.Stderr), Link: term.ColorEnabled(os.Stderr)}, 0))
			return 1
		}
		errUI.failLine(fmt.Sprintf("rotor bundle: %v", err))
		return 1
	}
```

Add imports `"errors"`, `"rotor/internal/diagframe"`, `"rotor/internal/term"` to `cmd/rotor/bundle.go`.

- [ ] **Step 6: Run tests + commit**

Run: `go test ./internal/bundle/ ./cmd/rotor/ -run 'Bundle' -v`
Expected: PASS.

```bash
git add internal/bundle/bundle.go cmd/rotor/bundle.go internal/bundle/bundle_test.go cmd/rotor/bundle_test.go
git commit -m "feat(bundle): structured parse error + code frame in CLI"
```

---

## Task 8: wire `rotor pack`

**Files:**

- Modify: `internal/pack/luau.go:160-165`
- Test: `internal/pack/luau_test.go`
- [ ] **Step 1: Inspect the call site**

The pack path embeds `diags[0].Message` into a generated Luau string for a node that fails to compile (`internal/pack/luau.go:162-164`). This is emitted *into the packed artifact*, not printed to stderr — so a full ANSI frame is wrong here. Instead, enrich the embedded message with `line:col` (keep it a plain string, no color).

- [ ] **Step 2: Write the failing test**

```go
func TestPack_LuauSyntaxErrorMessageHasLocation(t *testing.T) {
	// node whose Source has a syntax error; assert the embedded failure string
	// contains "line:col" (e.g. ":2:") and the original message.
	// (adapt to the existing pack test harness / fixture builders)
}
```

- [ ] **Step 3: Add location to the embedded message**

Replace lines 162-164:

```go
	if _, diags := cst.Parse(n.Source); len(diags) != 0 {
		d := diags[0]
		return n.id, luauString(fmt.Sprintf("rotor pack: %s failed to compile at %d:%d: %s",
			n.Name, d.Pos.Line, d.Pos.Col, d.Message))
	}
```

- [ ] **Step 4: Run test + commit**

Run: `go test ./internal/pack/ -run TestPack_LuauSyntaxErrorMessageHasLocation -v`
Expected: PASS.

```bash
git add internal/pack/luau.go internal/pack/luau_test.go
git commit -m "feat(pack): include line:col in embedded Luau compile-failure message"
```

---

## Task 9: full-suite regression + roadmap

- [ ] **Step 1: Run the whole suite**

Run: `go test ./...`
Expected: PASS. (If `cmd/rotor` golden tests for minify/bundle changed output, update the goldens deliberately and re-run.)

- [ ] **Step 2: Manual smoke (optional but recommended)**

```bash
go run ./cmd/rotor minify <(printf 'return print(1 2)\n') || true
```

Expected: a framed error with the source line and a caret. (On Windows use a temp file instead of process substitution.)

- [ ] **Step 3: Update roadmap**

Per the standing convention, tick the diagnostics-frame items in `roadmap.md` and record that Plan 1 (renderer + Luau wiring) is complete with measured test counts.

```bash
git add roadmap.md
git commit -m "docs(roadmap): diagframe renderer + Luau wiring complete (plan 1)"
```

---

## Self-review notes (author)

- Spec §1 (renderer) → Tasks 1–4; §2 (summary/group/truncate) → Task 5; §4 (Luau wiring) → Tasks 6–8. Watch/TS plumbing are Plan 2; init/doctor are Plan 3.
- Types are consistent across tasks: `Spot{Offset,Len,Severity,Code,Message,Help}`, `Options{Context,Color,Link}`, `Group{Path,Source,Lang,Spots}`, `RenderGroups(groups, o, maxErrors)`, `Render(path, source, lang, spots, o)`, `stylerFor(bool)`, `isKeyword(word, lang)`, `highlightKeywords(line, lang, *term.Styler)`.
- `term.ForceColorWriter`/`PlainWriter` are the single seam for forcing color; `ColorEnabled` honors them.
- No placeholders except the two clearly-marked "adapt to existing test harness" notes in Tasks 6–8, which depend on the (unseen) shapes of `minify_test.go`/`bundle_test.go`/`luau_test.go`; the implementer must mirror the existing per-command test IO seam.
