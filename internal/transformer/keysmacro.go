package transformer

import (
	"rotor/internal/luau"
	"rotor/tsgo/ast"
)

// This file implements the rotor $keys compile-time macro — a SUPERSET
// extension with no rbxtsc counterpart (the rbxts-transformer-keys staple).
// Surface:
//
//	$keys<{ x: number; y: string }>() -> { "x", "y" }
//
// The single type argument is resolved through the CHECKER: its apparent type's
// string-keyed properties are enumerated (in declaration order) and inlined as
// a Luau array literal of string literals. Number/symbol keys are skipped (they
// are not valid TypeScript string keys for this purpose). A type with no
// enumerable string keys (e.g. `{}`) yields an empty array — that is a valid,
// useful result, not an error. A MISSING type argument is the only diagnostic.
//
// INTERCEPTION POINT (why here): same rationale as $env/$nameof (see
// envmacro.go). The `$keys` identifier has no runtime value, so it must be
// consumed before the regular identifier transform emits the invalid Luau
// identifier `$keys`. transformOptionalChain flattens the chain FIRST, then
// transforms the base, so a single hook there (optionalchain.go) sees
// `$keys<T>()` before the base identifier is transformed. Dispatch keys on the
// IDENTIFIER's symbol (the global `$keys` const injected by compile's synthetic
// declaration file), exact and shadowing-safe. A bare `$keys` that is NOT the
// head of a call is rejected in TransformIdentifier (identifier.go).

// keysMacroSymbol resolves the global `$keys` symbol (memoized by the
// MacroManager's lazy SYMBOL registry). nil when the program has no $keys
// declaration (checker-light test projects that bypass compile's synthetic
// declaration injection).
func keysMacroSymbol(s *State) *ast.Symbol {
	if s.Checker == nil {
		return nil
	}
	return s.Macros().Symbol("$keys")
}

// isKeysMacroNode reports whether node (skipped downwards) is an identifier
// bound to THE global $keys symbol. The text pre-filter keeps the hot transform
// paths free of symbol lookups.
func isKeysMacroNode(s *State, node *ast.Node) bool {
	node = SkipDownwards(node)
	if !ast.IsIdentifier(node) || node.Text() != "$keys" {
		return false
	}
	symbol := keysMacroSymbol(s)
	return symbol != nil && s.Checker.GetSymbolAtLocation(node) == symbol
}

// keysOfTypeNode resolves typeNode to its type through the checker and returns
// its string property keys in declaration order. Number/symbol-named members
// (whose symbol name carries the checker's internal `__@`/numeric markers, or
// which fail the string-literal identifier test) are excluded. The APPARENT
// type is used so primitive and unioned forms still expose their members the
// way property access would see them.
func keysOfType(s *State, typeNode *ast.Node) []string {
	t := s.Checker.GetTypeFromTypeNode(typeNode)
	if t == nil {
		return nil
	}
	apparent := s.Checker.GetApparentType(t)
	props := s.Checker.GetPropertiesOfType(apparent)
	keys := make([]string, 0, len(props))
	for _, prop := range props {
		name := prop.Name
		if !isStringKeyName(name) {
			continue
		}
		keys = append(keys, name)
	}
	return keys
}

// isStringKeyName reports whether a property symbol name is a real string key
// (not a checker-internal computed/symbol name). The checker names ES-symbol
// and other computed members with a leading "__@" sentinel; those are excluded.
// Plain numeric-string names ("0", "1") are array indices, not object string
// keys for $keys purposes, and are also excluded.
func isStringKeyName(name string) bool {
	if name == "" {
		return false
	}
	if len(name) >= 3 && name[0] == '_' && name[1] == '_' && name[2] == '@' {
		// Computed/ES-symbol member sentinel (InternalSymbolName.* family).
		return false
	}
	// Pure-digit names are numeric indices, not string object keys.
	allDigits := true
	for i := 0; i < len(name); i++ {
		if name[i] < '0' || name[i] > '9' {
			allDigits = false
			break
		}
	}
	return !allDigits
}

// interceptKeysChain is the transformOptionalChain hook: when the chain's base
// expression is the $keys global, the leading call link is folded to the array
// of the type argument's string keys and the rest of the chain (if any)
// continues from that literal. handled=false means "not $keys — proceed
// normally".
func interceptKeysChain(s *State, chain []chainItem, expression *ast.Node) (luau.Expression, bool) {
	if len(chain) == 0 || !isKeysMacroNode(s, expression) {
		return nil, false
	}

	item := chain[0]
	if item.kind != chainCall {
		// Bare property/element access off $keys — it has no such surface.
		s.Diags.Add(DiagRotorKeysBadUsage(item.node))
		return luau.NewNone(), true
	}

	// The type argument lives on the CallExpression node, not in item.args.
	call := item.node.AsCallExpression()
	if call.TypeArguments == nil || len(call.TypeArguments.Nodes) != 1 {
		// No type argument to enumerate (`$keys()`). The checker also flags this
		// (the signature requires one), but emit the rotor diagnostic so the
		// failure is specific and never a panic.
		s.Diags.Add(DiagRotorKeysNoType(item.node))
		return luau.NewNone(), true
	}

	keys := keysOfType(s, call.TypeArguments.Nodes[0])
	members := luau.NewList[luau.Expression]()
	for _, key := range keys {
		members.Push(luau.Str(key))
	}
	value := luau.NewArray(members)

	// Continue any remaining chain links off the inlined array, e.g.
	// `$keys<T>().size()`.
	return transformOptionalChainInner(s, chain, value, nil, 1), true
}
