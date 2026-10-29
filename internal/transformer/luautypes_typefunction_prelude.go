package transformer

import (
	"rotor/internal/luau"
	"rotor/tsgo/ast"
)

// The Luau side of type-function emission: a small constructor DSL over the
// Luau AST for the ordinary Lua that a `type function` body is made of, the
// local helper functions a body may need (every body is self-contained,
// because type-function bodies see no outer names), and the prelude: the
// hand-written type functions that stand in for TypeScript's built-in
// helper types when those are used generically (`Partial<T>` with a free
// `T` becomes `__Partial<T>`). The translator in luautypes_typefunction.go
// builds its bodies from the same DSL and helpers, so a user alias and its
// prelude twin render identically.

// ---------------------------------------------------------------------------
// DSL
// ---------------------------------------------------------------------------

// tfTypeType is the `type` annotation every type-function parameter and
// helper carries: without it an early `return T` over-constrains the
// inferred return type and the body fails to type-check (probe P7e).
func tfTypeType() luau.TypeNode { return luau.TypeName("type") }

func tfArgs(args ...luau.Expression) *luau.List[luau.Expression] {
	return luau.NewList[luau.Expression](args...)
}

func tfStmts(statements ...luau.Statement) *luau.List[luau.Statement] {
	return luau.NewList[luau.Statement](statements...)
}

// tfTypes is `types.name`.
func tfTypes(name string) *luau.PropertyAccessExpression {
	return luau.NewPropertyAccess(luau.ID("types"), name)
}

// tfTypesCall is `types.name(args)`.
func tfTypesCall(name string, args ...luau.Expression) *luau.CallExpression {
	return luau.NewCall(tfTypes(name), tfArgs(args...))
}

// tfCall is `fn(args)` for a local function or a library member.
func tfCall(fn luau.IndexableExpression, args ...luau.Expression) *luau.CallExpression {
	return luau.NewCall(fn, tfArgs(args...))
}

// tfLibCall is `lib.name(args)`.
func tfLibCall(lib, name string, args ...luau.Expression) *luau.CallExpression {
	return luau.NewCall(luau.NewPropertyAccess(luau.ID(lib), name), tfArgs(args...))
}

// tfMethod is `receiver:name(args)`.
func tfMethod(receiver luau.IndexableExpression, name string, args ...luau.Expression) *luau.MethodCallExpression {
	return luau.NewMethodCall(name, receiver, tfArgs(args...))
}

// tfIs is `value:is("tag")`.
func tfIs(value luau.IndexableExpression, tag string) *luau.MethodCallExpression {
	return tfMethod(value, "is", luau.Str(tag))
}

// tfProp is `receiver.name`.
func tfProp(receiver luau.IndexableExpression, name string) *luau.PropertyAccessExpression {
	return luau.NewPropertyAccess(receiver, name)
}

// tfLocal is `local name = value`.
func tfLocal(name string, value luau.Expression) *luau.VariableDeclaration {
	return luau.NewVariableDeclaration(luau.ID(name), value)
}

// tfTypedLocal is `local name: type`, the slot a branching computation
// assigns in every branch.
func tfTypedLocal(name string) *luau.VariableDeclaration {
	id := luau.ID(name)
	luau.SetBindingType(id, tfTypeType())
	return luau.NewVariableDeclaration(id, nil)
}

// tfSet is `target = value`.
func tfSet(target luau.WritableExpression, value luau.Expression) *luau.Assignment {
	return luau.NewAssignment(target, "=", value)
}

func tfReturn(value luau.Expression) *luau.ReturnStatement {
	return luau.NewReturn(value)
}

// tfIf is `if condition then ... [else ...] end`; a nil elseBody is no else,
// an *IfStatement elseBody is an `elseif` chain.
func tfIf(condition luau.Expression, then *luau.List[luau.Statement], elseBody luau.NodeOrList) *luau.IfStatement {
	return luau.NewIf(condition, then, elseBody)
}

// tfForIn is `for names in iterator do ... end`.
func tfForIn(names []string, iterator luau.Expression, body *luau.List[luau.Statement]) *luau.ForStatement {
	ids := luau.NewList[luau.AnyIdentifier]()
	for _, name := range names {
		ids.Push(luau.ID(name))
	}
	return luau.NewFor(ids, iterator, body)
}

