package transformer

import (
	"slices"

	"rotor/internal/luau"
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

// Emission sites of the experimental luauTypes mode: what a function's
// parameters, `...`, `self`, generics and return get annotated with, which
// locals get a `: T`, and how a defaulted parameter is refined. Every type
// here comes from the shared builder (luautypes_builder.go); this file only
// decides where the builder is asked and with which context.
//
// Two shapes were settled by probing luau-lsp 1.70.1 (new solver) and
// luau-analyze 0.730 on 2026-10-29:
//   - `return nil`, which rbxtsc emits for a bare `return`, is rejected
//     against a `()` return pack, and a `nil` return rejects a body that
//     falls off its end, so a void function with an explicit return is
//     spelled `...nil`, the one pack both shapes satisfy;
//   - neither solver refines `c: number?` through rbxtsc's
//     `if c == nil then c = 1 end` default, nor a `_condition` temp through
//     the `if _condition == nil then _condition = b end` chain of `a ?? b`,
//     so both are re-bound right after as `local c = c :: number`.

// luauTypedSignatures reports whether function signatures (parameters,
// `...`, `self`, generics, returns and the dot-form method shape) are spelled
// by the luauTypes builder. A `//!native` file keeps the native policy for
// its signatures (plan WP6, interaction rule) and luauTypes owns the rest.
func (state *State) luauTypedSignatures() bool {
	return state.IsLuauTyped && !state.IsNative
}

// ---------------------------------------------------------------------------
// Signatures
// ---------------------------------------------------------------------------

// getLuauParameterType is the annotation of one parameter: an identifier, or
// the `_param` temp a binding pattern is destructured from. A written type
// is spelled from its node (so `typeof x` survives); otherwise the checker's
// type. `?` and an initializer make it optional.
func getLuauParameterType(state *State, parameterNode *ast.Node) luau.TypeNode {
	parameter := parameterNode.AsParameterDeclaration()
	node := luauDeclaredType(state, parameter.Type, parameter.Name(), typeContext{Position: positionParameter, Owner: parameterNode})
	if parameter.QuestionToken != nil || parameter.Initializer != nil {
		node = luau.OptionalType(node)
	}
	return node
}

// getLuauBindingElementType is the annotation of a parameter flattened out of
// a `...[a, b]` rest pattern (optimizeArraySpreadParameter): the element's
// type, optional when it has a default.
func getLuauBindingElementType(state *State, element *ast.Node) luau.TypeNode {
	bindingElement := element.AsBindingElement()
	ctx := typeContext{Position: positionParameter, Owner: element}
	node := state.types().Build(state.Checker.GetTypeAtLocation(bindingElement.Name()), ctx).Node
	if bindingElement.Initializer != nil {
		node = luau.OptionalType(node)
	}
	return node
}

// getLuauVarargType is the `...: T` annotation of a function's rest
// parameter: the element type of its array type, `any` (with a loss) when
// the rest type is a tuple or a bare type parameter, nil when the function
// emits no `...` (no rest parameter, or a `...[a, b]` pattern flattened into
// plain parameters).
func getLuauVarargType(state *State, node *ast.Node) luau.TypeNode {
	for _, parameterNode := range node.Parameters() {
		parameter := parameterNode.AsParameterDeclaration()
		if parameter.DotDotDotToken == nil {
			continue
		}
		name := parameter.Name()
		if ast.IsArrayBindingPattern(name) && !arrayLikeExpressionContainsSpread(name) {
			return nil
		}
		ctx := typeContext{Position: positionParameter, Owner: parameterNode}
		restType := state.Checker.GetTypeAtLocation(name)
		if restType == nil {
			return state.types().Build(nil, ctx).Node
		}
		if element := state.Checker.GetElementTypeOfArrayType(restType); element != nil {
			return state.types().Build(element, ctx).Node
		}
		return state.types().lose(lossUnspellable, restType, anyType(), ctx, "a rest parameter that is not an array has no Luau spelling")
	}
	return nil
}

// getLuauReturnType is the annotation of a function's return. An async
// function's emitted body is the callback handed to TS.async, so it returns
// the settled value (GetPromisedTypeOfPromise); a generator keeps its
// `Generator<...>` reference, which the alias registry mirrors. A void
// function is `()` unless its body returns explicitly (see the file comment).
func getLuauReturnType(state *State, node *ast.Node) luau.TypeNode {
	signature := state.Checker.GetSignatureFromDeclaration(node)
	if signature == nil {
		return nil
	}
	returnType := state.Checker.GetReturnTypeOfSignature(signature)
	if ast.HasSyntacticModifier(node, ast.ModifierFlagsAsync) && returnType != nil {
		if promised := state.Checker.GetPromisedTypeOfPromise(returnType); promised != nil {
			returnType = promised
		}
	}
	pack := state.types().BuildReturnPack(returnType, typeContext{Position: positionReturn, Owner: node})
	if isEmptyTypePack(pack) && functionReturnsExplicitly(node) {
		return luau.NewTypeVariadic(luau.TypeName(nativeTypeNil))
	}
	return pack
}

func isEmptyTypePack(node luau.TypeNode) bool {
	pack, ok := node.(*luau.TypePack)
	return ok && pack.Elements.IsEmpty()
}

// functionReturnsExplicitly reports whether the function's emitted body has
// a `return` of its own: a ReturnStatement outside nested functions and
// classes that is not rerouted through a try statement (those return
// through `unpack`, which `()` accepts), or an expression body, which is an
// implicit `return <expression>`.
func functionReturnsExplicitly(node *ast.Node) bool {
	body := node.Body()
	if body == nil {
		return false
	}
	if !ast.IsBlock(body) {
		return true
	}
	var visit func(child *ast.Node) bool
	visit = func(child *ast.Node) bool {
		switch {
		case ast.IsReturnStatement(child):
			return !isReturnBlockedByTryStatement(child)
		case ast.IsFunctionLike(child), ast.IsClassLike(child):
			return false
		}
		return child.ForEachChild(visit)
	}
	return body.ForEachChild(visit)
}

// getLuauFunctionGenerics lists the type parameters a function declares, for
// `function f<K, V>`. An instance member of a class (a method or the
// constructor) re-declares the class's type parameters first, since Luau has
// no class-level generic scope (plan WP7); a parameter the member shadows is
// listed once, under the member's declaration. Statics cannot name the
// class's parameters in TS and declare only their own.
func getLuauFunctionGenerics(state *State, node *ast.Node) []luau.TypeGeneric {
	own := state.types().Generics(node)
	class := enclosingClassOfInstanceMember(node)
	if class == nil {
		return own
	}
	classGenerics := state.types().Generics(class)
	generics := make([]luau.TypeGeneric, 0, len(classGenerics)+len(own))
	for _, generic := range classGenerics {
		if !slices.ContainsFunc(own, func(existing luau.TypeGeneric) bool { return existing.Name == generic.Name }) {
			generics = append(generics, generic)
		}
	}
	return append(generics, own...)
}

// enclosingClassOfInstanceMember is the class a constructor, method or
// accessor declaration is an instance member of, or nil for a static member
// and any other function.
func enclosingClassOfInstanceMember(node *ast.Node) *ast.Node {
	if node.Parent == nil || !ast.IsClassLike(node.Parent) || ast.HasStaticModifier(node) {
		return nil
	}
	if ast.IsConstructorDeclaration(node) || ast.IsMethodDeclaration(node) || ast.IsAccessor(node) {
		return node.Parent
	}
	return nil
}

// getLuauSelfType is the annotation of the leading `self` parameter a method
// takes, or nil when this site does not decide it. A written `this`
// parameter wins; a function expression typed by a `this`-taking contextual
// signature takes that type; an object-literal method takes the literal's
// contextual type when the builder can name it (an interface or class
// alias) and `any` with a loss otherwise. A class member gets nil: the class
// transform passes the instance alias or `typeof(Class)` as the receiver
// (transformMethodDeclarationWithReceiver).
func getLuauSelfType(state *State, node *ast.Node) luau.TypeNode {
	b := state.types()
	ctx := typeContext{Position: positionParameter, Owner: node}
	if this := thisParameter(node); this != nil {
		return luauDeclaredType(state, this.AsParameterDeclaration().Type, this.Name(), ctx)
	}
	if thisType := contextualThisType(state, node); thisType != nil {
		return b.Build(thisType, ctx).Node
	}
	literal := objectLiteralOfMethod(node)
	if literal == nil {
		return nil
	}
	if contextual := state.Checker.GetContextualType(literal, checker.ContextFlagsNone); contextual != nil {
		contextual = state.Checker.GetNonNullableType(contextual)
		if reference, ok := b.BuildAliasReference(contextual, ctx); ok {
			return reference.Node
		}
		if symbol := contextual.Symbol(); symbol != nil && symbol.Flags&ast.SymbolFlagsClass != 0 && contextual.Flags()&checker.TypeFlagsObject != 0 {
			return b.Build(contextual, ctx).Node
		}
	}
	return b.loseText(lossObjectLiteralSelf, "this", anyType(), ctx, "an object literal method's receiver has no name")
}

// thisParameter is a function's written `this` parameter, or nil.
func thisParameter(node *ast.Node) *ast.Node {
	if parameters := node.Parameters(); len(parameters) > 0 && ast.IsThisIdentifier(parameters[0].Name()) {
		return parameters[0]
	}
	return nil
}

// contextualThisType is the `this` type of the signature a function
// expression is contextually typed by, or nil.
func contextualThisType(state *State, node *ast.Node) *checker.Type {
	if !ast.IsFunctionExpression(node) {
		return nil
	}
	contextual := state.Checker.GetContextualType(node, checker.ContextFlagsNone)
	if contextual == nil {
		return nil
	}
	for _, signature := range state.Checker.GetCallSignatures(contextual) {
		if this := signature.ThisParameter(); this != nil {
			return state.Checker.GetTypeOfSymbol(this)
		}
	}
	return nil
}

// objectLiteralOfMethod is the object literal a method declaration, or a
// function expression assigned to a property, belongs to; nil elsewhere.
func objectLiteralOfMethod(node *ast.Node) *ast.Node {
	parent := node.Parent
	if parent == nil {
		return nil
	}
	switch {
	case ast.IsMethodDeclaration(node) && ast.IsObjectLiteralExpression(parent):
		return parent
	case ast.IsFunctionExpression(node):
		if assignment := SkipUpwards(node).Parent; assignment != nil && ast.IsPropertyAssignment(assignment) &&
			assignment.Parent != nil && ast.IsObjectLiteralExpression(assignment.Parent) {
			return assignment.Parent
		}
	}
	return nil
}

// luauDeclaredType spells a declaration's type: from its written type node
// when there is one (BuildTypeNode keeps `typeof x`), else from the
// checker's type at the name.
func luauDeclaredType(state *State, typeNode, name *ast.Node, ctx typeContext) luau.TypeNode {
	if typeNode != nil {
		return state.types().BuildTypeNode(typeNode, ctx).Node
	}
	return state.types().Build(state.Checker.GetTypeAtLocation(name), ctx).Node
}

// luauDefaultRefinement is the statement that follows a parameter's default
// in luauTypes mode: `local c = c :: T`, a same-named shadow that gives the
// rest of the body the non-nil type TS gives it. nil when the parameter's
// type admits nil anyway, so there is nothing to refine.
func luauDefaultRefinement(state *State, name *ast.Node, owner *ast.Node, id luau.AnyIdentifier) luau.Statement {
	refined := state.types().Build(state.Checker.GetTypeAtLocation(name), typeContext{Position: positionLocal, Owner: owner}).Node
	if typeAdmitsNil(refined) {
		return nil
	}
	return luauRefinementShadow(id, refined)
}

// luauRefinedCondition re-binds the `_condition` temp of a logical chain
// (`a ?? b`, `a || b`, `a && b`) to the expression's TS type once the chain
// has assigned it: Luau types the temp as the union of everything assigned
// to it, so `partial.host ?? config.host` stays `string?` where TS says
// `string`. The shadow is skipped when the TS type admits nil, and the
// identifier to use for the value is returned.
func luauRefinedCondition(state *State, node *ast.Node, conditionID *luau.TemporaryIdentifier) luau.Expression {
	if !state.IsLuauTyped {
		return conditionID
	}
	refined := state.types().Build(state.GetType(node), typeContext{Position: positionLocal, Owner: node}).Node
	if typeAdmitsNil(refined) {
		return conditionID
	}
	shadow := luauRefinementShadow(conditionID, refined)
	state.Prereq(shadow)
	return shadow.Left.(luau.Expression)
}

// luauRefinementShadow is `local id = id :: refined`: a same-named local
// whose type is the cast's. The shadow infers its type from the cast, so a
// binding annotation on id (a parameter's `T?`) is not carried along.
func luauRefinementShadow(id luau.AnyIdentifier, refined luau.TypeNode) *luau.VariableDeclaration {
	shadow := cloneIdentifier(id)
	luau.SetBindingType(shadow, nil)
	return luau.NewVariableDeclaration(shadow, luau.NewCast(id, refined))
}

// typeAdmitsNil reports a spelling nil already satisfies: `T?`, `nil`,
// `any` and `unknown`.
func typeAdmitsNil(node luau.TypeNode) bool {
	switch t := node.(type) {
	case *luau.TypeOptional:
		return true
	case *luau.TypeReference:
		if t.Module != "" || (t.Arguments != nil && !t.Arguments.IsEmpty()) {
			return false
		}
		switch t.Name {
		case nativeTypeNil, nativeTypeAny, "unknown":
			return true
		}
	}
	return false
}

// signatureGenerics, signatureReturnType and signatureVarArgsType pick the
// annotation source for a function: the luauTypes builder, or the native
// emitter in a `//!native` file (nil outside one).
func signatureGenerics(state *State, node *ast.Node) []luau.TypeGeneric {
	if state.luauTypedSignatures() {
		return getLuauFunctionGenerics(state, node)
	}
	return getNativeFunctionGenerics(state, node)
}

func signatureReturnType(state *State, node *ast.Node) luau.TypeNode {
	if state.luauTypedSignatures() {
		return getLuauReturnType(state, node)
	}
	return getNativeReturnType(state, node)
}

func signatureVarArgsType(state *State, node *ast.Node) luau.TypeNode {
	if state.luauTypedSignatures() {
		return getLuauVarargType(state, node)
	}
	return nil
}

// typeGenericNames lists the names a generic list puts in scope.
func typeGenericNames(generics []luau.TypeGeneric) []string {
	if len(generics) == 0 {
		return nil
	}
	names := make([]string, len(generics))
	for index, generic := range generics {
		names[index] = generic.Name
	}
	return names
}

// ---------------------------------------------------------------------------
// Locals
// ---------------------------------------------------------------------------

// getLuauLocalType is the annotation of a local bound to identifier with
// right as its value (nil for a hoisted or uninitialized local), or nil to
// let Luau infer it. A written type is spelled from its node.
func getLuauLocalType(state *State, identifier *ast.Node, right luau.Expression) luau.TypeNode {
	symbol := state.Checker.GetSymbolAtLocation(identifier)
	if symbol == nil {
		return nil
	}
	if right == nil {
		// A hoisted class or enum local carries the shape its own transform
		// fills in later (luautypes_class.go).
		if hoisted := luauHoistedDeclarationType(state, symbol, identifier.Parent); hoisted != nil {
			return hoisted
		}
	}
	if !shouldAnnotateLocal(state, symbol, right) {
		return nil
	}
	ctx := typeContext{Position: positionLocal, Owner: identifier.Parent}
	if declaration := symbol.ValueDeclaration; declaration != nil && ast.IsVariableDeclaration(declaration) {
		if typeNode := declaration.AsVariableDeclaration().Type; typeNode != nil {
			return state.types().BuildTypeNode(typeNode, ctx).Node
		}
	}
	return state.types().Build(state.Checker.GetTypeOfSymbol(symbol), ctx).Node
}

// shouldAnnotateLocal decides whether a local gets a `: T`. Code with an
// initializer reads like hand-written Luau: the type comes from the value.
// A local is annotated only where that would lose or change its type:
//
//   - it has no initializer (a hoisted or uninitialized local);
//   - its written TS type is not the initializer's own (widened) type, such
//     as an upcast to a base class or an interface the value only satisfies,
//     because Luau would otherwise keep the narrower inferred type;
//   - its value is a function wrapped by `TS.async`, whose result is `any`
//     to Luau, so the annotation is the function's only signature.
//
// Classes, enums and namespaces are declared by their own transforms and
// are never annotated here.
func shouldAnnotateLocal(state *State, symbol *ast.Symbol, right luau.Expression) bool {
	if !state.IsLuauTyped || symbol.Flags&(ast.SymbolFlagsClass|ast.SymbolFlagsEnum|ast.SymbolFlagsModule) != 0 {
		return false
	}
	if right == nil {
		return true
	}
	if isRuntimeLibCall(right, "async") {
		return true
	}
	declaration := symbol.ValueDeclaration
	if declaration == nil || !ast.IsVariableDeclaration(declaration) {
		return false
	}
	variable := declaration.AsVariableDeclaration()
	if variable.Type == nil || variable.Initializer == nil {
		return false
	}
	written := state.Checker.GetTypeFromTypeNode(variable.Type)
	inferred := state.Checker.GetWidenedType(state.Checker.GetBaseTypeOfLiteralType(state.Checker.GetTypeAtLocation(variable.Initializer)))
	return !state.Checker.IsTypeAssignableTo(written, inferred) || !state.Checker.IsTypeAssignableTo(inferred, written)
}

// isRuntimeLibCall reports a direct call of `TS.<name>(...)`.
func isRuntimeLibCall(expression luau.Expression, name string) bool {
	call, ok := expression.(*luau.CallExpression)
	if !ok {
		return false
	}
	access, ok := call.Expression.(*luau.PropertyAccessExpression)
	if !ok || access.Name != name {
		return false
	}
	root, ok := access.Expression.(*luau.Identifier)
	return ok && root.Name == "TS"
}

// luauTypedBinding annotates the identifier a local is bound to. An
// identifier shared through SymbolToID (a loop variable's copy) is cloned
// first so the annotation stays on this binding alone; a clone keeps its
// temp id and renders under the same name. nil t leaves id as it is.
func luauTypedBinding(state *State, symbol *ast.Symbol, id luau.AnyIdentifier, t luau.TypeNode) luau.AnyIdentifier {
	if t == nil {
		return id
	}
	if shared := state.SymbolToID[symbol]; shared != nil && luau.AnyIdentifier(shared) == id {
		id = cloneIdentifier(id)
	}
	luau.SetBindingType(id, t)
	return id
}

// cloneIdentifier copies an identifier for a second binding site.
func cloneIdentifier(id luau.AnyIdentifier) luau.AnyIdentifier {
	clone, ok := luau.WithOrigin(id, id.Origin()).(luau.AnyIdentifier)
	if !ok {
		panic("transformer: cloneIdentifier: clone is not an identifier")
	}
	return clone
}
