package transformer

import (
	"rotor/internal/luau"
	"rotor/tsgo/ast"
)

// This file ports TSTransformer/macros/propertyCallMacros.ts — the
// math-operation region (makeMathMethod/OPERATOR_TO_NAME_MAP/makeMathSet,
// L12-38), the PROPERTY_CALL_MACROS table rows (L919-939), and the comment
// logic every property-call macro is wrapped in (header/footer/
// wasExpressionPushed/wrapComments, L941-1001). The String/ArrayLike method
// regions live in stringmacros.go; ReadonlyArray/Array in arraymacros.go and
// arraymacros2.go; ReadonlySet/Set/ReadonlyMap/Map/Promise in
// collectionmacros.go.
//
// The math interfaces are different: they are declared by @rbxts/types
// (include/macro_math.d.ts), NOT compiler-types — including the `Number`
// interface row, whose `idiv` member merges into the compiler-types Number
// from macro_math.d.ts. Without real registration the fallback misses them
// and `v.add(w)` silently emits a method call `v:add(w)` instead of `v + w`
// (found by the Phase 3a randomness re-smoke: damage-numbers.ts).

// makeMathMethod ports propertyCallMacros.ts makeMathMethod (L12-20): the
// method call compiles to a Luau binary operation. A non-simple right operand
// gets an explicit ParenthesizedExpression (which also truncates call
// multi-returns); left-operand precedence parens are the renderer's job via
// parent pointers (render/parens.go needsParentheses).
func makeMathMethod(operator luau.BinaryOperator) PropertyCallMacro {
	return func(s *State, node *ast.Node, expression luau.Expression, args []luau.Expression) luau.Expression {
		rhs := args[0]
		if !luau.IsSimple(rhs) {
			rhs = luau.NewParenthesized(rhs)
		}
		return luau.NewBinary(expression, operator, rhs)
	}
}

// operatorToNameMap ports OPERATOR_TO_NAME_MAP (L22-28).
var operatorToNameMap = map[luau.BinaryOperator]string{
	"+":  "add",
	"-":  "sub",
	"*":  "mul",
	"/":  "div",
	"//": "idiv",
}

// makeMathSet ports makeMathSet (L30-38).
func makeMathSet(operators ...luau.BinaryOperator) map[string]PropertyCallMacro {
	result := make(map[string]PropertyCallMacro, len(operators))
	for _, operator := range operators {
		methodName, ok := operatorToNameMap[operator]
		if !ok {
			panic("makeMathSet: no method name for operator " + string(operator)) // upstream assert
		}
		result[methodName] = makeMathMethod(operator)
	}
	return result
}

// propertyCallMacroTable ports PROPERTY_CALL_MACROS (propertyCallMacros.ts
// L919-939) — all upstream rows. The registration loop (macromanager.go)
// builds each class's methodMap from its OWN interface declarations only, so
// e.g. Set's row carries just add/delete/clear; calls to the inherited
// isEmpty/size/has/forEach resolve to the ReadonlySet method symbols through
// interface inheritance, hitting the ReadonlySet row's registrations.
// WeakSet/WeakMap declare no methods and have no rows.
var propertyCallMacroTable = map[string]map[string]PropertyCallMacro{
	// math classes
	"CFrame":       makeMathSet("+", "-", "*"),
	"UDim":         makeMathSet("+", "-"),
	"UDim2":        makeMathSet("+", "-"),
	"Vector2":      makeMathSet("+", "-", "*", "/", "//"),
	"Vector2int16": makeMathSet("+", "-", "*", "/"),
	"Vector3":      makeMathSet("+", "-", "*", "/", "//"),
	"Vector3int16": makeMathSet("+", "-", "*", "/"),
	"Number":       makeMathSet("//"),
	// rotor extension — upstream PROPERTY_CALL_MACROS has no `vector` row
	// (propertyCallMacros.ts L919-939). Luau's native `vector` mirrors
	// Vector3's math methods exactly, so `a.sub(b)` compiles to `a - b`
	// instead of the meaningless `a:sub(b)` (a vector value has no methods —
	// indexing one at runtime errors). The types side (`declare interface
	// vector` carrying the same add/sub/mul/div/idiv macro math API) ships
	// with newer @rbxts/types releases; see macromanager.go
	// optionalPropertyCallClasses for why its absence is tolerated instead of
	// failing the registration audit.
	"vector": makeMathSet("+", "-", "*", "/", "//"),

	"String":        stringCallbacks,
	"ArrayLike":     arrayLikeMethods,
	"ReadonlyArray": readonlyArrayMethods,
	"Array":         arrayMethods,
	"ReadonlySet":   readonlySetMethods,
	"Set":           setMethods,
	"ReadonlyMap":   readonlyMapMethods,
	"Map":           mapMethods,
	"Promise":       promiseMethods,
}