func tfNot(expression luau.Expression) *luau.UnaryExpression {
	return luau.NewUnary("not", expression)
}

func tfAnd(left, right luau.Expression) *luau.BinaryExpression {
	return luau.NewBinary(left, "and", right)
}

func tfOr(left, right luau.Expression) *luau.BinaryExpression {
	return luau.NewBinary(left, "or", right)
}

func tfEq(left, right luau.Expression) *luau.BinaryExpression {
	return luau.NewBinary(left, "==", right)
}

func tfConcat(left, right luau.Expression) *luau.BinaryExpression {
	return luau.NewBinary(left, "..", right)
}

func tfLen(expression luau.Expression) *luau.UnaryExpression {
	return luau.NewUnary("#", expression)
}

// tfIndex is `table[index]`.
func tfIndex(table luau.IndexableExpression, index luau.Expression) *luau.ComputedIndexExpression {
	return luau.NewComputedIndex(table, index)
}

// tfError is the statement `error("message")`, how a type function reports
// an input it has no answer for.
func tfError(message string) *luau.CallStatement {
	return luau.NewCallStatement(tfCall(luau.ID("error"), luau.Str(message)))
}

// tfInsert is the statement `table.insert(list, value)`.
func tfInsert(list luau.Expression, value luau.Expression) *luau.CallStatement {
	return luau.NewCallStatement(tfLibCall("table", "insert", list, value))
}

// tfCallStatement is an expression statement.
func tfCallStatement(call luau.Expression) *luau.CallStatement {
	return luau.NewCallStatement(call)
}

// tfField is the `name = value` field of a Lua table.
func tfField(name string, value luau.Expression) *luau.MapField {
	return luau.NewMapField(luau.Str(name), value)
}

// tfMap is a Lua table of named fields; empty renders `{}`.
func tfMap(fields ...*luau.MapField) *luau.Map {
	return luau.NewMap(luau.NewList[*luau.MapField](fields...))
}

// tfArray is a Lua array literal.
func tfArray(members ...luau.Expression) *luau.Array {
	return luau.NewArray(tfArgs(members...))
}

// tfIfExpr is `if condition then a else b`.
func tfIfExpr(condition, a, b luau.Expression) *luau.IfExpression {
	return luau.NewIfExpression(condition, a, b)
}

// tfParameter names one helper parameter with its annotation.
type tfParameter struct {
	name string
	t    luau.TypeNode
}

// tfLocalFunction is `local function name(params): returns ... end`.
func tfLocalFunction(name string, parameters []tfParameter, returns luau.TypeNode, body *luau.List[luau.Statement]) *luau.FunctionDeclaration {
	ids := luau.NewList[luau.AnyIdentifier]()
	for _, parameter := range parameters {
		id := luau.ID(parameter.name)
		luau.SetBindingType(id, parameter.t)
		ids.Push(id)
	}
	declaration := luau.NewFunctionDeclaration(true, luau.ID(name), ids, false, body)
	declaration.ReturnType = returns
	return declaration
}

// tfTypeFunction is `type function name(T: type, ...) ... end`.
func tfTypeFunction(name string, parameters []string, body *luau.List[luau.Statement]) *luau.TypeFunctionDeclaration {
	ids := luau.NewList[luau.AnyIdentifier]()
	for _, parameter := range parameters {
		id := luau.ID(parameter)
		luau.SetBindingType(id, tfTypeType())
		ids.Push(id)
	}
	return luau.NewTypeFunctionDeclaration(name, ids, false, body)
}

// ---------------------------------------------------------------------------
// Helpers: local functions a body may need
// ---------------------------------------------------------------------------

// The helper names. Each is emitted as a `local function` at the top of the
// body that needs it, in typeFunctionHelperOrder, so a helper is always
// declared before the helpers and statements that call it.
const (
	helperDistribute = "distribute"
	helperJoin       = "join"
	helperWithoutNil = "withoutnil"
	helperKeys       = "keys"
	helperIndexOf    = "indexof"
	helperTemplate   = "template"
	helperReturnOf   = "returnof"
)

var typeFunctionHelperOrder = []string{
	helperDistribute, helperJoin, helperWithoutNil, helperKeys, helperIndexOf, helperTemplate, helperReturnOf,
}

