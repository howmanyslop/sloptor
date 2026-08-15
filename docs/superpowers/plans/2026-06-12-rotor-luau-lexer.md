# rotor Luau Lexer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `internal/luau/lex` — a Luau tokenizer that produces a flat token
stream **including whitespace and comment trivia**, with byte-exact lexeme text and
source positions, so a later CST parser can attach trivia and roundtrip Luau
byte-for-byte.

**Architecture:** A hand-written, single-pass byte scanner. Every token (including
`Whitespace` and `Comment`) carries its exact source text and start/end `Pos`. The
core invariant: **concatenating every token's `Text` in order reproduces the input
exactly** (the EOF token has empty text). The lexer is deliberately *permissive* —
it captures lexemes (e.g. a number's full run) and leaves validity judgments to the
parser; it never panics and recovers from malformed input with a diagnostic.

**Tech stack:** Go 1.26, standard `testing` (+ native fuzzing). Matches the existing
`internal/luau` package conventions (`uint8` kind enums, table-driven tests).

This is sub-project **A.1** of the Luau toolchain
(`docs/superpowers/specs/2026-06-12-rotor-luau-toolchain-design.md`). A.2 (CST +
parser) and A.3 (generators) get their own plans and depend on this.

---

## File structure

- Create `internal/luau/lex/token.go` — `Kind`, `Pos`, `Token`, `Diagnostic`, `Kind.String()`.
- Create `internal/luau/lex/lex.go` — `Tokenize`, the `scanner`, the run loop, whitespace/name/symbol scanners, char classifiers.
- Create `internal/luau/lex/number.go` — number scanner.
- Create `internal/luau/lex/string.go` — short strings, long strings, comments, long-bracket helper.
- Create `internal/luau/lex/interp.go` — backtick interpolated-string scanning + brace-depth tracking.
- Tests: `lex_test.go`, `number_test.go`, `string_test.go`, `interp_test.go`, `roundtrip_test.go`, `fuzz_test.go`.

**Reused from elsewhere:** none at the API boundary (the lexer is standalone). The
parser later reuses `luau.IsValidNumberLiteral` / `luau.IsValidIdentifier`, not the
lexer.

---

### Task 1: Token types + empty-input skeleton

**Files:** Create `internal/luau/lex/token.go`, `internal/luau/lex/lex.go`, test `internal/luau/lex/lex_test.go`.

- [ ] **Step 1: Write the failing test**

```go
// lex_test.go
package lex

import "testing"

func TestTokenizeEmpty(t *testing.T) {
	toks, diags := Tokenize("")
	if len(diags) != 0 {
		t.Fatalf("unexpected diags: %v", diags)
	}
	if len(toks) != 1 || toks[0].Kind != EOF {
		t.Fatalf("want [EOF], got %v", toks)
	}
	if toks[0].Start != (Pos{0, 1, 1}) {
		t.Fatalf("EOF pos = %v", toks[0].Start)
	}
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `go test ./internal/luau/lex/ -run TestTokenizeEmpty`
Expected: FAIL — `undefined: Tokenize` / `EOF` / `Pos`.

- [ ] **Step 3: Implement token.go**

```go
// token.go
package lex

import "fmt"

type Kind uint8

const (
	EOF Kind = iota
	Whitespace   // trivia: maximal run of space/tab/newline/CR/VT/FF
	Comment      // trivia: -- line or --[[ ]] long comment
	Name         // identifier or keyword (parser disambiguates keywords)
	Number       // numeric literal lexeme (validity checked later)
	String       // short '..'/".." or long [[..]] string
	InterpSimple // `...` backtick string with no interpolation hole
	InterpStart  // `... {   (backtick through first {)
	InterpMid    // }...{
	InterpEnd    // }...`
	Symbol       // operator or punctuation
	Invalid      // an unrecognized character (recovered)
)

func (k Kind) String() string {
	switch k {
	case EOF:
		return "EOF"
	case Whitespace:
		return "Whitespace"
	case Comment:
		return "Comment"
	case Name:
		return "Name"
	case Number:
		return "Number"
	case String:
		return "String"
	case InterpSimple:
		return "InterpSimple"
	case InterpStart:
		return "InterpStart"
	case InterpMid:
		return "InterpMid"
	case InterpEnd:
		return "InterpEnd"
	case Symbol:
		return "Symbol"
	case Invalid:
		return "Invalid"
	default:
		return fmt.Sprintf("Kind(%d)", uint8(k))
	}
}

// Pos is a source position. Offset is a 0-based byte offset; Line and Col are
// 1-based (Col counts bytes within the current line).
type Pos struct {
	Offset int
	Line   int
	Col    int
}

type Token struct {
	Kind  Kind
	Text  string
	Start Pos
	End   Pos
}

type Diagnostic struct {
	Pos     Pos
	Message string
}
```

- [ ] **Step 4: Implement lex.go skeleton (run loop emits only EOF for now)**

```go
// lex.go
package lex

// Tokenize scans src into a token stream that includes whitespace and comment
// trivia, terminated by a single EOF token. The concatenation of every token's
// Text equals src.
func Tokenize(src string) ([]Token, []Diagnostic) {
	s := &scanner{src: src, line: 1, col: 1}
	s.run()
	return s.toks, s.diags
}

type scanner struct {
	src    string
	offset int
	line   int
	col    int
	interp []int // brace-depth stack; len>0 => inside an interpolation expression
	toks   []Token
	diags  []Diagnostic
}

func (s *scanner) atEnd() bool      { return s.offset >= len(s.src) }
func (s *scanner) peek() byte       { if s.atEnd() { return 0 }; return s.src[s.offset] }
func (s *scanner) peekAt(n int) byte {
	if i := s.offset + n; i < len(s.src) {
		return s.src[i]
	}
	return 0
}
func (s *scanner) pos() Pos { return Pos{s.offset, s.line, s.col} }

func (s *scanner) advance() byte {
	c := s.src[s.offset]
	s.offset++
	if c == '\n' {
		s.line++
		s.col = 1
	} else {
		s.col++
	}
	return c
}

func (s *scanner) emit(kind Kind, start Pos) {
	s.toks = append(s.toks, Token{Kind: kind, Text: s.src[start.Offset:s.offset], Start: start, End: s.pos()})
}

func (s *scanner) run() {
	for !s.atEnd() {
		// task 2+ fills this in
		break
	}
	s.toks = append(s.toks, Token{Kind: EOF, Start: s.pos(), End: s.pos()})
}
```

- [ ] **Step 5: Run to verify it passes**

Run: `go test ./internal/luau/lex/ -run TestTokenizeEmpty`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add internal/luau/lex/
git commit -m "feat(lex): token types and empty-input skeleton"
```

---

### Task 2: Whitespace, names, and the run loop

**Files:** Modify `internal/luau/lex/lex.go`. Test `internal/luau/lex/lex_test.go`.

- [ ] **Step 1: Write the failing test**

```go
func tokKinds(toks []Token) []Kind {
	ks := make([]Kind, len(toks))
	for i, t := range toks {
		ks[i] = t.Kind
	}
	return ks
}

func TestWhitespaceAndNames(t *testing.T) {
	toks, diags := Tokenize("  local  x")
	if len(diags) != 0 {
		t.Fatalf("diags: %v", diags)
	}
	want := []Kind{Whitespace, Name, Whitespace, Name, EOF}
	got := tokKinds(toks)
	if len(got) != len(want) {
		t.Fatalf("kinds = %v", got)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("kinds = %v, want %v", got, want)
		}
	}
	if toks[1].Text != "local" || toks[3].Text != "x" {
		t.Fatalf("texts = %q %q", toks[1].Text, toks[3].Text)
	}
	// newline advances line/col
	toks2, _ := Tokenize("a\nb")
	if toks2[2].Start != (Pos{2, 2, 1}) {
		t.Fatalf("second name start = %v", toks2[2].Start)
	}
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `go test ./internal/luau/lex/ -run TestWhitespaceAndNames`
Expected: FAIL (only EOF emitted).

- [ ] **Step 3: Implement classifiers + scanners + wire the run loop**

Add to `lex.go`:

```go
func isSpace(c byte) bool {
	return c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\v' || c == '\f'
}
func isDigit(c byte) bool     { return c >= '0' && c <= '9' }
func isHex(c byte) bool       { return isDigit(c) || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F') }
func isNameStart(c byte) bool { return c == '_' || (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') }
func isNameCont(c byte) bool  { return isNameStart(c) || isDigit(c) }

func (s *scanner) scanWhitespace() {
	start := s.pos()
	for !s.atEnd() && isSpace(s.peek()) {
		s.advance()
	}
	s.emit(Whitespace, start)
}

func (s *scanner) scanName() {
	start := s.pos()
	for !s.atEnd() && isNameCont(s.peek()) {
		s.advance()
	}
	s.emit(Name, start)
}
```

Replace the `run` body's loop with the dispatch (the later tasks add the remaining
cases; for now whitespace + names + a fallback that advances one byte as `Invalid`
so we never infinite-loop):

```go
func (s *scanner) run() {
	for !s.atEnd() {
		c := s.peek()
		switch {
		case isSpace(c):
			s.scanWhitespace()
		case isNameStart(c):
			s.scanName()
		default:
			start := s.pos()
			s.advance()
			s.emit(Invalid, start)
			s.diags = append(s.diags, Diagnostic{Pos: start, Message: "unexpected character"})
		}
	}
	s.toks = append(s.toks, Token{Kind: EOF, Start: s.pos(), End: s.pos()})
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `go test ./internal/luau/lex/ -run TestWhitespaceAndNames`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add internal/luau/lex/
git commit -m "feat(lex): whitespace and name scanning"
```

---

### Task 3: Symbols (longest-match)

**Files:** Modify `internal/luau/lex/lex.go`. Test `internal/luau/lex/lex_test.go`.

- [ ] **Step 1: Write the failing test**

```go
func TestSymbols(t *testing.T) {
	cases := map[string][]string{
		"a + b":        {"a", " ", "+", " ", "b"},
		"x...y":        {"x", "...", "y"},
		"a..=b":        {"a", "..=", "b"},
		"a//=b":        {"a", "//=", "b"},
		"1//2":         {"1", "//", "2"}, // floor div (number scanning lands in task 4; here numbers are names? no—digits)
		"a<=b>=c~=d":   {"a", "<=", "b", ">=", "c", "~=", "d"},
		"a::T->b":      {"a", "::", "T", "->", "b"},
		"f().x":        {"f", "(", ")", ".", "x"},
	}
	for src, wantTexts := range cases {
		toks, _ := Tokenize(src)
		var got []string
		for _, tk := range toks {
			if tk.Kind == EOF {
				break
			}
			got = append(got, tk.Text)
		}
		if len(got) != len(wantTexts) {
			t.Fatalf("%q -> %q, want %q", src, got, wantTexts)
		}
		for i := range wantTexts {
			if got[i] != wantTexts[i] {
				t.Fatalf("%q -> %q, want %q", src, got, wantTexts)
			}
		}
	}
}
```

(Note: the `1//2` / `1` cases rely on number scanning from Task 4. To keep Task 3
self-contained, the `1//2` and `f().x` digit pieces are fine because a leading digit
currently hits the `Invalid` fallback per-byte — so add the number cases only after
Task 4. For Task 3, trim the test to the symbol-only rows: `a + b`, `x...y`,
`a..=b`, `a//=b`, `a<=b>=c~=d`, `a::T->b`. Re-add digit rows in Task 4.)

- [ ] **Step 2: Run to verify it fails**

Run: `go test ./internal/luau/lex/ -run TestSymbols`
Expected: FAIL — symbols hit the `Invalid` fallback.

- [ ] **Step 3: Implement symbol scanning**

Add to `lex.go`:

```go
var symbols3 = [...]string{"...", "//=", "..="}
var symbols2 = [...]string{"==", "~=", "<=", ">=", "..", "::", "->", "+=", "-=", "*=", "/=", "%=", "^=", "//"}

func isSingleSymbol(c byte) bool {
	switch c {
	case '+', '-', '*', '/', '%', '^', '#', '<', '>', '=',
		'(', ')', '{', '}', '[', ']', ';', ':', ',', '.',
		'&', '|', '?', '@':
		return true
	}
	return false
}

func (s *scanner) scanSymbol() {
	start := s.pos()
	if s.offset+3 <= len(s.src) {
		three := s.src[s.offset : s.offset+3]
		for _, sym := range symbols3 {
			if three == sym {
				s.advance(); s.advance(); s.advance()
				s.emit(Symbol, start)
				return
			}
		}
	}
	if s.offset+2 <= len(s.src) {
		two := s.src[s.offset : s.offset+2]
		for _, sym := range symbols2 {
			if two == sym {
				s.advance(); s.advance()
				s.emit(Symbol, start)
				return
			}
		}
	}
	c := s.peek()
	if isSingleSymbol(c) {
		s.advance()
		s.emit(Symbol, start)
		s.trackBrace(c) // no-op until interpolation (Task 7)
		return
	}
	s.advance()
	s.emit(Invalid, start)
	s.diags = append(s.diags, Diagnostic{Pos: start, Message: "unexpected character"})
}

// trackBrace updates interpolation brace depth; defined fully in interp.go (Task 7).
func (s *scanner) trackBrace(c byte) {}
```

Replace the `default` arm of `run`'s switch with `s.scanSymbol()` (delete the inline
Invalid fallback):

```go
		default:
			s.scanSymbol()
```

- [ ] **Step 4: Run to verify it passes**

Run: `go test ./internal/luau/lex/ -run TestSymbols`
Expected: PASS (symbol-only rows).

- [ ] **Step 5: Commit**

```bash
git add internal/luau/lex/
git commit -m "feat(lex): longest-match symbol scanning"
```

---

### Task 4: Numbers

**Files:** Create `internal/luau/lex/number.go`, modify `lex.go` (dispatch), test `internal/luau/lex/number_test.go`.

- [ ] **Step 1: Write the failing test**

```go
// number_test.go
package lex

import "testing"

func TestNumbers(t *testing.T) {
	one := func(src string) Token {
		toks, diags := Tokenize(src)
		if len(diags) != 0 {
			t.Fatalf("%q diags: %v", src, diags)
		}
		if toks[0].Kind != Number || toks[0].Text != src {
			t.Fatalf("%q -> kind %v text %q", src, toks[0].Kind, toks[0].Text)
		}
		return toks[0]
	}
	for _, src := range []string{
		"0", "123", "1_000", "1.5", ".5", "3.", "1e10", "1E+5", "1.2e-3",
		"0xFF", "0X_ff_0", "0b1010", "0B_1010", "0x1.8p1", "0x1p-2",
	} {
		one(src)
	}
	// '..' must NOT be swallowed into a number
	toks, _ := Tokenize("1..2")
	if toks[0].Text != "1" || toks[1].Text != ".." || toks[2].Text != "2" {
		t.Fatalf("1..2 -> %q %q %q", toks[0].Text, toks[1].Text, toks[2].Text)
	}
	// method/field access after a number-less name is unaffected
	toks2, _ := Tokenize("1.5.foo")
	if toks2[0].Text != "1.5" || toks2[1].Text != "." || toks2[2].Text != "foo" {
		t.Fatalf("1.5.foo -> %q %q %q", toks2[0].Text, toks2[1].Text, toks2[2].Text)
	}
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `go test ./internal/luau/lex/ -run TestNumbers`
Expected: FAIL — digits hit symbol/invalid.

- [ ] **Step 3: Implement number.go**

```go
// number.go
package lex

// scanNumber consumes a numeric lexeme. It is permissive: it captures the run of
// number characters (the parser validates with luau.IsValidNumberLiteral). It
// never consumes a ".." (concatenation) as part of the number.
func (s *scanner) scanNumber() {
	start := s.pos()
	if s.peek() == '0' && (s.peekAt(1) == 'x' || s.peekAt(1) == 'X') {
		s.advance()
		s.advance()
		s.consumeWhile(func(c byte) bool { return isHex(c) || c == '_' })
		s.consumeFractionAndExponent(isHex, 'p', 'P')
	} else if s.peek() == '0' && (s.peekAt(1) == 'b' || s.peekAt(1) == 'B') {
		s.advance()
		s.advance()
		s.consumeWhile(func(c byte) bool { return c == '0' || c == '1' || c == '_' })
	} else {
		s.consumeWhile(func(c byte) bool { return isDigit(c) || c == '_' })
		s.consumeFractionAndExponent(isDigit, 'e', 'E')
	}
	s.emit(Number, start)
}

func (s *scanner) consumeWhile(pred func(byte) bool) {
	for !s.atEnd() && pred(s.peek()) {
		s.advance()
	}
}

// consumeFractionAndExponent consumes an optional single '.' fraction (guarded so a
// ".." is never swallowed) and an optional exponent introduced by e1/e2.
func (s *scanner) consumeFractionAndExponent(digit func(byte) bool, e1, e2 byte) {
	if !s.atEnd() && s.peek() == '.' && s.peekAt(1) != '.' {
		s.advance()
		s.consumeWhile(func(c byte) bool { return digit(c) || c == '_' })
	}
	if !s.atEnd() && (s.peek() == e1 || s.peek() == e2) {
		s.advance()
		if s.peek() == '+' || s.peek() == '-' {
			s.advance()
		}
		s.consumeWhile(func(c byte) bool { return isDigit(c) || c == '_' })
	}
}
```

Add the dispatch case to `run`'s switch (before the symbol default):

```go
		case isDigit(c) || (c == '.' && isDigit(s.peekAt(1))):
			s.scanNumber()
```

- [ ] **Step 4: Run to verify it passes**

Run: `go test ./internal/luau/lex/ -run TestNumbers`
Expected: PASS. Then re-add the digit rows to `TestSymbols` and re-run `-run TestSymbols`.

- [ ] **Step 5: Commit**

```bash
git add internal/luau/lex/
git commit -m "feat(lex): number scanning (decimal/hex/binary, no .. swallow)"
```

---

### Task 5: Short strings

**Files:** Create `internal/luau/lex/string.go`, modify `lex.go` (dispatch), test `internal/luau/lex/string_test.go`.

- [ ] **Step 1: Write the failing test**

```go
// string_test.go
package lex

import "testing"

func TestShortStrings(t *testing.T) {
	ok := func(src string) {
		toks, diags := Tokenize(src)
		if len(diags) != 0 {
			t.Fatalf("%q diags: %v", src, diags)
		}
		if toks[0].Kind != String || toks[0].Text != src {
			t.Fatalf("%q -> %v %q", src, toks[0].Kind, toks[0].Text)
		}
	}
	ok(`"hello"`)
	ok(`'world'`)
	ok(`"a\"b"`)        // escaped quote
	ok(`"tab\there"`)   // escape
	ok(`"\u{1F600}"`)   // unicode escape (bytes skipped, not decoded)
	ok("\"a\\z\n   b\"") // \z line-continuation skips following whitespace incl newline
	ok("'line\\\ncont'") // backslash-newline continuation

	// unterminated -> one diag, token still emitted
	toks, diags := Tokenize(`"oops`)
	if len(diags) != 1 || toks[0].Kind != String {
		t.Fatalf("unterminated: diags=%v kind=%v", diags, toks[0].Kind)
	}
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `go test ./internal/luau/lex/ -run TestShortStrings`
Expected: FAIL — quotes hit symbol/invalid.

- [ ] **Step 3: Implement string.go short-string scanner**

```go
// string.go
package lex

import "strings"

func (s *scanner) scanShortString() {
	start := s.pos()
	quote := s.advance() // ' or "
	for !s.atEnd() {
		c := s.peek()
		switch {
		case c == '\\':
			s.advance() // backslash
			if s.atEnd() {
				break
			}
			e := s.peek()
			s.advance() // the escaped byte (covers \n, \", \\, \xHH start, etc.)
			if e == 'z' {
				// \z skips subsequent whitespace, including newlines
				s.consumeWhile(isSpace)
			}
		case c == quote:
			s.advance()
			s.emit(String, start)
			return
		case c == '\n':
			s.diags = append(s.diags, Diagnostic{Pos: start, Message: "unterminated string"})
			s.emit(String, start)
			return
		default:
			s.advance()
		}
	}
	s.diags = append(s.diags, Diagnostic{Pos: start, Message: "unterminated string"})
	s.emit(String, start)
}
```

Add the dispatch case to `run` (before the symbol default):

```go
		case c == '"' || c == '\'':
			s.scanShortString()
```

- [ ] **Step 4: Run to verify it passes**

Run: `go test ./internal/luau/lex/ -run TestShortStrings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add internal/luau/lex/
git commit -m "feat(lex): short string scanning with escapes and \\z"
```

---

### Task 6: Long strings, long comments, line comments

**Files:** Modify `internal/luau/lex/string.go`, `lex.go` (dispatch). Test `internal/luau/lex/string_test.go`.

- [ ] **Step 1: Write the failing test**

```go
func TestLongStringsAndComments(t *testing.T) {
	kindText := func(src string) (Kind, string) {
		toks, diags := Tokenize(src)
		if len(diags) != 0 {
			t.Fatalf("%q diags: %v", src, diags)
		}
		return toks[0].Kind, toks[0].Text
	}
	if k, txt := kindText("[[hello]]"); k != String || txt != "[[hello]]" {
		t.Fatalf("long string -> %v %q", k, txt)
	}
	if k, txt := kindText("[==[a]]b]==]"); k != String || txt != "[==[a]]b]==]" {
		t.Fatalf("leveled long string -> %v %q", k, txt)
	}
	if k, txt := kindText("-- line\nx"); k != Comment || txt != "-- line" {
		t.Fatalf("line comment -> %v %q", k, txt)
	}
	if k, txt := kindText("--[[ block ]] x"); k != Comment || txt != "--[[ block ]]" {
		t.Fatalf("block comment -> %v %q", k, txt)
	}
	if k, txt := kindText("--[==[ b ]==]"); k != Comment || txt != "--[==[ b ]==]" {
		t.Fatalf("leveled block comment -> %v %q", k, txt)
	}
	// a plain '[' is a symbol, not a long string
	toks, _ := Tokenize("a[1]")
	if toks[1].Kind != Symbol || toks[1].Text != "[" {
		t.Fatalf("plain bracket -> %v %q", toks[1].Kind, toks[1].Text)
	}
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `go test ./internal/luau/lex/ -run TestLongStringsAndComments`
Expected: FAIL.

- [ ] **Step 3: Implement the long-bracket helper, long string, and comment scanners**

Add to `string.go`:

```go
// longBracketLevel reports whether the current offset begins a long bracket
// '[' '='* '[' and, if so, the number of '=' signs (the level).
func (s *scanner) longBracketLevel() (level int, ok bool) {
	if s.peek() != '[' {
		return 0, false
	}
	i := s.offset + 1
	for i < len(s.src) && s.src[i] == '=' {
		level++
		i++
	}
	if i < len(s.src) && s.src[i] == '[' {
		return level, true
	}
	return 0, false
}

// consumeLongBody consumes an opening long bracket of the given level and the body
// up to and including the matching close, or to EOF. Returns false if unterminated.
func (s *scanner) consumeLongBody(level int) bool {
	s.advance() // [
	for i := 0; i < level; i++ {
		s.advance() // =
	}
	s.advance() // [
	closing := "]" + strings.Repeat("=", level) + "]"
	for !s.atEnd() {
		if s.peek() == ']' && s.offset+len(closing) <= len(s.src) && s.src[s.offset:s.offset+len(closing)] == closing {
			for range closing {
				s.advance()
			}
			return true
		}
		s.advance()
	}
	return false
}

func (s *scanner) scanLongString(level int) {
	start := s.pos()
	if !s.consumeLongBody(level) {
		s.diags = append(s.diags, Diagnostic{Pos: start, Message: "unterminated long string"})
	}
	s.emit(String, start)
}

func (s *scanner) scanComment() {
	start := s.pos()
	s.advance() // -
	s.advance() // -
	if !s.atEnd() && s.peek() == '[' {
		if level, ok := s.longBracketLevel(); ok {
			if !s.consumeLongBody(level) {
				s.diags = append(s.diags, Diagnostic{Pos: start, Message: "unterminated long comment"})
			}
			s.emit(Comment, start)
			return
		}
	}
	for !s.atEnd() && s.peek() != '\n' {
		s.advance()
	}
	s.emit(Comment, start)
}
```

Add the dispatch cases to `run` (comment case before the symbol/number handling so
`--` wins over `-`; long-string case guards `[`):

```go
		case c == '-' && s.peekAt(1) == '-':
			s.scanComment()
		case c == '[':
			if level, ok := s.longBracketLevel(); ok {
				s.scanLongString(level)
			} else {
				s.scanSymbol()
			}
```

- [ ] **Step 4: Run to verify it passes**

Run: `go test ./internal/luau/lex/ -run TestLongStringsAndComments`
Expected: PASS. Run the whole package: `go test ./internal/luau/lex/` — all green.

- [ ] **Step 5: Commit**

```bash
git add internal/luau/lex/
git commit -m "feat(lex): long strings, long comments, line comments"
```

---

### Task 7: Interpolated (backtick) strings

**Files:** Create `internal/luau/lex/interp.go`, modify `lex.go` (dispatch + `trackBrace`). Test `internal/luau/lex/interp_test.go`.

- [ ] **Step 1: Write the failing test**

```go
// interp_test.go
package lex

import "testing"

func texts(toks []Token) []string {
	var out []string
	for _, t := range toks {
		if t.Kind == EOF {
			break
		}
		out = append(out, t.Text)
	}
	return out
}

func TestInterpolatedStrings(t *testing.T) {
	// no hole
	toks, diags := Tokenize("`hello`")
	if len(diags) != 0 || toks[0].Kind != InterpSimple || toks[0].Text != "`hello`" {
		t.Fatalf("simple -> %v %q diags=%v", toks[0].Kind, toks[0].Text, diags)
	}
	// one hole: `a{x}b`
	toks, _ = Tokenize("`a{x}b`")
	wantKinds := []Kind{InterpStart, Name, InterpEnd, EOF}
	gk := tokKinds(toks)
	for i := range wantKinds {
		if gk[i] != wantKinds[i] {
			t.Fatalf("`a{x}b` kinds = %v", gk)
		}
	}
	if toks[0].Text != "`a{" || toks[2].Text != "}b`" {
		t.Fatalf("texts = %q %q", toks[0].Text, toks[2].Text)
	}
	// two holes -> InterpMid in the middle
	toks, _ = Tokenize("`{a}{b}`")
	if got := tokKinds(toks); got[0] != InterpStart || got[2] != InterpMid || got[4] != InterpEnd {
		t.Fatalf("`{a}{b}` kinds = %v", got)
	}
	// a table literal inside a hole: braces balance, not an interp boundary
	toks, _ = Tokenize("`{ {x=1} }`")
	got := texts(toks)
	want := []string{"`{", " ", "{", "x", "=", "1", "}", " ", "}`"}
	if len(got) != len(want) {
		t.Fatalf("nested braces -> %q", got)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("nested braces -> %q, want %q", got, want)
		}
	}
	// nested interpolation: `outer {`inner`} end`
	toks, _ = Tokenize("`a{`b`}c`")
	gk = tokKinds(toks)
	if gk[0] != InterpStart || gk[1] != InterpSimple || gk[2] != InterpEnd {
		t.Fatalf("nested interp kinds = %v", gk)
	}
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `go test ./internal/luau/lex/ -run TestInterpolatedStrings`
Expected: FAIL — backtick hits symbol/invalid.

- [ ] **Step 3: Implement interp.go and brace tracking**

```go
// interp.go
package lex

// scanInterpStart handles a leading backtick. It either produces an InterpSimple
// token (no hole) or an InterpStart token (body ended at the first '{'), pushing an
// interpolation frame whose value is the current brace nesting depth (0).
func (s *scanner) scanInterpStart() {
	start := s.pos()
	s.advance() // `
	if s.scanInterpBody() {
		s.interp = append(s.interp, 0)
		s.emit(InterpStart, start)
	} else {
		s.emit(InterpSimple, start)
	}
}

// scanInterpResume handles a '}' that closes an interpolation hole (only called when
// the top interpolation frame has depth 0). It produces InterpMid (another hole
// follows) or InterpEnd (string closed), popping the frame on InterpEnd.
func (s *scanner) scanInterpResume() {
	start := s.pos()
	s.advance() // }
	if s.scanInterpBody() {
		s.emit(InterpMid, start)
	} else {
		s.interp = s.interp[:len(s.interp)-1]
		s.emit(InterpEnd, start)
	}
}

// scanInterpBody scans backtick-string content (honoring \ escapes) until an
// unescaped '`' (returns false) or an unescaped '{' (returns true). It consumes the
// terminating delimiter.
func (s *scanner) scanInterpBody() (endedAtBrace bool) {
	for !s.atEnd() {
		c := s.peek()
		switch {
		case c == '\\':
			s.advance()
			if !s.atEnd() {
				s.advance()
			}
		case c == '`':
			s.advance()
			return false
		case c == '{':
			s.advance()
			return true
		case c == '\n':
			s.diags = append(s.diags, Diagnostic{Pos: s.pos(), Message: "unterminated interpolated string"})
			return false
		default:
			s.advance()
		}
	}
	s.diags = append(s.diags, Diagnostic{Pos: s.pos(), Message: "unterminated interpolated string"})
	return false
}
```

Replace the placeholder `trackBrace` in `lex.go` with the real implementation:

```go
// trackBrace updates the top interpolation frame's brace depth after a single-char
// symbol is emitted. A '}' that closes a hole (depth 0) is handled earlier by the
// run loop via scanInterpResume and never reaches here.
func (s *scanner) trackBrace(c byte) {
	if len(s.interp) == 0 {
		return
	}
	top := len(s.interp) - 1
	switch c {
	case '{':
		s.interp[top]++
	case '}':
		s.interp[top]--
	}
}
```

Add dispatch cases to `run`'s switch (the `}`-resume case must come before the
generic symbol default):

```go
		case c == '`':
			s.scanInterpStart()
		case c == '}' && len(s.interp) > 0 && s.interp[len(s.interp)-1] == 0:
			s.scanInterpResume()
```

- [ ] **Step 4: Run to verify it passes**

Run: `go test ./internal/luau/lex/ -run TestInterpolatedStrings`
Expected: PASS. Run `go test ./internal/luau/lex/` — all green.

- [ ] **Step 5: Commit**

```bash
git add internal/luau/lex/
git commit -m "feat(lex): interpolated string scanning with nested holes"
```

---

### Task 8: Roundtrip golden over a real corpus

**Files:** Create `internal/luau/lex/roundtrip_test.go`.

- [ ] **Step 1: Write the test (this is the core invariant)**

```go
// roundtrip_test.go
package lex

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// Concatenating every token's Text must reproduce the source exactly.
func assertRoundtrip(t *testing.T, name, src string) {
	t.Helper()
	toks, _ := Tokenize(src)
	var b strings.Builder
	for _, tk := range toks {
		b.WriteString(tk.Text)
	}
	if b.String() != src {
		t.Fatalf("roundtrip mismatch for %s (len %d vs %d)", name, b.Len(), len(src))
	}
}

func TestRoundtripUnits(t *testing.T) {
	cases := []string{
		"",
		"\n\n",
		"local x = 1 -- comment\n",
		"return `a{b}c`\n",
		"local s = [==[ raw ]] still ]==]\n",
		"print('hi')\r\nprint(\"there\")",
		"local t = { a = 1, [\"b\"] = 2, 3 }",
		"x += 1 // 2 .. 'z'",
		"--no trailing newline",
		"`{ {nested=`{inner}`} }`",
	}
	for i, src := range cases {
		assertRoundtrip(t, "unit"+string(rune('0'+i)), src)
	}
}

// TestRoundtripCorpus walks real .luau/.lua files if a corpus root is provided.
// Run with: ROTOR_LUAU_CORPUS=<dir> go test ./internal/luau/lex/ -run TestRoundtripCorpus
func TestRoundtripCorpus(t *testing.T) {
	root := os.Getenv("ROTOR_LUAU_CORPUS")
	if root == "" {
		t.Skip("set ROTOR_LUAU_CORPUS to a directory of .luau/.lua files")
	}
	count := 0
	err := filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return err
		}
		if !strings.HasSuffix(path, ".luau") && !strings.HasSuffix(path, ".lua") {
			return nil
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		assertRoundtrip(t, path, string(data))
		count++
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("roundtripped %d files", count)
}
```

- [ ] **Step 2: Run the unit roundtrips**

Run: `go test ./internal/luau/lex/ -run TestRoundtripUnits`
Expected: PASS. Then point the corpus at rotor's own output and the runtime lib:
`ROTOR_LUAU_CORPUS=include go test ./internal/luau/lex/ -run TestRoundtripCorpus -v`
Expected: PASS, logs a file count > 0. Fix any mismatch before proceeding.

- [ ] **Step 3: Commit**

```bash
git add internal/luau/lex/roundtrip_test.go
git commit -m "test(lex): byte-exact roundtrip units + corpus harness"
```

---

### Task 9: Fuzz the roundtrip

**Files:** Create `internal/luau/lex/fuzz_test.go`.

- [ ] **Step 1: Write the fuzz test**

```go
// fuzz_test.go
package lex

import (
	"strings"
	"testing"
)

func FuzzRoundtrip(f *testing.F) {
	seeds := []string{
		"local x = 1", "`a{b}c`", "[==[x]==]", "-- c\n", "0x1.8p2", "'\\z\n  q'",
		"a..=b//c", "}{}{", "```", "[[", "--[[", "\"\\", "`{`{`x`}`}`",
	}
	for _, s := range seeds {
		f.Add(s)
	}
	f.Fuzz(func(t *testing.T, src string) {
		toks, _ := Tokenize(src) // must never panic
		var b strings.Builder
		for _, tk := range toks {
			b.WriteString(tk.Text)
		}
		if b.String() != src {
			t.Fatalf("roundtrip mismatch (len %d vs %d)", b.Len(), len(src))
		}
		// positions must be monotonic and bounded
		for _, tk := range toks {
			if tk.Start.Offset < 0 || tk.End.Offset > len(src) || tk.Start.Offset > tk.End.Offset {
				t.Fatalf("bad span %v..%v for %v", tk.Start, tk.End, tk.Kind)
			}
		}
	})
}
```

- [ ] **Step 2: Run a bounded fuzz**

Run: `go test ./internal/luau/lex/ -run xxx -fuzz FuzzRoundtrip -fuzztime 30s`
Expected: no failures, no panics. If a crash is found, add the minimized input as a
`f.Add` seed, fix the scanner, and re-run.

- [ ] **Step 3: Commit**

```bash
git add internal/luau/lex/fuzz_test.go
git commit -m "test(lex): fuzz the byte-exact roundtrip + span invariants"
```

---

### Task 10: Diagnostics & recovery polish

**Files:** Modify `internal/luau/lex/lex.go` / `string.go` as needed. Test `internal/luau/lex/lex_test.go`.

- [ ] **Step 1: Write the failing test**

```go
func TestDiagnosticsRecover(t *testing.T) {
	// an invalid char does not stop later tokens
	toks, diags := Tokenize("a $ b")
	if len(diags) != 1 {
		t.Fatalf("want 1 diag, got %v", diags)
	}
	last := toks[len(toks)-1]
	if last.Kind != EOF {
		t.Fatalf("want trailing EOF")
	}
	// the 'b' after the bad char is still tokenized
	var sawB bool
	for _, tk := range toks {
		if tk.Kind == Name && tk.Text == "b" {
			sawB = true
		}
	}
	if !sawB {
		t.Fatalf("recovery dropped tokens: %v", tokKinds(toks))
	}
	// diag carries a position
	if diags[0].Pos.Col == 0 {
		t.Fatalf("diag missing position")
	}
}
```

- [ ] **Step 2: Run to verify it fails or passes**

Run: `go test ./internal/luau/lex/ -run TestDiagnosticsRecover`
Expected: PASS already (the design recovers per-character). If it fails, ensure the
`Invalid` arm advances exactly one byte and continues the loop.

- [ ] **Step 3: Final full-package run + vet**

Run: `go test ./internal/luau/lex/ -count=1 && go vet ./internal/luau/lex/ && gofmt -l internal/luau/lex/`
Expected: tests PASS, vet clean, `gofmt -l` prints nothing.

- [ ] **Step 4: Commit**

```bash
git add internal/luau/lex/
git commit -m "test(lex): diagnostics carry positions and recover"
```

---

## Self-review notes (author)

- **Spec coverage:** covers the A.1 lexer requirements from the toolchain spec —
  trivia-preserving tokens (Whitespace/Comment kept), positions, all Luau lexical
  forms (long strings/comments, interpolation, compound-assign/`//`/`::`/`->`,
  numeric separators + `0x`/`0b` + hex floats), permissive lexemes, non-panicking
  recovery, byte-exact roundtrip (unit + corpus + fuzz). The CST/parser and
  generators are explicitly out of scope (A.2/A.3, separate plans).
- **No placeholders:** every step has concrete code/tests. `trackBrace` is a real
  no-op in Task 3 and is replaced with the real body in Task 7 (called out).
- **Type consistency:** `Pos{Offset,Line,Col}`, `Token{Kind,Text,Start,End}`,
  `Diagnostic{Pos,Message}`, scanner method names (`scanNumber`, `consumeWhile`,
  `consumeFractionAndExponent`, `longBracketLevel`, `consumeLongBody`,
  `scanInterpBody`, `trackBrace`) are used consistently across tasks.
- **Corpus note:** `include/` contains the vendored RuntimeLib + Promise Luau; the
  acceptance project's `out/` is rotor's own emitted Luau — both are good
  roundtrip corpora. `reference/luau-ast` is TS, not a Luau corpus.

```text
```
