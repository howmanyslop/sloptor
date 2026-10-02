package transformer

import (
	"rotor/internal/luau"
	"rotor/tsgo/ast"
)

// This file ports the Array region of
// TSTransformer/macros/propertyCallMacros.ts: ARRAY_METHODS (L587-734, 9
// entries). The ReadonlyArray region lives in arraymacros.go; the table rows
// and the wrapComments machinery applied to every entry live in
// propertycallmacros.go.

// arrayMethods ports ARRAY_METHODS (L587-734).
var arrayMethods = map[string]PropertyCallMacro{
	// push (L588-605): zero args -> `#exp` even as a statement ("always emit
	// luau.unary so the call doesn't disappear in emit"); otherwise one
	// `table.insert(exp, arg)` CallStatement per argument. push returns the
	// new length, so a consumed value emits `#exp`; statement position emits
	// nothing.
	"push": func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		// for `a.push()` always emit luau.unary so the call doesn't disappear in emit
		if len(args) == 0 {
			return luau.NewUnary("#", expression)
		}

		expression = s.PushToVarIfComplex(expression, "exp")
		if target := s.getArrayAppendTarget(node); target != nil {
			for _, argument := range args {
				s.Prereq(luau.NewAssignment(target.lengthID, "+=", luau.Num(1)))
				s.Prereq(luau.NewAssignment(
					luau.NewComputedIndex(convertToIndexableExpression(expression), target.lengthID),
					"=",
					argument,
				))
			}
			if !isUsedAsStatement(node) {
				return s.PushToVar(target.lengthID, "length")
			}
			return luau.NewNone()
		}

		for i := range args {
			s.Prereq(luau.NewCallStatement(
				luau.NewCall(luau.GlobalProperty("table", "insert"), luau.NewList(expression, args[i])),
			))
		}

		if !isUsedAsStatement(node) {
			return luau.NewUnary("#", expression)
		}
		return luau.NewNone()
	},

	// pop (L607-637): statement position is the one-liner `exp[#exp] = nil`;
	// a consumed value snapshots `local _length = #exp` and
	// `local _result = exp[_length]` first, clearing through the temp.
	"pop": func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		expression = s.PushToVarIfComplex(expression, "exp")

		var lengthExp luau.Expression = luau.NewUnary("#", expression)

		returnValueIsUsed := !isUsedAsStatement(node)
		var retValue *luau.TemporaryIdentifier
		if returnValueIsUsed {
			lengthExp = s.PushToVar(lengthExp, "length")
			retValue = s.PushToVar(
				luau.NewComputedIndex(convertToIndexableExpression(expression), lengthExp),
				"result",
			)
		}

		s.Prereq(luau.NewAssignment(
			luau.NewComputedIndex(convertToIndexableExpression(expression), lengthExp),
			"=",
			luau.Nil(),
		))

		if returnValueIsUsed {
			return retValue
		}
		return luau.NewNone()
	},

	// shift (L639): `table.remove(exp, 1)` — a plain expression, works as a
	// statement too.
	"shift": func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		return luau.NewCall(luau.GlobalProperty("table", "remove"), luau.NewList(expression, luau.Num(1)))
	},

	// unshift (L641-654): `table.insert(exp, 1, arg)` per argument in REVERSE
	// order; returns the new length `#exp` when consumed.
	"unshift": func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		expression = s.PushToVarIfComplex(expression, "exp")

		for i := len(args) - 1; i >= 0; i-- {
			arg := args[i]
			s.Prereq(luau.NewCallStatement(
				luau.NewCall(luau.GlobalProperty("table", "insert"), luau.NewList(expression, luau.Num(1), arg)),
			))
		}

		if !isUsedAsStatement(node) {
			return luau.NewUnary("#", expression)
		}
		return luau.NewNone()
	},

	// insert (L656-658): `table.insert(exp, index + 1, value)`.
	"insert": func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		return luau.NewCall(luau.GlobalProperty("table", "insert"), luau.NewList(expression, offsetExpr(args[0], 1), args[1]))
	},

	// remove (L660): `table.remove(exp, index + 1)`.
	"remove": func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		return luau.NewCall(luau.GlobalProperty("table", "remove"), luau.NewList(expression, offsetExpr(args[0], 1)))
	},

	// unorderedRemove (L662-707): the index temp comes BEFORE the base temp
	// (upstream order); `local _value = exp[_index]` is created UNCONDITIONALLY
	// even when the value is unused; the swap-and-clear runs under
	// `if _value ~= nil then`.
	"unorderedRemove": func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		indexExp := s.PushToVarIfComplex(offsetExpr(args[0], 1), "index")

		expression = s.PushToVarIfComplex(expression, "exp")

		lengthId := s.PushToVar(luau.NewUnary("#", expression), "length")

		valueIsUsed := !isUsedAsStatement(node)
		valueId := s.PushToVar(
			luau.NewComputedIndex(convertToIndexableExpression(expression), indexExp),
			"value",
		)

		s.Prereq(luau.NewIf(
			luau.NewBinary(valueId, "~=", luau.Nil()),
			luau.NewList[luau.Statement](
				luau.NewAssignment(
					luau.NewComputedIndex(convertToIndexableExpression(expression), indexExp),
					"=",
					luau.NewComputedIndex(convertToIndexableExpression(expression), lengthId),
				),
				luau.NewAssignment(
					luau.NewComputedIndex(convertToIndexableExpression(expression), lengthId),
					"=",
					luau.Nil(),
				),
			),
			nil,
		))

		if valueIsUsed {
			return valueId
		}
		return luau.NewNone()
	},

	// sort (L709-724): `table.sort(exp[, compareFn])` CallStatement; returns
	// the (possibly temped) array itself when the value is consumed.
	"sort": func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		valueIsUsed := !isUsedAsStatement(node)
		if valueIsUsed {
			expression = s.PushToVarIfComplex(expression, "exp")
		}

		args = append([]luau.Expression{expression}, args...)

		s.Prereq(luau.NewCallStatement(
			luau.NewCall(luau.GlobalProperty("table", "sort"), luau.NewList(args...)),
		))

		if valueIsUsed {
			return expression
		}
		return luau.NewNone()
	},

	// clear (L726-733): `table.clear(exp)` CallStatement; `nil` when the value
	// is consumed.
	"clear": func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		s.Prereq(luau.NewCallStatement(
			luau.NewCall(luau.GlobalProperty("table", "clear"), luau.NewList[luau.Expression](expression)),
		))
		if !isUsedAsStatement(node) {
			return luau.Nil()
		}
		return luau.NewNone()
	},
}
