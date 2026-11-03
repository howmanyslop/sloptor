package transformer

import (
	"rotor/internal/luau"
	"rotor/tsgo/ast"
)

type transformedFunctionBody struct {
	parameters   *luau.List[luau.AnyIdentifier]
	statements   *luau.List[luau.Statement]
	hasDotDotDot bool
	// generics are the function's own type parameters (luauTypes or
	// //!native), rendered as `<K, V>` on the emitted function.
	generics []luau.TypeGeneric
	// returnType is the return annotation, if any.
	returnType luau.TypeNode
	// varArgsType is the `...: T` annotation of the rest parameter
	// (luauTypes only), if any.
	varArgsType luau.TypeNode
}

// ---------------------------------------------------------------------------
// Functions — statements/transformFunctionDeclaration.ts,
// expressions/transformFunctionExpression.ts, nodes/transformMethodDeclaration.ts
// ---------------------------------------------------------------------------

// transformFunctionDeclaration ports transformFunctionDeclaration.ts (L13-76).
//
//   - bodiless overload signatures emit nothing;
//   - anonymous functions are only legal as `export default` and emit under
//     the literal name `default`;
//   - localize: an export-default anonymous function is always localized; a
//     NAMED function is localized unless its symbol was hoisted (a `local f`
//     already emitted by createHoistDeclaration — e.g. mutual recursion),
//     in which case the non-local `function f() end` assigns into it.
func transformFunctionDeclaration(s *State, node *ast.Node) *luau.List[luau.Statement] {
	declaration := node.AsFunctionDeclaration()

	// overload signatures emit nothing
	if declaration.Body == nil {
		return luau.NewList[luau.Statement]()
	}

	// NOTE ts.hasSyntacticModifier matches ANY selected flag, so a plain
	// `export function f` also sets isExportDefault — harmless upstream and
	// here, because the named branch below overrides localize and anonymous
	// declarations require a real `export default`. Ported verbatim.
	isExportDefault := ast.HasSyntacticModifier(node, ast.ModifierFlagsExportDefault)

	nameNode := declaration.Name()
	if nameNode == nil && !isExportDefault {
		panic("transformer: anonymous FunctionDeclaration must be export default") // upstream assert
	}

	var name luau.AnyIdentifier
	if nameNode != nil {
		ValidateIdentifier(s, nameNode)
		name = TransformIdentifierDefined(s, nameNode)
	} else {
		name = luau.ID("default")
	}

	body := transformFunctionBody(s, node)

	localize := isExportDefault
	if nameNode != nil {
		symbol := s.Checker.GetSymbolAtLocation(nameNode)
		if symbol == nil {
			panic("transformer: FunctionDeclaration name has no symbol") // upstream assert
		}
		localize = !s.IsHoisted[symbol]
	}

	isAsync := ast.HasSyntacticModifier(node, ast.ModifierFlagsAsync)

	if declaration.AsteriskToken != nil {
		if isAsync {
			s.Diags.Add(DiagNoAsyncGeneratorFunctions(node))
		}
		body.statements = wrapStatementsAsGenerator(s, node, body.statements)
	}

	// The async path REPLACES the FunctionDeclaration emit entirely: `local f
	// = TS.async(function() ... end)` when localized, `f = TS.async(...)` when
	// hoisted (the hoist machinery emitted the `local f` header at the
	// premature use; async function declarations are hoist-SENSITIVE — the
	// self-reference exemption in identifier.go excludes them). Generator
	// DECLARATIONS stay real FunctionDeclarations whose body is the
	// TS.generator return. With luauTypes on, the callback returns the
	// settled value and the local carries the full `(...) -> Promise<T>`
	// type, since TS.async itself is `any` to Luau.
	if isAsync {
		callback := luau.NewFunctionExpression(body.parameters, body.hasDotDotDot, body.statements)
		callback.Generics = body.generics
		callback.ReturnType = body.returnType
		callback.VarArgsType = body.varArgsType
		s.setNodeSourcePosition(callback, node)
		right := luau.NewCall(s.RuntimeLib(node, "async"), luau.NewList[luau.Expression](
			callback))
		if localize {
			if s.IsLuauTyped && nameNode != nil {
				if symbol := s.Checker.GetSymbolAtLocation(nameNode); symbol != nil {
					name = luauTypedBinding(s, symbol, name, getLuauLocalType(s, nameNode, right))
				}
			}
			return luau.NewList[luau.Statement](luau.NewVariableDeclaration(name, right))
		}
		return luau.NewList[luau.Statement](luau.NewAssignment(name, "=", right))
	}

	declarationNode := luau.NewFunctionDeclaration(localize, name, body.parameters, body.hasDotDotDot, body.statements)
	declarationNode.Generics = body.generics
	declarationNode.ReturnType = body.returnType
	declarationNode.VarArgsType = body.varArgsType
	return luau.NewList[luau.Statement](declarationNode)
}

