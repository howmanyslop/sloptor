package transformer

import (
	"bytes"
	"encoding/json"
	"errors"
	"sort"

	"rotor/internal/luau"
	"rotor/tsgo/ast"
)

// This file implements the rotor $file compile-time macro — a SUPERSET
// extension with no rbxtsc counterpart. Surface:
//
//	$file("config.json")  -> { Volume = 0.8, Name = "Game" }   (a Luau table)
//	$file("./notes.txt")  -> "raw text contents"               (a Luau string)
//
// The single string-literal argument is a path resolved from the project or importer: relative
// to the project root, or relative to the importing file when it begins with
// `./` or `../`. The file's contents are parsed at compile time and inlined as
// a Luau VALUE expression: `.json` becomes a table/array/scalar literal,
// everything else becomes a string literal of the raw text.
//
// DETERMINISM: $file is a pure function of the file's bytes — parity-safe and
// cacheable. Editing the file changes the output; rotor's incremental rebuild
// re-selects files whose source changed, and a $file source edit is detected
// through the importing .ts file's own dependency tracking (the macro reads the
// data file only at transform time).
//
// INTERCEPTION POINT (why here): same rationale as $env (see
// envmacro.go) — the `$file` identifier has no runtime value, so the leading
// call link is consumed before TransformExpression sees the base identifier.
// Dispatch keys on the IDENTIFIER's symbol, exact and shadowing-safe.

// FileResolver reads a project-relative (or importing-file-relative) data file's
// raw bytes for the $file macro. Implemented in internal/filemacro and plumbed
// onto State.Files per compile pass; nil in transformer-level unit tests.
//
// Read returns the sentinel error below (mapped to a not-found diagnostic) or
// any other error. importerPath is the absolute path of the source file
// containing the $file call, used to resolve `./`-relative paths.
type FileResolver interface {
	Read(importerPath, path string) (data []byte, err error)
}

// ErrFileNotFound: the resolved path names no readable file on disk.
var ErrFileNotFound = errors.New("file not found")

// fileMacroSymbol resolves the global `$file` symbol (memoized by the
// MacroManager's lazy SYMBOL registry). nil when the program has no $file
// declaration (checker-light test projects that bypass compile's synthetic
// declaration injection).
func fileMacroSymbol(s *State) *ast.Symbol {
	if s.Checker == nil {
		return nil
	}
	return s.Macros().Symbol("$file")
}

// isFileMacroNode reports whether node (skipped downwards) is an identifier
// bound to THE global $file symbol. The text pre-filter keeps the hot transform
// paths free of symbol lookups.
func isFileMacroNode(s *State, node *ast.Node) bool {
	node = SkipDownwards(node)
	if !ast.IsIdentifier(node) || node.Text() != "$file" {
		return false
	}
	symbol := fileMacroSymbol(s)
	return symbol != nil && s.Checker.GetSymbolAtLocation(node) == symbol
}

// fileStringLiteralText unwraps a string-literal-like argument, returning its
// cooked text. ok=false for any other expression — $file inlines at compile
// time, so a dynamic path cannot be read.
func fileStringLiteralText(node *ast.Node) (string, bool) {
	node = SkipDownwards(node)
	if ast.IsStringLiteralLike(node) {
		return node.Text(), true
	}
	return "", false
}

// hasJSONSuffix reports a case-insensitive ".json" suffix.
func hasJSONSuffix(path string) bool {
	if len(path) < 5 {
		return false
	}
	tail := path[len(path)-5:]
	for i, c := range []byte(".json") {
		t := tail[i]
		if t >= 'A' && t <= 'Z' {
			t += 'a' - 'A'
		}
		if t != c {
			return false
		}
	}
	return true
}

// resolveFileExpression reads path through the pass file resolver and parses it
// into a Luau value expression, or adds a diagnostic and returns None. node is
// the call expression (diagnostic position source).
func resolveFileExpression(s *State, node *ast.Node, path string) luau.Expression {
	if s.Files == nil {
		// No resolver attached (unit tests / single-file path without project
		// context). A clear diagnostic, never a panic.
		s.Diags.Add(DiagRotorFileNoResolver(node))
		return luau.NewNone()
	}
	importer := ""
	if s.SourceFile != nil {
		importer = s.SourceFile.FileName()
	}
	data, err := s.Files.Read(importer, path)
	if err != nil {
		if errors.Is(err, ErrFileNotFound) {
			s.Diags.Add(DiagRotorFileNotFound(node, path))
		} else {
			s.Diags.Add(DiagRotorFileNotFound(node, path))
		}
		return luau.NewNone()
	}

	if hasJSONSuffix(path) {
		value, jerr := jsonBytesToLuau(data)
		if jerr != nil {
			s.Diags.Add(DiagRotorFileInvalidJSON(node, path, jerr.Error()))
			return luau.NewNone()
		}
		return value
	}
	// Non-JSON text file: the raw contents become a single Luau string literal.
	return luau.Str(string(data))
}

