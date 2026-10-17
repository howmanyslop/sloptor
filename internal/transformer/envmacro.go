package transformer

import (
	"rotor/internal/luau"
	"rotor/tsgo/ast"
)

// This file implements the rotor $env compile-time environment macro — a
// SUPERSET extension with no rbxtsc counterpart (it replaces the community
// rbxts-transform-env transformer plugin). Surface:
//
//	$env("GAME_NAME")             -> "value" or nil
//	$env("GAME_NAME", "fallback") -> "value" or "fallback"
//	$env.GAME_NAME                -> "value" or nil
//	$env["GAME_NAME"]             -> "value" or nil
//
// Values resolve via dotenv.Env.Lookup (process env > .env.<NODE_ENV> >
// .env; see internal/dotenv), snapshotted once per compile pass on
// State.Env. Only string inlining is supported (v1) — names and fallbacks
// must be string literals so the value can be baked into the emitted Luau.
//
// INTERCEPTION POINTS (why here): the `$env` identifier itself has no
// runtime value, so it must be consumed before the regular identifier
// transform ever sees it (TransformIdentifier would emit the invalid Luau
// identifier `$env`). Every call / property-access / element-access
// expression funnels through transformOptionalChain, which flattens the
// chain FIRST and only then transforms the base expression — so a single
// hook there (optionalchain.go transformOptionalChain) sees `$env(...)`,
// `$env.X`, `$env["X"]`, and any longer chain hanging off them, before the
// base identifier is transformed. That is strictly cleaner than patching the
// three Inner transforms (call.go/access.go), which receive the base already
// transformed. The CALL_MACROS table is NOT used: its dispatch keys on the
// symbol of the callee's TYPE (GetFirstDefinedSymbol), which is well-defined
// for `declare function` ambients but not for $env's
// index-signature-and-call-signature intersection type; keying on the
// IDENTIFIER's symbol (the global `$env` const injected by
// compile.parseProjectCommandLine's synthetic declaration file) is exact and
// shadowing-safe — a user-defined local `$env` resolves to a different
// symbol and compiles as plain code, exactly like upstream macro globals.
// The remaining position — a bare `$env` that is NOT the head of a
// call/access chain (e.g. `const x = $env`) — is rejected with a diagnostic
// in TransformIdentifier (identifier.go).

// envMacroSymbol resolves the global `$env` symbol (memoized by the
// MacroManager's lazy SYMBOL registry). nil when the program has no $env
// declaration (checker-light test projects that bypass compile's synthetic
// declaration injection).
func envMacroSymbol(s *State) *ast.Symbol {
	if s.Checker == nil {
		return nil
	}
	return s.Macros().Symbol("$env")
}

// isEnvMacroNode reports whether node (skipped downwards) is an identifier
// bound to THE global $env symbol. The text pre-filter keeps the hot
// transform paths free of symbol lookups.
func isEnvMacroNode(s *State, node *ast.Node) bool {
	node = SkipDownwards(node)
	if !ast.IsIdentifier(node) || node.Text() != "$env" {
		return false
	}
	symbol := envMacroSymbol(s)
	return symbol != nil && s.Checker.GetSymbolAtLocation(node) == symbol
}

// envStringLiteralText unwraps a string-literal-like argument ("..." or
// `...` without substitutions), returning its cooked text. ok=false for any
// other expression — $env inlines at compile time, so dynamic names cannot
// be resolved.
func envStringLiteralText(node *ast.Node) (string, bool) {
	node = SkipDownwards(node)
	if ast.IsStringLiteralLike(node) {
		return node.Text(), true
	}
	return "", false
}

// envValueExpression resolves name through the pass env snapshot:
// the value as a Luau string literal, or nil when unset.
func envValueExpression(s *State, name string) luau.Expression {
	if value, ok := s.Env.Lookup(name); ok {
		return luau.Str(value)
	}
	return luau.Nil()
}

// interceptEnvChain is the transformOptionalChain hook: when the chain's
// base expression is the $env global, the first chain link is folded to its
// compile-time value and the rest of the chain (if any) continues from that
// literal. handled=false means "not $env — proceed normally". Optionality
// markers (`$env?.X`, `$env?.("X")`) are accepted and ignored: $env is never
// nil at the type level, so the nil checks rbxtsc would emit are vacuous and
// the inlined value is semantically identical.
func interceptEnvChain(s *State, chain []chainItem, expression *ast.Node) (luau.Expression, bool) {
	if len(chain) == 0 || !isEnvMacroNode(s, expression) {
		return nil, false
	}

	item := chain[0]
	var value luau.Expression
	switch item.kind {
	case chainCall:
		// $env("NAME") / $env("NAME", "fallback")
		if len(item.args) < 1 || len(item.args) > 2 {
			// Unreachable after typecheck (the ambient call signatures take
			// 1-2 args); defensive for checker-light paths.
			s.Diags.Add(DiagRotorEnvBadUsage(item.node))
			return luau.NewNone(), true
		}
		name, ok := envStringLiteralText(item.args[0])
		if !ok {
			s.Diags.Add(DiagRotorEnvNonLiteralArg(item.args[0]))
			return luau.NewNone(), true
		}
		if envValue, found := s.Env.Lookup(name); found {
			value = luau.Str(envValue)
		} else if len(item.args) == 2 {
			fallback, ok := envStringLiteralText(item.args[1])
			if !ok {
				s.Diags.Add(DiagRotorEnvNonLiteralArg(item.args[1]))
				return luau.NewNone(), true
			}
			value = luau.Str(fallback)
		} else {
			value = luau.Nil()
		}

	case chainPropertyAccess:
		// $env.NAME
		value = envValueExpression(s, item.name)

	case chainElementAccess:
		// $env["NAME"]
		name, ok := envStringLiteralText(item.argumentExpression)
		if !ok {
			s.Diags.Add(DiagRotorEnvNonLiteralArg(item.argumentExpression))
			return luau.NewNone(), true
		}
		value = envValueExpression(s, name)

	default:
		// chainPropertyCall/chainElementCall ($env.X(...), $env["X"](...)):
		// the checker already rejected these (string | undefined has no call
		// signatures); defensive for checker-light paths.
		s.Diags.Add(DiagRotorEnvBadUsage(item.node))
		return luau.NewNone(), true
	}

	// Continue any remaining chain links off the inlined literal, e.g.
	// `$env("A")!.size()`.
	return transformOptionalChainInner(s, chain, value, nil, 1), true
}
