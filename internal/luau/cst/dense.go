package cst

import (
	"strings"

	"rotor/internal/luau"
	"rotor/internal/luau/lex"
)

// MinifyOptions tunes the minifier. The zero value strips comments + whitespace
// only (semantically identical, faithful token stream); fields enable extra
// size-reducing rewrites that change tokens but preserve program behavior.
type MinifyOptions struct {
	// ConvertIndexToField rewrites string indexing into dotted field access
	// where the string is a valid Luau identifier (base["foo"] -> base.foo and
	// table keys ["foo"] = v -> foo = v). Always semantics-preserving in Luau.
	ConvertIndexToField bool
}

// Minify parses src and returns minified Luau. It preserves leading `--!` directive
// comments (e.g. `--!strict`, `--!native`), which affect how Luau compiles the file,
// but drops every other comment and all superfluous whitespace. Diagnostics from
// parsing are returned; on a clean parse the result is semantically identical to src
// (no variable renaming — that is a later minifier pass). The string-index-to-field
// rewrite is on by default (a pure size win); use MinifyWith to disable it.
func Minify(src string) (string, []Diagnostic) {
	return MinifyWith(src, MinifyOptions{ConvertIndexToField: true})
}

// MinifyWith is Minify with explicit options.
func MinifyWith(src string, opts MinifyOptions) (string, []Diagnostic) {
	file, diags := Parse(src)
	var b strings.Builder
	for _, d := range directives(file) {
		b.WriteString(d)
		b.WriteByte('\n')
	}
	b.WriteString(DenseWith(file, DenseOptions(opts)))
	return b.String(), diags
}

// directives returns the leading `--!` directive comment lines of the file, in order.
func directives(file *File) []string {
	ref := firstRef(file.Body)
	if ref == nil {
		ref = &file.EOF
	}
	var out []string
	for _, tr := range ref.Leading {
		if tr.Kind == lex.Comment && strings.HasPrefix(tr.Text, "--!") {
			out = append(out, tr.Text)
		}
	}
	return out
}

// firstRef returns the first non-EOF TokenRef beneath n, or nil if there is none.
func firstRef(n Node) *TokenRef {
	var found *TokenRef
	var walk func(Node)
	walk = func(n Node) {
		n.tokens(func(ref *TokenRef, child Node) {
			if found != nil {
				return
			}
			switch {
			case ref != nil:
				if ref.Token.Kind != lex.EOF {
					found = ref
				}
			case child != nil:
				walk(child)
			}
		})
	}
	walk(n)
	return found
}

// Dense serializes n to minified Luau: every significant token, no comments or
// superfluous whitespace, with a single space inserted only where two adjacent
// tokens would otherwise merge into a different token, and a ';' inserted between two
// statements when the next one begins with '(' (which would otherwise be misparsed as
// a call on the previous statement — the classic Lua ambiguity).
//
// Dense drops ALL trivia, including comments. Semantic `--!` directive comments must
// therefore be preserved by the caller (the minifier), not by Dense.
func Dense(n Node) string {
	return DenseWith(n, DenseOptions{})
}

// DenseOptions tunes the dense serializer. The zero value is a faithful dense
// serializer whose significant tokens are identical to the source's; setting a
// field enables a size-reducing rewrite that changes tokens but not semantics.
type DenseOptions struct {
	// ConvertIndexToField: see MinifyOptions.ConvertIndexToField.
	ConvertIndexToField bool
}

// DenseWith is Dense with explicit options.
func DenseWith(n Node, opts DenseOptions) string {
	w := &denseWriter{convertIndexToField: opts.ConvertIndexToField}
	w.node(n)
	return w.b.String()
}

type denseWriter struct {
	b                   strings.Builder
	prev                string
	prevKind            lex.Kind
	started             bool
	convertIndexToField bool
}

func (w *denseWriter) tok(t lex.Token) {
	if t.Kind == lex.EOF {
		return
	}
	// No separator is ever needed adjacent to an interpolation chunk: its `, {, }
	// delimiters already separate it from neighbouring tokens.
	if w.started && !isInterpKind(w.prevKind) && !isInterpKind(t.Kind) &&
		(needsSeparator(w.prev, t.Text) || (w.prevKind == lex.Number && numberGluesToNext(t.Text))) {
		w.b.WriteByte(' ')
	}
	w.b.WriteString(t.Text)
	w.prev = t.Text
	w.prevKind = t.Kind
	w.started = true
}

