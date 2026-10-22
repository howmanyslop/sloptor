package transformer

import (
	"strings"

	"rotor/internal/luau"
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

// This file ports TSTransformer/util/createTruthinessChecks.ts (complete).
//
// TS truthiness treats 0, NaN, "" as falsy; Luau only false/nil. Conditions
// therefore need extra guards whenever the static type admits 0, NaN, or "".

// WillCreateTruthinessChecks ports willCreateTruthinessChecks (L9-15).
func WillCreateTruthinessChecks(s *State, t *checker.Type) bool {
	return IsPossiblyType(s, t, IsNumberLiteralType(0)) ||
		IsPossiblyType(s, t, IsNaNType) ||
		IsPossiblyType(s, t, IsEmptyStringType)
}

// CreateTruthinessChecks ports createTruthinessChecks (L17-56). Check order
// is fixed: `exp ~= 0`, `exp == exp` (NaN), `exp ~= ""`, then exp itself,
// `and`-chained. A type assignable to literal 0 also receives the NaN check
// even when isNaNType alone would not fire (workaround for
// https://github.com/microsoft/TypeScript/issues/32778). When any check is
// added, exp is pinned to a temp (pushToVarIfComplex "value") so it evaluates
// once. Upstream computes the type itself (state.getType(node)); rotor takes
// it as a parameter — callers pass s.GetType(node).
func CreateTruthinessChecks(s *State, exp luau.Expression, node *ast.Node, t *checker.Type) luau.Expression {
	isAssignableToZero := IsPossiblyType(s, t, IsNumberLiteralType(0))
	isAssignableToNaN := IsPossiblyType(s, t, IsNaNType)
	isAssignableToEmptyString := IsPossiblyType(s, t, IsEmptyStringType)

	if isAssignableToZero || isAssignableToNaN || isAssignableToEmptyString {
		exp = s.PushToVarIfComplex(exp, "value")
	}

	var checks []luau.Expression

	if isAssignableToZero {
		checks = append(checks, luau.NewBinary(exp, "~=", luau.Num(0)))
	}

	// workaround for https://github.com/microsoft/TypeScript/issues/32778
	if isAssignableToZero || isAssignableToNaN {
		checks = append(checks, luau.NewBinary(exp, "==", exp))
	}

	if isAssignableToEmptyString {
		checks = append(checks, luau.NewBinary(exp, "~=", luau.Str("")))
	}

	checks = append(checks, exp)

	if s.LogTruthyChanges && (isAssignableToZero || isAssignableToNaN || isAssignableToEmptyString) {
		var checkStrs []string
		if isAssignableToZero {
			checkStrs = append(checkStrs, "0")
		}
		if isAssignableToZero || isAssignableToNaN {
			checkStrs = append(checkStrs, "NaN")
		}
		if isAssignableToEmptyString {
			checkStrs = append(checkStrs, `""`)
		}
		s.Diags.Add(DiagTruthyChange(node, strings.Join(checkStrs, ", ")))
	}

	var origins []*luau.Origin
	if s.SourceMapEnabled && len(checks) > 1 {
		if start := s.getOriginalSourcePosition(node, false); start != nil {
			origin := &luau.Origin{Start: *start}
			for _, check := range checks[:len(checks)-1] {
				luau.SetOrigin(check, origin)
			}
			origins = make([]*luau.Origin, len(checks)-1)
			for i := range origins {
				origins[i] = origin
			}
		}
	}
	return binaryExpressionChain(checks, "and", origins...)
}

// binaryExpressionChain ports util/expressionChain.ts binaryExpressionChain:
// `[a, b, c]` with `and` -> `a and b and c` (left fold).
func binaryExpressionChain(expressions []luau.Expression, operator luau.BinaryOperator, origins ...*luau.Origin) luau.Expression {
	result := expressions[0]
	for index, expression := range expressions[1:] {
		result = luau.NewBinary(result, operator, expression)
		if index < len(origins) {
			luau.SetOrigin(result, origins[index])
		}
	}
	return result
}
