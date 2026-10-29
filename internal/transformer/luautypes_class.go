package transformer

import (
	"slices"
	"strconv"
	"strings"

	"rotor/internal/luau"
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

// Classes and enums in luauTypes mode (plan WP7). A class becomes a
// structural instance alias plus a class table whose functions take an
// explicit, annotated `self`; an enum becomes a union (or `number`) alias
// plus an annotated object local. The shapes below were settled by probing
// luau-analyze 0.730 and luau-lsp 1.70.1 under both solvers (the probes live
// in testdata/luautypes/probes and are re-run by TestLuauTypesClassIdiomProbes):
//
//   - The class table is initialized on its declaration line
//     (`local Foo = setmetatable({}, {...})`): rbxtsc's `local Foo` followed by
//     `do Foo = ... end` types Foo as `T?` inside every function of the block.
//     The `super` local is declared right before it so the heritage
//     expression is still evaluated once.
//   - `constructor` stays out of the instance alias and is called through the
//     class table (`Foo.constructor(self, x) or self`): with it in the alias,
//     differing constructor arities break `Derived <: Base`.
//   - Inherited and overridden methods keep `self` typed as the class that
//     first declared them, so width subtyping gives `Derived <: Base` the way
//     TS does; redeclaring `self` as the derived class breaks it (`self` is
//     contravariant).
//   - Every function re-declares the class generics (`function Box.get<T>(self:
//     Box<T>): T`): Luau has no class-level generic scope. Statics take
//     `self: typeof(Foo)`.
//   - A class referenced before its declaration keeps rbxtsc's hoisted
//     `local Foo`, annotated `any` with a loss: the class table has no type
//     Luau can name before the table exists.
//
// The instance alias is registered through the shared registry
// (luautypes_alias.go), so a class from another file is mirrored as a
// hoisted alias like any other imported type, and a class expression's alias
// is hoisted under the class name, the assigned variable's name, or
// AnonymousClass_N.

// ---------------------------------------------------------------------------
// Instance alias body
// ---------------------------------------------------------------------------

// buildClassInstance spells a class instance: at the top of the class's own
// alias body it is the structural member table; everywhere else a reference
// to the alias, registered on first use so no reference dangles. A class the
// spelling declined is spelled structurally with `self: any`.
func (b *typeBuilder) buildClassInstance(t *checker.Type, ctx typeContext) builtType {
	symbol := t.Symbol()
	entry := b.aliasFor(symbol)
	if entry == nil {
		return b.finish(b.buildClassInstanceTable(t, ctx), false)
	}
	if entry.building && ctx.Position == positionAliasBody {
		table := b.buildClassInstanceTable(t, ctx)
		b.Origins[table] = symbol
		return b.finish(table, false)
	}
	return b.finish(b.classReference(entry, classTypeArguments(b.state, t), ctx), false)
}

// ClassInstanceReference is the reference to a class's instance alias with
// the class's own type parameters as arguments, fresh for each annotation
// site; `any` with a loss when the class has no alias.
func (b *typeBuilder) ClassInstanceReference(t *checker.Type, ctx typeContext) luau.TypeNode {
	entry := b.aliasFor(t.Symbol())
	if entry == nil {
		return b.lose(lossUnspellable, t, anyType(), ctx, "the class has no instance alias")
	}
	return b.classReference(entry, classTypeArguments(b.state, t), ctx)
}

// classReference spells `Name<A, B>` for a registered class alias. A
// reference back into the alias while its body is being built must pass the
// alias's own type parameters, as every recursive Luau alias must.
func (b *typeBuilder) classReference(entry *typeAliasEntry, arguments []*checker.Type, ctx typeContext) luau.TypeNode {
	if entry.building && !recursiveArgsMatch(entry, arguments) {
		return b.loseText(lossRecursiveAliasArgs, entry.Name, anyType(), ctx, "a recursive alias must be referenced with its own type arguments")
	}
	reference := luau.NewTypeReference(entry.Name, b.BuildTypeArgs(declaredArguments(entry, arguments), ctx.nested()))
	b.Origins[reference] = entry.Symbol
	return reference
}

// classTypeArguments are the type arguments of a class instance type that
// belong to the class's own type parameters: a declared generic class
// carries its parameters, an instantiation its arguments, and the checker
// may append the `this` type after them.
func classTypeArguments(state *State, t *checker.Type) []*checker.Type {
	arguments := getTypeArguments(state, t)
	symbol := t.Symbol()
	if symbol == nil {
		return arguments
	}
	if count := len(state.Checker.GetLocalTypeParametersOfClassOrInterfaceOrTypeAlias(symbol)); len(arguments) > count {
		arguments = arguments[:count]
	}
	return arguments
}

// buildClassInstanceTable spells the members of a class instance as the body
// of its alias: every property and method, inherited ones included, with
// `constructor` and symbol-keyed members left out. A method's `self` is the
// instance alias of the class that first declared it (see the file comment);
// `readonly` is dropped and private/protected members are exposed, one loss
// each per class; index signatures become the indexer.
func (b *typeBuilder) buildClassInstanceTable(t *checker.Type, ctx typeContext) *luau.TypeTable {
	fields := luau.NewList[luau.TypeTableField]()
	nested := ctx.nested()
	symbolKeyLost, readonlyLost, privateLost := false, false, false
	for _, property := range b.state.Checker.GetPropertiesOfType(t) {
		name := property.Name
		if name == constructorMethodName {
			continue
		}
		if isSymbolKeyedMember(name) {
			if !symbolKeyLost {
				b.record(lossSymbolKey, name, nil, ctx, "symbol-keyed members are skipped")
				symbolKeyLost = true
			}
			continue
		}
		propertyType := b.state.Checker.GetTypeOfSymbol(property)
		callable := b.state.Checker.GetNonNullableType(propertyType)
		var value luau.TypeNode
		if property.Flags&ast.SymbolFlagsMethod != 0 && len(b.state.Checker.GetCallSignatures(callable)) > 0 {
			value = b.BuildCallable(callable, b.classMemberReceiver(t, name, nested), nested)
		} else {
			value = b.Build(propertyType, nested).Node
		}
		if property.Flags&ast.SymbolFlagsOptional != 0 {
			value = luau.OptionalType(value)
		}
		if !readonlyLost && isReadonlyProperty(property) {
			b.loseText(lossClassReadonlyDropped, name, value, ctx, "class constructors write through self, so readonly members stay writable")
			readonlyLost = true
		}
		if !privateLost && checker.GetDeclarationModifierFlagsFromSymbol(property)&(ast.ModifierFlagsPrivate|ast.ModifierFlagsProtected) != 0 {
			b.loseText(lossPrivateExposed, name, value, ctx, "private and protected members are plain table fields in Luau")
			privateLost = true
		}
		fields.Push(luau.NewTypeTableProp(luau.TypeAccessNone, name, value))
	}
	if indexer := b.BuildIndexer(t, ctx); indexer != nil {
		fields.Push(indexer)
	}
	return luau.NewTypeTable(fields)
}

// isSymbolKeyedMember reports a computed or ES-symbol member name (tsgo
// spells a known symbol as `\xFE@iterator@<id>`), which has no Luau spelling.
func isSymbolKeyedMember(name string) bool {
	return strings.HasPrefix(name, "__@") || strings.HasPrefix(name, ast.InternalSymbolNamePrefix)
}

// classMemberReceiver is the `self` type of a method as listed in the alias
// of instance type t: the alias of the type that first declared the method
// (the root of t's `extends` chain that has it, then the interface it
// implements or extends that has it), with the type arguments t passes down
// to it. Luau compares table properties invariantly, so a derived alias
// must list an inherited or overridden method exactly as its base does for
// `Derived <: Base` to hold. The chain is walked through each type's
// declared supertypes; an argument the walk cannot thread (a compound
// expression over a generic type's parameter two levels up) widens to `any`
// with a loss.
func (b *typeBuilder) classMemberReceiver(t *checker.Type, name string, ctx typeContext) luau.TypeNode {
	symbol, arguments := t.Symbol(), classTypeArguments(b.state, t)
	for depth := 0; symbol != nil && depth < luauTypeMaxDepth; depth++ {
		next, ok := b.declaringSupertype(symbol, arguments, name, depth > 0, ctx)
		if !ok {
			break
		}
		symbol, arguments = next.symbol, next.arguments
	}
	entry := b.aliasFor(symbol)
	if entry == nil {
		symbol, arguments = t.Symbol(), classTypeArguments(b.state, t)
		if entry = b.aliasFor(symbol); entry == nil {
			return b.lose(lossUnspellable, t, anyType(), ctx, "the class has no instance alias")
		}
	}
	return b.classReference(entry, arguments, ctx)
}

// heritage is one supertype a class or interface declares, with the type
// arguments the subtype passes to it.
type heritage struct {
	symbol    *ast.Symbol
	arguments []*checker.Type
}

// declaringSupertype is the first supertype of symbol that declares a member
// called name: its base class before its implemented interfaces, in clause
// order.
func (b *typeBuilder) declaringSupertype(symbol *ast.Symbol, arguments []*checker.Type, name string, substitute bool, ctx typeContext) (heritage, bool) {
	for _, super := range b.supertypes(symbol, arguments, substitute, ctx) {
		declared := b.state.Checker.GetDeclaredTypeOfSymbol(super.symbol)
		if declared != nil && b.state.Checker.GetPropertyOfType(declared, name) != nil {
			return super, true
		}
	}
	return heritage{}, false
}

// supertypes lists the classes and interfaces symbol extends and
// implements, each with the arguments symbol passes to it. Base types are
// read off the declared type (the only form GetBaseTypes accepts) and the
// implements clauses off the class declaration, so they name symbol's own
// parameters: at the first level those are exactly arguments, deeper down
// a parameter is substituted by identity and a compound argument over a
// parameter has no cheap instantiation.
func (b *typeBuilder) supertypes(symbol *ast.Symbol, arguments []*checker.Type, substitute bool, ctx typeContext) []heritage {
	declared := b.state.Checker.GetDeclaredTypeOfSymbol(symbol)
	if declared == nil || declared.Flags()&checker.TypeFlagsObject == 0 || declared.ObjectFlags()&checker.ObjectFlagsClassOrInterface == 0 {
		return nil
	}
	candidates := slices.Clone(b.state.Checker.GetBaseTypes(declared))
	for _, declaration := range symbol.Declarations {
		if ast.IsClassLike(declaration) {
			for _, node := range ast.GetImplementsTypeNodes(declaration) {
				candidates = append(candidates, b.state.Checker.GetTypeFromTypeNode(node))
			}
		}
	}
	parameters := b.state.Checker.GetLocalTypeParametersOfClassOrInterfaceOrTypeAlias(symbol)
	var result []heritage
	for _, candidate := range candidates {
		if candidate == nil || candidate.Flags()&checker.TypeFlagsObject == 0 {
			continue
		}
		superSymbol := candidate.Symbol()
		if superSymbol == nil || superSymbol.Flags&(ast.SymbolFlagsClass|ast.SymbolFlagsInterface) == 0 {
			continue
		}
		superArguments := classTypeArguments(b.state, candidate)
		if substitute && len(parameters) > 0 {
			superArguments = b.threadTypeArguments(parameters, arguments, superArguments, ctx)
		}
		result = append(result, heritage{symbol: superSymbol, arguments: superArguments})
	}
	return result
}

// threadTypeArguments replaces, in a supertype's arguments, each of the
// subtype's own parameters by the argument the subtype received for it.
func (b *typeBuilder) threadTypeArguments(parameters, arguments, superArguments []*checker.Type, ctx typeContext) []*checker.Type {
	threaded := make([]*checker.Type, len(superArguments))
	for index, argument := range superArguments {
		switch position := slices.Index(parameters, argument); {
		case position >= 0 && position < len(arguments):
			threaded[index] = arguments[position]
		case argument.Flags()&(checker.TypeFlagsObject|checker.TypeFlagsUnionOrIntersection|checker.TypeFlagsInstantiable) != 0:
			b.lose(lossUnspellable, argument, anyType(), ctx, "a supertype argument over a generic type's parameter two levels up is not instantiated")
			threaded[index] = b.state.Checker.GetAnyType()
		default:
			threaded[index] = argument
		}
	}
	return threaded
}

// anonymousClassName numbers the alias of a class expression that neither
// has a name nor is assigned to a variable.
func (b *typeBuilder) anonymousClassName() string {
	b.anonymousClassCount++
	return "AnonymousClass_" + strconv.Itoa(b.anonymousClassCount)
}

// classExpressionAliasName is the base name of a class expression's alias:
// the name of the variable it initializes, else AnonymousClass_N.
func classExpressionAliasName(state *State, symbol *ast.Symbol) string {
	if declaration := symbol.ValueDeclaration; declaration != nil && ast.IsClassExpression(declaration) {
		if parent := declaration.Parent; parent != nil && ast.IsVariableDeclaration(parent) {
			if name := parent.Name(); name != nil && ast.IsIdentifier(name) {
				return name.Text()
			}
		}
	}
	return state.types().anonymousClassName()
}

// luauHoistedDeclarationType annotates the hoisted `local` of a class or
// enum referenced before its declaration: the enum's object type, and `any`
// for a class (its table has no type Luau can name before the table
// exists). nil for every other symbol.
func luauHoistedDeclarationType(state *State, symbol *ast.Symbol, owner *ast.Node) luau.TypeNode {
	switch {
	case symbol.Flags&ast.SymbolFlagsEnum != 0 && state.IsLuauTyped:
		if declaration := symbol.ValueDeclaration; declaration != nil && ast.IsEnumDeclaration(declaration) {
			return luauEnumObjectType(state, declaration)
		}
	case symbol.Flags&ast.SymbolFlagsClass != 0 && state.luauTypedSignatures():
		ctx := typeContext{Position: positionLocal, Owner: owner}
		return state.types().loseText(lossUnspellable, symbol.Name, anyType(), ctx, "a class referenced before its declaration has no type Luau can name there")
	}
	return nil
}

// ---------------------------------------------------------------------------
// Class emission
// ---------------------------------------------------------------------------

// luauClass is what the class transform needs to emit one class in luauTypes
// mode: its instance alias, generics and the identifier of its table.
type luauClass struct {
	state        *State
	node         *ast.Node
	name         luau.AnyIdentifier
	instanceType *checker.Type
	generics     []luau.TypeGeneric
}

// newLuauClass registers the class's instance alias (placed at the
// declaration, or hoisted for a class expression) and collects its generics.
func newLuauClass(s *State, node *ast.Node, name luau.AnyIdentifier) *luauClass {
	instanceType := s.Checker.GetDeclaredTypeOfSymbol(node.Symbol())
	s.types().aliasFor(instanceType.Symbol())
	return &luauClass{
		state:        s,
		node:         node,
		name:         name,
		instanceType: instanceType,
		generics:     s.types().Generics(node),
	}
}

// genericNames lists the class generics for the scope pushed around the
// class body.
func (c *luauClass) genericNames() []string {
	return typeGenericNames(c.generics)
}

// functionGenerics is the generic list every instance function of the class
// re-declares, copied so each declaration adopts its own.
func (c *luauClass) functionGenerics() []luau.TypeGeneric {
	return slices.Clone(c.generics)
}

// instanceReference is a fresh `Foo<T...>` for one annotation site.
func (c *luauClass) instanceReference() luau.TypeNode {
	return c.state.types().ClassInstanceReference(c.instanceType, typeContext{Position: positionParameter, Owner: c.node})
}

// classReference is a fresh `typeof(Foo)`, the type statics take as `self`.
func (c *luauClass) classReference() luau.TypeNode {
	return luau.NewTypeTypeof(cloneIdentifier(c.name))
}

// receiverFor is the `self` type of a class member: `typeof(Foo)` for a
// static member, the instance alias otherwise.
func (c *luauClass) receiverFor(member *ast.Node) luau.TypeNode {
	if ast.HasStaticModifier(member) {
		return c.classReference()
	}
	return c.instanceReference()
}

// selfParameter is the leading `self: Foo<T...>` of a constructor or
// metamethod.
func (c *luauClass) selfParameter() luau.AnyIdentifier {
	self := luau.GlobalID("self")
	luau.SetBindingType(self, c.instanceReference())
	return self
}

// boilerplate is the luauTypes form of createClassBoilerplate: the
// statements that declare the class table (outer, at the declaration
// line, or inside the block for a named class expression whose internal
// name must not leak) and the statements that open the block
// (`Foo.__index = Foo`, `Foo.new`). The abstract-class quirks of the rbxtsc
// shape are kept: an abstract class without `extends` is a plain table, and
// no abstract class gets `new`.
func (c *luauClass) boilerplate(isClassExpression bool) (outer, inner *luau.List[luau.Statement]) {
	s, node, className := c.state, c.node, c.name
	outer = luau.NewList[luau.Statement]()
	inner = luau.NewList[luau.Statement]()
	isAbstract := ast.HasAbstractModifier(node)
	extendsNode := getExtendsNode(node)
	plainTable := isAbstract && extendsNode == nil

	var value luau.Expression
	if plainTable {
		value = luau.NewMap(luau.NewList[*luau.MapField]())
	} else {
		metatableFields := luau.NewList[*luau.MapField]()
		tostringName := "Anonymous"
		if id, ok := className.(*luau.Identifier); ok {
			tostringName = id.Name
		}
		metatableFields.Push(luau.NewMapField(luau.Str("__tostring"), createClassNameFunction(tostringName)))
		if extendsNode != nil {
			extendsExp, extendsExpPrereqs := s.Capture(func() luau.Expression {
				return TransformExpression(s, extendsNode.Expression())
			})
			superID := luau.GlobalID("super")
			outer.PushList(extendsExpPrereqs)
			outer.Push(luau.NewVariableDeclaration(superID, extendsExp))
			metatableFields.Push(luau.NewMapField(luau.Str("__index"), superID))
		}
		value = luau.NewCall(luau.GlobalID("setmetatable"), luau.NewList[luau.Expression](
			luau.NewMap(luau.NewList[*luau.MapField]()),
			luau.NewMap(metatableFields),
		))
	}

	switch {
	case isClassExpression && node.Name() != nil:
		// The named class expression's inner name is a local of the block.
		inner.Push(luau.NewVariableDeclaration(TransformIdentifierDefined(s, node.Name()), value))
	case isClassHoisted(s, node):
		outer.Push(luau.NewAssignment(className, "=", value))
	default:
		outer.Push(luau.NewVariableDeclaration(className, value))
	}

	if !plainTable {
		inner.Push(luau.NewAssignment(luau.NewPropertyAccess(className, "__index"), "=", className))
	}
	if !isAbstract {
		inner.Push(c.newFunction())
	}
	return outer, inner
}

// newFunction is `function Foo.new<T...>(params): Foo<T...>` constructing
// the instance through the class table's constructor.
func (c *luauClass) newFunction() *luau.FunctionDeclaration {
	className := c.name
	parameters, hasDotDotDot, varArgsType, arguments := c.constructorArity().parameters()

	statements := luau.NewList[luau.Statement]()
	//	local self = (setmetatable({}, className) :: any) :: Foo<T...>
	statements.Push(luau.NewVariableDeclaration(
		luau.GlobalID("self"),
		luau.NewCast(luau.NewCast(
			luau.NewCall(luau.GlobalID("setmetatable"), luau.NewList[luau.Expression](
				luau.NewMap(luau.NewList[*luau.MapField]()),
				className,
			)),
			anyType()), c.instanceReference()),
	))
	//	return className.constructor(self, ...) or self
	arguments.Unshift(luau.GlobalID("self"))
	statements.Push(luau.NewReturn(luau.NewBinary(
		luau.NewCall(luau.NewPropertyAccess(className, constructorMethodName), arguments),
		"or",
		luau.GlobalID("self"),
	)))

	function := luau.NewFunctionDeclaration(false, luau.NewPropertyAccess(className, "new"), parameters, hasDotDotDot, statements)
	function.Generics = c.functionGenerics()
	function.ReturnType = c.instanceReference()
	function.VarArgsType = varArgsType
	return function
}

// constructorFunction wraps a transformed constructor body as
// `function Foo.constructor<T...>(self: Foo<T...>, params)`.
func (c *luauClass) constructorFunction(parameters *luau.List[luau.AnyIdentifier], hasDotDotDot bool, statements *luau.List[luau.Statement], varArgsType luau.TypeNode) *luau.FunctionDeclaration {
	parameters.Unshift(c.selfParameter())
	function := luau.NewFunctionDeclaration(false, luau.NewPropertyAccess(c.name, constructorMethodName), parameters, hasDotDotDot, statements)
	function.Generics = c.functionGenerics()
	function.VarArgsType = varArgsType
	return function
}

// implicitConstructor is the luauTypes form of
// transformImplicitClassConstructor: a derived class forwards the base
// constructor's parameters (`...` when the base has overloads), then the
// property initializers run.
func (c *luauClass) implicitConstructor() *luau.List[luau.Statement] {
	s, node := c.state, c.node
	statements := luau.NewList[luau.Statement]()
	parameters := luau.NewList[luau.AnyIdentifier]()
	hasDotDotDot := false
	var varArgsType luau.TypeNode
	if getExtendsNode(node) != nil {
		var arguments *luau.List[luau.Expression]
		parameters, hasDotDotDot, varArgsType, arguments = c.constructorArity().parameters()
		arguments.Unshift(luau.GlobalID("self"))
		statements.Push(luau.NewCallStatement(luau.NewCall(
			luau.NewPropertyAccess(luau.GlobalID("super"), constructorMethodName), arguments)))
	}
	statements.PushList(transformPropertyInitializers(s, node))
	return luau.NewList[luau.Statement](c.constructorFunction(parameters, hasDotDotDot, statements, varArgsType))
}

// toStringFunction is `function Foo.__tostring<T...>(self: Foo<T...>): string`
// forwarding to the TS toString method.
func (c *luauClass) toStringFunction() luau.Statement {
	function := luau.NewFunctionDeclaration(
		false,
		luau.NewPropertyAccess(c.name, "__tostring"),
		luau.NewList[luau.AnyIdentifier](c.selfParameter()),
		false,
		luau.NewList[luau.Statement](luau.NewReturn(
			luau.NewMethodCall(magicToStringMethod, luau.GlobalID("self"), luau.NewList[luau.Expression]()),
		)),
	)
	function.Generics = c.functionGenerics()
	function.ReturnType = luau.TypeName(nativeTypeString)
	return function
}

// constructorArity is how `new` and an implicit constructor take their
// arguments: from the constructor declaration, from the single construct
// signature a derived class inherits, or as a bare `...` when the base has
// overloads.
func (c *luauClass) constructorArity() luauConstructorArity {
	arity := luauConstructorArity{class: c}
	if constructor := findConstructor(c.node); constructor != nil {
		arity.declaration = constructor
		return arity
	}
	classType := c.state.Checker.GetTypeOfSymbolAtLocation(c.node.Symbol(), c.node)
	if signatures := c.state.Checker.GetConstructSignatures(classType); len(signatures) == 1 {
		arity.signature = signatures[0]
		return arity
	}
	arity.variadic = getExtendsNode(c.node) != nil
	return arity
}

// luauConstructorArity builds fresh parameter lists (every caller adopts its
// own nodes) plus the argument list that forwards them.
type luauConstructorArity struct {
	class       *luauClass
	declaration *ast.Node
	signature   *checker.Signature
	variadic    bool
}

// parameters is the annotated parameter list, whether it ends in `...`, the
// `...` annotation, and the expressions that forward every parameter.
func (a luauConstructorArity) parameters() (parameters *luau.List[luau.AnyIdentifier], hasDotDotDot bool, varArgsType luau.TypeNode, arguments *luau.List[luau.Expression]) {
	parameters = luau.NewList[luau.AnyIdentifier]()
	arguments = luau.NewList[luau.Expression]()
	add := func(id luau.AnyIdentifier, t luau.TypeNode) {
		luau.SetBindingType(id, t)
		parameters.Push(id)
		argument := cloneIdentifier(id)
		luau.SetBindingType(argument, nil)
		arguments.Push(argument)
	}
	switch {
	case a.declaration != nil:
		hasDotDotDot, varArgsType = a.declaredParameters(add)
	case a.signature != nil:
		hasDotDotDot, varArgsType = a.signatureParameters(add)
	case a.variadic:
		hasDotDotDot = true
	}
	if hasDotDotDot {
		arguments.Push(luau.NewVarArgs())
	}
	return parameters, hasDotDotDot, varArgsType, arguments
}

// declaredParameters mirrors transformParameters over the constructor
// declaration: identifiers keep their names, binding patterns become
// `_param` temps, a `...[a, b]` rest pattern flattens into its elements and
// any other rest parameter is `...`.
func (a luauConstructorArity) declaredParameters(add func(luau.AnyIdentifier, luau.TypeNode)) (hasDotDotDot bool, varArgsType luau.TypeNode) {
	s := a.class.state
	for _, parameterNode := range a.declaration.Parameters() {
		parameter := parameterNode.AsParameterDeclaration()
		name := parameter.Name()
		if ast.IsThisIdentifier(name) {
			continue
		}
		if parameter.DotDotDotToken != nil && ast.IsArrayBindingPattern(name) && !arrayLikeExpressionContainsSpread(name) {
			for _, element := range name.AsBindingPattern().Elements.Nodes {
				if isOmittedBindingElement(element) {
					add(luau.TempID(""), nil)
					continue
				}
				bindingElement := element.AsBindingElement()
				if bindingElement.DotDotDotToken != nil {
					break
				}
				add(a.parameterIdentifier(bindingElement.Name()), getLuauBindingElementType(s, element))
			}
			continue
		}
		if parameter.DotDotDotToken != nil {
			return true, getLuauVarargType(s, a.declaration)
		}
		add(a.parameterIdentifier(name), getLuauParameterType(s, parameterNode))
	}
	return false, nil
}

// parameterIdentifier is a fresh identifier for a declared parameter name:
// the transformed identifier (cloned when the symbol was renamed, so this
// binding carries its own annotation), or a `_param` temp for a pattern.
func (a luauConstructorArity) parameterIdentifier(name *ast.Node) luau.AnyIdentifier {
	if ast.IsIdentifier(name) {
		return cloneIdentifier(TransformIdentifierDefined(a.class.state, name))
	}
	return luau.TempID("param")
}

// signatureParameters spells an inherited construct signature the way
// BuildSignature does: parameters past the required count are optional and
// a rest parameter is `...`.
func (a luauConstructorArity) signatureParameters(add func(luau.AnyIdentifier, luau.TypeNode)) (hasDotDotDot bool, varArgsType luau.TypeNode) {
	s := a.class.state
	b := s.types()
	ctx := typeContext{Position: positionParameter, Owner: a.class.node}
	signatureParameters := a.signature.Parameters()
	minArgumentCount := a.signature.MinArgumentCount()
	hasRest := a.signature.HasRestParameter()
	for index, parameter := range signatureParameters {
		parameterType := s.Checker.GetTypeOfSymbol(parameter)
		if hasRest && index == len(signatureParameters)-1 {
			if element := s.Checker.GetElementTypeOfArrayType(parameterType); element != nil {
				return true, b.Build(element, ctx).Node
			}
			return true, b.lose(lossUnspellable, parameterType, anyType(), ctx, "a rest parameter that is not an array has no Luau spelling")
		}
		node := b.Build(parameterType, ctx).Node
		if index >= minArgumentCount {
			node = luau.OptionalType(node)
		}
		var id luau.AnyIdentifier = luau.TempID("param")
		if luau.IsValidIdentifier(parameter.Name) && !luau.IsReservedIdentifier(parameter.Name) && parameter.Name != "self" {
			id = luau.ID(parameter.Name)
		}
		add(id, node)
	}
	return false, nil
}

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

// luauEnumObjectType is the annotation of an enum's object local: `read`
// string singletons for a string enum, which is a plain map; `number`
// members plus the `[number]: string` inverse indexer for the rest, writable
// because the general path assigns `Codes[index] = value` after the table is
// created.
func luauEnumObjectType(state *State, node *ast.Node) luau.TypeNode {
	members := node.AsEnumDeclaration().Members.Nodes
	allStrings := !slices.ContainsFunc(members, func(member *ast.Node) bool { return needsInverseEntry(state, member) })
	access := luau.TypeAccessNone
	if allStrings {
		access = luau.TypeAccessRead
	}
	fields := luau.NewList[luau.TypeTableField]()
	for _, member := range members {
		name := ast.GetPropertyNameForPropertyNameNode(member.Name())
		if name == ast.InternalSymbolNameMissing {
			continue
		}
		var value luau.TypeNode = luau.TypeName(nativeTypeNumber)
		if text, ok := state.Checker.GetConstantValue(member).(string); ok {
			value = luau.NewTypeStringSingleton(text)
		}
		fields.Push(luau.NewTypeTableProp(access, name, value))
	}
	if !allStrings {
		fields.Push(luau.NewTypeTableIndexer(luau.TypeAccessNone, luau.TypeName(nativeTypeNumber), luau.TypeName(nativeTypeString)))
	}
	return luau.NewTypeTable(fields)
}

// luauEnumBinding is the enum's identifier annotated with its object type,
// cloned so the annotation stays on the declaration alone.
func luauEnumBinding(state *State, node *ast.Node, id luau.AnyIdentifier) luau.AnyIdentifier {
	if !state.IsLuauTyped {
		return id
	}
	declared := cloneIdentifier(id)
	luau.SetBindingType(declared, luauEnumObjectType(state, node))
	return declared
}

// luauEnumTableCast widens the `setmetatable({}, { __index = _inverse })`
// a numeric enum starts from to `any` in luauTypes mode: the empty table is
// not a subtype of the annotated object type until its members are
// assigned.
func luauEnumTableCast(state *State, table luau.Expression) luau.Expression {
	if !state.IsLuauTyped {
		return table
	}
	return luau.NewCast(table, anyType())
}