// jsonBytesToLuau parses JSON bytes (numbers kept lossless via json.Number) and
// converts the decoded value into a Luau literal expression.
func jsonBytesToLuau(data []byte) (luau.Expression, error) {
	dec := json.NewDecoder(bytes.NewReader(data))
	dec.UseNumber()
	var v any
	if err := dec.Decode(&v); err != nil {
		return nil, err
	}
	// Reject trailing garbage after the first JSON value so malformed files
	// surface a diagnostic rather than silently inlining a prefix.
	if dec.More() {
		var extra any
		if err := dec.Decode(&extra); err == nil {
			return nil, errors.New("unexpected trailing JSON value")
		}
	}
	return jsonValueToLuau(v), nil
}

// jsonValueToLuau converts a decoded JSON value into a Luau literal.
//
//   - object  -> Luau map literal (keys sorted for deterministic output; a JSON
//     `null` member is DROPPED, since a Luau table field set to nil is absent —
//     inlining `key = nil` would be a no-op field and changes table shape vs.
//     the source intent, so the key is omitted entirely).
//   - array   -> Luau array literal (a `null` element becomes the Luau `nil`
//     literal, preserving positions; note Luau arrays with embedded nils are
//     sparse, matching the JSON intent as closely as Luau allows).
//   - string  -> Luau string literal.
//   - number  -> Luau number literal (raw JSON digits preserved: ints stay
//     ints, floats keep their precision — no float64 round-trip).
//   - bool    -> Luau true/false literal.
//   - null    -> Luau nil literal.
func jsonValueToLuau(v any) luau.Expression {
	switch val := v.(type) {
	case map[string]any:
		keys := make([]string, 0, len(val))
		for k := range val {
			keys = append(keys, k)
		}
		sort.Strings(keys)
		fields := luau.NewList[*luau.MapField]()
		for _, k := range keys {
			member := val[k]
			if member == nil {
				// JSON null member: a Luau table field set to nil is absent, so
				// dropping the key is the faithful representation.
				continue
			}
			fields.Push(luau.NewMapField(luau.Str(k), jsonValueToLuau(member)))
		}
		return luau.NewMap(fields)
	case []any:
		members := luau.NewList[luau.Expression]()
		for _, element := range val {
			members.Push(jsonValueToLuau(element))
		}
		return luau.NewArray(members)
	case string:
		return luau.Str(val)
	case json.Number:
		// Preserve the exact source digits (int vs float, precision).
		return luau.NewNumberLiteral(string(val))
	case bool:
		return luau.Bool(val)
	case nil:
		return luau.Nil()
	default:
		// Unreachable: encoding/json only produces the cases above.
		return luau.Nil()
	}
}

// interceptFileChain is the transformOptionalChain hook: when the chain's base
// expression is the $file global, the leading call link is folded to the parsed
// file value and the rest of the chain (if any) continues from that literal.
// handled=false means "not $file — proceed normally".
func interceptFileChain(s *State, chain []chainItem, expression *ast.Node) (luau.Expression, bool) {
	if len(chain) == 0 || !isFileMacroNode(s, expression) {
		return nil, false
	}

	item := chain[0]
	if item.kind != chainCall {
		// Bare property/element access off $file — it has no such surface.
		s.Diags.Add(DiagRotorFileBadUsage(item.node))
		return luau.NewNone(), true
	}
	if len(item.args) != 1 {
		// Unreachable after typecheck (the ambient signature takes exactly one
		// arg); defensive for checker-light paths.
		s.Diags.Add(DiagRotorFileBadUsage(item.node))
		return luau.NewNone(), true
	}
	path, ok := fileStringLiteralText(item.args[0])
	if !ok {
		s.Diags.Add(DiagRotorFileNonLiteralArg(item.args[0]))
		return luau.NewNone(), true
	}

	value := resolveFileExpression(s, item.node, path)

	// Continue any remaining chain links off the inlined value, e.g.
	// `$file("config.json").Volume`.
	return transformOptionalChainInner(s, chain, value, nil, 1), true
}