// ---------------------------------------------------------------------------
// comment logic — propertyCallMacros.ts L941-1001
// ---------------------------------------------------------------------------

// macroCommentHeader / macroCommentFooter port header/footer (L943-949):
// rendered `-- ▼ ReadonlyArray.reduce ▼` / `-- ▲ ReadonlyArray.reduce ▲`.
func macroCommentHeader(text string) *luau.Comment { return luau.NewComment(" ▼ " + text + " ▼") }
func macroCommentFooter(text string) *luau.Comment { return luau.NewComment(" ▲ " + text + " ▲") }

// wasExpressionPushed ports wasExpressionPushed (L951-963): the first prereq
// is a VariableDeclaration whose left is a single TemporaryIdentifier and
// whose right is pointer-identical to the INCOMING base expression — i.e. the
// macro began with `pushToVarIfComplex(expression, "exp")` on a complex base.
// That `local _exp = <base>` line is exempt from the comment markers (it
// stays ABOVE the header).
func wasExpressionPushed(statements *luau.List[luau.Statement], expression luau.Expression) bool {
	if statements.IsNonEmpty() {
		if firstStatement, ok := statements.Head.Value.(*luau.VariableDeclaration); ok {
			if !luau.IsList(firstStatement.Left) {
				if _, isTemp := firstStatement.Left.(*luau.TemporaryIdentifier); isTemp {
					if right, ok := firstStatement.Right.(luau.Expression); ok && right == expression {
						return true
					}
				}
			}
		}
	}
	return false
}

// wrapComments ports wrapComments (L965-994): capture the macro's prereqs;
// when it produced MORE THAN ONE statement after excluding the header-exempt
// base push, bracket them with `-- ▼ X ▼` / `-- ▲ X ▲` comment markers.
// Single-statement macros (and the zero-prereq majority) emit no markers —
// byte parity depends on this exactly.
func wrapComments(methodName string, callback PropertyCallMacro) PropertyCallMacro {
	return func(s *State, callNode *ast.Node, callExp luau.Expression, args []luau.Expression) luau.Expression {
		expression, prereqs := s.Capture(func() luau.Expression {
			return callback(s, callNode, callExp, args)
		})

		size := prereqs.Size()
		if size > 0 {
			// detect the case of `expression = state.pushToVarIfComplex(expression, "exp");` and put header after
			wasPushed := wasExpressionPushed(prereqs, callExp)
			var pushStatement luau.Statement
			if wasPushed {
				pushStatement, _ = prereqs.Shift()
				size--
			}
			if size > 1 {
				prereqs.Unshift(macroCommentHeader(methodName))
				if wasPushed && pushStatement != nil {
					prereqs.Unshift(pushStatement)
				}
				prereqs.Push(macroCommentFooter(methodName))
			} else if wasPushed && pushStatement != nil {
				prereqs.Unshift(pushStatement)
			}
		}

		s.PrereqList(prereqs)
		return expression
	}
}

// init applies the comment wrapping to every PROPERTY_CALL_MACROS entry,
// porting the module-init loop (L996-1001):
// `macroList[methodName] = wrapComments("ClassName.methodName", macro)`.
// CALL_MACROS / constructor / identifier macros are NOT comment-wrapped.
// The math macros are pure-expression (no prereqs), so wrapping them is a
// no-op on output — but they are wrapped anyway, exactly as upstream.
func init() {
	for className, macroList := range propertyCallMacroTable {
		for methodName, macro := range macroList {
			macroList[methodName] = wrapComments(className+"."+methodName, macro)
		}
	}
}
