package transformer

import (
	"rotor/internal/luau"
	"rotor/tsgo/ast"
)

// This file implements the rotor $nameof compile-time macro — a SUPERSET
// extension with no rbxtsc counterpart. Surface:
//
//	$nameof(player.Humanoid.Health) -> "Health"
//	$nameof(foo)                    -> "foo"
//
// The single argument's RAW AST is inspected (never transformed): the trailing
// identifier/property name is inlined as a Luau string literal. Index/call
// expressions and other non-named forms have no statically-knowable trailing
// name and surface a clear diagnostic.
//
// INTERCEPTION POINT (why here): same rationale as $env (see
// envmacro.go). The `$nameof` identifier has no runtime value, so it must be
// consumed before the regular identifier transform emits the invalid Luau
// identifier `$nameof`. transformOptionalChain flattens the chain FIRST and
// only then transforms the base, so a single hook there (optionalchain.go) sees
// `$nameof(...)` before the base identifier is transformed. Dispatch keys on
// the IDENTIFIER's symbol (the global `$nameof` const injected by compile's
// synthetic declaration file), exact and shadowing-safe: a user-defined local
// `$nameof` resolves to a different symbol and compiles as plain code. A bare
// `$nameof` that is NOT the head of a call is rejected in TransformIdentifier
// (identifier.go).

// nameofMacroSymbol resolves the global `$nameof` symbol (memoized by the
// MacroManager's lazy SYMBOL registry). nil when the program has no $nameof
// declaration (checker-light test projects that bypass compile's synthetic
// declaration injection).
func nameofMacroSymbol(s *State) *ast.Symbol {
	if s.Checker == nil {
		return nil
	}
	return s.Macros().Symbol("$nameof")
}

// isNameofMacroNode reports whether node (skipped downwards) is an identifier
// bound to THE global $nameof symbol. The text pre-filter keeps the hot
// transform paths free of symbol lookups.
func isNameofMacroNode(s *State, node *ast.Node) bool {
	node = SkipDownwards(node)
	if !ast.IsIdentifier(node) || node.Text() != "$nameof" {
		return false
	}
	symbol := nameofMacroSymbol(s)
	return symbol != nil && s.Checker.GetSymbolAtLocation(node) == symbol
}

// nameofTrailingName extracts the trailing identifier/property name of the raw
// argument expression WITHOUT transforming it. Wrapper kinds (parens, `as`,
// `!`, ...) are skipped first. A plain identifier yields its text; a property
// access yields the accessed member name; everything else (element access, call,
// `this`, literals, ...) has no statically-knowable name. ok=false then.
func nameofTrailingName(node *ast.Node) (string, bool) {
	node = SkipDownwards(node)
	switch {
	case ast.IsIdentifier(node):
		return node.Text(), true
	case ast.IsPropertyAccessExpression(node):
		// The member name token is always an Identifier or PrivateIdentifier;
		// .Name().Text() is the spelled name.
		name := node.AsPropertyAccessExpression().Name()
		if name != nil {
			return name.Text(), true
		}
	}
	return "", false
}

// interceptNameofChain is the transformOptionalChain hook: when the chain's
// base expression is the $nameof global, the leading call link is folded to the
// trailing name string and the rest of the chain (if any) continues from that
// literal. handled=false means "not $nameof — proceed normally".
func interceptNameofChain(s *State, chain []chainItem, expression *ast.Node) (luau.Expression, bool) {
	if len(chain) == 0 || !isNameofMacroNode(s, expression) {
		return nil, false
	}

	item := chain[0]
	if item.kind != chainCall {
		// Bare property/element access off $nameof — it has no such surface.
		// The checker already rejected these (a function has no members);
		// defensive for checker-light paths.
		s.Diags.Add(DiagRotorNameofBadUsage(item.node))
		return luau.NewNone(), true
	}
	if len(item.args) != 1 {
		// Unreachable after typecheck (the ambient signature takes exactly one
		// arg); defensive for checker-light paths.
		s.Diags.Add(DiagRotorNameofBadUsage(item.node))
		return luau.NewNone(), true
	}

	name, ok := nameofTrailingName(item.args[0])
	if !ok {
		s.Diags.Add(DiagRotorNameofInvalid(item.args[0]))
		return luau.NewNone(), true
	}
	value := luau.Str(name)

	// Continue any remaining chain links off the inlined literal, e.g.
	// `$nameof(a.b).upper()`.
	return transformOptionalChainInner(s, chain, value, nil, 1), true
}