// typeFunctionHelperDependencies lists the helpers a helper calls.
var typeFunctionHelperDependencies = map[string][]string{
	helperWithoutNil: {helperDistribute, helperJoin},
	helperKeys:       {helperJoin},
	helperIndexOf:    {helperJoin},
}

// typeFunctionHelperStatements lists the declarations of the requested
// helpers and everything they depend on, in declaration order.
func typeFunctionHelperStatements(requested map[string]struct{}) []luau.Statement {
	needed := make(map[string]struct{}, len(requested))
	var require func(name string)
	require = func(name string) {
		if _, seen := needed[name]; seen {
			return
		}
		needed[name] = struct{}{}
		for _, dependency := range typeFunctionHelperDependencies[name] {
			require(dependency)
		}
	}
	for name := range requested {
		require(name)
	}
	var statements []luau.Statement
	for _, name := range typeFunctionHelperOrder {
		if _, ok := needed[name]; ok {
			statements = append(statements, typeFunctionHelper(name))
		}
	}
	return statements
}

// typeFunctionHelper builds one helper by name.
func typeFunctionHelper(name string) luau.Statement {
	typeParameter := func(parameter string) []tfParameter {
		return []tfParameter{{name: parameter, t: tfTypeType()}}
	}
	typeList := luau.NewTypeArray(tfTypeType())
	switch name {
	case helperDistribute:
		// The members a conditional or a mapped key distributes over: a
		// union's components, nothing for never, the type itself otherwise.
		value := luau.ID("value")
		return tfLocalFunction(name, typeParameter("value"), luau.NewTypeArray(tfTypeType()), tfStmts(
			tfIf(tfIs(value, "union"), tfStmts(tfReturn(tfMethod(value, "components"))), nil),
			tfIf(tfIs(value, "never"), tfStmts(tfReturn(tfMap())), nil),
			tfReturn(tfArray(value)),
		))
	case helperJoin:
		// The union of a list of types; types.unionof needs two or more.
		parts := luau.ID("parts")
		return tfLocalFunction(name, []tfParameter{{name: "parts", t: typeList}}, tfTypeType(), tfStmts(
			tfIf(tfEq(tfLen(parts), luau.Num(0)), tfStmts(tfReturn(tfTypes("never"))), nil),
			tfIf(tfEq(tfLen(parts), luau.Num(1)), tfStmts(tfReturn(tfIndex(parts, luau.Num(1)))), nil),
			tfReturn(tfTypesCall("unionof", tfLibCall("table", "unpack", parts))),
		))
	case helperWithoutNil:
		// `-?` and NonNullable: the type without its nil member.
		value, parts, component := luau.ID("value"), luau.ID("parts"), luau.ID("component")
		return tfLocalFunction(name, typeParameter("value"), tfTypeType(), tfStmts(
			tfLocal("parts", tfMap()),
			tfForIn([]string{"_", "component"}, tfCall(luau.ID(helperDistribute), value), tfStmts(
				tfIf(tfNot(tfIs(component, "nil")), tfStmts(tfInsert(parts, component)), nil),
			)),
			tfReturn(tfCall(luau.ID(helperJoin), parts)),
		))
	case helperKeys:
		// `keyof T`: the union of a table's property keys.
		value, parts, key := luau.ID("value"), luau.ID("parts"), luau.ID("key")
		return tfLocalFunction(name, typeParameter("value"), tfTypeType(), tfStmts(
			tfIf(tfNot(tfIs(value, "table")), tfStmts(tfError("keyof: expected a table type")), nil),
			tfLocal("parts", tfMap()),
			tfForIn([]string{"key"}, tfMethod(value, "properties"), tfStmts(tfInsert(parts, key))),
			tfReturn(tfCall(luau.ID(helperJoin), parts)),
		))
	case helperIndexOf:
		// `T[K]`: the property's read type, distributed over a union key.
		object, key, parts, component, value := luau.ID("object"), luau.ID("key"), luau.ID("parts"), luau.ID("component"), luau.ID("value")
		return tfLocalFunction(name, []tfParameter{{name: "object", t: tfTypeType()}, {name: "key", t: tfTypeType()}}, tfTypeType(), tfStmts(
			tfIf(tfIs(key, "union"), tfStmts(
				tfLocal("parts", tfMap()),
				tfForIn([]string{"_", "component"}, tfMethod(key, "components"), tfStmts(
					tfInsert(parts, tfCall(luau.ID(helperIndexOf), object, component)),
				)),
				tfReturn(tfCall(luau.ID(helperJoin), parts)),
			), nil),
			tfIf(tfNot(tfIs(object, "table")), tfStmts(tfError("indexed access: expected a table type")), nil),
			tfLocal("value", tfMethod(object, "readproperty", key)),
			tfIf(tfNot(value), tfStmts(tfError("indexed access: the property does not exist")), nil),
			tfReturn(value),
		))
	case helperTemplate:
		// A template literal: the singleton of the concatenated parts, or
		// `string` once a hole is not a singleton.
		parts, text, part := luau.ID("parts"), luau.ID("text"), luau.ID("part")
		partType := luau.NewTypeArray(luau.NewTypeUnion(luau.NewList[luau.TypeNode](luau.TypeName(nativeTypeString), tfTypeType())))
		return tfLocalFunction(name, []tfParameter{{name: "parts", t: partType}}, tfTypeType(), tfStmts(
			tfLocal("text", luau.Str("")),
			tfForIn([]string{"_", "part"}, parts, tfStmts(
				tfIf(tfEq(tfCall(luau.ID("type"), part), luau.Str("string")), tfStmts(
					tfSet(text, tfConcat(text, part)),
				), tfIf(tfIs(part, "singleton"), tfStmts(
					tfSet(text, tfConcat(text, tfCall(luau.ID("tostring"), tfMethod(part, "value")))),
				), tfStmts(
					tfReturn(tfTypes("string")),
				))),
			)),
			tfReturn(tfTypesCall("singleton", text)),
		))
	case helperReturnOf:
		// `T extends (...args: any) => infer R`: the first return type,
		// the variadic return, or nil for a function that returns nothing.
		fn, returns := luau.ID("fn"), luau.ID("returns")
		head := func() luau.Expression { return tfProp(returns, "head") }
		return tfLocalFunction(name, typeParameter("fn"), tfTypeType(), tfStmts(
			tfLocal("returns", tfMethod(fn, "returns")),
			tfIf(tfAnd(head(), tfIndex(tfProp(returns, "head"), luau.Num(1))), tfStmts(
				tfReturn(tfIndex(tfProp(returns, "head"), luau.Num(1))),
			), nil),
			tfIf(tfProp(returns, "tail"), tfStmts(tfReturn(tfProp(returns, "tail"))), nil),
			tfReturn(tfTypesCall("singleton", luau.Nil())),
		))
	}
	panic("transformer: unknown type function helper " + name)
}

