package transformer

import (
	"slices"
	"strconv"
	"strings"

	"rotor/internal/luau"
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

// Type-function translation (plan WP8): a generic type alias whose body
// contains a mapped, conditional or template literal type has no Luau
// spelling as an alias, but Luau's new solver can compute it at analysis
// time from a user-defined `type function`. The translator here works from
// the TS AST of the alias body and emits the Lua of that body over the
// `types` library (luautypes_typefunction_prelude.go holds the DSL, the
// local helpers and the prelude for TS's built-in helper types).
//
// What is translated, and how:
//   - a type parameter, an `infer` variable or a mapped key is the Lua
//     local that holds its `type`;
//   - `keyof T` is the union of a table's property keys; `T[K]` is the
//     property's read type (distributed over a union key), or, inside the
//     mapped type that iterates `T`, the property record itself;
//   - a homomorphic mapped type (`[K in keyof T]`, or `[K in P]` with `P`
//     bound by `keyof T`) copies the table and rewrites every property,
//     honouring `?`, `-?`, `readonly`, `-readonly` and an `as` clause; any
//     other mapped type builds a new table from the key union;
//   - a conditional type is `if check:issubtypeof(extends)`, distributed
//     over the components of a naked type parameter; `infer` is supported
//     in `T extends (infer U)[]` and `T extends (...args: any) => infer R`;
//   - a template literal concatenates singleton values, or is `string`
//     once a hole is not a singleton;
//   - unions, intersections, optionals, literals, primitives and static
//     object, array and function types go through the `types.*`
//     constructors.
//
// Anything else, including a reference to another alias or type function
// (type-function bodies see no outer names), makes the whole alias fall
// back to `type Name<...> = any` with a loss comment; instantiated uses of
// such an alias are still spelled from the checker's resolved type, which
// is exact.

// inferReason is the loss reason for an `infer` outside the two shapes.
const inferReason = "`infer` is only translated in `T extends (infer U)[]` and `T extends (...args: any) => infer R`"

// valueSink places a computed type value: `return v`, `name = v` or
// `table.insert(parts, v)`. Branching computations call it once per branch.
type valueSink func(value luau.Expression) luau.Statement

// mappedModifier is a mapped type's `?` or `readonly` modifier.
type mappedModifier uint8

const (
	modifierNone mappedModifier = iota
	modifierAdd
	modifierRemove
)

// indexedBinding is what `Object[Key]` means while a mapped type iterates
// the properties of Object: the property's current type in the slot being
// rewritten. A nil value means the key is not bound to a property record
// and the access goes through the indexof helper.
type indexedBinding struct {
	object *ast.Symbol
	value  func() luau.Expression
}

// typeFunctionTranslator turns one alias body into type-function statements.
type typeFunctionTranslator struct {
	state       *State
	name        string
	declaration *ast.Node
	// distributable are the alias's own type parameters and the infer
	// variables in scope: a conditional over one of them distributes.
	distributable map[*ast.Symbol]struct{}
	// env maps a type parameter, infer variable or mapped key to a factory
	// for the Lua expression holding its `type`; a factory, because every
	// use site adopts its own node.
	env     map[*ast.Symbol]func() luau.Expression
	indexed map[*ast.Symbol]indexedBinding
	// losses are the non-fatal substitutions (number literals, tuples);
	// failure is the loss that abandoned the translation.
	losses  []fidelityLoss
	failure *fidelityLoss
	// topKind is the loss kind of the body's own construct, for a failure
	// with no more specific kind.
	topKind fidelityLossKind
	names   map[string]struct{}
	helpers map[string]struct{}
	block   *luau.List[luau.Statement]
}

// luauTypeFunctionFor translates the body of a generic type alias into a
// `type function` with every parameter annotated `: type`. false, with the
// failure recorded as a loss against the declaration, when the body is
// outside the supported subset; the caller then emits the `any` fallback.
func (state *State) luauTypeFunctionFor(alias *ast.Symbol, declaration *ast.Node) (*luau.TypeFunctionDeclaration, bool) {
	b := state.types()
	if alias == nil || declaration == nil || !ast.IsTypeAliasDeclaration(declaration) {
		return nil, false
	}
	tr := &typeFunctionTranslator{
		state:         state,
		name:          alias.Name,
		declaration:   declaration,
		distributable: make(map[*ast.Symbol]struct{}),
		env:           make(map[*ast.Symbol]func() luau.Expression),
		indexed:       make(map[*ast.Symbol]indexedBinding),
		names:         make(map[string]struct{}),
		helpers:       make(map[string]struct{}),
	}
	for _, reserved := range []string{"types", "type", "error", "table", "tostring", "nil"} {
		tr.names[reserved] = struct{}{}
	}
	for _, helper := range typeFunctionHelperOrder {
		tr.names[helper] = struct{}{}
	}
	var parameters []string
	for _, parameter := range declaredTypeParameters(declaration) {
		name := parameter.Name()
		symbol := parameter.Symbol()
		if name == nil || symbol == nil || !luau.IsValidIdentifier(name.Text()) {
			return nil, false
		}
		parameterName := name.Text()
		parameters = append(parameters, parameterName)
		tr.names[parameterName] = struct{}{}
		tr.distributable[symbol] = struct{}{}
		tr.env[symbol] = func() luau.Expression { return luau.ID(parameterName) }
	}
	statements, ok := tr.translate(declaration.AsTypeAliasDeclaration().Type)
	b.losses = append(b.losses, tr.losses...)
	if !ok {
		failure := tr.failure
		if failure == nil {
			failure = tr.loss(tr.topKind, "the alias body is outside the translated subset")
		}
		b.losses = append(b.losses, *failure)
		return nil, false
	}
	return tfTypeFunction(alias.Name, parameters, statements), true
}

// translate emits the body: the helpers it needed first, then the
// statements, ending in `return <type>`.
func (tr *typeFunctionTranslator) translate(body *ast.Node) (*luau.List[luau.Statement], bool) {
	if body == nil {
		return nil, tr.fail(lossUnspellable, "the alias has no body")
	}
	tr.topKind = topLevelLossKind(body)
	tr.block = luau.NewList[luau.Statement]()
	if !tr.value(body, func(value luau.Expression) luau.Statement { return tfReturn(value) }, true) {
		return nil, false
	}
	statements := tfStmts(typeFunctionHelperStatements(tr.helpers)...)
	statements.PushList(tr.block)
	return statements, true
}

// ---------------------------------------------------------------------------
// Bookkeeping
// ---------------------------------------------------------------------------

// loss builds a loss record against the declaration, with the TS type as
// the checker prints it and `any` as the replacement.
func (tr *typeFunctionTranslator) loss(kind fidelityLossKind, reason string) *fidelityLoss {
	tsText := tr.name
	if symbol := tr.declaration.Symbol(); symbol != nil {
		if declared := tr.state.Checker.GetDeclaredTypeOfSymbol(symbol); declared != nil {
			tsText = tr.state.Checker.TypeToString(declared)
		}
	}
	return &fidelityLoss{Kind: kind, TSText: tsText, Luau: nativeTypeAny, Reason: reason, Node: tr.declaration}
}

// fail abandons the translation with the first failure's kind and reason.
// It always returns false so a caller can `return nil, tr.fail(...)`.
func (tr *typeFunctionTranslator) fail(kind fidelityLossKind, reason string) bool {
	if tr.failure == nil {
		tr.failure = tr.loss(kind, reason)
	}
	return false
}

// lose records a non-fatal substitution.
func (tr *typeFunctionTranslator) lose(kind fidelityLossKind, tsText, luauText, reason string) {
	tr.losses = append(tr.losses, fidelityLoss{Kind: kind, TSText: tsText, Luau: luauText, Reason: reason, Node: tr.declaration})
}

// fresh picks a local name that collides with no parameter, helper or
// earlier local: base, else base_2, base_3, ...
func (tr *typeFunctionTranslator) fresh(base string) string {
	name := base
	for suffix := 2; ; suffix++ {
		if _, taken := tr.names[name]; !taken && luau.IsValidIdentifier(name) {
			tr.names[name] = struct{}{}
			return name
		}
		name = base + "_" + strconv.Itoa(suffix)
	}
}

func (tr *typeFunctionTranslator) emit(statement luau.Statement) {
	tr.block.Push(statement)
}

// nested runs emit into a fresh block and returns it.
func (tr *typeFunctionTranslator) nested(run func() bool) (*luau.List[luau.Statement], bool) {
	saved := tr.block
	tr.block = luau.NewList[luau.Statement]()
	ok := run()
	block := tr.block
	tr.block = saved
	return block, ok
}

// bind maps symbol to an expression factory for the duration of run.
func (tr *typeFunctionTranslator) bind(symbol *ast.Symbol, factory func() luau.Expression, run func() bool) bool {
	saved, had := tr.env[symbol]
	tr.env[symbol] = factory
	ok := run()
	if had {
		tr.env[symbol] = saved
	} else {
		delete(tr.env, symbol)
	}
	return ok
}

// pin makes an expression reusable: an identifier is re-spelled at each
// use, anything else is bound to a local first.
func (tr *typeFunctionTranslator) pin(expression luau.Expression, base string) func() luau.Expression {
	if id, ok := expression.(*luau.Identifier); ok {
		name := id.Name
		return func() luau.Expression { return luau.ID(name) }
	}
	name := tr.fresh(base)
	tr.emit(tfLocal(name, expression))
	return func() luau.Expression { return luau.ID(name) }
}

// pinID is pin for a receiver of `:is` / `:issubtypeof` calls.
func (tr *typeFunctionTranslator) pinID(expression luau.Expression, base string) func() *luau.Identifier {
	factory := tr.pin(expression, base)
	return func() *luau.Identifier {
		id, ok := factory().(*luau.Identifier)
		if !ok {
			panic("transformer: pinned expression is not an identifier")
		}
		return id
	}
}

func (tr *typeFunctionTranslator) helper(name string) *luau.Identifier {
	tr.helpers[name] = struct{}{}
	return luau.ID(name)
}

// ---------------------------------------------------------------------------
// Values and expressions
// ---------------------------------------------------------------------------

// value computes node and hands the result to sink. A conditional or mapped
// type emits its branches with the sink applied in each; tail marks the
// body's own result position, where a guard may `return` early.
func (tr *typeFunctionTranslator) value(node *ast.Node, sink valueSink, tail bool) bool {
	node = skipParenthesizedTypes(node)
	switch node.Kind {
	case ast.KindConditionalType:
		return tr.conditional(node, sink, tail)
	case ast.KindMappedType:
		return tr.mapped(node, sink, tail)
	}
	expression, ok := tr.typeExpr(node)
	if !ok {
		return false
	}
	tr.emit(sink(expression))
	return true
}

// typeExpr spells node as a Lua expression of type `type`, emitting the
// statements it needs first.
func (tr *typeFunctionTranslator) typeExpr(node *ast.Node) (luau.Expression, bool) {
	node = skipParenthesizedTypes(node)
	switch node.Kind {
	case ast.KindConditionalType, ast.KindMappedType:
		name := tr.fresh("value")
		tr.emit(tfTypedLocal(name))
		ok := tr.value(node, func(value luau.Expression) luau.Statement { return tfSet(luau.ID(name), value) }, false)
		return luau.ID(name), ok
	case ast.KindTypeReference:
		return tr.reference(node)
	case ast.KindTypeOperator:
		return tr.operator(node)
	case ast.KindIndexedAccessType:
		return tr.indexedAccess(node)
	case ast.KindTemplateLiteralType:
		return tr.template(node)
	case ast.KindUnionType:
		return tr.union(node)
	case ast.KindIntersectionType:
		return tr.intersection(node)
	case ast.KindInferType:
		return nil, tr.fail(lossInferType, inferReason)
	}
	return tr.staticType(node)
}

// reference spells a type parameter, infer variable or mapped key from the
// environment; any other name is outside the type function.
func (tr *typeFunctionTranslator) reference(node *ast.Node) (luau.Expression, bool) {
	reference := node.AsTypeReferenceNode()
	if symbol := tr.state.Checker.GetSymbolAtLocation(reference.TypeName); symbol != nil {
		if factory := tr.env[symbol]; factory != nil && len(node.TypeArguments()) == 0 {
			return factory(), true
		}
	}
	return nil, tr.fail(tr.topKind, "`"+entityNameText(reference.TypeName)+"` names a type outside the type function; type-function bodies see no outer names")
}

// envSymbol is the environment symbol node refers to, or nil.
func (tr *typeFunctionTranslator) envSymbol(node *ast.Node) *ast.Symbol {
	node = skipParenthesizedTypes(node)
	if node.Kind != ast.KindTypeReference || len(node.TypeArguments()) > 0 {
		return nil
	}
	symbol := tr.state.Checker.GetSymbolAtLocation(node.AsTypeReferenceNode().TypeName)
	if symbol == nil || tr.env[symbol] == nil {
		return nil
	}
	return symbol
}

// operator spells `keyof T` through the keys helper and `readonly T[]` as a
// read-only array.
func (tr *typeFunctionTranslator) operator(node *ast.Node) (luau.Expression, bool) {
	operator := node.AsTypeOperatorNode()
	switch operator.Operator {
	case ast.KindKeyOfKeyword:
		inner, ok := tr.typeExpr(operator.Type)
		if !ok {
			return nil, false
		}
		return tfCall(tr.helper(helperKeys), inner), true
	case ast.KindReadonlyKeyword:
		inner := skipParenthesizedTypes(operator.Type)
		switch inner.Kind {
		case ast.KindArrayType:
			return tr.array(inner, true)
		case ast.KindTupleType:
			return tr.tuple(inner, true)
		}
		return tr.typeExpr(operator.Type)
	}
	return nil, tr.fail(tr.topKind, "`unique symbol` has no Luau spelling")
}

// indexedAccess spells `T[K]`: the property record of the mapped iteration
// over T when K is its key, else the indexof helper.
func (tr *typeFunctionTranslator) indexedAccess(node *ast.Node) (luau.Expression, bool) {
	access := node.AsIndexedAccessTypeNode()
	if key := tr.envSymbol(access.IndexType); key != nil {
		if binding, ok := tr.indexed[key]; ok && binding.value != nil && binding.object != nil && tr.envSymbol(access.ObjectType) == binding.object {
			return binding.value(), true
		}
	}
	object, ok := tr.typeExpr(access.ObjectType)
	if !ok {
		return nil, false
	}
	index, ok := tr.typeExpr(access.IndexType)
	if !ok {
		return nil, false
	}
	return tfCall(tr.helper(helperIndexOf), object, index), true
}

// template spells a template literal type through the template helper.
func (tr *typeFunctionTranslator) template(node *ast.Node) (luau.Expression, bool) {
	literal := node.AsTemplateLiteralTypeNode()
	var parts []luau.Expression
	if text := literal.Head.Text(); text != "" {
		parts = append(parts, luau.Str(text))
	}
	for _, span := range literal.TemplateSpans.Nodes {
		typeSpan := span.AsTemplateLiteralTypeSpan()
		hole, ok := tr.typeExpr(typeSpan.Type)
		if !ok {
			return nil, false
		}
		parts = append(parts, hole)
		if text := typeSpan.Literal.Text(); text != "" {
			parts = append(parts, luau.Str(text))
		}
	}
	return tfCall(tr.helper(helperTemplate), tfArray(parts...)), true
}

// union spells `A | B` as types.unionof, with nil-like members folded into
// types.optional.
func (tr *typeFunctionTranslator) union(node *ast.Node) (luau.Expression, bool) {
	var members []luau.Expression
	optional := false
	for _, member := range node.AsUnionTypeNode().Types.Nodes {
		if isNilTypeNode(member) {
			optional = true
			continue
		}
		expression, ok := tr.typeExpr(member)
		if !ok {
			return nil, false
		}
		members = append(members, expression)
	}
	var result luau.Expression
	switch len(members) {
	case 0:
		return tfTypesCall("singleton", luau.Nil()), true
	case 1:
		result = members[0]
	default:
		result = tfTypesCall("unionof", members...)
	}
	if optional {
		result = tfTypesCall("optional", result)
	}
	return result, true
}

// intersection spells `A & B` as types.intersectionof.
func (tr *typeFunctionTranslator) intersection(node *ast.Node) (luau.Expression, bool) {
	var members []luau.Expression
	for _, member := range node.AsIntersectionTypeNode().Types.Nodes {
		expression, ok := tr.typeExpr(member)
		if !ok {
			return nil, false
		}
		members = append(members, expression)
	}
	if len(members) == 1 {
		return members[0], true
	}
	return tfTypesCall("intersectionof", members...), true
}

// staticType spells the type nodes that need no environment: literals,
// primitives, arrays, tuples, object and function types.
func (tr *typeFunctionTranslator) staticType(node *ast.Node) (luau.Expression, bool) {
	switch node.Kind {
	case ast.KindLiteralType:
		return tr.literal(node)
	case ast.KindStringKeyword:
		return tfTypes("string"), true
	case ast.KindNumberKeyword:
		return tfTypes("number"), true
	case ast.KindBooleanKeyword:
		return tfTypes("boolean"), true
	case ast.KindAnyKeyword:
		return tfTypes("any"), true
	case ast.KindUnknownKeyword:
		return tfTypes("unknown"), true
	case ast.KindNeverKeyword:
		return tfTypes("never"), true
	case ast.KindUndefinedKeyword, ast.KindVoidKeyword, ast.KindNullKeyword:
		return tfTypesCall("singleton", luau.Nil()), true
	case ast.KindObjectKeyword:
		return tfTypesCall("newtable", luau.Nil(), tfMap(
			tfField("index", tfTypes("any")), tfField("readresult", tfTypes("any")), tfField("writeresult", tfTypes("any")),
		)), true
	case ast.KindSymbolKeyword:
		tr.lose(lossSymbolKey, "symbol", nativeTypeAny, "Luau has no symbol type")
		return tfTypes("any"), true
	case ast.KindBigIntKeyword:
		tr.lose(lossBigInt, "bigint", nativeTypeNumber, "Luau has no bigint")
		return tfTypes("number"), true
	case ast.KindArrayType:
		return tr.array(node, false)
	case ast.KindTupleType:
		return tr.tuple(node, false)
	case ast.KindTypeLiteral:
		return tr.typeLiteral(node)
	case ast.KindFunctionType:
		return tr.functionType(node)
	case ast.KindThisType:
		return nil, tr.fail(tr.topKind, "`this` has no meaning inside a type function")
	case ast.KindTypeQuery:
		return nil, tr.fail(tr.topKind, "`typeof` names a value outside the type function")
	}
	return nil, tr.fail(tr.topKind, "a "+node.Kind.String()+" has no translation")
}

// literal spells a literal type: string and boolean singletons, nil, and
// number widened to `number`.
func (tr *typeFunctionTranslator) literal(node *ast.Node) (luau.Expression, bool) {
	literal := node.AsLiteralTypeNode().Literal
	switch literal.Kind {
	case ast.KindStringLiteral, ast.KindNoSubstitutionTemplateLiteral:
		return tfTypesCall("singleton", luau.Str(literal.Text())), true
	case ast.KindTrueKeyword:
		return tfTypesCall("singleton", luau.Bool(true)), true
	case ast.KindFalseKeyword:
		return tfTypesCall("singleton", luau.Bool(false)), true
	case ast.KindNullKeyword:
		return tfTypesCall("singleton", luau.Nil()), true
	case ast.KindNumericLiteral, ast.KindPrefixUnaryExpression:
		tr.lose(lossNumberLiteral, tr.state.Checker.TypeToString(tr.state.Checker.GetTypeFromTypeNode(node)), nativeTypeNumber, "Luau has no number singletons")
		return tfTypes("number"), true
	case ast.KindBigIntLiteral:
		tr.lose(lossBigInt, literal.Text(), nativeTypeNumber, "Luau has no bigint")
		return tfTypes("number"), true
	}
	return nil, tr.fail(tr.topKind, "unknown literal type")
}

// array spells `E[]` as a number-indexed table; readonly drops the write
// result.
func (tr *typeFunctionTranslator) array(node *ast.Node, readonly bool) (luau.Expression, bool) {
	element, ok := tr.typeExpr(node.AsArrayTypeNode().ElementType)
	if !ok {
		return nil, false
	}
	return tr.arrayOf(element, readonly), true
}

func (tr *typeFunctionTranslator) arrayOf(element luau.Expression, readonly bool) luau.Expression {
	if readonly {
		return tfTypesCall("newtable", luau.Nil(), tfMap(tfField("index", tfTypes("number")), tfField("readresult", element)))
	}
	pinned := tr.pin(element, "element")
	return tfTypesCall("newtable", luau.Nil(), tfMap(
		tfField("index", tfTypes("number")), tfField("readresult", pinned()), tfField("writeresult", pinned()),
	))
}

// tuple spells `[A, B]` as an array of the element union, as the builder
// does for tuples (positions are lost).
func (tr *typeFunctionTranslator) tuple(node *ast.Node, readonly bool) (luau.Expression, bool) {
	var elements []luau.Expression
	for _, element := range node.AsTupleTypeNode().Elements.Nodes {
		if element.Kind == ast.KindNamedTupleMember {
			element = element.AsNamedTupleMember().Type
		}
		switch skipParenthesizedTypes(element).Kind {
		case ast.KindOptionalType, ast.KindRestType:
			return nil, tr.fail(tr.topKind, "optional and rest tuple elements are not translated")
		}
		expression, ok := tr.typeExpr(element)
		if !ok {
			return nil, false
		}
		elements = append(elements, expression)
	}
	var member luau.Expression
	switch len(elements) {
	case 0:
		member = tfTypes("never")
	case 1:
		member = elements[0]
	default:
		member = tfTypesCall("unionof", elements...)
	}
	tr.lose(lossTuple, tr.state.Checker.TypeToString(tr.state.Checker.GetTypeFromTypeNode(node)), "{ "+nativeTypeAny+" }", "Luau has no tuple types; positions are lost")
	return tr.arrayOf(member, readonly), true
}

// typeLiteral spells `{ a: A, readonly b?: B, [k: string]: V }` through
// types.newtable. Property keys are string singletons; with a read-only
// member every property takes the `{ read, write }` record form, because
// newtable accepts one form per call.
func (tr *typeFunctionTranslator) typeLiteral(node *ast.Node) (luau.Expression, bool) {
	type property struct {
		name     string
		value    func() luau.Expression
		readonly bool
	}
	var properties []property
	var indexer *luau.Map
	anyReadonly := false
	for _, member := range node.Members() {
		switch member.Kind {
		case ast.KindPropertySignature:
			signature := member.AsPropertySignatureDeclaration()
			name, ok := ast.TryGetTextOfPropertyName(signature.Name())
			if !ok {
				return nil, tr.fail(tr.topKind, "a computed property name has no translation")
			}
			if strings.HasPrefix(name, "__@") {
				tr.lose(lossSymbolKey, name, "", "symbol-keyed members are skipped")
				continue
			}
			value, ok := tr.memberType(signature.Type)
			if !ok {
				return nil, false
			}
			if signature.PostfixToken != nil && signature.PostfixToken.Kind == ast.KindQuestionToken {
				value = tfTypesCall("optional", value)
			}
			readonly := ast.HasSyntacticModifier(member, ast.ModifierFlagsReadonly)
			anyReadonly = anyReadonly || readonly
			properties = append(properties, property{name: name, value: tr.pin(value, "member"), readonly: readonly})
		case ast.KindIndexSignature:
			if indexer != nil {
				return nil, tr.fail(tr.topKind, "Luau allows one indexer")
			}
			parameters := member.Parameters()
			if len(parameters) != 1 {
				return nil, tr.fail(tr.topKind, "an index signature needs one parameter")
			}
			key, ok := tr.memberType(parameters[0].AsParameterDeclaration().Type)
			if !ok {
				return nil, false
			}
			value, ok := tr.memberType(member.AsIndexSignatureDeclaration().Type)
			if !ok {
				return nil, false
			}
			fields := []*luau.MapField{tfField("index", key)}
			if ast.HasSyntacticModifier(member, ast.ModifierFlagsReadonly) {
				fields = append(fields, tfField("readresult", value))
			} else {
				pinned := tr.pin(value, "indexed")
				fields = append(fields, tfField("readresult", pinned()), tfField("writeresult", pinned()))
			}
			indexer = tfMap(fields...)
		default:
			return nil, tr.fail(tr.topKind, "a "+member.Kind.String()+" member has no translation")
		}
	}
	var arguments []luau.Expression
	if len(properties) > 0 {
		fields := make([]*luau.MapField, 0, len(properties))
		for _, p := range properties {
			key := tfTypesCall("singleton", luau.Str(p.name))
			switch {
			case !anyReadonly:
				fields = append(fields, luau.NewMapField(key, p.value()))
			case p.readonly:
				fields = append(fields, luau.NewMapField(key, tfMap(tfField("read", p.value()))))
			default:
				fields = append(fields, luau.NewMapField(key, tfMap(tfField("read", p.value()), tfField("write", p.value()))))
			}
		}
		arguments = append(arguments, tfMap(fields...))
	}
	if indexer != nil {
		if len(arguments) == 0 {
			arguments = append(arguments, luau.Nil())
		}
		arguments = append(arguments, indexer)
	}
	return tfTypesCall("newtable", arguments...), true
}

// memberType spells a member's written type, `any` when it has none.
func (tr *typeFunctionTranslator) memberType(node *ast.Node) (luau.Expression, bool) {
	if node == nil {
		return tfTypes("any"), true
	}
	return tr.typeExpr(node)
}

// functionType spells `(a: A, b?: B, ...rest: R[]) => T` through
// types.newfunction; a `this` parameter is the first head entry, a void
// return is an empty pack.
func (tr *typeFunctionTranslator) functionType(node *ast.Node) (luau.Expression, bool) {
	if len(node.TypeParameters()) > 0 {
		return nil, tr.fail(tr.topKind, "generic function types are not translated")
	}
	var head []luau.Expression
	var tail luau.Expression
	for _, parameterNode := range node.Parameters() {
		parameter := parameterNode.AsParameterDeclaration()
		if parameter.DotDotDotToken != nil {
			rest := parameter.Type
			if rest != nil {
				rest = skipParenthesizedTypes(rest)
			}
			switch {
			case rest == nil || rest.Kind == ast.KindAnyKeyword:
				tail = tfTypes("any")
			case rest.Kind == ast.KindArrayType:
				element, ok := tr.typeExpr(rest.AsArrayTypeNode().ElementType)
				if !ok {
					return nil, false
				}
				tail = element
			default:
				return nil, tr.fail(tr.topKind, "a rest parameter that is not an array has no Luau spelling")
			}
			continue
		}
		value, ok := tr.memberType(parameter.Type)
		if !ok {
			return nil, false
		}
		if parameter.QuestionToken != nil {
			value = tfTypesCall("optional", value)
		}
		head = append(head, value)
	}
	var parameterFields []*luau.MapField
	if len(head) > 0 {
		parameterFields = append(parameterFields, tfField("head", tfArray(head...)))
	}
	if tail != nil {
		parameterFields = append(parameterFields, tfField("tail", tail))
	}
	returns := tfMap()
	if returnNode := node.AsFunctionTypeNode().Type; returnNode != nil && !isNilTypeNode(returnNode) {
		value, ok := tr.typeExpr(returnNode)
		if !ok {
			return nil, false
		}
		returns = tfMap(tfField("head", tfArray(value)))
	}
	return tfTypesCall("newfunction", tfMap(parameterFields...), returns), true
}

// ---------------------------------------------------------------------------
// Conditional types
// ---------------------------------------------------------------------------

// conditional emits `T extends U ? X : Y`, distributed over the components
// of a naked type parameter.
func (tr *typeFunctionTranslator) conditional(node *ast.Node, sink valueSink, tail bool) bool {
	conditional := node.AsConditionalTypeNode()
	check := tr.envSymbol(conditional.CheckType)
	if check == nil {
		return tr.branches(conditional, sink, tail)
	}
	if _, distributes := tr.distributable[check]; !distributes {
		return tr.branches(conditional, sink, tail)
	}
	parts := tr.fresh("parts")
	item := tr.fresh("item")
	tr.emit(tfLocal(parts, tfMap()))
	source := tr.env[check]()
	body, ok := tr.nested(func() bool {
		return tr.bind(check, func() luau.Expression { return luau.ID(item) }, func() bool {
			return tr.branches(conditional, func(value luau.Expression) luau.Statement { return tfInsert(luau.ID(parts), value) }, false)
		})
	})
	if !ok {
		return false
	}
	tr.emit(tfForIn([]string{"_", item}, tfCall(tr.helper(helperDistribute), source), body))
	tr.emit(sink(tfCall(tr.helper(helperJoin), luau.ID(parts))))
	return true
}

// branches emits the `if check:issubtypeof(extends) then X else Y end` of a
// conditional, binding an `infer` variable for the true branch when the
// extends type is `(infer U)[]` or `(...args: any) => infer R`.
func (tr *typeFunctionTranslator) branches(conditional *ast.ConditionalTypeNode, sink valueSink, tail bool) bool {
	checkExpression, ok := tr.typeExpr(conditional.CheckType)
	if !ok {
		return false
	}
	check := tr.pinID(checkExpression, "check")
	extends := skipParenthesizedTypes(conditional.ExtendsType)
	var condition luau.Expression
	var inferred *ast.Symbol
	var inferredValue func() luau.Expression
	var inferredPrelude luau.Statement
	arrayInfer := arrayInferSymbol(extends)
	functionInfer := functionInferSymbol(extends)
	switch {
	case arrayInfer != nil:
		inferred = arrayInfer
		indexer := tr.fresh("indexer")
		tr.emit(tfLocal(indexer, tfIfExpr(tfIs(check(), "table"), tfMethod(check(), "indexer"), luau.Nil())))
		condition = tfAnd(luau.ID(indexer), tfEq(tfProp(luau.ID(indexer), "index"), tfTypes("number")))
		inferredValue = func() luau.Expression { return tfProp(luau.ID(indexer), "readresult") }
	case functionInfer != nil:
		inferred = functionInfer
		condition = tfIs(check(), "function")
		name := tr.fresh(functionInfer.Name)
		inferredPrelude = tfLocal(name, tfCall(tr.helper(helperReturnOf), check()))
		inferredValue = func() luau.Expression { return luau.ID(name) }
	case containsInferType(extends):
		return tr.fail(lossInferType, inferReason)
	default:
		extendsExpression, ok := tr.typeExpr(conditional.ExtendsType)
		if !ok {
			return false
		}
		condition = tfMethod(check(), "issubtypeof", extendsExpression)
	}
	trueBlock, ok := tr.nested(func() bool {
		if inferred == nil {
			return tr.value(conditional.TrueType, sink, tail)
		}
		if inferredPrelude != nil {
			tr.emit(inferredPrelude)
		}
		tr.distributable[inferred] = struct{}{}
		defer delete(tr.distributable, inferred)
		return tr.bind(inferred, inferredValue, func() bool { return tr.value(conditional.TrueType, sink, tail) })
	})
	if !ok {
		return false
	}
	falseBlock, ok := tr.nested(func() bool { return tr.value(conditional.FalseType, sink, tail) })
	if !ok {
		return false
	}
	tr.emit(tfIf(condition, trueBlock, falseBlock))
	return true
}

// arrayInferSymbol is the infer variable of `(infer U)[]` or
// `readonly (infer U)[]`, or nil.
func arrayInferSymbol(extends *ast.Node) *ast.Symbol {
	if extends.Kind == ast.KindTypeOperator && extends.AsTypeOperatorNode().Operator == ast.KindReadonlyKeyword {
		extends = skipParenthesizedTypes(extends.AsTypeOperatorNode().Type)
	}
	if extends.Kind != ast.KindArrayType {
		return nil
	}
	element := skipParenthesizedTypes(extends.AsArrayTypeNode().ElementType)
	if element.Kind != ast.KindInferType {
		return nil
	}
	return element.AsInferTypeNode().TypeParameter.Symbol()
}

// functionInferSymbol is the infer variable of `(...args: any) => infer R`
// (any rest parameter whose type holds no infer, or no parameter), or nil.
func functionInferSymbol(extends *ast.Node) *ast.Symbol {
	if extends.Kind != ast.KindFunctionType {
		return nil
	}
	returnType := extends.AsFunctionTypeNode().Type
	if returnType == nil || skipParenthesizedTypes(returnType).Kind != ast.KindInferType {
		return nil
	}
	parameters := extends.Parameters()
	if len(parameters) > 1 || len(extends.TypeParameters()) > 0 {
		return nil
	}
	if len(parameters) == 1 {
		parameter := parameters[0].AsParameterDeclaration()
		if parameter.DotDotDotToken == nil || parameter.Type != nil && containsInferType(parameter.Type) {
			return nil
		}
	}
	return skipParenthesizedTypes(returnType).AsInferTypeNode().TypeParameter.Symbol()
}

// containsInferType reports an `infer` anywhere under node.
func containsInferType(node *ast.Node) bool {
	if node.Kind == ast.KindInferType {
		return true
	}
	found := false
	var visit func(child *ast.Node) bool
	visit = func(child *ast.Node) bool {
		if child.Kind == ast.KindInferType {
			found = true
			return true
		}
		return child.ForEachChild(visit)
	}
	node.ForEachChild(visit)
	return found
}

// ---------------------------------------------------------------------------
// Mapped types
// ---------------------------------------------------------------------------

// mapped emits `{ [K in Keys as Name]?: Value }`: over the properties of a
// table when the keys are `keyof T` (or a parameter bound by `keyof T`),
// else over the components of the key union.
func (tr *typeFunctionTranslator) mapped(node *ast.Node, sink valueSink, tail bool) bool {
	mapped := node.AsMappedTypeNode()
	if len(node.Members()) > 0 {
		return tr.fail(lossMappedType, "a mapped type with extra members is not translated")
	}
	if mapped.TypeParameter == nil || mapped.Type == nil {
		return tr.fail(lossMappedType, "a mapped type without a key constraint or a value type is not translated")
	}
	keySymbol := mapped.TypeParameter.Symbol()
	constraint := mapped.TypeParameter.AsTypeParameterDeclaration().Constraint
	if keySymbol == nil || constraint == nil {
		return tr.fail(lossMappedType, "a mapped type without a key constraint is not translated")
	}
	readonly := mappedModifierOf(mapped.ReadonlyToken)
	optional := mappedModifierOf(mapped.QuestionToken)
	if source, filter := tr.homomorphicSource(constraint); source != nil {
		return tr.mappedOverProperties(mapped, source, filter, keySymbol, readonly, optional, sink, tail)
	}
	return tr.mappedOverKeys(mapped, constraint, keySymbol, readonly, optional, sink)
}

// mappedModifierOf reads a `+`, `-`, `readonly` or `?` token.
func mappedModifierOf(token *ast.Node) mappedModifier {
	switch {
	case token == nil:
		return modifierNone
	case token.Kind == ast.KindMinusToken:
		return modifierRemove
	}
	return modifierAdd
}

// homomorphicSource is the object type a mapped type iterates when its
// keys are `keyof Source`, with the type parameter that filters the keys
// when the constraint is a parameter itself bound by `keyof Source`
// (`Pick`'s shape). nil for any other key type.
func (tr *typeFunctionTranslator) homomorphicSource(constraint *ast.Node) (source *ast.Node, filter *ast.Symbol) {
	constraint = skipParenthesizedTypes(constraint)
	if constraint.Kind == ast.KindTypeOperator && constraint.AsTypeOperatorNode().Operator == ast.KindKeyOfKeyword {
		return constraint.AsTypeOperatorNode().Type, nil
	}
	symbol := tr.envSymbol(constraint)
	if symbol == nil {
		return nil, nil
	}
	if _, own := tr.distributable[symbol]; !own {
		return nil, nil
	}
	for _, declaration := range symbol.Declarations {
		if !ast.IsTypeParameterDeclaration(declaration) {
			continue
		}
		bound := declaration.AsTypeParameterDeclaration().Constraint
		if bound == nil {
			return nil, nil
		}
		bound = skipParenthesizedTypes(bound)
		if bound.Kind == ast.KindTypeOperator && bound.AsTypeOperatorNode().Operator == ast.KindKeyOfKeyword {
			return bound.AsTypeOperatorNode().Type, symbol
		}
	}
	return nil, nil
}

// mappedOverProperties emits the homomorphic shape: a non-table source is
// itself (the body's own result returns it early), a table is copied and
// every property rewritten; with an `as` clause or a key filter the table
// is rebuilt from the kept properties instead.
func (tr *typeFunctionTranslator) mappedOverProperties(mapped *ast.MappedTypeNode, sourceNode *ast.Node, filter, keySymbol *ast.Symbol, readonly, optional mappedModifier, sink valueSink, tail bool) bool {
	sourceExpression, ok := tr.typeExpr(sourceNode)
	if !ok {
		return false
	}
	sourceSymbol := tr.envSymbol(sourceNode)
	source := tr.pinID(sourceExpression, "source")
	rebuild := mapped.NameType != nil || filter != nil
	result := tr.fresh("result")
	key := tr.fresh("key")
	prop := tr.fresh("prop")

	loop, ok := tr.nested(func() bool {
		return tr.bind(keySymbol, func() luau.Expression { return luau.ID(key) }, func() bool {
			tr.indexed[keySymbol] = indexedBinding{object: sourceSymbol}
			defer delete(tr.indexed, keySymbol)
			target := func() luau.Expression { return luau.ID(key) }
			if mapped.NameType != nil {
				nameExpression, ok := tr.typeExpr(mapped.NameType)
				if !ok {
					if tr.failure != nil {
						tr.failure.Kind = lossKeyRemap
					}
					return false
				}
				name := tr.fresh("name")
				tr.emit(tfLocal(name, nameExpression))
				target = func() luau.Expression { return luau.ID(name) }
			}
			rewrite, ok := tr.nested(func() bool {
				return tr.propertyRewrite(mapped.Type, keySymbol, result, prop, target, readonly, optional, !rebuild)
			})
			if !ok {
				return false
			}
			if mapped.NameType != nil {
				named, ok := target().(*luau.Identifier)
				if !ok {
					return false
				}
				rewrite = tfStmts(tfIf(tfIs(named, "singleton"), rewrite,
					tfIf(tfNot(tfIs(luau.ID(named.Name), "never")), tfStmts(tfError(tr.name+": a mapped key must be a string literal")), nil)))
			}
			if filter != nil {
				rewrite = tfStmts(tfIf(tfMethod(luau.ID(key), "issubtypeof", tr.env[filter]()), rewrite, nil))
			}
			tr.block.PushList(rewrite)
			return true
		})
	})
	if !ok {
		return false
	}

	var body []luau.Statement
	if rebuild {
		tr.emit(tfIf(tfNot(tfIs(source(), "table")), tfStmts(tfError(tr.name+": expected a table type")), nil))
		body = append(body, tfLocal(result, tfTypesCall("newtable")))
	} else {
		body = append(body, tfLocal(result, tfTypesCall("copy", source())))
	}
	body = append(body, tfForIn([]string{key, prop}, tfMethod(source(), "properties"), loop), sink(luau.ID(result)))
	switch {
	case rebuild:
		for _, statement := range body {
			tr.emit(statement)
		}
	case tail:
		tr.emit(tfIf(tfNot(tfIs(source(), "table")), tfStmts(tfReturn(source())), nil))
		for _, statement := range body {
			tr.emit(statement)
		}
	default:
		tr.emit(tfIf(tfNot(tfIs(source(), "table")), tfStmts(sink(source())), tfStmts(body...)))
	}
	return true
}

// propertyRewrite emits the per-property statements of a homomorphic
// mapped type for the property record `prop` under key target: the value
// type is spelled once per slot (read, write, or the one current type when
// readonly is being set or cleared) with `Source[K]` bound to that slot.
func (tr *typeFunctionTranslator) propertyRewrite(valueNode *ast.Node, keySymbol *ast.Symbol, result, prop string, target func() luau.Expression, readonly, optional mappedModifier, copied bool) bool {
	slot := func(current func() luau.Expression, consume func(value luau.Expression) []luau.Statement) (*luau.List[luau.Statement], bool) {
		return tr.nested(func() bool {
			binding := tr.indexed[keySymbol]
			binding.value = current
			tr.indexed[keySymbol] = binding
			defer func() {
				binding.value = nil
				tr.indexed[keySymbol] = binding
			}()
			value, ok := tr.typeExpr(valueNode)
			if !ok {
				return false
			}
			for _, statement := range consume(tr.wrapOptional(value, optional)) {
				tr.emit(statement)
			}
			return true
		})
	}
	setter := func(name string, value luau.Expression) luau.Statement {
		return tfCallStatement(tfMethod(luau.ID(result), name, target(), value))
	}
	if readonly == modifierNone {
		read, ok := slot(func() luau.Expression { return tfProp(luau.ID(prop), "read") }, func(value luau.Expression) []luau.Statement {
			return []luau.Statement{setter("setreadproperty", value)}
		})
		if !ok {
			return false
		}
		write, ok := slot(func() luau.Expression { return tfProp(luau.ID(prop), "write") }, func(value luau.Expression) []luau.Statement {
			return []luau.Statement{setter("setwriteproperty", value)}
		})
		if !ok {
			return false
		}
		tr.emit(tfIf(tfProp(luau.ID(prop), "read"), read, nil))
		tr.emit(tfIf(tfProp(luau.ID(prop), "write"), write, nil))
		return true
	}
	current := tr.fresh("current")
	tr.emit(tfLocal(current, tfOr(tfProp(luau.ID(prop), "read"), tfProp(luau.ID(prop), "write"))))
	block, ok := slot(func() luau.Expression { return luau.ID(current) }, func(value luau.Expression) []luau.Statement {
		if readonly == modifierRemove {
			return []luau.Statement{setter("setproperty", value)}
		}
		statements := []luau.Statement{setter("setreadproperty", value)}
		if copied {
			statements = append(statements, setter("setwriteproperty", luau.Nil()))
		}
		return statements
	})
	if !ok {
		return false
	}
	tr.emit(tfIf(luau.ID(current), block, nil))
	return true
}

// wrapOptional applies a `?` modifier to a property type.
func (tr *typeFunctionTranslator) wrapOptional(value luau.Expression, optional mappedModifier) luau.Expression {
	switch optional {
	case modifierAdd:
		return tfTypesCall("optional", value)
	case modifierRemove:
		return tfCall(tr.helper(helperWithoutNil), value)
	}
	return value
}

// mappedOverKeys emits the non-homomorphic shape: a new table with one
// property per singleton in the key union (an indexer for `string` or
// `number`, nothing for `never`).
func (tr *typeFunctionTranslator) mappedOverKeys(mapped *ast.MappedTypeNode, constraint *ast.Node, keySymbol *ast.Symbol, readonly, optional mappedModifier, sink valueSink) bool {
	keys, ok := tr.typeExpr(constraint)
	if !ok {
		return false
	}
	result := tr.fresh("result")
	key := tr.fresh("key")
	loop, ok := tr.nested(func() bool {
		return tr.bind(keySymbol, func() luau.Expression { return luau.ID(key) }, func() bool {
			tr.indexed[keySymbol] = indexedBinding{}
			defer delete(tr.indexed, keySymbol)
			target := key
			if mapped.NameType != nil {
				nameExpression, ok := tr.typeExpr(mapped.NameType)
				if !ok {
					if tr.failure != nil {
						tr.failure.Kind = lossKeyRemap
					}
					return false
				}
				target = tr.fresh("name")
				tr.emit(tfLocal(target, nameExpression))
			}
			valueExpression, ok := tr.typeExpr(mapped.Type)
			if !ok {
				return false
			}
			value := tr.fresh("value")
			tr.emit(tfLocal(value, tr.wrapOptional(valueExpression, optional)))
			property, indexer := "setproperty", "setindexer"
			if readonly == modifierAdd {
				property, indexer = "setreadproperty", "setreadindexer"
			}
			tr.emit(tfIf(tfIs(luau.ID(target), "singleton"), tfStmts(
				tfCallStatement(tfMethod(luau.ID(result), property, luau.ID(target), luau.ID(value))),
			), tfIf(tfOr(tfIs(luau.ID(target), "string"), tfIs(luau.ID(target), "number")), tfStmts(
				tfCallStatement(tfMethod(luau.ID(result), indexer, luau.ID(target), luau.ID(value))),
			), tfIf(tfNot(tfIs(luau.ID(target), "never")), tfStmts(
				tfError(tr.name+": a mapped key must be a string literal, string or number"),
			), nil))))
			return true
		})
	})
	if !ok {
		return false
	}
	tr.emit(tfLocal(result, tfTypesCall("newtable")))
	tr.emit(tfForIn([]string{"_", key}, tfCall(tr.helper(helperDistribute), keys), loop))
	tr.emit(sink(luau.ID(result)))
	return true
}

// ---------------------------------------------------------------------------
// AST helpers
// ---------------------------------------------------------------------------

func skipParenthesizedTypes(node *ast.Node) *ast.Node {
	for node != nil && node.Kind == ast.KindParenthesizedType {
		node = node.AsParenthesizedTypeNode().Type
	}
	return node
}

// isNilTypeNode reports `undefined`, `void` and `null`.
func isNilTypeNode(node *ast.Node) bool {
	node = skipParenthesizedTypes(node)
	switch node.Kind {
	case ast.KindUndefinedKeyword, ast.KindVoidKeyword, ast.KindNullKeyword:
		return true
	case ast.KindLiteralType:
		return node.AsLiteralTypeNode().Literal.Kind == ast.KindNullKeyword
	}
	return false
}

// entityNameText spells `A.B.C`.
func entityNameText(name *ast.Node) string {
	if name.Kind == ast.KindQualifiedName {
		qualified := name.AsQualifiedName()
		return entityNameText(qualified.Left) + "." + qualified.Right.Text()
	}
	return name.Text()
}

// typeLevelNodeKinds are the type nodes that make a generic alias body a
// type-level computation Luau can only express as a type function.
func isTypeLevelNodeKind(kind ast.Kind) bool {
	return kind == ast.KindMappedType || kind == ast.KindConditionalType || kind == ast.KindTemplateLiteralType
}

// typeLevelLossKind is the loss kind a type-level node falls back with.
func typeLevelLossKind(kind ast.Kind) fidelityLossKind {
	switch kind {
	case ast.KindMappedType:
		return lossMappedType
	case ast.KindConditionalType:
		return lossConditionalType
	case ast.KindTemplateLiteralType:
		return lossTemplateLiteral
	}
	return lossUnspellable
}

// firstTypeLevelNode is the outermost mapped, conditional or template
// literal node under body (body itself included), or nil.
func firstTypeLevelNode(body *ast.Node) *ast.Node {
	if body == nil {
		return nil
	}
	if body = skipParenthesizedTypes(body); isTypeLevelNodeKind(body.Kind) {
		return body
	}
	var found *ast.Node
	var visit func(child *ast.Node) bool
	visit = func(child *ast.Node) bool {
		if isTypeLevelNodeKind(child.Kind) {
			found = child
			return true
		}
		return child.ForEachChild(visit)
	}
	body.ForEachChild(visit)
	return found
}

// topLevelLossKind is the loss kind for a failure with no more specific
// kind: that of the body's outermost type-level node.
func topLevelLossKind(body *ast.Node) fidelityLossKind {
	if node := firstTypeLevelNode(body); node != nil {
		return typeLevelLossKind(node.Kind)
	}
	return lossUnspellable
}

// ---------------------------------------------------------------------------
// Registration: type function instead of alias
// ---------------------------------------------------------------------------

// isTypeLevelAlias reports a generic type alias whose body contains a
// mapped, conditional or template literal type. A body that is only
// `keyof T` or `T[K]` is not one: it stays an alias spelled with Luau's
// built-in `keyof<T>` / `index<T, K>`.
func isTypeLevelAlias(symbol *ast.Symbol, declaration *ast.Node) bool {
	if symbol == nil || symbol.Flags&ast.SymbolFlagsTypeAlias == 0 || declaration == nil || !ast.IsTypeAliasDeclaration(declaration) {
		return false
	}
	if len(declaration.TypeParameters()) == 0 {
		return false
	}
	return firstTypeLevelNode(declaration.AsTypeAliasDeclaration().Type) != nil
}

// preludeNameFor is the TS helper name when symbol is one of the lib's
// built-in helper types with a prelude twin (`Partial`, `Pick`, ...), else
// "".
func preludeNameFor(state *State, symbol *ast.Symbol) string {
	if symbol == nil || symbol.Flags&ast.SymbolFlagsTypeAlias == 0 {
		return ""
	}
	if _, ok := luauTypesPreludeNames[symbol.Name]; !ok {
		return ""
	}
	global := state.Checker.GetGlobalSymbol(symbol.Name, ast.SymbolFlagsType, nil)
	if global == nil || state.Checker.GetMergedSymbol(global) != state.Checker.GetMergedSymbol(symbol) {
		return ""
	}
	return symbol.Name
}

// isTypeLevelSymbol reports a symbol whose alias is a type function (or
// its `any` fallback): a prelude helper or a type-level alias.
func (b *typeBuilder) isTypeLevelSymbol(symbol *ast.Symbol, declaration *ast.Node) bool {
	return preludeNameFor(b.state, symbol) != "" || isTypeLevelAlias(symbol, declaration)
}

// typeLevelAlias registers the entry of a type-level symbol: the prelude
// twin of a built-in helper, the translated type function, or the
// `type Name<...> = any` fallback with the translation failure as its loss.
// (nil, false) for every other symbol.
func (b *typeBuilder) typeLevelAlias(symbol *ast.Symbol, declaration *ast.Node) (*typeAliasEntry, bool) {
	if prelude := preludeNameFor(b.state, symbol); prelude != "" {
		b.state.requirePrelude(prelude)
		entry := b.preludes[prelude]
		b.aliases[symbol] = entry
		return entry, true
	}
	if !isTypeLevelAlias(symbol, declaration) {
		return nil, false
	}
	name := b.uniqueName(b.spelling.AliasName(b.state, symbol))
	entry := &typeAliasEntry{
		Symbol:    symbol,
		Name:      name,
		Generics:  b.Generics(declaration),
		Kind:      nativeKindUnspellable,
		Exported:  b.state.isLuauExportedType(symbol),
		Placement: b.state.aliasPlacementFor(declaration),
	}
	for index, parameter := range declaredTypeParameters(declaration) {
		var parameterType *checker.Type
		if parameterSymbol := parameter.Symbol(); parameterSymbol != nil {
			parameterType = b.state.Checker.GetDeclaredTypeOfSymbol(parameterSymbol)
		}
		entry.Params = append(entry.Params, parameterType)
		if parameterName := parameter.Name(); parameterName != nil && luau.IsValidIdentifier(parameterName.Text()) {
			entry.GenericIndexes = append(entry.GenericIndexes, index)
		}
	}
	if declared := b.state.Checker.GetDeclaredTypeOfSymbol(symbol); declared != nil && declared.Flags()&checker.TypeFlagsObject != 0 && declared.ObjectFlags()&checker.ObjectFlagsMapped != 0 {
		entry.Kind = nativeKindTable
	}
	b.aliases[symbol] = entry
	b.aliasOrder = append(b.aliasOrder, entry)

	lossesBefore := len(b.losses)
	if len(entry.GenericIndexes) == len(entry.Params) {
		if declaration, ok := b.state.luauTypeFunctionFor(symbol, declaration); ok {
			declaration.Name = name
			entry.TypeFunction = declaration
		}
	} else {
		b.record(topLevelLossKind(declaration.AsTypeAliasDeclaration().Type), b.typeText(b.state.Checker.GetDeclaredTypeOfSymbol(symbol)), anyType(),
			typeContext{Owner: declaration}, "a type parameter Luau cannot name has no type-function parameter")
	}
	if entry.TypeFunction == nil {
		entry.Statement = luau.NewTypeAlias(name, entry.Generics, anyType())
	}
	for _, loss := range b.losses[lossesBefore:] {
		if loss.Node == declaration {
			entry.Losses = append(entry.Losses, loss)
		}
	}
	return entry, true
}

// typeLevelReference spells a use of a type-level alias. A use whose
// arguments hold no free type variable is exact when expanded from the
// checker's resolved type, so it keeps that spelling unless the alias is
// declared in this file and its translation was complete; a prelude helper
// or an alias of another file is then not even registered. A generic use
// is always `Name<Args>`.
func (b *typeBuilder) typeLevelReference(t *checker.Type, symbol *ast.Symbol, declaration *ast.Node, ctx typeContext) (builtType, bool) {
	var arguments []*checker.Type
	if alias := t.Alias(); alias != nil && alias.Symbol() == symbol {
		arguments = alias.TypeArguments()
	}
	closed := !slices.ContainsFunc(arguments, b.typeHasFreeVariables)
	if closed && (declaration == nil || ast.GetSourceFileOfNode(declaration) != b.state.SourceFile) {
		return builtType{}, false
	}
	entry := b.aliasFor(symbol)
	if entry == nil || closed && entry.TypeFunction == nil {
		return builtType{}, false
	}
	reference := luau.NewTypeReference(entry.Name, b.BuildTypeArgs(declaredArguments(entry, arguments), ctx.nested()))
	b.Origins[reference] = symbol
	return b.finish(reference, false), true
}

// typeLevelNodeReference is the call-site spelling of a written
// `Name<Args>` whose resolved type no longer names the alias (a conditional
// or template literal resolved to a plain type): `Name<Args>` when the alias
// is a complete translation declared in this file and the arguments are
// closed. false lets the resolved type be spelled.
func (b *typeBuilder) typeLevelNodeReference(node *ast.Node, ctx typeContext) (builtType, bool) {
	reference := node.AsTypeReferenceNode()
	symbol := b.state.Checker.GetSymbolAtLocation(reference.TypeName)
	if symbol != nil && symbol.Flags&ast.SymbolFlagsAlias != 0 {
		symbol = b.state.Checker.GetAliasedSymbol(symbol)
	}
	if symbol == nil {
		return builtType{}, false
	}
	declaration := aliasDeclarationOf(symbol)
	if !isTypeLevelAlias(symbol, declaration) || ast.GetSourceFileOfNode(declaration) != b.state.SourceFile {
		return builtType{}, false
	}
	t := b.state.Checker.GetTypeFromTypeNode(node)
	if t == nil || t.Alias() != nil && t.Alias().Symbol() == symbol {
		return builtType{}, false
	}
	argumentNodes := node.TypeArguments()
	for _, argument := range argumentNodes {
		if b.typeHasFreeVariables(b.state.Checker.GetTypeFromTypeNode(argument)) {
			return builtType{}, false
		}
	}
	entry := b.aliasFor(symbol)
	if entry == nil || entry.TypeFunction == nil || len(argumentNodes) > len(entry.Params) {
		return builtType{}, false
	}
	nested := ctx.nested()
	arguments := luau.NewList[luau.TypeNode]()
	for _, argument := range argumentNodes {
		arguments.Push(b.BuildTypeNode(argument, nested).Node)
	}
	for _, parameter := range entry.Params[len(argumentNodes):] {
		if parameter == nil {
			return builtType{}, false
		}
		defaultType := b.state.Checker.GetDefaultFromTypeParameter(parameter)
		if defaultType == nil {
			return builtType{}, false
		}
		arguments.Push(b.Build(defaultType, nested).Node)
	}
	result := luau.NewTypeReference(entry.Name, arguments)
	b.Origins[result] = symbol
	return b.finish(result, false), true
}

// typeHasFreeVariables reports a type that mentions an uninstantiated type
// parameter (or anything else the checker still has to instantiate), so a
// use of a type-level alias with it cannot be expanded eagerly.
func (b *typeBuilder) typeHasFreeVariables(t *checker.Type) bool {
	return b.hasFreeVariables(t, 0)
}

func (b *typeBuilder) hasFreeVariables(t *checker.Type, depth int) bool {
	if t == nil || depth > 16 {
		return false
	}
	flags := t.Flags()
	if flags&checker.TypeFlagsInstantiable != 0 {
		return true
	}
	anyFree := func(types []*checker.Type) bool {
		return slices.ContainsFunc(types, func(member *checker.Type) bool { return b.hasFreeVariables(member, depth+1) })
	}
	if flags&checker.TypeFlagsUnionOrIntersection != 0 {
		return anyFree(t.Types())
	}
	if flags&checker.TypeFlagsObject == 0 {
		return false
	}
	if alias := t.Alias(); alias != nil {
		return anyFree(alias.TypeArguments())
	}
	objectFlags := t.ObjectFlags()
	switch {
	case objectFlags&checker.ObjectFlagsReference != 0:
		return anyFree(getTypeArguments(b.state, t))
	case objectFlags&checker.ObjectFlagsMapped != 0:
		return true
	case objectFlags&checker.ObjectFlagsCouldContainTypeVariablesComputed != 0 && objectFlags&checker.ObjectFlagsCouldContainTypeVariables == 0:
		return false
	}
	// An anonymous object type (a type literal, a function type) is only
	// marked as possibly containing type variables, so its members decide.
	c := b.state.Checker
	for _, property := range c.GetPropertiesOfType(t) {
		if b.hasFreeVariables(c.GetTypeOfSymbol(property), depth+1) {
			return true
		}
	}
	for _, info := range c.GetIndexInfosOfType(t) {
		if b.hasFreeVariables(info.KeyType(), depth+1) || b.hasFreeVariables(info.ValueType(), depth+1) {
			return true
		}
	}
	signatures := append(c.GetCallSignatures(t), c.GetConstructSignatures(t)...)
	for _, signature := range signatures {
		for _, parameter := range signature.Parameters() {
			if b.hasFreeVariables(c.GetTypeOfSymbol(parameter), depth+1) {
				return true
			}
		}
		if b.hasFreeVariables(c.GetReturnTypeOfSignature(signature), depth+1) {
			return true
		}
	}
	return false
}
