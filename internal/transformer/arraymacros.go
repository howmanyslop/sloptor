package transformer

import (
	"rotor/internal/luau"
	"rotor/tsgo/ast"
)

// This file ports the ReadonlyArray region of
// TSTransformer/macros/propertyCallMacros.ts: makeEveryOrSomeMethod/
// makeEveryMethod/makeSomeMethod (L63-124) and READONLY_ARRAY_METHODS
// (L163-585, 15 entries). The Array region (ARRAY_METHODS, 9 entries) lives
// in arraymacros2.go; the table rows and the wrapComments machinery applied
// to every entry live in propertycallmacros.go.
//
// Callback invocation shape (every/some/forEach/map/mapFiltered/filter/find/
// findIndex): user callbacks receive `(value, index, array)` in JS — the
// generic-for yields the 1-based key, so the emitted call is
// `callback(_v, _k - 1, exp)` (offset(keyId, -1) folds onto literals).
// reduce's callback receives `(accumulator, value, index, array)` =
// `callback(_result, exp[_i], _i - 1, exp)` from a numeric for.

// makeEveryOrSomeMethod ports makeEveryOrSomeMethod (L63-124): build
//
//	local _result = <initialState>
//	for _k, _v in exp do
//	    if [not] _callback(<argsMaker(k, v, exp)>) then
//	        _result = <!initialState>
//	        break
//	    end
//	end
//
// `every` negates the callback result and flips _result to false; `some`
// uses it directly and flips _result to true.
func makeEveryOrSomeMethod(
	callbackArgsListMaker func(keyId, valueId *luau.TemporaryIdentifier, expression luau.Expression) []luau.Expression,
	initialState bool,
) PropertyCallMacro {
	return func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		expression = s.PushToVarIfComplex(expression, "exp")

		resultId := s.PushToVar(luau.Bool(initialState), "result")
		callbackId := s.PushToVarIfNonID(args[0], "callback")

		keyId := luau.TempID("k")
		valueId := luau.TempID("v")

		callCallback := luau.NewCall(callbackId, luau.NewList(callbackArgsListMaker(keyId, valueId, expression)...))
		var condition luau.Expression = callCallback
		if initialState {
			condition = luau.NewUnary("not", callCallback)
		}
		s.Prereq(luau.NewFor(
			luau.NewList[luau.AnyIdentifier](keyId, valueId),
			expression,
			luau.NewList[luau.Statement](
				luau.NewIf(
					condition,
					luau.NewList[luau.Statement](
						luau.NewAssignment(resultId, "=", luau.Bool(!initialState)),
						luau.NewBreak(),
					),
					nil,
				),
			),
		))

		return resultId
	}
}

// makeEveryMethod ports makeEveryMethod (L106-114).
func makeEveryMethod(
	callbackArgsListMaker func(keyId, valueId *luau.TemporaryIdentifier, expression luau.Expression) []luau.Expression,
) PropertyCallMacro {
	return makeEveryOrSomeMethod(callbackArgsListMaker, true)
}

// makeSomeMethod ports makeSomeMethod (L116-124).
func makeSomeMethod(
	callbackArgsListMaker func(keyId, valueId *luau.TemporaryIdentifier, expression luau.Expression) []luau.Expression,
) PropertyCallMacro {
	return makeEveryOrSomeMethod(callbackArgsListMaker, false)
}