// transformFunctionBody transforms a function's parameters and body. Its own
// type parameters are in scope while both are transformed, so a nested
// annotation can name them (and the return type can still use them).
func transformFunctionBody(s *State, node *ast.Node) transformedFunctionBody {
	generics := signatureGenerics(s, node)
	s.PushTypeGenerics(typeGenericNames(generics), false)
	defer s.PopTypeGenerics()

	parameters, statements, hasDotDotDot, varArgsData := transformParameters(s, node)
	restParam := registerOptimizableVarArgsForFunction(s, node, varArgsData)

	body := node.Body()
	if ast.IsBlock(body) {
		statements.PushList(TransformStatementList(s, body, body.AsBlock().Statements.Nodes, nil))
	} else {
		returnStatements := luau.NewList[luau.Statement]()
		prereqs := s.CaptureStatements(func() {
			returnStatements = transformReturnStatementInner(s, body)
		})
		s.setStatementListSourcePositions(body, prereqs, returnStatements)
		statements.PushList(prereqs)
		statements.PushList(returnStatements)
	}
	if restParam != nil {
		s.unregisterOptimizableVarArgs(restParam)
	}

	return transformedFunctionBody{
		parameters:   parameters,
		statements:   statements,
		hasDotDotDot: hasDotDotDot,
		generics:     generics,
		returnType:   signatureReturnType(s, node),
		varArgsType:  signatureVarArgsType(s, node),
	}
}

// transformFunctionExpression ports transformFunctionExpression.ts (L11-47):
// FunctionExpression and ArrowFunction share one transform. A named function
// expression is lifted to a local function declaration in ANY expression
// position — the prereq machinery already places the declaration where the
// expression is evaluated, so short-circuit operands and conditional arms
// stay conditional. Async names keep the declaration (the Luau debug name)
// and assign `name = TS.async(name)` so a self-call hits the wrapper.
// Generator names lift like a generator declaration: the body is
// `return TS.generator(...)`, so a self-call creates a new generator.
// Async generators still report noAsyncGeneratorFunctions. Arrow expression
// bodies reuse the full return transform with prereqs captured into the
// function body — that is the only implicit-return mechanism.
func transformFunctionExpression(s *State, node *ast.Node) luau.Expression {
	if ast.IsFunctionExpression(node) {
		if name := node.AsFunctionExpression().Name(); name != nil {
			ValidateIdentifier(s, name)
			identifier := luau.ExactTempID(name.Text())
			s.PrereqList(emitNamedFunctionExpression(
				s,
				node,
				identifier,
				true,
				identifier,
			))
			return identifier
		}
	}

	body := transformFunctionBody(s, node)

	isAsync := ast.HasSyntacticModifier(node, ast.ModifierFlagsAsync)

	var asteriskToken *ast.Node
	if ast.IsFunctionExpression(node) {
		asteriskToken = node.AsFunctionExpression().AsteriskToken
	}
	if asteriskToken != nil {
		if isAsync {
			s.Diags.Add(DiagNoAsyncGeneratorFunctions(node))
		}
		body.statements = wrapStatementsAsGenerator(s, node, body.statements)
	}

	functionExpression := luau.NewFunctionExpression(body.parameters, body.hasDotDotDot, body.statements)
	functionExpression.Generics = body.generics
	functionExpression.ReturnType = body.returnType
	functionExpression.VarArgsType = body.varArgsType
	var expression luau.Expression = functionExpression
	s.setNodeSourcePosition(expression, node)

	if isAsync {
		expression = luau.NewCall(s.RuntimeLib(node, "async"), luau.NewList[luau.Expression](expression))
	}

	return expression
}

// transformMethodDeclaration ports nodes/transformMethodDeclaration.ts
// (L14-106): object-literal methods (`{ m() {} }`, inline-map pointer) and
// class methods (identifier pointer) share this transform.
func transformMethodDeclaration(s *State, node *ast.Node, ptr *MapPointer) *luau.List[luau.Statement] {
	return transformMethodDeclarationWithReceiver(s, node, ptr, nil)
}