// ---------------------------------------------------------------------------
// Prelude
// ---------------------------------------------------------------------------

// luauTypesPreludeNames are the TypeScript built-in helper types that have a
// prelude type function, keyed by TS name.
var luauTypesPreludeNames = map[string]struct{}{
	"Partial": {}, "Required": {}, "Readonly": {}, "Pick": {}, "Omit": {}, "Record": {},
	"Exclude": {}, "Extract": {}, "NonNullable": {}, "ReturnType": {}, "Parameters": {},
}

// luauTypesPreludeTableNames are the prelude functions whose result is a
// table when their input is.
var luauTypesPreludeTableNames = map[string]struct{}{
	"Partial": {}, "Required": {}, "Readonly": {}, "Pick": {}, "Omit": {}, "Record": {}, "Parameters": {},
}

// luauTypesPrelude builds the prelude type function for a TS helper type
// (`Partial` -> `type function __Partial(T: type)`), or nil for a name
// without one. Every body mirrors what the translator emits for the same
// shape written by hand, with the helpers it needs declared first.
func luauTypesPrelude(name string) *luau.TypeFunctionDeclaration {
	if _, ok := luauTypesPreludeNames[name]; !ok {
		return nil
	}
	T, K, U := luau.ID("T"), luau.ID("K"), luau.ID("U")
	key, prop, result, current := luau.ID("key"), luau.ID("prop"), luau.ID("result"), luau.ID("current")
	parts, item := luau.ID("parts"), luau.ID("item")
	helpers := map[string]struct{}{}
	var parameters []string
	var body []luau.Statement
	// copyRewrite is the homomorphic shape: a non-table is itself, a table
	// is copied and every property rewritten through rewrite.
	copyRewrite := func(rewrite func(current luau.Expression) luau.Expression) {
		parameters = []string{"T"}
		body = append(body,
			tfIf(tfNot(tfIs(T, "table")), tfStmts(tfReturn(T)), nil),
			tfLocal("result", tfTypesCall("copy", T)),
			tfForIn([]string{"key", "prop"}, tfMethod(T, "properties"), tfStmts(
				tfIf(tfProp(prop, "read"), tfStmts(
					tfCallStatement(tfMethod(result, "setreadproperty", key, rewrite(tfProp(prop, "read")))),
				), nil),
				tfIf(tfProp(prop, "write"), tfStmts(
					tfCallStatement(tfMethod(result, "setwriteproperty", key, rewrite(tfProp(prop, "write")))),
				), nil),
			)),
			tfReturn(result),
		)
	}
	// pickRewrite is `{ [P in K]: T[P] }` with K bound by keyof T: the
	// properties whose key passes keep their read and write types.
	pickRewrite := func(keep func() luau.Expression) {
		parameters = []string{"T", "K"}
		body = append(body,
			tfIf(tfNot(tfIs(T, "table")), tfStmts(tfError(name+"<T, K>: expected a table type")), nil),
			tfLocal("result", tfTypesCall("newtable")),
			tfForIn([]string{"key", "prop"}, tfMethod(T, "properties"), tfStmts(
				tfIf(keep(), tfStmts(
					tfIf(tfProp(prop, "read"), tfStmts(
						tfCallStatement(tfMethod(result, "setreadproperty", key, tfProp(prop, "read"))),
					), nil),
					tfIf(tfProp(prop, "write"), tfStmts(
						tfCallStatement(tfMethod(result, "setwriteproperty", key, tfProp(prop, "write"))),
					), nil),
				), nil),
			)),
			tfReturn(result),
		)
	}
	// filterRewrite is the distributive conditional `T extends U ? T : never`
	// (Extract) or `T extends U ? never : T` (Exclude).
	filterRewrite := func(keep func() luau.Expression) {
		parameters = []string{"T", "U"}
		helpers[helperDistribute] = struct{}{}
		helpers[helperJoin] = struct{}{}
		body = append(body,
			tfLocal("parts", tfMap()),
			tfForIn([]string{"_", "item"}, tfCall(luau.ID(helperDistribute), T), tfStmts(
				tfIf(keep(), tfStmts(tfInsert(parts, item)), nil),
			)),
			tfReturn(tfCall(luau.ID(helperJoin), parts)),
		)
	}
	switch name {
	case "Partial":
		copyRewrite(func(current luau.Expression) luau.Expression { return tfTypesCall("optional", current) })
	case "Required":
		helpers[helperWithoutNil] = struct{}{}
		copyRewrite(func(current luau.Expression) luau.Expression { return tfCall(luau.ID(helperWithoutNil), current) })
	case "Readonly":
		parameters = []string{"T"}
		body = append(body,
			tfIf(tfNot(tfIs(T, "table")), tfStmts(tfReturn(T)), nil),
			tfLocal("result", tfTypesCall("copy", T)),
			tfForIn([]string{"key", "prop"}, tfMethod(T, "properties"), tfStmts(
				tfLocal("current", tfOr(tfProp(prop, "read"), tfProp(prop, "write"))),
				tfIf(current, tfStmts(
					tfCallStatement(tfMethod(result, "setreadproperty", key, current)),
					tfCallStatement(tfMethod(result, "setwriteproperty", key, luau.Nil())),
				), nil),
			)),
			tfReturn(result),
		)
	case "Pick":
		pickRewrite(func() luau.Expression { return tfMethod(key, "issubtypeof", K) })
	case "Omit":
		pickRewrite(func() luau.Expression { return tfNot(tfMethod(key, "issubtypeof", K)) })
	case "Record":
		parameters = []string{"K", "T"}
		helpers[helperDistribute] = struct{}{}
		body = append(body,
			tfLocal("result", tfTypesCall("newtable")),
			tfForIn([]string{"_", "key"}, tfCall(luau.ID(helperDistribute), K), tfStmts(
				tfIf(tfIs(key, "singleton"), tfStmts(
					tfCallStatement(tfMethod(result, "setproperty", key, T)),
				), tfIf(tfOr(tfIs(key, "string"), tfIs(key, "number")), tfStmts(
					tfCallStatement(tfMethod(result, "setindexer", key, T)),
				), tfIf(tfNot(tfIs(key, "never")), tfStmts(
					tfError("Record<K, T>: a mapped key must be a string literal, string or number"),
				), nil))),
			)),
			tfReturn(result),
		)
	case "Exclude":
		filterRewrite(func() luau.Expression { return tfNot(tfMethod(item, "issubtypeof", U)) })
	case "Extract":
		filterRewrite(func() luau.Expression { return tfMethod(item, "issubtypeof", U) })
	case "NonNullable":
		parameters = []string{"T"}
		helpers[helperWithoutNil] = struct{}{}
		body = append(body, tfReturn(tfCall(luau.ID(helperWithoutNil), T)))
	case "ReturnType":
		parameters = []string{"T"}
		helpers[helperReturnOf] = struct{}{}
		body = append(body,
			tfIf(tfIs(T, "any"), tfStmts(tfReturn(tfTypes("any"))), nil),
			tfIf(tfNot(tfIs(T, "function")), tfStmts(tfError("ReturnType<T>: expected a function type")), nil),
			tfReturn(tfCall(luau.ID(helperReturnOf), T)),
		)
	case "Parameters":
		// A tuple has no Luau spelling, so the parameters become an array of
		// their union, as the builder spells every tuple.
		parameters = []string{"T"}
		helpers[helperJoin] = struct{}{}
		signature, parameter, element := luau.ID("parameters"), luau.ID("parameter"), luau.ID("element")
		body = append(body,
			tfIf(tfNot(tfIs(T, "function")), tfStmts(tfError("Parameters<T>: expected a function type")), nil),
			tfLocal("parameters", tfMethod(T, "parameters")),
			tfLocal("parts", tfMap()),
			tfForIn([]string{"_", "parameter"}, tfOr(tfProp(signature, "head"), tfMap()), tfStmts(tfInsert(parts, parameter))),
			tfIf(tfProp(signature, "tail"), tfStmts(tfInsert(parts, tfProp(signature, "tail"))), nil),
			tfLocal("element", tfCall(luau.ID(helperJoin), parts)),
			tfReturn(tfTypesCall("newtable", luau.Nil(), tfMap(
				tfField("index", tfTypes("number")),
				tfField("readresult", element),
				tfField("writeresult", element),
			))),
		)
	}
	statements := tfStmts(typeFunctionHelperStatements(helpers)...)
	statements.PushList(tfStmts(body...))
	return tfTypeFunction("__"+name, parameters, statements)
}

