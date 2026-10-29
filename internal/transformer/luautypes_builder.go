package transformer

import (
	"slices"
	"strings"

	"rotor/internal/luau"
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

// The TS-to-Luau mapping. Every rule here is a full-fidelity version of the
// corresponding rule in nativetypes_type.go: the same checker calls decide the
// shape, but nothing is widened for the sake of a bytecode tag, `unknown` and
// `never` stay themselves, `readonly` becomes `read`, overloads stay
// intersections, and so on. What Luau cannot say (number singletons, tuples,
// template literals, symbols, `keyof T` on a resolved type...) becomes the
// closest spelling plus a fidelityLoss.

// luauTypeMaxDepth is the hard guard on nested Build calls. Named types
// recurse through their alias and anonymous object cycles are caught by the
// builder's stack, so only a pathological program reaches it.
const luauTypeMaxDepth = 64

// luauRobloxExtraTypeNames are Roblox types outside nativeRobloxTypeAllowlist
// that Luau's definitions still name directly.
var luauRobloxExtraTypeNames = map[string]struct{}{
	"Enum": {}, "EnumItem": {}, "Enums": {},
}

// Build spells t for the given context. The result's node is never nil.
func (b *typeBuilder) Build(t *checker.Type, ctx typeContext) builtType {
	losses := len(b.losses)
	var result builtType
	switch {
	case t == nil:
		result = b.finish(b.loseText(lossUnspellable, "", anyType(), ctx, "the checker has no type here"), false)
	case b.depth >= luauTypeMaxDepth:
		result = b.finish(b.lose(lossDepthExceeded, t, anyType(), ctx, "type nesting exceeds the Luau annotation depth guard"), false)
	default:
		b.depth++
		result = b.build(t, ctx)
		b.depth--
	}
	result.Lossy = len(b.losses) > losses
	return result
}

// build is Build's dispatch. Order matters: a LuaTuple is a nominal
// intersection, a Roblox enum is an aliased union, arrays, maps and Roblox
// datatypes are library interfaces with a shape of their own, and only then
// is a named type spelled by its alias.
func (b *typeBuilder) build(t *checker.Type, ctx typeContext) builtType {
	if ctx.Position == positionReturn && isNativeLuaTupleReturn(b.state, t) {
		return b.finish(b.BuildReturnPack(t, ctx), false)
	}
	if IsLuaTupleType(b.state).Check(t) {
		return b.finish(b.lose(lossLuaTupleOutsideReturn, t, anyType(), ctx, "a type pack is only spellable as a function return"), false)
	}
	if reference, ok := b.robloxEnumReference(t); ok {
		return b.finish(reference, false)
	}
	flags := t.Flags()
	if flags&checker.TypeFlagsObject != 0 {
		if special, ok := b.buildSpecialObject(t, ctx); ok {
			return special
		}
	}
	if alias, ok := b.BuildAliasReference(t, ctx); ok {
		return alias
	}

	switch {
	case t.IsUnion():
		return b.BuildUnion(t, ctx)
	case t.IsIntersection():
		return b.BuildIntersection(t, ctx)
	case flags&checker.TypeFlagsTypeParameter != 0:
		return b.BuildTypeParameter(t, ctx)
	case flags&checker.TypeFlagsIndexedAccess != 0:
		return b.buildIndexedAccess(t, ctx)
	case flags&checker.TypeFlagsIndex != 0:
		return b.buildKeyof(t, ctx)
	case flags&checker.TypeFlagsSubstitution != 0:
		return b.Build(t.AsSubstitutionType().BaseType(), ctx)
	case flags&checker.TypeFlagsConditional != 0:
		return b.buildConditional(t, ctx)
	case flags&(checker.TypeFlagsTemplateLiteral|checker.TypeFlagsStringMapping) != 0:
		return b.finish(b.lose(lossTemplateLiteral, t, luau.TypeName(nativeTypeString), ctx, "Luau has no template literal types"), false)
	case flags&checker.TypeFlagsLiteral != 0:
		return b.finish(b.BuildLiteral(t, ctx), false)
	case flags&checker.TypeFlagsObject != 0:
		return b.BuildObject(t, ctx)
	}
	return b.buildIntrinsic(t, ctx)
}

// finish classifies a built node.
func (b *typeBuilder) finish(node luau.TypeNode, outerGeneric bool) builtType {
	return builtType{Node: node, Kind: b.kindOf(node), OuterGeneric: outerGeneric}
}

// kindOf reads the native classification off a built node: references by
// name or origin, unions and optionals by their first member, tables and
// functions by shape. `any`, `unknown`, `never`, packs and type-function
// references have no tag.
func (b *typeBuilder) kindOf(node luau.TypeNode) nativeTypeKind {
	switch n := node.(type) {
	case *luau.TypeReference:
		if symbol := b.Origins[n]; symbol != nil {
			switch {
			case symbol.Flags&ast.SymbolFlagsTypeParameter != 0:
				return nativeKindGeneric
			case symbol.Flags&ast.SymbolFlagsClass != 0:
				return nativeKindTable
			}
			if entry := b.aliases[symbol]; entry != nil {
				return entry.Kind
			}
			return nativeKindVector
		}
		switch n.Name {
		case nativeTypeNumber:
			return nativeKindNumber
		case nativeTypeString:
			return nativeKindString
		case nativeTypeBoolean:
			return nativeKindBoolean
		case nativeTypeNil:
			return nativeKindNil
		}
		if n.Module == "" && isNativeVectorTypeName(n.Name) {
			return nativeKindVector
		}
	case *luau.TypeStringSingleton:
		return nativeKindString
	case *luau.TypeBooleanSingleton:
		return nativeKindBoolean
	case *luau.TypeOptional:
		return b.kindOf(n.Inner)
	case *luau.TypeUnion:
		if n.Members.Head != nil {
			return b.kindOf(n.Members.Head.Value)
		}
	case *luau.TypeIntersection:
		if n.Members.Head != nil {
			return b.kindOf(n.Members.Head.Value)
		}
	case *luau.TypeTable, *luau.TypeArray:
		return nativeKindTable
	case *luau.TypeFunction:
		return nativeKindFunction
	case *luau.TypePack:
		if n.Elements.IsEmpty() {
			return nativeKindNil
		}
	}
	return nativeKindUnspellable
}

// ---------------------------------------------------------------------------
// Unions, intersections, type variables, literals, intrinsics
// ---------------------------------------------------------------------------

// BuildUnion spells every member. `undefined`, `void` and `null` become the
// optional marker, `true | false` collapses to `boolean`, members are deduped
// structurally, and an `any` member makes the whole union `any`.
func (b *typeBuilder) BuildUnion(t *checker.Type, ctx typeContext) builtType {
	type member struct {
		node luau.TypeNode
		text string
	}
	var members []member
	optional, outerGeneric := false, false
	booleanPair := hasNativeBooleanLiteralPair(t)
	for _, unionMember := range t.Types() {
		if unionMember.Flags()&(checker.TypeFlagsVoidLike|checker.TypeFlagsNull) != 0 {
			optional = true
			continue
		}
		if booleanPair && isNativeBooleanLiteralType(unionMember) {
			continue
		}
		built := b.Build(unionMember, ctx)
		outerGeneric = outerGeneric || built.OuterGeneric
		if isAnyType(built.Node) {
			return b.finish(built.Node, outerGeneric)
		}
		text := renderTypeText(built.Node)
		if !slices.ContainsFunc(members, func(existing member) bool { return existing.text == text }) {
			members = append(members, member{node: built.Node, text: text})
		}
	}
	if booleanPair {
		members = append([]member{{node: luau.TypeName(nativeTypeBoolean), text: nativeTypeBoolean}}, members...)
	}
	if len(members) == 0 {
		return b.finish(b.buildNil(ctx), outerGeneric)
	}
	var node luau.TypeNode
	if len(members) == 1 {
		node = members[0].node
	} else {
		list := luau.NewList[luau.TypeNode]()
		for _, existing := range members {
			list.Push(existing.node)
		}
		node = luau.NewTypeUnion(list)
	}
	if optional {
		node = luau.OptionalType(node)
	}
	return b.finish(node, outerGeneric)
}

// BuildIntersection spells an intersection of tables, or of callables
// (overload sets), as a Luau intersection; `{}` members contribute nothing.
// Anything else (a branded primitive, a table intersected with a function)
// has no Luau meaning and widens to `any`.
func (b *typeBuilder) BuildIntersection(t *checker.Type, ctx typeContext) builtType {
	var members []builtType
	outerGeneric := false
	for _, intersectionMember := range t.Types() {
		built := b.Build(intersectionMember, ctx)
		outerGeneric = outerGeneric || built.OuterGeneric
		if table, ok := built.Node.(*luau.TypeTable); ok && table.Fields.IsEmpty() {
			continue
		}
		members = append(members, built)
	}
	switch len(members) {
	case 0:
		return b.finish(anyType(), outerGeneric)
	case 1:
		return members[0]
	}
	tables := !slices.ContainsFunc(members, func(m builtType) bool { return m.Kind != nativeKindTable })
	functions := !slices.ContainsFunc(members, func(m builtType) bool { return m.Kind != nativeKindFunction })
	if !tables && !functions {
		return b.finish(b.lose(lossUnspellable, t, anyType(), ctx, "only intersections of tables or of functions have a Luau spelling"), outerGeneric)
	}
	list := luau.NewList[luau.TypeNode]()
	for _, m := range members {
		list.Push(m.Node)
	}
	return b.finish(luau.NewTypeIntersection(list), outerGeneric)
}

// BuildTypeParameter spells a type parameter by name when it is in scope.
// Constraints are never substituted here (nativeSpelling does that through
// SpellTypeParameter); an out-of-scope parameter (a class's, an enclosing
// alias's) cannot be declared at this site and widens to `any`.
func (b *typeBuilder) BuildTypeParameter(t *checker.Type, ctx typeContext) builtType {
	if t.AsTypeParameter().IsThisType() {
		if ctx.This != nil {
			return b.finish(b.clone(ctx.This), false)
		}
		return b.finish(b.lose(lossThisType, t, anyType(), ctx, "the polymorphic this type has no name here"), false)
	}
	name, own, ok := getNativeTypeParameterName(b.state, t)
	if !ok {
		return b.finish(b.lose(lossUnspellable, t, anyType(), ctx, "the type parameter is not declarable at this site"), false)
	}
	outerGeneric := ctx.Position == positionParameter && !own
	if node := b.spelling.SpellTypeParameter(b, t, name); node != nil {
		return b.finish(node, outerGeneric)
	}
	reference := luau.TypeName(name)
	b.Origins[reference] = t.Symbol()
	return b.finish(reference, outerGeneric)
}

// buildIndexedAccess spells an unresolved `T[K]` as the `index<T, K>` type
// function (new solver only).
func (b *typeBuilder) buildIndexedAccess(t *checker.Type, ctx typeContext) builtType {
	access := t.AsIndexedAccessType()
	nested := ctx.nested()
	arguments := luau.NewList[luau.TypeNode](b.Build(access.ObjectType(), nested).Node, b.Build(access.IndexType(), nested).Node)
	return b.finish(luau.NewTypeReference("index", arguments), false)
}

// buildKeyof spells an unresolved `keyof T` as the `keyof<T>` type function.
func (b *typeBuilder) buildKeyof(t *checker.Type, ctx typeContext) builtType {
	arguments := luau.NewList[luau.TypeNode](b.Build(t.AsIndexType().Target(), ctx.nested()).Node)
	return b.finish(luau.NewTypeReference("keyof", arguments), false)
}

// buildConditional spells a deferred conditional type through its constraint
// or apparent type; a conditional the checker cannot reduce has no spelling.
func (b *typeBuilder) buildConditional(t *checker.Type, ctx typeContext) builtType {
	if constraint := b.state.Checker.GetBaseConstraintOfType(t); constraint != nil && constraint != t {
		return b.Build(constraint, ctx)
	}
	if apparent := b.state.Checker.GetApparentType(t); apparent != nil && apparent != t && apparent.Flags()&checker.TypeFlagsConditional == 0 {
		return b.Build(apparent, ctx)
	}
	return b.finish(b.lose(lossConditionalType, t, anyType(), ctx, "Luau has no conditional types"), false)
}

// BuildLiteral spells a literal type: string and boolean singletons exist,
// number and bigint literals widen. An enum member asks the spelling first.
func (b *typeBuilder) BuildLiteral(t *checker.Type, ctx typeContext) luau.TypeNode {
	flags := t.Flags()
	if flags&checker.TypeFlagsEnumLiteral != 0 {
		if node := b.spelling.SpellEnumMember(b, t); node != nil {
			return node
		}
	}
	switch {
	case flags&checker.TypeFlagsStringLiteral != 0:
		value, ok := t.AsLiteralType().Value().(string)
		if !ok {
			return luau.TypeName(nativeTypeString)
		}
		return luau.NewTypeStringSingleton(value)
	case flags&checker.TypeFlagsBooleanLiteral != 0:
		return luau.NewTypeBooleanSingleton(getNativeBooleanLiteralValue(t))
	case flags&checker.TypeFlagsNumberLiteral != 0:
		return b.lose(lossNumberLiteral, t, luau.TypeName(nativeTypeNumber), ctx, "Luau has no number singletons")
	case flags&checker.TypeFlagsBigIntLiteral != 0:
		return b.lose(lossBigInt, t, luau.TypeName(nativeTypeNumber), ctx, "Luau has no bigint")
	}
	return b.lose(lossUnspellable, t, anyType(), ctx, "unknown literal type")
}

// buildIntrinsic spells the primitives and the top and bottom types.
func (b *typeBuilder) buildIntrinsic(t *checker.Type, ctx typeContext) builtType {
	flags := t.Flags()
	var node luau.TypeNode
	switch {
	case flags&checker.TypeFlagsAny != 0:
		node = anyType()
	case flags&checker.TypeFlagsUnknown != 0:
		node = luau.TypeName("unknown")
	case flags&checker.TypeFlagsNever != 0:
		node = luau.TypeName("never")
	case flags&checker.TypeFlagsString != 0:
		node = luau.TypeName(nativeTypeString)
	case flags&(checker.TypeFlagsNumber|checker.TypeFlagsEnum) != 0:
		node = luau.TypeName(nativeTypeNumber)
	case flags&checker.TypeFlagsBoolean != 0:
		node = luau.TypeName(nativeTypeBoolean)
	case flags&checker.TypeFlagsBigInt != 0:
		node = b.lose(lossBigInt, t, luau.TypeName(nativeTypeNumber), ctx, "Luau has no bigint")
	case flags&checker.TypeFlagsESSymbolLike != 0:
		node = b.lose(lossSymbolKey, t, anyType(), ctx, "Luau has no symbol type")
	case flags&(checker.TypeFlagsVoidLike|checker.TypeFlagsNull) != 0:
		node = b.buildNil(ctx)
	case flags&checker.TypeFlagsNonPrimitive != 0:
		node = anyTable()
	default:
		node = b.lose(lossUnspellable, t, anyType(), ctx, "no Luau spelling")
	}
	return b.finish(node, false)
}

// buildNil is `nil`, or the empty pack `()` where a return is declared.
func (b *typeBuilder) buildNil(ctx typeContext) luau.TypeNode {
	if ctx.Position == positionReturn {
		return luau.NewTypePack(luau.NewList[luau.TypeNode]())
	}
	return luau.TypeName(nativeTypeNil)
}

// ---------------------------------------------------------------------------
// Named types: aliases, Roblox types, enums
// ---------------------------------------------------------------------------

// BuildAliasReference spells a named interface, type alias or enum as a
// reference to its `type` statement, registering the statement on first use.
// It declines (false) when the spelling does not alias the symbol, when the
// body was rejected, and at the top of the alias's own body, where the type
// is spelled structurally. A reference back into an alias whose body is
// still being built must repeat the alias's own type arguments: Luau
// requires recursive aliases to be referenced with identical arguments.
func (b *typeBuilder) BuildAliasReference(t *checker.Type, ctx typeContext) (builtType, bool) {
	symbol, declaration := b.aliasSymbolOf(t)
	if symbol == nil || !b.spelling.ShouldAlias(b.state, symbol, declaration) {
		return builtType{}, false
	}
	entry := b.aliasFor(symbol)
	if entry == nil {
		return builtType{}, false
	}
	if entry.building && ctx.Position == positionAliasBody {
		return builtType{}, false
	}
	arguments := b.aliasTypeArguments(t, entry)
	if entry.building && !recursiveArgsMatch(entry, arguments) {
		node := b.lose(lossRecursiveAliasArgs, t, anyType(), ctx, "a recursive alias must be referenced with its own type arguments")
		return b.finish(node, false), true
	}
	reference := luau.NewTypeReference(entry.Name, b.BuildTypeArgs(arguments, ctx.nested()))
	b.Origins[reference] = symbol
	return b.finish(reference, false), true
}

// aliasSymbolOf is the symbol a type can be aliased by, with its
// declaration: the alias symbol of an aliased type, the symbol of an
// interface (not a class), or an enum (whose declared type is a union of its
// member literals, or the single member literal itself).
func (b *typeBuilder) aliasSymbolOf(t *checker.Type) (*ast.Symbol, *ast.Node) {
	if alias := t.Alias(); alias != nil {
		if symbol := alias.Symbol(); symbol != nil && symbol.Flags&ast.SymbolFlagsTypeAlias != 0 {
			return symbol, aliasDeclarationOf(symbol)
		}
	}
	symbol := t.Symbol()
	if symbol == nil {
		return nil, nil
	}
	flags := t.Flags()
	switch {
	case flags&checker.TypeFlagsObject != 0 && symbol.Flags&ast.SymbolFlagsInterface != 0 && symbol.Flags&ast.SymbolFlagsClass == 0:
		return symbol, aliasDeclarationOf(symbol)
	case flags&checker.TypeFlagsEnumLiteral != 0 && symbol.Flags&ast.SymbolFlagsEnum != 0:
		return symbol, aliasDeclarationOf(symbol)
	case flags&checker.TypeFlagsEnumLiteral != 0 && symbol.Flags&ast.SymbolFlagsEnumMember != 0:
		parent := symbol.Parent
		if parent != nil && parent.Flags&ast.SymbolFlagsEnum != 0 && b.state.Checker.GetDeclaredTypeOfSymbol(parent) == t {
			return parent, aliasDeclarationOf(parent)
		}
	}
	return nil, nil
}

// aliasDeclarationOf is the first interface, type alias or enum declaration
// of symbol.
func aliasDeclarationOf(symbol *ast.Symbol) *ast.Node {
	for _, declaration := range symbol.Declarations {
		if ast.IsInterfaceDeclaration(declaration) || ast.IsTypeAliasDeclaration(declaration) || ast.IsEnumDeclaration(declaration) {
			return declaration
		}
	}
	return nil
}

// aliasFor registers symbol's `type` statement on first use and builds its
// body with the alias's own generics in a barrier scope, then lets the
// spelling finish (or reject) the body. The entry is registered before the
// body is built, so a self reference resolves to the name.
func (b *typeBuilder) aliasFor(symbol *ast.Symbol) *typeAliasEntry {
	if symbol == nil {
		return nil
	}
	if entry, seen := b.aliases[symbol]; seen {
		return entry
	}
	declaration := aliasDeclarationOf(symbol)
	if declaration == nil {
		b.aliases[symbol] = nil
		return nil
	}
	generics := b.Generics(declaration)
	name := b.spelling.AliasName(b.state, symbol)
	statement := luau.NewTypeAlias(name, generics, nil)
	declared := b.state.Checker.GetDeclaredTypeOfSymbol(symbol)
	entry := &typeAliasEntry{Symbol: symbol, Statement: statement, Name: name, Generics: generics, Kind: nativeKindTable, building: true}
	if declared != nil && !ast.IsInterfaceDeclaration(declaration) && len(b.state.Checker.GetCallSignatures(declared)) > 0 {
		entry.Kind = nativeKindFunction
		entry.Callable = true
	}
	for _, parameter := range declaredTypeParameters(declaration) {
		if parameterSymbol := parameter.Symbol(); parameterSymbol != nil {
			entry.Params = append(entry.Params, b.state.Checker.GetDeclaredTypeOfSymbol(parameterSymbol))
		}
	}
	b.aliases[symbol] = entry
	b.aliasOrder = append(b.aliasOrder, entry)

	// The body names the alias itself for `self` and `this`, with its own
	// generics as arguments.
	var receiverArguments *luau.List[luau.TypeNode]
	if len(generics) > 0 {
		receiverArguments = luau.NewList[luau.TypeNode]()
		for _, generic := range generics {
			receiverArguments.Push(luau.TypeName(generic.Name))
		}
	}
	receiver := luau.NewTypeReference(name, receiverArguments)
	b.Origins[receiver] = symbol

	names := make([]string, len(generics))
	for index, generic := range generics {
		names[index] = generic.Name
	}
	b.state.PushTypeGenerics(names, true)
	savedDepth, savedStack := b.depth, b.stack
	b.depth, b.stack = 0, nil
	bodyContext := typeContext{Position: positionAliasBody, Receiver: receiver, This: receiver, Owner: declaration}
	built := b.Build(declared, bodyContext)
	b.depth, b.stack = savedDepth, savedStack
	b.state.PopTypeGenerics()

	entry.building = false
	entry.Kind = built.Kind
	entry.Callable = built.Kind == nativeKindFunction
	body, keep := b.spelling.FinishAliasBody(b.state, entry, built.Node)
	if !keep {
		b.aliases[symbol] = nil
		b.aliasOrder = slices.DeleteFunc(b.aliasOrder, func(existing *typeAliasEntry) bool { return existing == entry })
		return nil
	}
	statement.SetBody(body)
	return entry
}

// aliasTypeArguments are the arguments a reference passes to the alias: the
// alias's own arguments for an aliased type, else the interface reference's
// arguments without the trailing `this` argument the checker may append.
func (b *typeBuilder) aliasTypeArguments(t *checker.Type, entry *typeAliasEntry) []*checker.Type {
	if alias := t.Alias(); alias != nil && alias.Symbol() == entry.Symbol {
		return alias.TypeArguments()
	}
	arguments := getTypeArguments(b.state, t)
	if len(arguments) > len(entry.Params) {
		arguments = arguments[:len(entry.Params)]
	}
	return arguments
}

// recursiveArgsMatch reports whether a reference into an alias under
// construction passes exactly the alias's own type parameters.
func recursiveArgsMatch(entry *typeAliasEntry, arguments []*checker.Type) bool {
	if len(arguments) != len(entry.Params) {
		return false
	}
	for index, argument := range arguments {
		if argument != entry.Params[index] {
			return false
		}
	}
	return true
}

// robloxEnumReference spells a Roblox enum type (`Enum.Material`, declared as
// a union alias under the `Enum` namespace) or one of its items
// (`Enum.Material.Plastic`, an interface two levels down) as the qualified
// `Enum.Material`: Luau names the enum, never a single item.
func (b *typeBuilder) robloxEnumReference(t *checker.Type) (luau.TypeNode, bool) {
	symbol := t.Symbol()
	if alias := t.Alias(); alias != nil && alias.Symbol() != nil {
		symbol = alias.Symbol()
	}
	if symbol == nil || !isRobloxSymbol(symbol) {
		return nil, false
	}
	for current := symbol; current != nil && current.Parent != nil; current = current.Parent {
		parent := current.Parent
		if parent.Parent == nil && parent.Name == "Enum" && parent.Flags&ast.SymbolFlagsModule != 0 {
			if !luau.IsValidIdentifier(current.Name) {
				return nil, false
			}
			reference := luau.NewTypeQualifiedReference("Enum", current.Name, nil)
			b.Origins[reference] = symbol
			return reference, true
		}
	}
	return nil, false
}

// isRobloxSymbol is IsRobloxType's test on a symbol: a declaration inside the
// @rbxts/types package.
func isRobloxSymbol(symbol *ast.Symbol) bool {
	for _, declaration := range symbol.Declarations {
		if sourceFile := ast.GetSourceFileOfNode(declaration); sourceFile != nil &&
			strings.Contains(sourceFile.FileName(), "node_modules/@rbxts/types/") {
			return true
		}
	}
	return false
}

// robloxTypeName is the Luau name of a Roblox type: the allowlisted
// datatypes, `Instance` subclasses, and the enum base types. "" for a Roblox
// type Luau cannot name (its members are an implementation detail).
func robloxTypeName(state *State, t *checker.Type) string {
	if name := getNativeRobloxTypeName(state, t); name != "" {
		return name
	}
	if symbol := t.Symbol(); symbol != nil && IsRobloxType(state).Check(t) {
		if _, ok := luauRobloxExtraTypeNames[symbol.Name]; ok {
			return symbol.Name
		}
	}
	return ""
}

// ---------------------------------------------------------------------------
// Objects
// ---------------------------------------------------------------------------

// buildSpecialObject spells the object types whose Luau shape is not their
// member list: a Roblox type by name, arrays, tuples, maps and sets by
// shape, and the `Function` and `Object` globals. false for everything else.
func (b *typeBuilder) buildSpecialObject(t *checker.Type, ctx typeContext) (builtType, bool) {
	if IsRobloxType(b.state).Check(t) {
		if name := robloxTypeName(b.state, t); name != "" {
			reference := luau.TypeName(name)
			b.Origins[reference] = t.Symbol()
			return b.finish(reference, false), true
		}
		return b.finish(b.lose(lossUnspellable, t, anyType(), ctx, "Roblox type without a Luau type name"), false), true
	}
	switch {
	case IsArrayType(b.state).Check(t):
		return b.buildArray(t, ctx), true
	case IsMapType(b.state).Check(t):
		return b.buildMap(t, ctx), true
	case IsSetType(b.state).Check(t):
		return b.buildSet(t, ctx), true
	case symbolIsAmbient(b.state, t, "Function"):
		return b.finish(anyFunction(), false), true
	case symbolIsAmbient(b.state, t, "Object"):
		return b.finish(anyTable(), false), true
	}
	return builtType{}, false
}

// BuildObject spells an object type: the special shapes above, a class
// instance by reference, a class value as its `{ new, statics }` table, a
// callable as a function type, anything else as a table.
func (b *typeBuilder) BuildObject(t *checker.Type, ctx typeContext) builtType {
	if special, ok := b.buildSpecialObject(t, ctx); ok {
		return special
	}
	symbol := t.Symbol()
	constructSignatures := b.state.Checker.GetConstructSignatures(t)
	if symbol != nil && symbol.Flags&ast.SymbolFlagsClass != 0 && len(constructSignatures) == 0 {
		return b.buildClassInstance(t, ctx)
	}
	if len(constructSignatures) > 0 {
		return b.buildClassValue(t, ctx)
	}
	if len(b.state.Checker.GetCallSignatures(t)) > 0 {
		if len(b.state.Checker.GetPropertiesOfType(t)) > 0 {
			b.lose(lossUnspellable, t, anyFunction(), ctx, "properties of a callable object are dropped")
		}
		return b.finish(b.BuildCallable(t, nil, ctx), false)
	}
	if len(b.state.Checker.GetPropertiesOfType(t)) == 0 && len(b.state.Checker.GetIndexInfosOfType(t)) == 0 {
		// `{}` admits primitives too, so only a declared empty interface
		// keeps the empty table.
		if ctx.Position == positionAliasBody {
			return b.finish(luau.NewTypeTable(luau.NewList[luau.TypeTableField]()), false)
		}
		return b.finish(anyType(), false)
	}
	table := b.BuildTable(t, ctx)
	if table == nil {
		return b.finish(b.lose(lossUnspellable, t, anyType(), ctx, "recursive anonymous object type"), false)
	}
	if symbol != nil && symbol.Flags&(ast.SymbolFlagsInterface|ast.SymbolFlagsEnum|ast.SymbolFlagsClass) != 0 {
		b.Origins[table] = symbol
	}
	return b.finish(table, false)
}

// buildArray spells `T[]` as `{ T }`, `ReadonlyArray<T>` as `{ read T }`,
// and a tuple as an array of the element union.
func (b *typeBuilder) buildArray(t *checker.Type, ctx typeContext) builtType {
	if t.IsTupleType() {
		return b.buildTuple(t, ctx)
	}
	element := anyType()
	if elementType := b.state.Checker.GetElementTypeOfArrayType(t); elementType != nil {
		element = b.Build(elementType, ctx.nested()).Node
	}
	array := luau.NewTypeArray(element)
	if symbolIsAmbient(b.state, t, "ReadonlyArray") {
		array.Access = luau.TypeAccessRead
	}
	return b.finish(array, false)
}

// buildTuple spells a tuple value type as `{ A | B }`: Luau has packs only in
// parameter and return positions, so the positions are lost.
func (b *typeBuilder) buildTuple(t *checker.Type, ctx typeContext) builtType {
	type element struct {
		node luau.TypeNode
		text string
	}
	var elements []element
	for _, elementType := range getTypeArguments(b.state, t) {
		node := b.Build(elementType, ctx.nested()).Node
		text := renderTypeText(node)
		if !slices.ContainsFunc(elements, func(existing element) bool { return existing.text == text }) {
			elements = append(elements, element{node: node, text: text})
		}
	}
	var member luau.TypeNode
	switch len(elements) {
	case 0:
		member = anyType()
	case 1:
		member = elements[0].node
	default:
		list := luau.NewList[luau.TypeNode]()
		for _, existing := range elements {
			list.Push(existing.node)
		}
		member = luau.NewTypeUnion(list)
	}
	array := luau.NewTypeArray(member)
	if t.TargetTupleType().IsReadonly() {
		array.Access = luau.TypeAccessRead
	}
	return b.finish(b.lose(lossTuple, t, array, ctx, "Luau has no tuple types; positions are lost"), false)
}

// buildMap spells Map, ReadonlyMap and WeakMap as `{ [K]: V }`.
func (b *typeBuilder) buildMap(t *checker.Type, ctx typeContext) builtType {
	arguments := getTypeArguments(b.state, t)
	if len(arguments) < 2 {
		return b.finish(anyTable(), false)
	}
	nested := ctx.nested()
	indexer := luau.NewTypeTableIndexer(luau.TypeAccessNone, b.Build(arguments[0], nested).Node, b.Build(arguments[1], nested).Node)
	if symbolIsAmbient(b.state, t, "ReadonlyMap") {
		indexer.Access = luau.TypeAccessRead
	}
	return b.finish(luau.NewTypeTable(luau.NewList[luau.TypeTableField](indexer)), false)
}

// buildSet spells Set, ReadonlySet and WeakSet as `{ [T]: true }`.
func (b *typeBuilder) buildSet(t *checker.Type, ctx typeContext) builtType {
	arguments := getTypeArguments(b.state, t)
	if len(arguments) < 1 {
		return b.finish(anyTable(), false)
	}
	indexer := luau.NewTypeTableIndexer(luau.TypeAccessNone, b.Build(arguments[0], ctx.nested()).Node, luau.NewTypeBooleanSingleton(true))
	if symbolIsAmbient(b.state, t, "ReadonlySet") {
		indexer.Access = luau.TypeAccessRead
	}
	return b.finish(luau.NewTypeTable(luau.NewList[luau.TypeTableField](indexer)), false)
}

// buildClassInstance spells a class instance as a reference to the class's
// instance alias (emitted at the class declaration by WP7), with the class's
// type arguments.
func (b *typeBuilder) buildClassInstance(t *checker.Type, ctx typeContext) builtType {
	symbol := t.Symbol()
	arguments := getTypeArguments(b.state, t)
	for _, declaration := range symbol.Declarations {
		if ast.IsClassLike(declaration) {
			if count := len(declaration.TypeParameters()); len(arguments) > count {
				arguments = arguments[:count]
			}
			break
		}
	}
	reference := luau.NewTypeReference(b.spelling.AliasName(b.state, symbol), b.BuildTypeArgs(arguments, ctx.nested()))
	b.Origins[reference] = symbol
	return b.finish(reference, false)
}

// buildClassValue spells a value with construct signatures (`typeof Foo`, a
// constructor interface) as the structural `{ new: ..., statics }` table the
// spelling may override. The overload intersection sits on the `new` member,
// never around two tables.
func (b *typeBuilder) buildClassValue(t *checker.Type, ctx typeContext) builtType {
	if node := b.spelling.SpellClassValue(b, t); node != nil {
		return b.finish(node, false)
	}
	table := b.BuildTable(t, ctx)
	if table == nil {
		return b.finish(b.lose(lossUnspellable, t, anyTable(), ctx, "recursive class value type"), false)
	}
	if symbol := t.Symbol(); symbol != nil && symbol.Flags&ast.SymbolFlagsClass != 0 {
		b.Origins[table] = symbol
	}
	return b.finish(table, false)
}

// BuildTable spells one object level as a table type, or nil when the type
// is already on the stack (a recursive anonymous type). A value with
// construct signatures is a class factory whose members are statics, called
// with `.`; everywhere else a METHOD member takes the receiver `obj:method()`
// passes, while a function-typed property does not.
func (b *typeBuilder) BuildTable(t *checker.Type, ctx typeContext) *luau.TypeTable {
	if slices.Contains(b.stack, t) {
		return nil
	}
	b.stack = append(b.stack, t)
	defer func() { b.stack = b.stack[:len(b.stack)-1] }()

	fields := luau.NewList[luau.TypeTableField]()
	receivers := len(b.state.Checker.GetConstructSignatures(t)) == 0
	if construct := b.constructMember(t, ctx); construct != nil {
		fields.Push(construct)
	}
	receiver := ctx.Receiver
	symbolKeyLost, readonlyLost, privateLost := false, false, false
	for _, property := range b.state.Checker.GetPropertiesOfType(t) {
		name := property.Name
		if strings.HasPrefix(name, "__@") {
			// Computed/ES-symbol member: no Luau spelling.
			if !symbolKeyLost {
				b.record(lossSymbolKey, name, nil, ctx, "symbol-keyed members are skipped")
				symbolKeyLost = true
			}
			continue
		}
		propertyType := b.state.Checker.GetTypeOfSymbol(property)
		var value luau.TypeNode
		callable := b.state.Checker.GetNonNullableType(propertyType)
		if receivers && property.Flags&ast.SymbolFlagsMethod != 0 && len(b.state.Checker.GetCallSignatures(callable)) > 0 {
			if receiver == nil {
				receiver = b.lose(lossObjectLiteralSelf, t, anyType(), ctx, "methods of an unnamed object type take `self: any`")
			}
			value = b.BuildCallable(callable, receiver, ctx.nested())
		} else {
			value = b.Build(propertyType, ctx.nested()).Node
		}
		if property.Flags&ast.SymbolFlagsOptional != 0 {
			value = luau.OptionalType(value)
		}
		access := luau.TypeAccessNone
		if isReadonlyProperty(property) {
			if ctx.ClassInstance {
				if !readonlyLost {
					b.loseText(lossClassReadonlyDropped, name, value, ctx, "class constructors write through self, so readonly members stay writable")
					readonlyLost = true
				}
			} else {
				access = luau.TypeAccessRead
			}
		}
		if ctx.ClassInstance && !privateLost &&
			checker.GetDeclarationModifierFlagsFromSymbol(property)&(ast.ModifierFlagsPrivate|ast.ModifierFlagsProtected) != 0 {
			b.loseText(lossPrivateExposed, name, value, ctx, "private and protected members are plain table fields in Luau")
			privateLost = true
		}
		fields.Push(luau.NewTypeTableProp(access, name, value))
	}
	if indexer := b.BuildIndexer(t, ctx); indexer != nil {
		fields.Push(indexer)
	}
	return luau.NewTypeTable(fields)
}

// isReadonlyProperty is the checker's isReadonlySymbol for members: a
// `readonly` modifier, a mapped `Readonly<T>` property, a getter without a
// setter, or an enum member.
func isReadonlyProperty(property *ast.Symbol) bool {
	return property.CheckFlags&ast.CheckFlagsReadonly != 0 ||
		property.Flags&ast.SymbolFlagsProperty != 0 && checker.GetDeclarationModifierFlagsFromSymbol(property)&ast.ModifierFlagsReadonly != 0 ||
		property.Flags&ast.SymbolFlagsAccessor != 0 && property.Flags&ast.SymbolFlagsSetAccessor == 0 ||
		property.Flags&ast.SymbolFlagsEnumMember != 0
}

// constructMember spells the `new` member of a value with construct
// signatures, or nil. An overloaded constructor keeps the intersection on the
// member, which is what preserves the table tag.
func (b *typeBuilder) constructMember(t *checker.Type, ctx typeContext) luau.TypeTableField {
	signatures := b.state.Checker.GetConstructSignatures(t)
	if len(signatures) == 0 {
		return nil
	}
	nested := ctx.nested()
	var value luau.TypeNode
	if len(signatures) == 1 {
		value = b.BuildSignature(signatures[0], nil, nested)
	} else {
		members := luau.NewList[luau.TypeNode]()
		for _, signature := range signatures {
			members.Push(b.BuildSignature(signature, nil, nested))
		}
		value = luau.NewTypeIntersection(members)
	}
	return luau.NewTypeTableProp(luau.TypeAccessNone, "new", value)
}

// BuildIndexer collapses a type's index signatures into Luau's single
// indexer: `[string]: V`, `[number]: V`, or `[any]: V` when both exist (`any`
// value when they differ). A symbol-keyed signature is skipped.
func (b *typeBuilder) BuildIndexer(t *checker.Type, ctx typeContext) *luau.TypeTableIndexer {
	var stringInfo, numberInfo *checker.IndexInfo
	for _, info := range b.state.Checker.GetIndexInfosOfType(t) {
		key := info.KeyType()
		switch {
		case key.Flags()&checker.TypeFlagsNumber != 0:
			if numberInfo == nil {
				numberInfo = info
			}
		case key.Flags()&checker.TypeFlagsESSymbolLike != 0:
			b.record(lossSymbolKey, b.typeText(key), nil, ctx, "symbol index signatures are skipped")
		default:
			if stringInfo == nil {
				stringInfo = info
			}
		}
	}
	nested := ctx.nested()
	switch {
	case stringInfo != nil && numberInfo != nil:
		stringValue := b.Build(stringInfo.ValueType(), nested).Node
		numberValue := b.Build(numberInfo.ValueType(), nested).Node
		value := stringValue
		if renderTypeText(stringValue) != renderTypeText(numberValue) {
			value = anyType()
		}
		b.loseText(lossIndexerCollapsed, "[string] + [number]", value, ctx, "Luau allows one indexer, so both keys collapse to [any]")
		access := luau.TypeAccessNone
		if stringInfo.IsReadonly() && numberInfo.IsReadonly() {
			access = luau.TypeAccessRead
		}
		return luau.NewTypeTableIndexer(access, anyType(), value)
	case stringInfo != nil:
		return b.indexerFor(luau.TypeName(nativeTypeString), stringInfo, nested)
	case numberInfo != nil:
		return b.indexerFor(luau.TypeName(nativeTypeNumber), numberInfo, nested)
	}
	return nil
}

func (b *typeBuilder) indexerFor(key luau.TypeNode, info *checker.IndexInfo, ctx typeContext) *luau.TypeTableIndexer {
	access := luau.TypeAccessNone
	if info.IsReadonly() {
		access = luau.TypeAccessRead
	}
	return luau.NewTypeTableIndexer(access, key, b.Build(info.ValueType(), ctx).Node)
}

// ---------------------------------------------------------------------------
// Callables
// ---------------------------------------------------------------------------

// BuildCallable spells a callable object as a function type, or an overload
// set as an intersection of function types at every position (the native
// policy decides where that is too strict). receiver is spelled as the
// leading `self` parameter of every signature; nil means no receiver.
func (b *typeBuilder) BuildCallable(t *checker.Type, receiver luau.TypeNode, ctx typeContext) luau.TypeNode {
	signatures := b.state.Checker.GetCallSignatures(t)
	switch len(signatures) {
	case 0:
		return b.lose(lossUnspellable, t, anyFunction(), ctx, "callable without a call signature")
	case 1:
		return b.BuildSignature(signatures[0], receiver, ctx)
	}
	members := luau.NewList[luau.TypeNode]()
	for _, signature := range signatures {
		members.Push(b.BuildSignature(signature, receiver, ctx))
	}
	return luau.NewTypeIntersection(members)
}

// BuildSignature spells one call or construct signature. Its own type
// parameters are declared and put in scope (constraints dropped), a `this`
// parameter or the receiver becomes `self`, parameters past the required
// count become `T?`, a rest parameter becomes `...E`, and the return goes
// through BuildReturnPack.
func (b *typeBuilder) BuildSignature(signature *checker.Signature, receiver luau.TypeNode, ctx typeContext) *luau.TypeFunction {
	generics := b.signatureGenerics(signature)
	names := make([]string, len(generics))
	for index, generic := range generics {
		names[index] = generic.Name
	}
	b.state.PushTypeGenerics(names, false)
	defer b.state.PopTypeGenerics()

	nested := ctx.nested()
	var parameters []luau.TypeFunctionParameter
	if this := signature.ThisParameter(); this != nil {
		parameters = append(parameters, luau.TypeFunctionParameter{Name: "self", Type: b.Build(b.state.Checker.GetTypeOfSymbol(this), nested).Node})
	} else if receiver != nil {
		parameters = append(parameters, luau.TypeFunctionParameter{Name: "self", Type: b.clone(receiver)})
	}
	var varArgs luau.TypeNode
	signatureParameters := signature.Parameters()
	minArgumentCount := signature.MinArgumentCount()
	hasRest := signature.HasRestParameter()
	for index, parameter := range signatureParameters {
		parameterType := b.state.Checker.GetTypeOfSymbol(parameter)
		if hasRest && index == len(signatureParameters)-1 {
			if element := b.state.Checker.GetElementTypeOfArrayType(parameterType); element != nil {
				varArgs = luau.NewTypeVariadic(b.Build(element, nested).Node)
			} else {
				varArgs = luau.NewTypeVariadic(b.lose(lossUnspellable, parameterType, anyType(), ctx, "a rest parameter that is not an array has no Luau spelling"))
			}
			continue
		}
		node := b.Build(parameterType, nested).Node
		if index >= minArgumentCount {
			node = luau.OptionalType(node)
		}
		name := ""
		if luau.IsValidIdentifier(parameter.Name) {
			name = parameter.Name
		}
		parameters = append(parameters, luau.TypeFunctionParameter{Name: name, Type: node})
	}
	returnContext := nested
	returnContext.Position = positionReturn
	returnType := b.BuildReturnPack(b.state.Checker.GetReturnTypeOfSignature(signature), returnContext)
	return luau.NewTypeFunction(generics, parameters, varArgs, returnType)
}

// signatureGenerics lists a signature's spellable type parameters.
func (b *typeBuilder) signatureGenerics(signature *checker.Signature) []luau.TypeGeneric {
	var generics []luau.TypeGeneric
	for _, parameter := range signature.TypeParameters() {
		symbol := parameter.Symbol()
		if symbol == nil || !luau.IsValidIdentifier(symbol.Name) {
			continue
		}
		generics = append(generics, luau.TypeGeneric{Name: symbol.Name})
		if b.state.Checker.GetConstraintOfTypeParameter(parameter) != nil {
			b.constraintLoss(symbol)
		}
	}
	return generics
}

// Generics lists the spellable type parameters a function, class, interface
// or type alias declaration declares, ready for `<K, V>`. A parameter Luau
// cannot name is skipped (references to it widen to `any`); a constraint is
// dropped and recorded once per parameter.
func (b *typeBuilder) Generics(node *ast.Node) []luau.TypeGeneric {
	var generics []luau.TypeGeneric
	for _, parameter := range declaredTypeParameters(node) {
		name := parameter.Name()
		if name == nil || !luau.IsValidIdentifier(name.Text()) {
			continue
		}
		generics = append(generics, luau.TypeGeneric{Name: name.Text()})
		if parameter.AsTypeParameterDeclaration().Constraint != nil {
			b.constraintLoss(parameter.Symbol())
		}
	}
	return generics
}

// declaredTypeParameters is Node.TypeParameters for the declarations that
// can carry them, and nil for the rest (an enum, say) instead of a panic.
func declaredTypeParameters(node *ast.Node) []*ast.Node {
	if ast.IsClassLike(node) || ast.IsInterfaceDeclaration(node) || ast.IsTypeAliasDeclaration(node) || node.FunctionLikeData() != nil {
		return node.TypeParameters()
	}
	return nil
}

// constraintLoss records a dropped `extends` clause once per type parameter.
func (b *typeBuilder) constraintLoss(symbol *ast.Symbol) {
	if symbol == nil {
		return
	}
	if _, seen := b.constraintLosses[symbol]; seen {
		return
	}
	b.constraintLosses[symbol] = struct{}{}
	b.losses = append(b.losses, fidelityLoss{Kind: lossConstraintDropped, TSText: symbol.Name, Luau: symbol.Name, Reason: "Luau generics have no constraints"})
}

// BuildReturnPack spells what follows `->` or `:` on a function: `()` for
// void, a pack for a LuaTuple of one fixed tuple, `...any` for a LuaTuple
// Luau cannot unwrap or an optional LuaTuple (`?` after a pack is an
// optional function, never an optional return), otherwise one type.
func (b *typeBuilder) BuildReturnPack(t *checker.Type, ctx typeContext) luau.TypeNode {
	ctx.Position = positionReturn
	anyPack := func() luau.TypeNode { return luau.NewTypeVariadic(anyType()) }
	if t == nil {
		return b.loseText(lossUnspellable, "", anyPack(), ctx, "the checker has no return type here")
	}
	if isUndefinedTypeCheck(t) {
		return luau.NewTypePack(luau.NewList[luau.TypeNode]())
	}
	if IsLuaTupleType(b.state).Check(t) {
		tuple := getNativeLuaTupleTuple(b.state, t)
		if tuple == nil {
			return b.lose(lossUnspellable, t, anyPack(), ctx, "a LuaTuple of an array, a type parameter or a union of tuples has no pack spelling")
		}
		pack, ok := b.BuildTuplePack(tuple, ctx)
		if !ok {
			return anyPack()
		}
		return pack
	}
	if t.IsUnion() && slices.ContainsFunc(t.Types(), IsLuaTupleType(b.state).Check) {
		return b.lose(lossOptionalPack, t, anyPack(), ctx, "`?` after a return pack would make the function optional")
	}
	return b.Build(t, ctx).Node
}

// BuildTuplePack spells a fixed tuple as a return pack: `(A, B?)`, a
// trailing rest as `...E`, `()` when empty. false (with a loss recorded) when
// a rest element is not last.
func (b *typeBuilder) BuildTuplePack(tuple *checker.Type, ctx typeContext) (*luau.TypePack, bool) {
	infos := tuple.TargetTupleType().ElementInfos()
	arguments := getTypeArguments(b.state, tuple)
	elements := luau.NewList[luau.TypeNode]()
	if len(infos) == 0 {
		return luau.NewTypePack(elements), true
	}
	if len(arguments) != len(infos) {
		b.record(lossUnspellable, b.typeText(tuple), nil, ctx, "tuple element count does not match its type arguments")
		return nil, false
	}
	nested := ctx.nested()
	for index, info := range infos {
		elementType := arguments[index]
		flags := info.TupleElementFlags()
		if flags&checker.ElementFlagsVariable != 0 {
			if index != len(infos)-1 {
				b.record(lossUnspellable, b.typeText(tuple), nil, ctx, "a rest element that is not last has no pack spelling")
				return nil, false
			}
			// A rest element's type argument is already its element type in
			// a resolved tuple; an unresolved array still unwraps.
			if element := b.state.Checker.GetElementTypeOfArrayType(elementType); element != nil {
				elementType = element
			}
			elements.Push(luau.NewTypeVariadic(b.Build(elementType, nested).Node))
			continue
		}
		node := b.Build(elementType, nested).Node
		if flags&checker.ElementFlagsOptional != 0 {
			node = luau.OptionalType(node)
		}
		elements.Push(node)
	}
	return luau.NewTypePack(elements), true
}

// BuildTypeArgs spells a type argument list, or nil for none.
func (b *typeBuilder) BuildTypeArgs(arguments []*checker.Type, ctx typeContext) *luau.List[luau.TypeNode] {
	if len(arguments) == 0 {
		return nil
	}
	list := luau.NewList[luau.TypeNode]()
	for _, argument := range arguments {
		list.Push(b.Build(argument, ctx).Node)
	}
	return list
}

// ---------------------------------------------------------------------------
// Type nodes
// ---------------------------------------------------------------------------

// BuildTypeNode spells a written TS type. `typeof x` keeps Luau's own
// `typeof(x)` when x is a value Luau can see at the annotation site: a
// top-level `const`, function or class of this file, declared (textually)
// before the owner, not imported, not renamed by the transformer. Everything
// else, and every other type node, is spelled from the checker's resolved
// type.
func (b *typeBuilder) BuildTypeNode(node *ast.Node, ctx typeContext) builtType {
	if node.Kind == ast.KindTypeQuery {
		if expression := node.AsTypeQueryNode().ExprName; expression != nil && ast.IsIdentifier(expression) {
			if identifier, ok := b.localTypeofIdentifier(expression, ctx); ok {
				return b.finish(luau.NewTypeTypeof(identifier), false)
			}
		}
	}
	return b.Build(b.state.Checker.GetTypeFromTypeNode(node), ctx)
}

// localTypeofIdentifier is the Luau identifier `typeof(x)` may name, per
// BuildTypeNode's rule.
func (b *typeBuilder) localTypeofIdentifier(expression *ast.Node, ctx typeContext) (luau.Expression, bool) {
	symbol := b.state.Checker.GetSymbolAtLocation(expression)
	if symbol == nil || symbol.Flags&(ast.SymbolFlagsVariable|ast.SymbolFlagsFunction|ast.SymbolFlagsClass) == 0 {
		return nil, false
	}
	declaration := symbol.ValueDeclaration
	if declaration == nil || ast.GetSourceFileOfNode(declaration) != b.state.SourceFile || getDeclarationFromImport(symbol) != nil {
		return nil, false
	}
	if ast.IsVariableDeclaration(declaration) && !ast.IsVarConst(declaration) {
		return nil, false
	}
	if !isTopLevelDeclaration(declaration) || !luau.IsValidIdentifier(symbol.Name) || b.state.SymbolToID[symbol] != nil {
		return nil, false
	}
	if ctx.Owner != nil && declaration.Pos() > ctx.Owner.Pos() {
		return nil, false
	}
	return luau.ID(symbol.Name), true
}

// isTopLevelDeclaration reports a declaration whose nearest enclosing scope
// is the source file: not inside a block, a function, a class or a
// namespace.
func isTopLevelDeclaration(declaration *ast.Node) bool {
	for parent := declaration.Parent; parent != nil; parent = parent.Parent {
		switch {
		case ast.IsSourceFile(parent):
			return true
		case ast.IsBlock(parent), ast.IsModuleDeclaration(parent), ast.IsFunctionLike(parent), ast.IsClassLike(parent):
			return false
		}
	}
	return false
}

// clone copies a node reused at several sites and carries its origin along.
func (b *typeBuilder) clone(node luau.TypeNode) luau.TypeNode {
	clone := cloneType(node)
	if symbol, ok := b.Origins[node]; ok {
		b.Origins[clone] = symbol
	}
	return clone
}
