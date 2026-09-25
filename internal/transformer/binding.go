package transformer

import (
	"rotor/internal/luau"
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

// This file ports the shared destructuring plumbing:
// nodes/binding/transformBindingName.ts, util/binding/
// getAccessorForBindingType.ts, and util/binding/objectAccessor.ts.
// The pattern transforms themselves live in bindingarray.go /
// bindingobject.go.

// ---------------------------------------------------------------------------
// transformBindingName — nodes/binding/transformBindingName.ts (L8-30)
// ---------------------------------------------------------------------------

// transformBindingName produces the single identifier bound by a BindingName:
// identifiers directly, patterns through a `_binding` temp whose destructure
// statements are appended to initializers (used by for-of loop headers;
// variable declarations go through transformVariableDeclaration instead).
func transformBindingName(s *State, name *ast.Node, initializers *luau.List[luau.Statement]) luau.AnyIdentifier {
	if ast.IsIdentifier(name) {
		return TransformIdentifierDefined(s, name)
	}
	id := luau.TempID("binding")
	initializers.PushList(s.CaptureStatements(func() {
		if ast.IsArrayBindingPattern(name) {
			transformArrayBindingPattern(s, name, id)
		} else {
			transformObjectBindingPattern(s, name, id)
		}
	}))
	return id
}

// isOmittedBindingElement is the rotor equivalent of upstream
// `ts.isOmittedExpression(element)` over ArrayBindingPattern elements: the
// tsgo parser represents an array-binding hole (`const [, x] = ...`) as a
// BindingElement with a nil name, not as strada's OmittedExpression node
// (parser.go parseArrayBindingElement: "These are all nil for a missing
// element").
func isOmittedBindingElement(element *ast.Node) bool {
	return ast.IsOmittedExpression(element) ||
		(ast.IsBindingElement(element) && element.AsBindingElement().Name() == nil)
}

// ---------------------------------------------------------------------------
// Accessor table — util/binding/getAccessorForBindingType.ts (COMPLETE)
// ---------------------------------------------------------------------------

// bindingAccessor reads one pattern position; state preserves iterator progress.
type bindingAccessor func(s *State, parentID luau.AnyIdentifier, index int, state *bindingState, isOmitted bool) luau.Expression

// arrayAccessor ports the array entry (L32-37): `parentId[index + 1]` with
// the +1 folded into the literal; an omitted element emits nothing.
func arrayAccessor(s *State, parentID luau.AnyIdentifier, index int, state *bindingState, isOmitted bool) luau.Expression {
	return luau.NewComputedIndex(parentID, luau.Num(float64(index+1)))
}

func stringAccessor(s *State, parentID luau.AnyIdentifier, index int, state *bindingState, isOmitted bool) luau.Expression {
	var id luau.AnyIdentifier
	if state.matcher == nil {
		id = s.PushToVar(
			luau.NewCall(luau.GlobalProperty("string", "gmatch"),
				luau.NewList[luau.Expression](parentID, luau.GlobalProperty("utf8", "charpattern"))),
			"matcher",
		)
		state.matcher = id
	} else {
		id = state.matcher
	}

	callExp := luau.NewCall(id, luau.NewList[luau.Expression]())

	if isOmitted {
		s.Prereq(luau.NewCallStatement(callExp))
		return luau.NewNone()
	}
	return callExp
}

// iterableFunctionLuaTupleAccessor ports the IterableFunction<LuaTuple<T>>
// entry (L105-117): value = `{ parentId() }` (the call's multiple returns
// packed); an omitted element calls the function as a statement to advance.
func iterableFunctionLuaTupleAccessor(s *State, parentID luau.AnyIdentifier, index int, state *bindingState, isOmitted bool) luau.Expression {
	callExp := luau.NewCall(parentID, luau.NewList[luau.Expression]())
	if isOmitted {
		s.Prereq(luau.NewCallStatement(callExp))
		return luau.NewNone()
	}
	return luau.NewArray(luau.NewList[luau.Expression](callExp))
}

// iterableFunctionAccessor ports the IterableFunction<T> entry (L119-131):
// value = `parentId()`; an omitted element calls as a statement to advance.
func iterableFunctionAccessor(s *State, parentID luau.AnyIdentifier, index int, state *bindingState, isOmitted bool) luau.Expression {
	callExp := luau.NewCall(parentID, luau.NewList[luau.Expression]())
	if isOmitted {
		s.Prereq(luau.NewCallStatement(callExp))
		return luau.NewNone()
	}
	return callExp
}

// noneAccessor stands in for upstream's `() => luau.none()` accessor returned
// after the noIterableIteration diagnostic was raised at dispatch.
func noneAccessor(s *State, parentID luau.AnyIdentifier, index int, state *bindingState, isOmitted bool) luau.Expression {
	return luau.NewNone()
}

// getAccessorForBindingType ports getAccessorForBindingType (L143-167): the
// 8-entry isDefinitelyType dispatch, in upstream order. Iterable<T> keeps
// upstream's own noIterableIteration error. The fallthrough is upstream's
// `assert(false, ...)`.
func getAccessorForBindingType(s *State, node *ast.Node, t *checker.Type, parentID luau.AnyIdentifier) (bindingAccessor, *bindingState) {
	state := &bindingState{}
	if IsDefinitelyType(s, t, IsArrayType(s)) {
		return arrayAccessor, state
	} else if IsDefinitelyType(s, t, IsStringType) {
		return stringAccessor, state
	} else if IsDefinitelyType(s, t, IsSetType(s)) {
		return iteratorBindingAccessor, collectionBindingState(s, parentID, false)
	} else if IsDefinitelyType(s, t, IsMapType(s)) || IsSharedTableType(s, t) {
		return iteratorBindingAccessor, collectionBindingState(s, parentID, true)
	} else if IsDefinitelyType(s, t, IsIterableFunctionLuaTupleType(s)) {
		return iterableFunctionLuaTupleAccessor, state
	} else if IsDefinitelyType(s, t, IsIterableFunctionType(s)) {
		return iterableFunctionAccessor, state
	} else if IsDefinitelyType(s, t, IsIterableType(s)) {
		s.Diags.Add(DiagNoIterableIteration(node))
		return noneAccessor, state
	} else if IsDefinitelyType(s, t, IsGeneratorType(s)) ||
		IsDefinitelyType(s, t, IsObjectType) ||
		node.Kind == ast.KindThisKeyword {
		return iteratorBindingAccessor, generatorBindingState(s, parentID)
	}
	panic("transformer: Destructuring not supported for type: " + s.Checker.TypeToString(t)) // upstream assert(false)
}

// ---------------------------------------------------------------------------
// objectAccessor — util/binding/objectAccessor.ts (L11-36)
// ---------------------------------------------------------------------------

// objectAccessor produces the read expression for one object-pattern
// property. t is the PARENT pattern's type.
//
// QUIRK (port verbatim): computed names get the +1 array adjustment, but
// literal numeric names do NOT — `{ 0: x }` over a tuple emits `parent[0]`
// while `{ [0]: x }` emits `parent[1]` when the parent is array-typed.
func objectAccessor(s *State, parentID luau.AnyIdentifier, t *checker.Type, name *ast.Node) luau.Expression {
	if name.Kind == ast.KindBigIntLiteral {
		s.Diags.Add(DiagNoBigInt(name))
		return luau.NewNone()
	}

	addIndexDiagnostics(s, name, s.GetType(name))

	if ast.IsIdentifier(name) {
		return luau.NewPropertyAccess(parentID, name.Text())
	} else if ast.IsComputedPropertyName(name) {
		return luau.NewComputedIndex(parentID,
			addOneIfArrayType(s, t, TransformExpression(s, name.AsComputedPropertyName().Expression)))
	} else if ast.IsNumericLiteral(name) || ast.IsStringLiteral(name) || ast.IsNoSubstitutionTemplateLiteral(name) {
		return luau.NewComputedIndex(parentID, TransformExpression(s, name))
	} else if ast.IsPrivateIdentifier(name) {
		s.Diags.Add(DiagNoPrivateIdentifier(name))
		return luau.NewNone()
	}
	panic("transformer: objectAccessor unexpected name kind: " + kindName(name.Kind)) // upstream assertNever
}