// requirePrelude registers the prelude type function for a TS helper type
// once per file (hoisted after the header, never exported) and returns the
// Luau name a generic use refers to (`__Partial`, or `__Partial_2` when the
// name is taken), or "" for a name without a prelude.
func (state *State) requirePrelude(name string) string {
	b := state.types()
	if entry := b.preludes[name]; entry != nil {
		return entry.Name
	}
	declaration := luauTypesPrelude(name)
	if declaration == nil {
		return ""
	}
	declaration.Name = b.uniqueName(declaration.Name)
	entry := &typeAliasEntry{
		Symbol:       &ast.Symbol{Name: declaration.Name},
		TypeFunction: declaration,
		Name:         declaration.Name,
		Kind:         nativeKindUnspellable,
		Placement:    hoisted,
	}
	if _, table := luauTypesPreludeTableNames[name]; table {
		entry.Kind = nativeKindTable
	}
	declaration.Parameters.ForEach(func(parameter luau.AnyIdentifier) {
		if id, ok := parameter.(*luau.Identifier); ok {
			entry.Generics = append(entry.Generics, luau.TypeGeneric{Name: id.Name})
		}
	})
	b.preludes[name] = entry
	b.aliases[entry.Symbol] = entry
	b.aliasOrder = append(b.aliasOrder, entry)
	return entry.Name
}