// readonlyArrayMethods ports READONLY_ARRAY_METHODS (L163-585).
var readonlyArrayMethods = map[string]PropertyCallMacro{
	// isEmpty (L164): `#expression == 0`.
	"isEmpty": func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		return luau.NewBinary(luau.NewUnary("#", expression), "==", luau.Num(0))
	},

	// join (L166-200): default separator ", "; when the element type is not
	// definitely string-or-number, table.concat would error, so a tostring()
	// pre-pass copies the array first. Always `table.concat(exp, sep)`.
	"join": func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		args = argumentsWithDefaults(s, args, []luau.Expression{luau.Str(", ")})
		// upstream: state.typeChecker.getIndexTypeOfType(
		//     state.getType(node.expression.expression), ts.IndexKind.Number)
		// — the ELEMENT type of the array (node.expression.expression is the
		// base, not the call).
		indexType := s.Checker.GetIndexTypeOfType(
			s.GetType(node.AsCallExpression().Expression.Expression()),
			s.Checker.GetNumberType(),
		)

		// table.concat only works on string and number types, so call tostring() otherwise
		if indexType != nil && !IsDefinitelyType(s, indexType, IsStringType, IsNumberType) {
			expression = s.PushToVarIfComplex(expression, "exp")
			id := s.PushToVar(
				luau.NewCall(luau.GlobalProperty("table", "create"), luau.NewList[luau.Expression](luau.NewUnary("#", expression))),
				"result",
			)
			keyId := luau.TempID("k")
			valueId := luau.TempID("v")
			s.Prereq(luau.NewFor(
				luau.NewList[luau.AnyIdentifier](keyId, valueId),
				expression,
				luau.NewList[luau.Statement](
					luau.NewAssignment(
						luau.NewComputedIndex(id, keyId),
						"=",
						luau.NewCall(luau.GlobalID("tostring"), luau.NewList[luau.Expression](valueId)),
					),
				),
			))

			expression = id
		}

		return luau.NewCall(luau.GlobalProperty("table", "concat"), luau.NewList(expression, args[0]))
	},

	// move (L202-208): `table.move(exp, sourceStart+1, sourceEnd+1, dest+1[,
	// target])` — args[3] is the target ARRAY, not an index: no offset.
	"move": func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		moveArgs := []luau.Expression{expression, offsetExpr(args[0], 1), offsetExpr(args[1], 1), offsetExpr(args[2], 1)}
		if len(args) > 3 {
			moveArgs = append(moveArgs, args[3])
		}
		return luau.NewCall(luau.GlobalProperty("table", "move"), luau.NewList(moveArgs...))
	},

	// includes (L210-216): `table.find(exp, value[, fromIndex+1]) ~= nil`.
	"includes": func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		callArgs := []luau.Expression{expression, args[0]}
		if len(args) > 1 {
			callArgs = append(callArgs, offsetExpr(args[1], 1))
		}
		return luau.NewBinary(
			luau.NewCall(luau.GlobalProperty("table", "find"), luau.NewList(callArgs...)),
			"~=",
			luau.Nil(),
		)
	},

	// indexOf (L218-233): `(table.find(exp, value[, fromIndex+1]) or 0) - 1`
	// — the `- 1` offset appends to the raw `or 0` binary (the renderer
	// parenthesizes the lower-precedence `or` left operand).
	"indexOf": func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		findArgs := []luau.Expression{expression, args[0]}

		if len(args) > 1 {
			findArgs = append(findArgs, offsetExpr(args[1], 1))
		}

		return offsetExpr(
			luau.NewBinary(
				luau.NewCall(luau.GlobalProperty("table", "find"), luau.NewList(findArgs...)),
				"or",
				luau.Num(0),
			),
			-1,
		)
	},

	// every / some (L235-237): callback args `(value, index - 1, array)`.
	"every": makeEveryMethod(func(keyId, valueId *luau.TemporaryIdentifier, expression luau.Expression) []luau.Expression {
		return []luau.Expression{valueId, offsetExpr(keyId, -1), expression}
	}),

	"some": makeSomeMethod(func(keyId, valueId *luau.TemporaryIdentifier, expression luau.Expression) []luau.Expression {
		return []luau.Expression{valueId, offsetExpr(keyId, -1), expression}
	}),

	// forEach (L239-258): generic-for CallStatement `callback(_v, _k - 1, exp)`;
	// returns `nil` when the value is consumed, NOTHING in statement position.
	"forEach": func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		expression = s.PushToVarIfComplex(expression, "exp")

		callbackId := s.PushToVarIfNonID(args[0], "callback")
		keyId := luau.TempID("k")
		valueId := luau.TempID("v")
		s.Prereq(luau.NewFor(
			luau.NewList[luau.AnyIdentifier](keyId, valueId),
			expression,
			luau.NewList[luau.Statement](
				luau.NewCallStatement(
					luau.NewCall(callbackId, luau.NewList[luau.Expression](valueId, offsetExpr(keyId, -1), expression)),
				),
			),
		))

		if !isUsedAsStatement(node) {
			return luau.Nil()
		}
		return luau.NewNone()
	},

	// map (L260-287): `local _newValue = table.create(#exp)` then generic-for
	// `_newValue[_k] = callback(_v, _k - 1, exp)`.
	"map": func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		expression = s.PushToVarIfComplex(expression, "exp")
		newValueId := s.PushToVar(
			luau.NewCall(luau.GlobalProperty("table", "create"), luau.NewList[luau.Expression](luau.NewUnary("#", expression))),
			"newValue",
		)
		callbackId := s.PushToVarIfNonID(args[0], "callback")
		keyId := luau.TempID("k")
		valueId := luau.TempID("v")
		s.Prereq(luau.NewFor(
			luau.NewList[luau.AnyIdentifier](keyId, valueId),
			expression,
			luau.NewList[luau.Statement](
				luau.NewAssignment(
					luau.NewComputedIndex(newValueId, keyId),
					"=",
					luau.NewCall(callbackId, luau.NewList[luau.Expression](valueId, offsetExpr(keyId, -1), expression)),
				),
			),
		))

		return newValueId
	},

	// mapFiltered (L289-331): declaration order newValue, callback, length
	// (byte-relevant); body keeps only non-nil callback results.
	"mapFiltered": func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		expression = s.PushToVarIfComplex(expression, "exp")

		newValueId := s.PushToVar(luau.NewArray(luau.NewList[luau.Expression]()), "newValue")
		callbackId := s.PushToVarIfNonID(args[0], "callback")
		lengthId := s.PushToVar(luau.Num(0), "length")
		keyId := luau.TempID("k")
		valueId := luau.TempID("v")
		resultId := luau.TempID("result")
		s.Prereq(luau.NewFor(
			luau.NewList[luau.AnyIdentifier](keyId, valueId),
			expression,
			luau.NewList[luau.Statement](
				luau.NewVariableDeclaration(
					resultId,
					luau.NewCall(callbackId, luau.NewList[luau.Expression](valueId, offsetExpr(keyId, -1), expression)),
				),
				luau.NewIf(
					luau.NewBinary(resultId, "~=", luau.Nil()),
					luau.NewList[luau.Statement](
						luau.NewAssignment(lengthId, "+=", luau.Num(1)),
						luau.NewAssignment(luau.NewComputedIndex(newValueId, lengthId), "=", resultId),
					),
					nil,
				),
			),
		))

		return newValueId
	},

	// filterUndefined (L333-401): two passes — generic-for with ONE id to find
	// the max index, then numeric-for copying non-nil values.
	"filterUndefined": func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		expression = s.PushToVarIfComplex(expression, "exp")

		lengthId := s.PushToVar(luau.Num(0), "length")
		indexId1 := luau.TempID("i")
		s.Prereq(luau.NewFor(
			luau.NewList[luau.AnyIdentifier](indexId1),
			expression,
			luau.NewList[luau.Statement](
				luau.NewIf(
					luau.NewBinary(indexId1, ">", lengthId),
					luau.NewList[luau.Statement](
						luau.NewAssignment(lengthId, "=", indexId1),
					),
					nil,
				),
			),
		))

		resultId := s.PushToVar(luau.NewArray(luau.NewList[luau.Expression]()), "result")
		resultLengthId := s.PushToVar(luau.Num(0), "resultLength")
		indexId2 := luau.TempID("i")
		valueId := luau.TempID("v")
		s.Prereq(luau.NewNumericFor(
			indexId2,
			luau.Num(1),
			lengthId,
			nil,
			luau.NewList[luau.Statement](
				luau.NewVariableDeclaration(
					valueId,
					luau.NewComputedIndex(convertToIndexableExpression(expression), indexId2),
				),
				luau.NewIf(
					luau.NewBinary(valueId, "~=", luau.Nil()),
					luau.NewList[luau.Statement](
						luau.NewAssignment(resultLengthId, "+=", luau.Num(1)),
						luau.NewAssignment(luau.NewComputedIndex(resultId, resultLengthId), "=", valueId),
					),
					nil,
				),
			),
		))

		return resultId
	},

	// filter (L403-444): like mapFiltered, but the condition is the EXPLICIT
	// `callback(_v, _k - 1, exp) == true` comparison and the kept value is the
	// element itself.
	"filter": func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		expression = s.PushToVarIfComplex(expression, "exp")

		newValueId := s.PushToVar(luau.NewArray(luau.NewList[luau.Expression]()), "newValue")
		callbackId := s.PushToVarIfNonID(args[0], "callback")
		lengthId := s.PushToVar(luau.Num(0), "length")
		keyId := luau.TempID("k")
		valueId := luau.TempID("v")
		s.Prereq(luau.NewFor(
			luau.NewList[luau.AnyIdentifier](keyId, valueId),
			expression,
			luau.NewList[luau.Statement](
				luau.NewIf(
					luau.NewBinary(
						luau.NewCall(callbackId, luau.NewList[luau.Expression](valueId, offsetExpr(keyId, -1), expression)),
						"==",
						luau.Bool(true),
					),
					luau.NewList[luau.Statement](
						luau.NewAssignment(lengthId, "+=", luau.Num(1)),
						luau.NewAssignment(luau.NewComputedIndex(newValueId, lengthId), "=", valueId),
					),
					nil,
				),
			),
		))

		return newValueId
	},

	// reduce (L446-512): without an initialValue, guard the empty array with a
	// runtime `error(...)` (plain global, not the runtime lib), seed _result
	// from exp[1], and start the numeric-for at 2; with one, seed from args[1]
	// and start at 1. The callback is pushed with pushToVar (NOT IfNonId),
	// AFTER the result temp — byte order. step 1 is omitted from the loop.
	"reduce": func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		expression = s.PushToVarIfComplex(expression, "exp")

		var start = luau.Num(1)
		end := luau.NewUnary("#", expression)
		const step = 1

		lengthExp := luau.NewUnary("#", expression)

		var resultId *luau.TemporaryIdentifier
		// if there was no initialValue supplied
		if len(args) < 2 {
			s.Prereq(luau.NewIf(
				luau.NewBinary(lengthExp, "==", luau.Num(0)),
				luau.NewList[luau.Statement](
					luau.NewCallStatement(luau.NewCall(luau.GlobalID("error"), luau.NewList[luau.Expression](
						luau.Str("Attempted to call `ReadonlyArray.reduce()` on an empty array without an initialValue."),
					))),
				),
				nil,
			))
			resultId = s.PushToVar(
				luau.NewComputedIndex(convertToIndexableExpression(expression), start),
				"result",
			)
			start = offsetExpr(start, step)
		} else {
			resultId = s.PushToVar(args[1], "result")
		}
		callbackId := s.PushToVar(args[0], "callback")

		iteratorId := luau.TempID("i")
		s.Prereq(luau.NewNumericFor(
			iteratorId,
			start,
			end,
			nil, // step === 1 -> omitted
			luau.NewList[luau.Statement](
				luau.NewAssignment(
					resultId,
					"=",
					luau.NewCall(callbackId, luau.NewList[luau.Expression](
						resultId,
						luau.NewComputedIndex(convertToIndexableExpression(expression), iteratorId),
						offsetExpr(iteratorId, -1),
						expression,
					)),
				),
			),
		))

		return resultId
	},

	// find (L514-548): VALUELESS `local _result`; break on the first element
	// whose `callback(_v, _i - 1, exp) == true`.
	"find": func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		expression = s.PushToVarIfComplex(expression, "exp")

		callbackId := s.PushToVarIfNonID(args[0], "callback")
		loopId := luau.TempID("i")
		valueId := luau.TempID("v")
		resultId := s.PushToVar(nil, "result")

		s.Prereq(luau.NewFor(
			luau.NewList[luau.AnyIdentifier](loopId, valueId),
			expression,
			luau.NewList[luau.Statement](
				luau.NewIf(
					luau.NewBinary(
						luau.NewCall(callbackId, luau.NewList[luau.Expression](valueId, offsetExpr(loopId, -1), expression)),
						"==",
						luau.Bool(true),
					),
					luau.NewList[luau.Statement](
						luau.NewAssignment(resultId, "=", valueId),
						luau.NewBreak(),
					),
					nil,
				),
			),
		))

		return resultId
	},

	// findIndex (L550-584): like find, but `local _result = -1` and the
	// assignment stores the 0-based index `_i - 1`.
	"findIndex": func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		expression = s.PushToVarIfComplex(expression, "exp")

		callbackId := s.PushToVarIfNonID(args[0], "callback")
		loopId := luau.TempID("i")
		valueId := luau.TempID("v")
		resultId := s.PushToVar(luau.Num(-1), "result")

		s.Prereq(luau.NewFor(
			luau.NewList[luau.AnyIdentifier](loopId, valueId),
			expression,
			luau.NewList[luau.Statement](
				luau.NewIf(
					luau.NewBinary(
						luau.NewCall(callbackId, luau.NewList[luau.Expression](valueId, offsetExpr(loopId, -1), expression)),
						"==",
						luau.Bool(true),
					),
					luau.NewList[luau.Statement](
						luau.NewAssignment(resultId, "=", offsetExpr(loopId, -1)),
						luau.NewBreak(),
					),
					nil,
				),
			),
		))

		return resultId
	},
}