// numberGluesToNext reports whether a token beginning with s, emitted directly
// after a numeric literal, would be misread by Luau's greedy number scanner as a
// single malformed number (e.g. `100` then `print` -> `100print`, or `1` then
// `..` -> `1..`). rotor's lexer is more permissive and splits these into two
// valid tokens, so needsSeparator's re-lex misses the hazard; the dense writer
// must force a separator. The trigger set is every char Luau's number scanner
// would keep consuming: identifier chars, digits, and `.`.
func numberGluesToNext(s string) bool {
	if s == "" {
		return false
	}
	c := s[0]
	return c == '_' || c == '.' ||
		(c >= '0' && c <= '9') ||
		(c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z')
}

// forced writes a fixed separator token (e.g. ";") without a leading space.
func (w *denseWriter) forced(s string) {
	w.b.WriteString(s)
	w.prev = s
	w.prevKind = lex.Symbol
	w.started = true
}

func isInterpKind(k lex.Kind) bool {
	return k == lex.InterpSimple || k == lex.InterpStart || k == lex.InterpMid || k == lex.InterpEnd
}

func (w *denseWriter) node(n Node) {
	switch v := n.(type) {
	case *File:
		w.block(v.Body)
	case *Block:
		w.block(v)
	case *IndexExpr:
		// base["foo"] -> base.foo when "foo" is a valid identifier.
		if w.convertIndexToField {
			if name, ok := fieldName(v.Key); ok {
				w.node(v.Base)
				w.tok(lex.Token{Kind: lex.Symbol, Text: "."})
				w.tok(lex.Token{Kind: lex.Name, Text: name})
				return
			}
		}
		w.emitTokens(v)
	case *Table:
		// ["foo"] = v -> foo = v for valid-identifier keys; emit fields one by one.
		if w.convertIndexToField {
			w.table(v)
			return
		}
		w.emitTokens(v)
	default:
		w.emitTokens(n)
	}
}

// emitTokens is the faithful default emission: every leaf token, recursing into
// child nodes (which may themselves rewrite).
func (w *denseWriter) emitTokens(n Node) {
	n.tokens(func(ref *TokenRef, child Node) {
		switch {
		case ref != nil:
			w.tok(ref.Token)
		case child != nil:
			w.node(child)
		}
	})
}

// table emits a table constructor, collapsing ["name"] = v keys to name = v when
// the key is a valid identifier (the convertIndexToField rewrite).
func (w *denseWriter) table(t *Table) {
	w.tok(t.Open.Token)
	for i := range t.Fields {
		w.field(&t.Fields[i])
	}
	w.tok(t.Close.Token)
}

func (w *denseWriter) field(f *TableField) {
	if f.LBracket != nil {
		if name, ok := fieldName(f.Key); ok {
			w.tok(lex.Token{Kind: lex.Name, Text: name})
			if f.Eq != nil {
				w.tok(f.Eq.Token)
			}
			if f.Value != nil {
				w.node(f.Value)
			}
			if f.Sep != nil {
				w.tok(f.Sep.Token)
			}
			return
		}
	}
	w.emitTokens(f)
}

// fieldName returns the identifier a string-literal key collapses to in dotted
// field access, and whether the collapse is valid — the decoded string must be a
// Luau identifier that is not a reserved keyword (`t["end"]` cannot become
// `t.end`, which is a syntax error).
func fieldName(key Node) (string, bool) {
	s, ok := key.(*String)
	if !ok {
		return "", false
	}
	value, ok := StringValue(s)
	if !ok {
		return "", false
	}
	if !luau.IsValidIdentifier(value) {
		return "", false
	}
	return value, true
}

func (w *denseWriter) block(b *Block) {
	for i := range b.Stmts {
		if i > 0 && startsWithParen(b.Stmts[i]) {
			w.forced(";")
		}
		w.node(b.Stmts[i])
	}
}

// startsWithParen reports whether a statement's first significant token is '(' (the
// only statement-start that can glue onto a preceding statement as a call).
func startsWithParen(n Node) bool {
	t, ok := firstToken(n)
	return ok && t.Kind == lex.Symbol && t.Text == "("
}

// firstToken returns the first non-EOF token beneath n in source order.
func firstToken(n Node) (lex.Token, bool) {
	var found lex.Token
	var ok bool
	var walk func(Node)
	walk = func(n Node) {
		n.tokens(func(ref *TokenRef, child Node) {
			if ok {
				return
			}
			switch {
			case ref != nil:
				if ref.Token.Kind != lex.EOF {
					found = ref.Token
					ok = true
				}
			case child != nil:
				walk(child)
			}
		})
	}
	walk(n)
	return found, ok
}

// needsSeparator reports whether the tokens with texts a then b must be separated by
// whitespace to keep them as two distinct tokens. It is exact: it re-lexes the
// concatenation and checks that it still yields exactly the two original tokens.
func needsSeparator(a, b string) bool {
	toks, _ := lex.Tokenize(a + b)
	var sig []lex.Token
	for _, t := range toks {
		if t.Kind == lex.Whitespace || t.Kind == lex.Comment || t.Kind == lex.EOF {
			continue
		}
		sig = append(sig, t)
		if len(sig) > 2 {
			return true
		}
	}
	return len(sig) != 2 || sig[0].Text != a || sig[1].Text != b
}