// transformMethodDeclarationWithReceiver is transformMethodDeclaration with
// the luauTypes type of `self` supplied by the caller: the class transform
// passes the instance alias (or `typeof(Class)` for a static member), which
// getLuauSelfType cannot name on its own. Only a written `this` parameter
// beats the receiver; nil, or any other mode, leaves `self` to
// getLuauSelfType.
//
// With luauTypes on, a method that would be `function obj:m(...)` is emitted
// in dot form with an explicit leading `self`, `function obj.m(self: T, ...)`:
// colon-form `self` cannot be annotated, and an unannotated `self` is a
// fresh table the checker never relates to the alias (probe P9b).
func transformMethodDeclarationWithReceiver(s *State, node *ast.Node, ptr *MapPointer, receiver luau.TypeNode) *luau.List[luau.Statement] {
	result := luau.NewList[luau.Statement]()

	declaration := node.AsMethodDeclaration()
	if declaration.Body == nil {
		return luau.NewList[luau.Statement]()
	}

	nameNode := declaration.Name()
	if nameNode == nil {
		panic("transformer: MethodDeclaration has no name") // upstream assert
	}
	if nameNode.Kind == ast.KindPrivateIdentifier {
		s.Diags.Add(DiagNoPrivateIdentifier(nameNode))
		return luau.NewList[luau.Statement]()
	}

	// A method is its own function: its type parameters are in scope for its
	// parameters, body, and return type.
	luauGenerics := signatureGenerics(s, node)
	s.PushTypeGenerics(typeGenericNames(luauGenerics), false)
	defer s.PopTypeGenerics()

	parameters, statements, hasDotDotDot, varArgsData := transformParameters(s, node)
	if receiver != nil && s.luauTypedSignatures() && parameters.Head != nil && thisParameter(node) == nil && isMethod(s, node) {
		luau.SetBindingType(parameters.Head.Value, receiver)
	}
	restParam := registerOptimizableVarArgsForFunction(s, node, varArgsData)
	statements.PushList(TransformStatementList(s, declaration.Body, declaration.Body.AsBlock().Statements.Nodes, nil))
	if restParam != nil {
		s.unregisterOptimizableVarArgs(restParam)
	}
	returnType := signatureReturnType(s, node)
	varArgsType := signatureVarArgsType(s, node)

	name := transformPropertyName(s, nameNode)

	// Decorator key pinning (upstream L36-49): a decorated method (or a method
	// with decorated parameters) records the object key the decorator
	// transforms re-read (transformDecorators, Phase 3c Task 3); a computed
	// non-literal key is pinned to a temp first so it only evaluates once.
	hasParameterDecorators := false
	for _, parameter := range node.Parameters() {
		if ast.HasDecorators(parameter) {
			hasParameterDecorators = true
			break
		}
	}
	if ast.HasDecorators(node) || hasParameterDecorators {
		if !luau.IsSimplePrimitive(name) {
			tempID := luau.TempID("key")
			result.Push(luau.NewVariableDeclaration(tempID, name))
			name = tempID
		}
		s.SetClassElementObjectKey(node, name)
	}

	isAsync := ast.HasSyntacticModifier(node, ast.ModifierFlagsAsync)

	// Generator wrap happens FIRST and does NOT exclude the method shape — a
	// generator method may still emit as `function Class:m()` whose body is
	// `return TS.generator(...)`.
	if declaration.AsteriskToken != nil {
		if isAsync {
			s.Diags.Add(DiagNoAsyncGeneratorFunctions(node))
		}
		statements = wrapStatementsAsGenerator(s, node, statements)
	}

	// can we use `function class:name() end`? — only when the pointer was
	// already spilled to a temp id (an inline map field can't hold a
	// function statement). Async methods are EXCLUDED (the !isAsync gate):
	// they fall to the map-pointer path with `self` kept in parameters
	// (`work = TS.async(function(self, n)`).
	nameStr, nameIsStr := name.(*luau.StringLiteral)
	_, ptrIsMap := ptr.Value.(*luau.Map)
	if !isAsync && nameIsStr && !ptrIsMap && luau.IsValidIdentifier(nameStr.Value) {
		if isMethod(s, node) && !s.luauTypedSignatures() {
			parameters.Shift() // remove `self`
			method := luau.NewMethodDeclaration(
				ptr.Value.(luau.IndexableExpression), nameStr.Value, parameters, hasDotDotDot, statements)
			method.Generics = luauGenerics
			method.ReturnType = returnType
			result.Push(method)
		} else {
			function := luau.NewFunctionDeclaration(
				false, /*localize*/
				luau.NewPropertyAccess(ptr.Value.(luau.IndexableExpression), nameStr.Value),
				parameters, hasDotDotDot, statements)
			function.Generics = luauGenerics
			function.ReturnType = returnType
			function.VarArgsType = varArgsType
			result.Push(function)
		}
		s.setNodeSourcePosition(result.Tail.Value, node)
		return result
	}

	methodExpression := luau.NewFunctionExpression(parameters, hasDotDotDot, statements)
	methodExpression.Generics = luauGenerics
	methodExpression.ReturnType = returnType
	methodExpression.VarArgsType = varArgsType
	var expression luau.Expression = methodExpression
	s.setNodeSourcePosition(expression, node)

	if isAsync {
		expression = luau.NewCall(s.RuntimeLib(node, "async"), luau.NewList[luau.Expression](expression))
	}

	// we have to use `class[name] = function()`
	result.PushList(s.CaptureStatements(func() {
		AssignToMapPointer(s, ptr, name, expression)
	}))

	return result
}
