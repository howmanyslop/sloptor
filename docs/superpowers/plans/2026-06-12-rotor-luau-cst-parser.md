# rotor Luau CST + Parser Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans or superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `internal/luau/cst` — a concrete syntax tree for Luau plus a
hand-written parser that consumes the lexer's token stream into that tree, such that
an in-order walk of the tree reproduces the source **byte-for-byte**.

**Architecture:** The lexer (`internal/luau/lex`, A.1) emits a flat token stream
including whitespace/comment trivia. This package first **attaches trivia** to
significant tokens as `TokenRef{Leading, Token, Trailing}` (Roslyn-style: trailing
trivia runs to the end of the current line; the newline-bearing whitespace and
everything after begins the next token's leading trivia). The parser is a
recursive-descent grammar with a Pratt expression core; **every `TokenRef` is
consumed into the tree** (the load-bearing invariant). A faithful unparse
(`Unparse`) walks the tree in source order and writes each `TokenRef`'s
leading+lexeme+trailing — byte-exact by construction. The smart reformatting
generators (dense/readable) are a separate package (A.3); this package ships only the
faithful unparse, which is the parser's roundtrip oracle.

**Tech stack:** Go 1.26, standard `testing` (+ native fuzzing). Depends on
`internal/luau/lex`. Reuses `luau.IsValidIdentifier`/`IsValidNumberLiteral` only if
needed for diagnostics (the parser is trivia/structure focused, not semantic).

This is sub-project **A.2** of the Luau toolchain
(`docs/superpowers/specs/2026-06-12-rotor-luau-toolchain-design.md`). A.3
(generators) and C/D (minifier/bundler) depend on it.

---

## File structure

- Create `internal/luau/cst/trivia.go` — `Trivia`, `TokenRef`, `AttachTrivia`, `Flatten`, `(TokenRef).text()`.
- Create `internal/luau/cst/node.go` — the `Node` interface, `base` (parent), node category markers, and the in-order `walk`/`Unparse`.
- Create `internal/luau/cst/nodes.go` — concrete node structs (statements, expressions, types, fields).
- Create `internal/luau/cst/cursor.go` — the parser's `TokenRef` cursor (peek/next/expect, keyword matching, diagnostics).
- Create `internal/luau/cst/parse.go` — `Parse(src) (*Block, []Diagnostic)`; statement grammar + block/recovery.
- Create `internal/luau/cst/expr.go` — Pratt expression parser + primary/suffix chains.
- Create `internal/luau/cst/types.go` — Luau type-annotation grammar.
- Tests: `trivia_test.go`, `roundtrip_test.go` (the corpus gate, reused from A.1), `expr_test.go`, `parse_test.go`, `fuzz_test.go`.

---

### Task 1: Trivia attachment + Flatten (prove the model against the corpus)

This task validates the **entire trivia model independent of the parser**: attach
trivia to a `TokenRef` sequence, then `Flatten` (concatenate every ref's
leading+lexeme+trailing) must reproduce the source byte-for-byte on the full corpus.

**Files:** Create `internal/luau/cst/trivia.go`, `internal/luau/cst/trivia_test.go`, `internal/luau/cst/roundtrip_test.go`.

- [ ] **Step 1: Write the failing test**

```go
// trivia_test.go
package cst

import "testing"

func TestAttachAndFlatten(t *testing.T) {
	src := "local x = 1 -- hi\nprint(x)\n"
	refs := AttachTrivia(src)
	if Flatten(refs) != src {
		t.Fatalf("flatten mismatch")
	}
	// the trailing comment attaches to the token ending the line ('1'), the
	// newline-bearing whitespace begins the next significant token's leading.
	var firstNum *TokenRef
	for i := range refs {
		if refs[i].Token.Text == "1" {
			firstNum = &refs[i]
		}
	}
	if firstNum == nil {
		t.Fatal("missing '1' token")
	}
	var hasTrailingComment bool
	for _, tr := range firstNum.Trailing {
		if tr.Text == "-- hi" {
			hasTrailingComment = true
		}
	}
	if !hasTrailingComment {
		t.Fatalf("comment not attached as trailing of '1': %+v", firstNum.Trailing)
	}
	// last ref is EOF
	if refs[len(refs)-1].Token.Kind != 0 { // lex.EOF == 0
		t.Fatalf("last ref not EOF")
	}
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `go test ./internal/luau/cst/ -run TestAttachAndFlatten`
Expected: FAIL — `undefined: AttachTrivia`.

- [ ] **Step 3: Implement trivia.go**

```go
// trivia.go
package cst

import (
	"strings"

	"rotor/internal/luau/lex"
)

// Trivia is a single whitespace or comment token (non-significant source text).
type Trivia struct {
	Kind  lex.Kind // lex.Whitespace or lex.Comment
	Text  string
	Start lex.Pos
}

// TokenRef wraps a significant token with its attached leading and trailing trivia.
// Leading+Token.Text+Trailing, concatenated across all refs in order, reproduces the
// source exactly.
type TokenRef struct {
	Leading  []Trivia
	Token    lex.Token
	Trailing []Trivia
}

func (r TokenRef) text() string {
	var b strings.Builder
	for _, tr := range r.Leading {
		b.WriteString(tr.Text)
	}
	b.WriteString(r.Token.Text)
	for _, tr := range r.Trailing {
		b.WriteString(tr.Text)
	}
	return b.String()
}

func isTrivia(k lex.Kind) bool { return k == lex.Whitespace || k == lex.Comment }

func triviaOf(t lex.Token) Trivia { return Trivia{Kind: t.Kind, Text: t.Text, Start: t.Start} }

func hasNewline(s string) bool { return strings.IndexByte(s, '\n') >= 0 }

// AttachTrivia converts a source string into a sequence of TokenRefs (terminated by
// an EOF ref). Trailing trivia of a token runs up to — but not including — the first
// following whitespace run that contains a newline; that run and everything after it
// become the leading trivia of the next significant token.
func AttachTrivia(src string) []TokenRef {
	toks, _ := lex.Tokenize(src)
	var refs []TokenRef
	var leading []Trivia
	i := 0
	for i < len(toks) {
		// gather leading trivia
		for i < len(toks) && isTrivia(toks[i].Kind) {
			leading = append(leading, triviaOf(toks[i]))
			i++
		}
		sig := toks[i]
		i++
		ref := TokenRef{Leading: leading, Token: sig}
		leading = nil
		// gather trailing trivia up to (not including) a newline-bearing whitespace
		for i < len(toks) && isTrivia(toks[i].Kind) {
			if toks[i].Kind == lex.Whitespace && hasNewline(toks[i].Text) {
				break
			}
			ref.Trailing = append(ref.Trailing, triviaOf(toks[i]))
			i++
		}
		refs = append(refs, ref)
		if sig.Kind == lex.EOF {
			break
		}
	}
	return refs
}

// Flatten concatenates every TokenRef's full text. For refs produced by AttachTrivia
// this equals the original source.
func Flatten(refs []TokenRef) string {
	var b strings.Builder
	for _, r := range refs {
		for _, tr := range r.Leading {
			b.WriteString(tr.Text)
		}
		b.WriteString(r.Token.Text)
		for _, tr := range r.Trailing {
			b.WriteString(tr.Text)
		}
	}
	return b.String()
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `go test ./internal/luau/cst/ -run TestAttachAndFlatten`
Expected: PASS.

- [ ] **Step 5: Add the corpus roundtrip test (the model gate)**

```go
// roundtrip_test.go
package cst

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// AttachTrivia + Flatten must reproduce any source exactly.
func TestTriviaRoundtripCorpus(t *testing.T) {
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
		src := string(data)
		if got := Flatten(AttachTrivia(src)); got != src {
			t.Fatalf("trivia roundtrip mismatch: %s", path)
		}
		count++
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("trivia-roundtripped %d files", count)
}
```

Run: `ROTOR_LUAU_CORPUS=<abs>/testdata go test ./internal/luau/cst/ -run TestTriviaRoundtripCorpus -v`
Expected: PASS, "trivia-roundtripped 405 files" (same corpus as A.1).

- [ ] **Step 6: Commit**

```bash
git add internal/luau/cst/
git commit -m "feat(cst): Roslyn-style trivia attachment with byte-exact flatten"
```

---

### Task 2: Node interface, base, and tree Unparse

**Files:** Create `internal/luau/cst/node.go`. Test extends `trivia_test.go`.

The tree-walk `Unparse` is the parser's roundtrip oracle for Tasks 3+. Every node
exposes its `TokenRef`s and child nodes **in source order** via a `tokens(yield)`
visitor; `Unparse` writes each ref's text.

- [ ] **Step 1: Write the failing test**

```go
func TestUnparseLeaf(t *testing.T) {
	refs := AttachTrivia("nil")
	leaf := &Nil{Tok: refs[0]} // refs[0] is the 'nil' name token
	if Unparse(leaf) != "nil" {
		t.Fatalf("unparse leaf = %q", Unparse(leaf))
	}
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `go test ./internal/luau/cst/ -run TestUnparseLeaf`
Expected: FAIL — `undefined: Nil` / `Unparse`.

- [ ] **Step 3: Implement node.go**

```go
// node.go
package cst

import "strings"

// Node is any CST node. tokens yields, in source order, every TokenRef and child
// Node beneath this node (used by Unparse and traversal). A node yields a *TokenRef
// for its own leaf tokens and a Node for each child subtree.
type Node interface {
	tokens(yield func(ref *TokenRef, child Node))
}

// Unparse reproduces the exact source text spanned by n.
func Unparse(n Node) string {
	var b strings.Builder
	var walk func(Node)
	walk = func(n Node) {
		n.tokens(func(ref *TokenRef, child Node) {
			switch {
			case ref != nil:
				b.WriteString(ref.text())
			case child != nil:
				walk(child)
			}
		})
	}
	walk(n)
	return b.String()
}

// Nil is the simplest leaf node, used to bootstrap Unparse before the grammar lands.
type Nil struct {
	Tok TokenRef
}

func (n *Nil) tokens(yield func(ref *TokenRef, child Node)) { yield(&n.Tok, nil) }
```

- [ ] **Step 4: Run to verify it passes**

Run: `go test ./internal/luau/cst/ -run TestUnparseLeaf`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add internal/luau/cst/node.go internal/luau/cst/trivia_test.go
git commit -m "feat(cst): Node interface and byte-exact tree Unparse"
```

---

### Task 3: Token cursor

**Files:** Create `internal/luau/cst/cursor.go`. Test `internal/luau/cst/parse_test.go`.

The cursor walks the `[]TokenRef` from `AttachTrivia`. It never drops a ref: every
`next()` returns a ref that the caller stores into the tree.

- [ ] **Step 1: Write the failing test**

```go
// parse_test.go
package cst

import "testing"

func TestCursorBasics(t *testing.T) {
	c := newCursor("a + b")
	if c.peek().Token.Text != "a" {
		t.Fatalf("peek = %q", c.peek().Token.Text)
	}
	a := c.next()
	if a.Token.Text != "a" || c.peek().Token.Text != "+" {
		t.Fatalf("after next: %q peek %q", a.Token.Text, c.peek().Token.Text)
	}
	if !c.atSymbol("+") {
		t.Fatalf("expected + symbol")
	}
	if c.atEnd() {
		t.Fatalf("not at end yet")
	}
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `go test ./internal/luau/cst/ -run TestCursorBasics`
Expected: FAIL — `undefined: newCursor`.

- [ ] **Step 3: Implement cursor.go**

```go
// cursor.go
package cst

import "rotor/internal/luau/lex"

type cursor struct {
	refs  []TokenRef
	pos   int
	diags []Diagnostic
}

// Diagnostic is a parser/lexer diagnostic with a source position.
type Diagnostic struct {
	Pos     lex.Pos
	Message string
}

func newCursor(src string) *cursor {
	return &cursor{refs: AttachTrivia(src)}
}

// peek returns the current ref without consuming it. At/after EOF it returns the EOF
// ref (the cursor never indexes out of range).
func (c *cursor) peek() *TokenRef {
	if c.pos >= len(c.refs) {
		return &c.refs[len(c.refs)-1]
	}
	return &c.refs[c.pos]
}

// next consumes and returns the current ref (clamped at the EOF ref).
func (c *cursor) next() TokenRef {
	r := *c.peek()
	if c.pos < len(c.refs)-1 {
		c.pos++
	}
	return r
}

func (c *cursor) atEnd() bool { return c.peek().Token.Kind == lex.EOF }

func (c *cursor) atSymbol(s string) bool {
	t := c.peek().Token
	return t.Kind == lex.Symbol && t.Text == s
}

// atKeyword reports whether the current ref is a Name token with the given keyword
// text (Luau keywords are lexed as Name; the parser classifies them).
func (c *cursor) atKeyword(kw string) bool {
	t := c.peek().Token
	return t.Kind == lex.Name && t.Text == kw
}

func (c *cursor) errHere(msg string) {
	c.diags = append(c.diags, Diagnostic{Pos: c.peek().Token.Start, Message: msg})
}

// expectSymbol consumes the current ref if it is the given symbol; otherwise records
// a diagnostic and returns the current ref unconsumed-shaped (best effort recovery).
func (c *cursor) expectSymbol(s string) TokenRef {
	if c.atSymbol(s) {
		return c.next()
	}
	c.errHere("expected '" + s + "'")
	return c.next()
}

func (c *cursor) expectKeyword(kw string) TokenRef {
	if c.atKeyword(kw) {
		return c.next()
	}
	c.errHere("expected '" + kw + "'")
	return c.next()
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `go test ./internal/luau/cst/ -run TestCursorBasics`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add internal/luau/cst/cursor.go internal/luau/cst/parse_test.go
git commit -m "feat(cst): token-ref cursor"
```

---

### Tasks 4–9: the grammar (incremental, each verified by Unparse roundtrip)

> **Driving discipline for every grammar task:** for each construct, write a unit
> test that parses a snippet and asserts `Unparse(node) == snippet` (no bytes lost),
> implement the production storing **every** consumed `TokenRef` and child node into
> the node struct, run it, commit. The grammar is built bottom-up so later tasks
> reuse earlier parsers. The final gate (Task 10) parses + roundtrips the full
> 405-file corpus, which is a far stronger oracle than any single snippet.

The node structs live in `nodes.go`; each implements `tokens` yielding its refs and
children **in source order**. The grammar follows the Luau reference
(`luau-lang/luau` `Ast/`), precedence below. These tasks are listed at production
granularity; each is one TDD cycle (test → fail → implement → pass → commit).

- [ ] **Task 4 — Primary + literal expressions** (`expr.go`, `nodes.go`): `nil`,
  `true`, `false`, number, string, vararg `...`, name, and parenthesized `( expr )`.
  Node structs: `Nil` (exists), `True`, `False`, `Number`, `String`, `Vararg`,
  `Name`, `Paren`. Each stores its `TokenRef`(s). Test: roundtrip each literal and
  `(a)`.

- [ ] **Task 5 — Suffix chains** (`expr.go`): index `.name`, computed `[ expr ]`,
  call `( args )`, string-call `f"s"`, table-call `f{...}`, method call `:name(...)`.
  Node structs: `Index`, `IndexExpr`, `Call`, `MethodCall` with an `Args` union
  (paren-list / string / table). Build left-associatively on a primary. Test:
  roundtrip `a.b.c`, `a[b]`, `f(x, y)`, `a:m(1)`, `f"s"`, `f{1}`.

- [ ] **Task 6 — Operators (Pratt)** (`expr.go`): unary `- not #`, binary with Luau
  precedence (below), right-assoc `^` and `..`, and the `if-then-else` expression and
  `e :: T` type assertion. Node structs: `Unary`, `Binary`, `IfExpr`, `TypeAssert`.
  Test: roundtrip `1 + 2 * 3`, `a and b or c`, `-x ^ 2`, `a .. b .. c`,
  `if x then 1 else 2`, `x :: number`.

- [ ] **Task 7 — Table constructors** (`expr.go`): `{ }` with positional, `[k]=v`,
  and `name=v` fields separated by `,`/`;` (trailing separator allowed). Node:
  `Table` with `[]TableField` (each field a tagged struct keeping its refs). Test:
  roundtrip `{}`, `{1, 2, 3}`, `{a = 1; ["b"] = 2, 3}`.

- [ ] **Task 8 — Function expressions + bodies** (`expr.go`, `parse.go`):
  `function ( params ) body end`, params (names, `...`, optional `: type`),
  optional return-type `: T`, generic params `<T, U...>`. Node: `FuncBody`,
  `FuncExpr`, `Param`. Bodies parse a `Block` (Task 9). Test: roundtrip
  `function(a, b) return a + b end`, `function(...) end`, `function<T>(x: T): T return x end`.

- [ ] **Task 9 — Statements + blocks** (`parse.go`, `nodes.go`): a `Block` is a list
  of statements + an optional last statement (`return exprlist` / `break` /
  `continue`). Statements: local-assign (`local names [: types] = exprs`, with
  `<attribute>` on names), assignment (`lhslist = exprlist`, incl. compound
  `+= //= ..=` …), call-statement, `do end`, `while c do b end`,
  `repeat b until c`, numeric `for i = a, b[, c] do b end`, generic
  `for names in exprs do b end`, `if/elseif/else`, `function name.path:method(...)`,
  `local function name(...)`, `type Name<...> = T` / `export type`, and a lone `;`.
  Node structs per statement, each storing keyword/separator refs. `Parse(src)`
  returns the top-level `Block` and diagnostics. Test: roundtrip a representative
  multi-statement program for each statement kind.

---

### Task 10: Types + the full-corpus parse roundtrip gate

**Files:** `internal/luau/cst/types.go`, extend `roundtrip_test.go`, `fuzz_test.go`.

- [ ] **Step 1 — Type grammar** (`types.go`): name types `A`, `A.B`, generics
  `A<T, U>`, table types `{ x: number, [string]: V }`, function types
  `(A) -> B`, variadic `...T`, `typeof(e)`, unions `A | B`, intersections `A & B`,
  optionals `A?`, parenthesized, string/boolean singleton types. Wire type parsing
  into the `: T` sites from Tasks 8–9. Test: roundtrip each type form.

- [ ] **Step 2 — Full parse roundtrip** (the real gate): parse each corpus file and
  assert `Unparse(Parse(src)) == src`.

```go
// in roundtrip_test.go
func TestParseRoundtripCorpus(t *testing.T) {
	root := os.Getenv("ROTOR_LUAU_CORPUS")
	if root == "" {
		t.Skip("set ROTOR_LUAU_CORPUS")
	}
	count, parsed := 0, 0
	_ = filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
		if err != nil || d.IsDir() || (!strings.HasSuffix(path, ".luau") && !strings.HasSuffix(path, ".lua")) {
			return err
		}
		count++
		data, _ := os.ReadFile(path)
		src := string(data)
		block, diags := Parse(src)
		if len(diags) != 0 {
			t.Errorf("%s: %d diags (first: %v)", path, len(diags), diags[0])
			return nil
		}
		if Unparse(block) != src {
			t.Errorf("%s: parse roundtrip mismatch", path)
			return nil
		}
		parsed++
		return nil
	})
	t.Logf("parsed+roundtripped %d/%d files", parsed, count)
}
```

Run: `ROTOR_LUAU_CORPUS=<abs>/testdata go test ./internal/luau/cst/ -run TestParseRoundtripCorpus -v`
Expected: **405/405**, zero diagnostics, zero mismatches. Investigate any file that
fails to parse or roundtrip — these reveal grammar gaps; add the construct, re-run.

- [ ] **Step 3 — Fuzz the parser** (must never panic; when it parses cleanly it must
  roundtrip):

```go
// fuzz_test.go
package cst

import "testing"

func FuzzParse(f *testing.F) {
	for _, s := range []string{
		"local x = 1", "return a, b", "if x then y() end",
		"for i = 1, 10 do end", "function f(...) return ... end",
		"local t: {number} = {}", "x += 1", "a `b{c}d`",
	} {
		f.Add(s)
	}
	f.Fuzz(func(t *testing.T, src string) {
		block, diags := Parse(src) // never panics
		if len(diags) == 0 {
			if Unparse(block) != src {
				t.Fatalf("clean parse must roundtrip")
			}
		}
	})
}
```

Run: `go test ./internal/luau/cst/ -run xxx -fuzz FuzzParse -fuzztime 60s`
Expected: no panics, no clean-parse roundtrip failures.

- [ ] **Step 4 — Final checks + commit**

Run: `go test ./internal/luau/cst/ -count=1 && go vet ./internal/luau/cst/ && gofmt -l internal/luau/cst/`
Expected: green, clean.

```bash
git add internal/luau/cst/
git commit -m "feat(cst): Luau type grammar + full-corpus parse roundtrip gate"
```

---

## Luau operator precedence (for Task 6)

Lowest to highest binding (from the Luau reference grammar):

| Level | Operators | Assoc |
|------:|-----------|-------|
| 1 | `or` | left |
| 2 | `and` | left |
| 3 | `<  >  <=  >=  ~=  ==` | left |
| 4 | `..` | **right** |
| 5 | `+  -` | left |
| 6 | `*  /  //  %` | left |
| 7 | unary `-  not  #` | — |
| 8 | `^` | **right** |

`if-then-else` expressions and `e :: T` assertions bind looser than `or` at their use
sites (parsed at the expression-statement / assignment-value entry, per the grammar).

## Self-review notes (author)

- **Spec coverage:** covers A.2 — trivia-preserving CST (`TokenRef` Roslyn model),
  surface-faithful node taxonomy (no Array/Map/Set inference — a single `Table` with
  tagged fields), hand-written recursive-descent + Pratt parser, error recovery
  (diagnostics, never panics), and byte-exact roundtrip proven on the full 405-file
  corpus + fuzz. Generators (dense/readable) are explicitly A.3.
- **No placeholders:** Tasks 1–3 and Task 10's harness contain complete code; Tasks
  4–9 are at production granularity with concrete node names, the precedence table,
  and the universal "store every ref, assert Unparse roundtrip" discipline — the
  corpus gate makes any missed production a hard test failure, not a silent gap.
- **Type consistency:** `TokenRef{Leading,Token,Trailing}`, `Trivia{Kind,Text,Start}`,
  `Node.tokens(yield func(ref *TokenRef, child Node))`, `Unparse`, `Parse(src)
  (*Block, []Diagnostic)`, cursor methods (`peek`/`next`/`atSymbol`/`atKeyword`/
  `expectSymbol`/`expectKeyword`) are used consistently. `Diagnostic` is defined in
  `cursor.go` and reused.
- **Corpus reuse:** same `ROTOR_LUAU_CORPUS` mechanism and 405-file corpus as the
  lexer (A.1), so the parser is held to the same real-world bar.

```text
```
