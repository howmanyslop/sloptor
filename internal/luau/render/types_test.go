package render

import (
	"reflect"
	"strings"
	"testing"

	"rotor/internal/luau"
)

func typeNodes(nodes ...luau.TypeNode) *luau.List[luau.TypeNode] {
	return luau.NewList[luau.TypeNode](nodes...)
}

func tableFields(fields ...luau.TypeTableField) *luau.List[luau.TypeTableField] {
	return luau.NewList[luau.TypeTableField](fields...)
}

func tn(name string) *luau.TypeReference { return luau.TypeName(name) }

func union(members ...luau.TypeNode) *luau.TypeUnion {
	return luau.NewTypeUnion(typeNodes(members...))
}

func intersection(members ...luau.TypeNode) *luau.TypeIntersection {
	return luau.NewTypeIntersection(typeNodes(members...))
}

func pack(elements ...luau.TypeNode) *luau.TypePack {
	return luau.NewTypePack(typeNodes(elements...))
}

func param(name string, t luau.TypeNode) luau.TypeFunctionParameter {
	return luau.TypeFunctionParameter{Name: name, Type: t}
}

func fnType(parameters []luau.TypeFunctionParameter, varArgs, returnType luau.TypeNode) *luau.TypeFunction {
	return luau.NewTypeFunction(nil, parameters, varArgs, returnType)
}

func prop(name string, value luau.TypeNode) *luau.TypeTableProp {
	return luau.NewTypeTableProp(luau.TypeAccessNone, name, value)
}

func table(fields ...luau.TypeTableField) *luau.TypeTable {
	return luau.NewTypeTable(tableFields(fields...))
}

// renderTypeText renders node as an alias body through both public APIs and
// returns the body text alone.
func renderTypeText(t *testing.T, node luau.TypeNode) string {
	t.Helper()
	got := renderAST(t, stmts(luau.NewTypeAlias("T", nil, node)))
	const prefix, suffix = "type T = ", "\n"
	if !strings.HasPrefix(got, prefix) || !strings.HasSuffix(got, suffix) {
		t.Fatalf("alias rendered as %q", got)
	}
	return strings.TrimSuffix(strings.TrimPrefix(got, prefix), suffix)
}

type typeCase struct {
	name string
	node luau.TypeNode
	want string
}

func runTypeCases(t *testing.T, cases []typeCase) {
	t.Helper()
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if got := renderTypeText(t, c.node); got != c.want {
				t.Errorf("got %q, want %q", got, c.want)
			}
		})
	}
}

func TestRenderTypeAtoms(t *testing.T) {
	runTypeCases(t, []typeCase{
		{"name", tn("number"), "number"},
		{"nil", tn("nil"), "nil"},
		{"qualified", luau.NewTypeQualifiedReference("Enum", "Material", nil), "Enum.Material"},
		{"arguments", luau.NewTypeReference("Map", typeNodes(tn("string"), tn("number"))), "Map<string, number>"},
		{"qualified arguments", luau.NewTypeQualifiedReference("M", "T", typeNodes(tn("A"))), "M.T<A>"},
		{"empty arguments", luau.NewTypeReference("Foo", typeNodes()), "Foo"},
		{"pack argument", luau.NewTypeReference("Foo", typeNodes(pack(tn("A"), tn("B")))), "Foo<(A, B)>"},
		{"variadic argument", luau.NewTypeReference("Foo", typeNodes(luau.NewTypeVariadic(tn("number")))), "Foo<...number>"},
		{"generic pack argument", luau.NewTypeReference("Foo", typeNodes(luau.NewTypeGenericPack("U"))), "Foo<U...>"},
		{"trailing generic pack", luau.NewTypeReference("List", typeNodes(tn("number"), luau.NewTypeGenericPack("Rest"))), "List<number, Rest...>"},
		{"union argument", luau.NewTypeReference("Foo", typeNodes(union(tn("A"), tn("B")))), "Foo<A | B>"},
		{"function argument", luau.NewTypeReference("Foo", typeNodes(fnType([]luau.TypeFunctionParameter{param("", tn("A"))}, nil, tn("B")))), "Foo<(A) -> B>"},
		{"raw", luau.NewTypeRaw("{ [any]: any }"), "{ [any]: any }"},
		{"raw in optional is never parenthesized", luau.NewTypeOptional(luau.NewTypeRaw("A | B")), "A | B?"},
	})
}

func TestRenderTypeSingletons(t *testing.T) {
	runTypeCases(t, []typeCase{
		{"string", luau.NewTypeStringSingleton("up"), `"up"`},
		{"escapes", luau.NewTypeStringSingleton("a\"b\\c\nd\re\tf"), `"a\"b\\c\nd\re\tf"`},
		{"control", luau.NewTypeStringSingleton("\x01\x7f"), `"\x01\x7F"`},
		{"unicode", luau.NewTypeStringSingleton("😀"), `"😀"`},
		{"true", luau.NewTypeBooleanSingleton(true), "true"},
		{"false", luau.NewTypeBooleanSingleton(false), "false"},
		{"singleton union", union(luau.NewTypeStringSingleton("r"), luau.NewTypeStringSingleton("w")), `"r" | "w"`},
	})
}

func TestRenderTypeOptional(t *testing.T) {
	unit := fnType([]luau.TypeFunctionParameter{param("a", tn("number"))}, nil, pack())
	runTypeCases(t, []typeCase{
		{"name", luau.NewTypeOptional(tn("number")), "number?"},
		{"union", luau.NewTypeOptional(union(tn("A"), tn("B"))), "(A | B)?"},
		{"intersection", luau.NewTypeOptional(intersection(tn("A"), tn("B"))), "(A & B)?"},
		{"function", luau.NewTypeOptional(unit), "((a: number) -> ())?"},
		{"optional", luau.NewTypeOptional(luau.NewTypeOptional(tn("A"))), "(A?)?"},
		{"array", luau.NewTypeOptional(luau.NewTypeArray(tn("number"))), "{ number }?"},
		{"table", luau.NewTypeOptional(table(prop("a", tn("number")))), "{ a: number }?"},
		{"typeof", luau.NewTypeOptional(luau.NewTypeTypeof(luau.ID("x"))), "typeof(x)?"},
		{"singleton", luau.NewTypeOptional(luau.NewTypeStringSingleton("a")), `"a"?`},
		{"normalized optional", luau.OptionalType(luau.NewTypeOptional(tn("A"))), "A?"},
		{"normalized nil", luau.OptionalType(tn("nil")), "nil"},
		{"normalized any", luau.OptionalType(tn("any")), "any"},
		{"normalized unknown", luau.OptionalType(tn("unknown")), "unknown"},
		{"normalized qualified", luau.OptionalType(luau.NewTypeQualifiedReference("M", "any", nil)), "M.any?"},
		{"normalized name", luau.OptionalType(tn("number")), "number?"},
		{"normalized union", luau.OptionalType(union(tn("A"), tn("B"))), "(A | B)?"},
	})
}

func TestRenderTypeUnionIntersection(t *testing.T) {
	predicate := fnType([]luau.TypeFunctionParameter{param("a", tn("number"))}, nil, tn("boolean"))
	method := fnType([]luau.TypeFunctionParameter{param("self", tn("T"))}, nil, pack())
	runTypeCases(t, []typeCase{
		{"union", union(tn("A"), tn("B"), tn("C")), "A | B | C"},
		{"union of optional", union(luau.NewTypeOptional(tn("A")), tn("B")), "A? | B"},
		{"union of function", union(predicate, tn("nil")), "((a: number) -> boolean) | nil"},
		{"union of intersection", union(intersection(tn("A"), tn("B")), tn("C")), "(A & B) | C"},
		{"union of union", union(union(tn("A"), tn("B")), tn("C")), "(A | B) | C"},
		{"union of tables", union(table(prop("ok", luau.NewTypeBooleanSingleton(true))), table(prop("ok", luau.NewTypeBooleanSingleton(false)))), "{ ok: true } | { ok: false }"},
		{"intersection", intersection(tn("A"), tn("B")), "A & B"},
		{"intersection of optional", intersection(luau.NewTypeOptional(tn("A")), tn("B")), "(A?) & B"},
		{"intersection of union", intersection(union(tn("A"), tn("B")), tn("C")), "(A | B) & C"},
		{"intersection of intersection", intersection(intersection(tn("A"), tn("B")), tn("C")), "(A & B) & C"},
		{"intersection of function", intersection(method, table(prop("x", tn("number")))), "((self: T) -> ()) & { x: number }"},
		{"intersection of tables", intersection(table(prop("x", tn("number"))), table(prop("y", tn("number")))), "{ x: number } & { y: number }"},
		{"overloads", intersection(predicate, method), "((a: number) -> boolean) & ((self: T) -> ())"},
	})
}

func TestRenderTypeTables(t *testing.T) {
	runTypeCases(t, []typeCase{
		{"empty", table(), "{}"},
		{"props", table(prop("a", tn("number")), prop("b", luau.NewTypeOptional(tn("string")))), "{ a: number, b: string? }"},
		{"quoted key", table(prop("kebab-case", tn("number"))), `{ ["kebab-case"]: number }`},
		{"keyword key", table(prop("end", tn("boolean"))), `{ ["end"]: boolean }`},
		{"escaped key", table(prop(`say "hi"`, tn("number"))), `{ ["say \"hi\""]: number }`},
		{"read write", table(luau.NewTypeTableProp(luau.TypeAccessRead, "x", tn("number")), luau.NewTypeTableProp(luau.TypeAccessWrite, "y", tn("string"))), "{ read x: number, write y: string }"},
		{"read prop named read", table(luau.NewTypeTableProp(luau.TypeAccessRead, "read", tn("number"))), "{ read read: number }"},
		{"indexer", table(luau.NewTypeTableIndexer(luau.TypeAccessNone, tn("string"), tn("number"))), "{ [string]: number }"},
		{"read indexer", table(luau.NewTypeTableIndexer(luau.TypeAccessRead, tn("string"), tn("number"))), "{ read [string]: number }"},
		{"indexer and prop", table(luau.NewTypeTableIndexer(luau.TypeAccessNone, tn("number"), tn("string")), prop("n", tn("number"))), "{ [number]: string, n: number }"},
		{"union key", table(luau.NewTypeTableIndexer(luau.TypeAccessNone, union(tn("A"), tn("B")), tn("number"))), "{ [A | B]: number }"},
		{"function field", table(prop("f", fnType([]luau.TypeFunctionParameter{param("self", tn("Foo"))}, nil, tn("number")))), "{ f: (self: Foo) -> number }"},
		{"union field", table(prop("u", union(tn("A"), tn("B")))), "{ u: A | B }"},
		{"array", luau.NewTypeArray(tn("number")), "{ number }"},
		{"nested array", luau.NewTypeArray(luau.NewTypeArray(tn("number"))), "{ { number } }"},
		{"read array", &luau.TypeArray{Access: luau.TypeAccessRead, Element: tn("number")}, "{ read number }"},
		{"array of union", luau.NewTypeArray(union(tn("A"), tn("B"))), "{ A | B }"},
		{"array of function", luau.NewTypeArray(fnType(nil, nil, pack())), "{ () -> () }"},
	})
}

func TestRenderTypeFunctions(t *testing.T) {
	params := func(ps ...luau.TypeFunctionParameter) []luau.TypeFunctionParameter { return ps }
	generics := func(gs ...luau.TypeGeneric) []luau.TypeGeneric { return gs }
	runTypeCases(t, []typeCase{
		{"unit", fnType(nil, nil, pack()), "() -> ()"},
		{"unnamed", fnType(params(param("", tn("number")), param("", tn("string"))), nil, tn("boolean")), "(number, string) -> boolean"},
		{"named", fnType(params(param("a", tn("number")), param("b", luau.NewTypeOptional(tn("string")))), nil, tn("boolean")), "(a: number, b: string?) -> boolean"},
		{"mixed names", fnType(params(param("a", tn("number")), param("", tn("string"))), nil, pack()), "(a: number, string) -> ()"},
		{"generic pack", luau.NewTypeFunction(generics(luau.TypeGeneric{Name: "T"}, luau.TypeGeneric{Name: "U", IsPack: true}), params(param("value", tn("T"))), luau.NewTypeGenericPack("U"), luau.NewTypeGenericPack("U")), "<T, U...>(value: T, U...) -> U..."},
		{"variadic", fnType(nil, luau.NewTypeVariadic(tn("number")), pack()), "(...number) -> ()"},
		{"variadic return", fnType(nil, nil, luau.NewTypeVariadic(tn("any"))), "() -> ...any"},
		{"curried", fnType(params(param("n", tn("number"))), nil, fnType(params(param("s", tn("string"))), nil, tn("boolean"))), "(n: number) -> (s: string) -> boolean"},
		{"generic pack return", fnType(nil, nil, luau.NewTypeGenericPack("T")), "() -> T..."},
		{"pack return", fnType(nil, nil, pack(tn("number"), tn("string"))), "() -> (number, string)"},
		{"pack return variadic tail", fnType(nil, nil, pack(tn("A"), luau.NewTypeVariadic(tn("B")))), "() -> (A, ...B)"},
		{"pack return generic tail", fnType(nil, nil, pack(tn("A"), luau.NewTypeGenericPack("U"))), "() -> (A, U...)"},
		{"param and variadic", fnType(params(param("x", tn("number"))), luau.NewTypeVariadic(tn("string")), luau.NewTypeVariadic(tn("any"))), "(x: number, ...string) -> ...any"},
		{"generic function param", luau.NewTypeFunction(generics(luau.TypeGeneric{Name: "A"}), params(param("", fnType(params(param("", tn("A"))), nil, pack()))), nil, pack()), "<A>((A) -> ()) -> ()"},
		{"union return", fnType(nil, nil, union(tn("A"), tn("B"))), "() -> (A | B)"},
		{"intersection return", fnType(nil, nil, intersection(tn("A"), tn("B"))), "() -> (A & B)"},
		{"optional return", fnType(nil, nil, luau.NewTypeOptional(tn("A"))), "() -> A?"},
		{"optional union return", fnType(nil, nil, luau.NewTypeOptional(union(tn("A"), tn("B")))), "() -> (A | B)?"},
		{"optional function return", fnType(nil, nil, luau.NewTypeOptional(fnType(nil, nil, tn("T")))), "() -> (() -> T)?"},
		{"union param", fnType(params(param("a", union(tn("A"), tn("B")))), nil, pack()), "(a: A | B) -> ()"},
		{"variadic union", fnType(nil, luau.NewTypeVariadic(union(tn("A"), tn("B"))), pack()), "(...(A | B)) -> ()"},
		{"variadic intersection", fnType(nil, luau.NewTypeVariadic(intersection(tn("A"), tn("B"))), pack()), "(...(A & B)) -> ()"},
		{"variadic optional", fnType(nil, luau.NewTypeVariadic(luau.NewTypeOptional(tn("A"))), pack()), "(...A?) -> ()"},
		{"variadic function", fnType(nil, luau.NewTypeVariadic(fnType(nil, nil, pack())), pack()), "(...() -> ()) -> ()"},
	})
}

func TestRenderTypeTypeof(t *testing.T) {
	call := luau.NewCall(luau.ID("setmetatable"), exprList(luau.NewMap(luau.NewList[*luau.MapField]()), luau.ID("Foo")))
	runTypeCases(t, []typeCase{
		{"call", luau.NewTypeTypeof(call), "typeof(setmetatable({}, Foo))"},
		{"binary", luau.NewTypeTypeof(luau.NewBinary(luau.ID("a"), "+", luau.ID("b"))), "typeof(a + b)"},
		{"cast", luau.NewTypeTypeof(luau.NewCast(luau.ID("x"), tn("Foo"))), "typeof(x :: Foo)"},
	})
}

func TestRenderCastExpression(t *testing.T) {
	x, y := func() *luau.Identifier { return luau.ID("x") }, func() *luau.Identifier { return luau.ID("y") }
	call := luau.NewCall(luau.ID("setmetatable"), exprList(luau.NewMap(luau.NewList[*luau.MapField]()), luau.ID("Foo")))
	cases := []struct {
		name string
		node luau.Expression
		want string
	}{
		{"cast", luau.NewCast(x(), tn("number")), "x :: number"},
		{"double cast", luau.NewCast(luau.NewCast(call, tn("any")), tn("Foo")), "(setmetatable({}, Foo) :: any) :: Foo"},
		{"binary operand", luau.NewCast(luau.NewBinary(x(), "+", y()), tn("number")), "(x + y) :: number"},
		{"unary operand", luau.NewCast(luau.NewUnary("not", x()), tn("boolean")), "(not x) :: boolean"},
		{"if operand", luau.NewCast(luau.NewIfExpression(luau.ID("c"), x(), y()), tn("T")), "(if c then x else y) :: T"},
		{"left of less than", luau.NewBinary(luau.NewCast(x(), tn("T")), "<", y()), "(x :: T) < y"},
		{"right of less than", luau.NewBinary(x(), "<", luau.NewCast(y(), tn("T"))), "x < y :: T"},
		{"left of plus", luau.NewBinary(luau.NewCast(x(), tn("number")), "+", luau.Num(1)), "x :: number + 1"},
		{"left of less equal", luau.NewBinary(luau.NewCast(x(), tn("number")), "<=", luau.Num(1)), "x :: number <= 1"},
		{"under unary", luau.NewUnary("-", luau.NewCast(x(), tn("number"))), "-x :: number"},
		{"union type", luau.NewCast(x(), union(tn("A"), tn("B"))), "x :: A | B"},
		{"optional type", luau.NewCast(x(), luau.NewTypeOptional(tn("T"))), "x :: T?"},
		{"function type", luau.NewCast(x(), fnType(nil, nil, pack())), "x :: () -> ()"},
		{"parenthesized then indexed", luau.NewPropertyAccess(luau.NewParenthesized(luau.NewCast(x(), tn("T"))), "y"), "(x :: T).y"},
		{"argument", luau.NewCall(luau.ID("f"), exprList(luau.NewCast(x(), tn("T")))), "f(x :: T)"},
		{"table member", luau.NewArray(exprList(luau.NewCast(x(), tn("T")))), "{ x :: T }"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if got := renderExpr(t, c.node); got != c.want {
				t.Errorf("got %q, want %q", got, c.want)
			}
		})
	}
}

func TestRenderCastStatementEnding(t *testing.T) {
	// A cast never ends with an indexable expression, so no `;` is needed
	// before a statement that starts with `(`.
	first := luau.NewVariableDeclaration(luau.ID("a"), luau.NewCast(luau.ID("b"), tn("T")))
	paren := luau.NewParenthesized(luau.NewBinary(luau.ID("c"), "+", luau.ID("d")))
	second := luau.NewAssignment(luau.NewPropertyAccess(paren, "e"), "=", luau.ID("f"))
	if got, want := renderAST(t, stmts(first, second)), "local a = b :: T\n(c + d).e = f\n"; got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

func TestRenderTypeStatements(t *testing.T) {
	generics := func(gs ...luau.TypeGeneric) []luau.TypeGeneric { return gs }
	params := func(ps ...luau.TypeFunctionParameter) []luau.TypeFunctionParameter { return ps }
	exported := func(alias *luau.TypeAlias) *luau.TypeAlias { alias.Export = true; return alias }
	cases := []struct {
		name string
		ast  *luau.List[luau.Statement]
		want string
	}{
		{
			"structured alias",
			stmts(luau.NewTypeAlias("Node", generics(luau.TypeGeneric{Name: "K"}, luau.TypeGeneric{Name: "V"}), table(prop("key", tn("K")), prop("value", tn("V"))))),
			"type Node<K, V> = { key: K, value: V }\n",
		},
		{
			"export",
			stmts(exported(luau.NewTypeAlias("Foo", nil, tn("number")))),
			"export type Foo = number\n",
		},
		{
			"defaults",
			stmts(exported(luau.NewTypeAlias("Fn",
				generics(luau.TypeGeneric{Name: "T", Default: tn("number")}, luau.TypeGeneric{Name: "U", IsPack: true, Default: luau.NewTypeVariadic(tn("string"))}),
				fnType(params(param("", tn("T"))), luau.NewTypeGenericPack("U"), pack())))),
			"export type Fn<T = number, U... = ...string> = (T, U...) -> ()\n",
		},
		{
			"default referencing earlier parameter",
			stmts(luau.NewTypeAlias("Pair", generics(luau.TypeGeneric{Name: "T"}, luau.TypeGeneric{Name: "U", Default: tn("T")}), table(prop("first", tn("T")), prop("second", tn("U"))))),
			"type Pair<T, U = T> = { first: T, second: U }\n",
		},
		{
			"empty pack default",
			stmts(luau.NewTypeAlias("Returns", generics(luau.TypeGeneric{Name: "T", IsPack: true, Default: pack()}), fnType(nil, nil, luau.NewTypeGenericPack("T")))),
			"type Returns<T... = ()> = () -> T...\n",
		},
		{
			"generic packs",
			stmts(luau.NewTypeAlias("Callback", generics(luau.TypeGeneric{Name: "Args", IsPack: true}, luau.TypeGeneric{Name: "Rets", IsPack: true}),
				table(prop("f", fnType(nil, luau.NewTypeGenericPack("Args"), luau.NewTypeGenericPack("Rets")))))),
			"type Callback<Args..., Rets...> = { f: (Args...) -> Rets... }\n",
		},
		{
			"set body",
			stmts(func() *luau.TypeAlias {
				alias := luau.NewTypeAlias("Tree", generics(luau.TypeGeneric{Name: "a"}), nil)
				alias.SetBody(table(prop("data", tn("a")), prop("children", luau.NewTypeArray(luau.NewTypeReference("Tree", typeNodes(tn("a")))))))
				return alias
			}()),
			"type Tree<a> = { data: a, children: { Tree<a> } }\n",
		},
		{
			"type function",
			stmts(func() luau.Statement {
				declaration := luau.NewTypeFunctionDeclaration("Keys", luau.NewList[luau.AnyIdentifier](luau.ID("t")), false, stmts(
					luau.NewVariableDeclaration(luau.ID("result"), luau.NewMap(luau.NewList[*luau.MapField]())),
					luau.NewReturn(luau.ID("result")),
				))
				declaration.Export = true
				return declaration
			}()),
			"export type function Keys(t)\n\tlocal result = {}\n\treturn result\nend\n",
		},
		{
			"type function with varargs",
			stmts(luau.NewTypeFunctionDeclaration("Partial", luau.NewList[luau.AnyIdentifier](luau.ID("T")), true, stmts(luau.NewReturn(luau.ID("T"))))),
			"type function Partial(T, ...)\n\treturn T\nend\n",
		},
		{
			"alias inside a function body",
			stmts(luau.NewFunctionDeclaration(true, luau.ID("f"), luau.NewList[luau.AnyIdentifier](), false, stmts(
				luau.NewTypeAlias("Local", nil, tn("number")),
				luau.NewReturn(luau.Nil()),
			))),
			"local function f()\n\ttype Local = number\n\treturn nil\nend\n",
		},
		{
			"type function inside a do block",
			stmts(luau.NewDo(stmts(luau.NewTypeFunctionDeclaration("F", luau.NewList[luau.AnyIdentifier](), false, stmts(luau.NewReturn(luau.ID("types"))))))),
			"do\n\ttype function F()\n\t\treturn types\n\tend\nend\n",
		},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if got := renderAST(t, c.ast); got != c.want {
				t.Errorf("got %q, want %q", got, c.want)
			}
		})
	}
}

func TestRenderTypedDeclarations(t *testing.T) {
	annotated := func(name string, t luau.TypeNode) *luau.Identifier {
		id := luau.ID(name)
		luau.SetBindingType(id, t)
		return id
	}
	cases := []struct {
		name string
		ast  *luau.List[luau.Statement]
		want string
	}{
		{
			"generic function with typed varargs",
			stmts(func() luau.Statement {
				declaration := luau.NewFunctionDeclaration(true, luau.ID("f"), luau.NewList[luau.AnyIdentifier](annotated("a", tn("K"))), true, stmts())
				declaration.Generics = []luau.TypeGeneric{{Name: "K"}, {Name: "V"}}
				declaration.VarArgsType = tn("V")
				declaration.ReturnType = tn("V")
				return declaration
			}()),
			"local function f<K, V>(a: K, ...: V): V\nend\n",
		},
		{
			"generic pack varargs",
			stmts(func() luau.Statement {
				declaration := luau.NewFunctionDeclaration(false, luau.ID("f"), luau.NewList[luau.AnyIdentifier](), true, stmts())
				declaration.Generics = []luau.TypeGeneric{{Name: "T", IsPack: true}}
				declaration.VarArgsType = luau.NewTypeGenericPack("T")
				return declaration
			}()),
			"function f<T...>(...: T...)\nend\n",
		},
		{
			"untyped varargs",
			stmts(luau.NewFunctionDeclaration(true, luau.ID("f"), luau.NewList[luau.AnyIdentifier](luau.ID("a")), true, stmts())),
			"local function f(a, ...)\nend\n",
		},
		{
			"annotated locals",
			stmts(luau.NewVariableDeclaration(
				luau.NewList[luau.AnyIdentifier](annotated("a", tn("number")), annotated("b", luau.NewTypeOptional(tn("string")))),
				exprList(luau.Num(1), luau.Nil()),
			)),
			"local a: number, b: string? = 1, nil\n",
		},
		{
			"annotated local without value",
			stmts(luau.NewVariableDeclaration(annotated("x", luau.NewTypeArray(tn("number"))), nil)),
			"local x: { number }\n",
		},
		{
			"annotated temp local",
			stmts(func() luau.Statement {
				temp := luau.TempID("binding")
				luau.SetBindingType(temp, tn("Foo"))
				return luau.NewVariableDeclaration(temp, luau.ID("v"))
			}()),
			"local _binding: Foo = v\n",
		},
		{
			"mixed annotated list",
			stmts(luau.NewVariableDeclaration(
				luau.NewList[luau.AnyIdentifier](luau.ID("a"), annotated("b", tn("number"))),
				exprList(luau.Num(1), luau.Num(2)),
			)),
			"local a, b: number = 1, 2\n",
		},
		{
			"assignment never renders annotations",
			stmts(luau.NewAssignment(annotated("x", tn("number")), "=", luau.Num(1))),
			"x = 1\n",
		},
		{
			"uses never render annotations",
			stmts(func() luau.Statement {
				x := annotated("x", tn("number"))
				return luau.NewReturn(luau.NewBinary(x, "+", x))
			}()),
			"return x + x\n",
		},
		{
			"function expression",
			stmts(luau.NewVariableDeclaration(luau.ID("f"), func() luau.Expression {
				expression := luau.NewFunctionExpression(luau.NewList[luau.AnyIdentifier](annotated("x", tn("T"))), false, stmts(luau.NewReturn(luau.ID("x"))))
				expression.Generics = []luau.TypeGeneric{{Name: "T"}}
				expression.ReturnType = tn("T")
				return expression
			}())),
			"local f = function<T>(x: T): T\n\treturn x\nend\n",
		},
		{
			"empty function expression with annotated temp",
			stmts(luau.NewVariableDeclaration(luau.ID("f"), func() luau.Expression {
				temp := luau.TempID("param")
				luau.SetBindingType(temp, tn("Foo"))
				return luau.NewFunctionExpression(luau.NewList[luau.AnyIdentifier](temp), false, stmts())
			}())),
			"local f = function(_param: Foo) end\n",
		},
		{
			"method",
			stmts(func() luau.Statement {
				method := luau.NewMethodDeclaration(luau.ID("Class"), "method", luau.NewList[luau.AnyIdentifier](annotated("x", tn("number"))), false, stmts(luau.NewReturn(luau.ID("x"))))
				method.ReturnType = luau.NewTypeOptional(tn("number"))
				return method
			}()),
			"function Class:method(x: number): number?\n\treturn x\nend\n",
		},
		{
			"declared union return stays bare",
			stmts(func() luau.Statement {
				declaration := luau.NewFunctionDeclaration(true, luau.ID("f"), luau.NewList[luau.AnyIdentifier](), false, stmts())
				declaration.ReturnType = union(luau.NewTypeArray(tn("string")), luau.NewTypeArray(tn("number")))
				return declaration
			}()),
			"local function f(): { string } | { number }\nend\n",
		},
		{
			"declared pack return",
			stmts(func() luau.Statement {
				declaration := luau.NewFunctionDeclaration(true, luau.ID("f"), luau.NewList[luau.AnyIdentifier](), false, stmts())
				declaration.ReturnType = pack(tn("number"), tn("string"))
				return declaration
			}()),
			"local function f(): (number, string)\nend\n",
		},
		{
			"declared function return",
			stmts(func() luau.Statement {
				declaration := luau.NewFunctionDeclaration(true, luau.ID("f"), luau.NewList[luau.AnyIdentifier](), false, stmts())
				declaration.ReturnType = fnType([]luau.TypeFunctionParameter{param("a", tn("number"))}, nil, tn("boolean"))
				return declaration
			}()),
			"local function f(): (a: number) -> boolean\nend\n",
		},
		{
			"declared variadic return",
			stmts(func() luau.Statement {
				declaration := luau.NewFunctionDeclaration(true, luau.ID("f"), luau.NewList[luau.AnyIdentifier](), false, stmts())
				declaration.ReturnType = luau.NewTypeVariadic(tn("any"))
				return declaration
			}()),
			"local function f(): ...any\nend\n",
		},
		{
			"cast at construction",
			stmts(luau.NewVariableDeclaration(luau.ID("self"), luau.NewCast(
				luau.NewCast(luau.NewCall(luau.ID("setmetatable"), exprList(luau.NewMap(luau.NewList[*luau.MapField]()), luau.ID("Foo"))), tn("any")),
				tn("Foo")))),
			"local self = (setmetatable({}, Foo) :: any) :: Foo\n",
		},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if got := renderAST(t, c.ast); got != c.want {
				t.Errorf("got %q, want %q", got, c.want)
			}
		})
	}
}

func TestRenderMultilineTypes(t *testing.T) {
	multiline := func(table *luau.TypeTable) *luau.TypeTable { table.Multiline = true; return table }
	multilineUnion := func(union *luau.TypeUnion) *luau.TypeUnion { union.Multiline = true; return union }
	okTrue := func() *luau.TypeTable { return table(prop("ok", luau.NewTypeBooleanSingleton(true))) }
	okFalse := func() *luau.TypeTable { return table(prop("ok", luau.NewTypeBooleanSingleton(false))) }
	cases := []struct {
		name string
		ast  *luau.List[luau.Statement]
		want string
	}{
		{
			"table",
			stmts(luau.NewTypeAlias("T", nil, multiline(table(prop("a", tn("number")), prop("b", tn("string")))))),
			"type T = {\n\ta: number,\n\tb: string,\n}\n",
		},
		{
			"empty table never breaks",
			stmts(luau.NewTypeAlias("T", nil, multiline(table()))),
			"type T = {}\n",
		},
		{
			"nested tables",
			stmts(luau.NewTypeAlias("T", nil, multiline(table(prop("inner", multiline(table(prop("a", tn("number"))))), prop("flat", table(prop("b", tn("number")))))))),
			"type T = {\n\tinner: {\n\t\ta: number,\n\t},\n\tflat: { b: number },\n}\n",
		},
		{
			"table inside a function body",
			stmts(luau.NewFunctionDeclaration(true, luau.ID("f"), luau.NewList[luau.AnyIdentifier](), false, stmts(
				luau.NewTypeAlias("T", nil, multiline(table(prop("a", tn("number"))))),
			))),
			"local function f()\n\ttype T = {\n\t\ta: number,\n\t}\nend\n",
		},
		{
			"table in a local annotation",
			stmts(func() luau.Statement {
				x := luau.ID("x")
				luau.SetBindingType(x, multiline(table(prop("a", tn("number")))))
				return luau.NewVariableDeclaration(x, luau.ID("v"))
			}()),
			"local x: {\n\ta: number,\n} = v\n",
		},
		{
			"table in a single-line union",
			stmts(luau.NewTypeAlias("T", nil, union(multiline(table(prop("a", tn("number")))), tn("nil")))),
			"type T = {\n\ta: number,\n} | nil\n",
		},
		{
			"union alias body",
			stmts(luau.NewTypeAlias("R", nil, multilineUnion(union(okTrue(), okFalse())))),
			"type R =\n\t| { ok: true }\n\t| { ok: false }\n",
		},
		{
			"exported union alias body",
			stmts(func() luau.Statement {
				alias := luau.NewTypeAlias("R", []luau.TypeGeneric{{Name: "T"}}, multilineUnion(union(tn("T"), tn("nil"))))
				alias.Export = true
				return alias
			}()),
			"export type R<T> =\n\t| T\n\t| nil\n",
		},
		{
			"union member parenthesization survives",
			stmts(luau.NewTypeAlias("R", nil, multilineUnion(union(fnType(nil, nil, pack()), luau.NewTypeOptional(tn("A")))))),
			"type R =\n\t| (() -> ())\n\t| A?\n",
		},
		{
			"union of multiline tables",
			stmts(luau.NewTypeAlias("R", nil, multilineUnion(union(multiline(okTrue()), okFalse())))),
			"type R =\n\t| {\n\t\tok: true,\n\t}\n\t| { ok: false }\n",
		},
		{
			"union alias inside a function body",
			stmts(luau.NewFunctionDeclaration(true, luau.ID("f"), luau.NewList[luau.AnyIdentifier](), false, stmts(
				luau.NewTypeAlias("R", nil, multilineUnion(union(okTrue(), okFalse()))),
			))),
			"local function f()\n\ttype R =\n\t\t| { ok: true }\n\t\t| { ok: false }\nend\n",
		},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if got := renderAST(t, c.ast); got != c.want {
				t.Errorf("got %q, want %q", got, c.want)
			}
		})
	}
}

func TestLayoutTypes(t *testing.T) {
	flat := func() *luau.TypeTable { return table(prop("a", tn("number")), prop("b", tn("string"))) } // 24 columns
	t.Run("table threshold", func(t *testing.T) {
		ast := stmts(luau.NewTypeAlias("T", nil, flat()))
		LayoutTypes(ast, 24)
		if got, want := renderAST(t, ast), "type T = { a: number, b: string }\n"; got != want {
			t.Errorf("width 24: got %q, want %q", got, want)
		}
		LayoutTypes(ast, 23)
		if got, want := renderAST(t, ast), "type T = {\n\ta: number,\n\tb: string,\n}\n"; got != want {
			t.Errorf("width 23: got %q, want %q", got, want)
		}
		// Widening again clears the flag: the pass is a function of width.
		LayoutTypes(ast, DefaultTypeLineWidth)
		if got, want := renderAST(t, ast), "type T = { a: number, b: string }\n"; got != want {
			t.Errorf("default width: got %q, want %q", got, want)
		}
	})
	t.Run("children before parents", func(t *testing.T) {
		ast := stmts(luau.NewTypeAlias("T", nil, table(prop("inner", flat())))) // 35 columns
		LayoutTypes(ast, 30)
		if got, want := renderAST(t, ast), "type T = {\n\tinner: { a: number, b: string },\n}\n"; got != want {
			t.Errorf("width 30: got %q, want %q", got, want)
		}
		LayoutTypes(ast, 20)
		if got, want := renderAST(t, ast), "type T = {\n\tinner: {\n\t\ta: number,\n\t\tb: string,\n\t},\n}\n"; got != want {
			t.Errorf("width 20: got %q, want %q", got, want)
		}
	})
	t.Run("alias body union", func(t *testing.T) {
		body := union(table(prop("ok", luau.NewTypeBooleanSingleton(true))), table(prop("ok", luau.NewTypeBooleanSingleton(false)))) // 28 columns
		ast := stmts(luau.NewTypeAlias("R", nil, body))
		LayoutTypes(ast, 28)
		if got, want := renderAST(t, ast), "type R = { ok: true } | { ok: false }\n"; got != want {
			t.Errorf("width 28: got %q, want %q", got, want)
		}
		LayoutTypes(ast, 27)
		if got, want := renderAST(t, ast), "type R =\n\t| { ok: true }\n\t| { ok: false }\n"; got != want {
			t.Errorf("width 27: got %q, want %q", got, want)
		}
		LayoutTypes(ast, 10)
		if got, want := renderAST(t, ast), "type R =\n\t| {\n\t\tok: true,\n\t}\n\t| {\n\t\tok: false,\n\t}\n"; got != want {
			t.Errorf("width 10: got %q, want %q", got, want)
		}
	})
	t.Run("union outside an alias body stays single-line", func(t *testing.T) {
		x := luau.ID("x")
		luau.SetBindingType(x, union(tn("A"), table(prop("a", tn("number")))))
		ast := stmts(luau.NewVariableDeclaration(x, luau.ID("v")))
		LayoutTypes(ast, 1)
		if got, want := renderAST(t, ast), "local x: A | {\n\ta: number,\n} = v\n"; got != want {
			t.Errorf("got %q, want %q", got, want)
		}
	})
	t.Run("nested union is not an alias body", func(t *testing.T) {
		ast := stmts(luau.NewTypeAlias("T", nil, table(prop("u", union(tn("A"), tn("B"))))))
		LayoutTypes(ast, 1)
		if got, want := renderAST(t, ast), "type T = {\n\tu: A | B,\n}\n"; got != want {
			t.Errorf("got %q, want %q", got, want)
		}
	})
	t.Run("types in every annotation slot", func(t *testing.T) {
		a := luau.ID("a")
		luau.SetBindingType(a, table(prop("p", tn("number"))))
		declaration := luau.NewFunctionDeclaration(true, luau.ID("f"), luau.NewList[luau.AnyIdentifier](a), true, stmts(
			luau.NewReturn(luau.NewCast(luau.ID("v"), table(prop("c", tn("number"))))),
		))
		declaration.VarArgsType = table(prop("v", tn("number")))
		declaration.ReturnType = table(prop("r", tn("number")))
		ast := stmts(declaration)
		LayoutTypes(ast, 1)
		want := "local function f(a: {\n\tp: number,\n}, ...: {\n\tv: number,\n}): {\n\tr: number,\n}\n\treturn v :: {\n\t\tc: number,\n\t}\nend\n"
		if got := renderAST(t, ast); got != want {
			t.Errorf("got %q, want %q", got, want)
		}
	})
	t.Run("width zero clears flags", func(t *testing.T) {
		body := flat()
		body.Multiline = true
		ast := stmts(luau.NewTypeAlias("T", nil, body))
		LayoutTypes(ast, 0)
		if got, want := renderAST(t, ast), "type T = { a: number, b: string }\n"; got != want {
			t.Errorf("got %q, want %q", got, want)
		}
	})
	t.Run("empty table never breaks", func(t *testing.T) {
		ast := stmts(luau.NewTypeAlias("T", nil, table()))
		LayoutTypes(ast, 1)
		if got, want := renderAST(t, ast), "type T = {}\n"; got != want {
			t.Errorf("got %q, want %q", got, want)
		}
	})
	t.Run("width counts runes", func(t *testing.T) {
		body := table(prop("e", luau.NewTypeStringSingleton("😀"))) // 12 runes, 15 bytes
		ast := stmts(luau.NewTypeAlias("T", nil, body))
		LayoutTypes(ast, 12)
		if got, want := renderAST(t, ast), "type T = { e: \"😀\" }\n"; got != want {
			t.Errorf("got %q, want %q", got, want)
		}
	})
	t.Run("RenderAST never breaks on its own", func(t *testing.T) {
		fields := tableFields()
		for _, name := range strings.Fields("alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi") {
			fields.Push(prop(name, tn("number")))
		}
		ast := stmts(luau.NewTypeAlias("Wide", nil, luau.NewTypeTable(fields)))
		got := renderAST(t, ast)
		if strings.Count(got, "\n") != 1 || len(got) < 150 {
			t.Errorf("wide alias must stay on one line: %q", got)
		}
	})
	t.Run("temporaries inside typeof", func(t *testing.T) {
		temp := luau.TempID("instance")
		ast := stmts(
			luau.NewVariableDeclaration(temp, luau.ID("v")),
			luau.NewTypeAlias("T", nil, table(prop("self", luau.NewTypeTypeof(temp)))),
		)
		LayoutTypes(ast, 1)
		if got, want := renderAST(t, ast), "local _instance = v\ntype T = {\n\tself: typeof(_instance),\n}\n"; got != want {
			t.Errorf("got %q, want %q", got, want)
		}
	})
}

func TestRenderTypePanics(t *testing.T) {
	generics := func(gs ...luau.TypeGeneric) []luau.TypeGeneric { return gs }
	cases := []struct {
		name string
		ast  *luau.List[luau.Statement]
	}{
		{"invalid type name", stmts(luau.NewTypeAlias("T", nil, tn("not valid")))},
		{"keyword type name", stmts(luau.NewTypeAlias("T", nil, tn("end")))},
		{"typeof as a name", stmts(luau.NewTypeAlias("T", nil, tn("typeof")))},
		{"invalid module", stmts(luau.NewTypeAlias("T", nil, luau.NewTypeQualifiedReference("not valid", "T", nil)))},
		{"nil type child", stmts(luau.NewTypeAlias("T", nil, luau.NewTypeOptional(nil)))},
		{"optional pack", stmts(luau.NewTypeAlias("T", nil, luau.NewTypeOptional(pack(tn("A")))))},
		{"optional variadic", stmts(luau.NewTypeAlias("T", nil, luau.NewTypeOptional(luau.NewTypeVariadic(tn("A")))))},
		{"empty union", stmts(luau.NewTypeAlias("T", nil, union()))},
		{"empty intersection", stmts(luau.NewTypeAlias("T", nil, intersection()))},
		{"two indexers", stmts(luau.NewTypeAlias("T", nil, table(
			luau.NewTypeTableIndexer(luau.TypeAccessNone, tn("string"), tn("number")),
			luau.NewTypeTableIndexer(luau.TypeAccessNone, tn("number"), tn("number")),
		)))},
		{"function without return", stmts(luau.NewTypeAlias("T", nil, luau.NewTypeFunction(nil, nil, nil, nil)))},
		{"invalid parameter name", stmts(luau.NewTypeAlias("T", nil, fnType([]luau.TypeFunctionParameter{param("not valid", tn("A"))}, nil, pack())))},
		{"varargs that is not a pack", stmts(luau.NewTypeAlias("T", nil, fnType(nil, tn("A"), pack())))},
		{"varargs explicit pack", stmts(luau.NewTypeAlias("T", nil, fnType(nil, pack(tn("A")), pack())))},
		{"pack inside a pack", stmts(luau.NewTypeAlias("T", nil, fnType(nil, nil, pack(pack(tn("A"))))))},
		{"variadic not last", stmts(luau.NewTypeAlias("T", nil, fnType(nil, nil, pack(luau.NewTypeVariadic(tn("A")), tn("B")))))},
		{"generic pack not last", stmts(luau.NewTypeAlias("T", nil, fnType(nil, nil, pack(luau.NewTypeGenericPack("U"), tn("B")))))},
		{"variadic of a pack", stmts(luau.NewTypeAlias("T", nil, luau.NewTypeVariadic(luau.NewTypeGenericPack("T"))))},
		{"invalid generic pack name", stmts(luau.NewTypeAlias("T", nil, luau.NewTypeGenericPack("not valid")))},
		{"generic before pack", stmts(luau.NewTypeAlias("T", generics(luau.TypeGeneric{Name: "U", IsPack: true}, luau.TypeGeneric{Name: "T"}), tn("T")))},
		{"invalid generic name", stmts(luau.NewTypeAlias("T", generics(luau.TypeGeneric{Name: "not valid"}), tn("T")))},
		{"default on a function type", stmts(luau.NewTypeAlias("T", nil, luau.NewTypeFunction(generics(luau.TypeGeneric{Name: "T", Default: tn("number")}), nil, nil, pack())))},
		{"default on a function declaration", stmts(func() luau.Statement {
			declaration := luau.NewFunctionDeclaration(true, luau.ID("f"), luau.NewList[luau.AnyIdentifier](), false, stmts())
			declaration.Generics = generics(luau.TypeGeneric{Name: "T", Default: tn("number")})
			return declaration
		}())},
		{"pack default on a type parameter", stmts(luau.NewTypeAlias("T", generics(luau.TypeGeneric{Name: "T", Default: pack()}), tn("T")))},
		{"type default on a pack parameter", stmts(luau.NewTypeAlias("T", generics(luau.TypeGeneric{Name: "T", IsPack: true, Default: tn("number")}), tn("T")))},
		{"missing default after a defaulted parameter", stmts(luau.NewTypeAlias("T", generics(luau.TypeGeneric{Name: "T", Default: tn("number")}, luau.TypeGeneric{Name: "U"}), tn("T")))},
		{"alias without body", stmts(luau.NewTypeAlias("T", nil, nil))},
		{"alias with a pack body", stmts(luau.NewTypeAlias("T", nil, pack(tn("A"), tn("B"))))},
		{"invalid alias name", stmts(luau.NewTypeAlias("not valid", nil, tn("A")))},
		{"invalid type function name", stmts(luau.NewTypeFunctionDeclaration("not valid", luau.NewList[luau.AnyIdentifier](), false, stmts()))},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			defer func() {
				if recover() == nil {
					t.Error("must panic")
				}
			}()
			RenderAST(c.ast)
		})
	}
}

func TestTypePositions(t *testing.T) {
	var cases []positionCase
	{
		x := luau.ID("x")
		number := tn("number")
		optional := luau.NewTypeOptional(number)
		luau.SetBindingType(x, optional)
		one := luau.Num(1)
		decl := luau.NewVariableDeclaration(x, one)
		cases = append(cases, positionCase{
			name: "annotated local",
			ast:  stmts(decl),
			text: "local x: number? = 1\n",
			ranges: []Range{
				nodeRange(decl, 0, 0, 0, 20),
				nodeRange(x, 0, 6, 0, 7),
				nodeRange(optional, 0, 9, 0, 16),
				nodeRange(number, 0, 9, 0, 15),
				nodeRange(one, 0, 19, 0, 20),
			},
		})
	}
	{
		a, b := tn("A"), tn("B")
		inner := union(a, b)
		optional := luau.NewTypeOptional(inner)
		alias := luau.NewTypeAlias("T", nil, optional)
		cases = append(cases, positionCase{
			name: "parentheses belong to the parenthesized type",
			ast:  stmts(alias),
			text: "type T = (A | B)?\n",
			ranges: []Range{
				nodeRange(alias, 0, 0, 0, 17),
				nodeRange(optional, 0, 9, 0, 17),
				nodeRange(inner, 0, 9, 0, 16),
				nodeRange(a, 0, 10, 0, 11),
				nodeRange(b, 0, 14, 0, 15),
			},
		})
	}
	{
		x := luau.ID("x")
		number := tn("number")
		cast := luau.NewCast(x, number)
		ret := luau.NewReturn(cast)
		cases = append(cases, positionCase{
			name: "cast operator",
			ast:  stmts(ret),
			text: "return x :: number\n",
			ranges: []Range{
				nodeRange(ret, 0, 0, 0, 18),
				{Node: cast, Start: luau.Position{Line: 0, Column: 7}, End: luau.Position{Line: 0, Column: 18}, Operator: &luau.Position{Line: 0, Column: 9}},
				nodeRange(x, 0, 7, 0, 8),
				nodeRange(number, 0, 12, 0, 18),
			},
		})
	}
	{
		param := luau.ID("t")
		use := luau.ID("t")
		ret := luau.NewReturn(use)
		declaration := luau.NewTypeFunctionDeclaration("F", luau.NewList[luau.AnyIdentifier](param), false, stmts(ret))
		cases = append(cases, positionCase{
			name: "type function closing",
			ast:  stmts(declaration),
			text: "type function F(t)\n\treturn t\nend\n",
			ranges: []Range{
				closedRange(declaration, 0, 0, 2, 3, 2, 0),
				nodeRange(param, 0, 16, 0, 17),
				nodeRange(ret, 1, 1, 1, 9),
				nodeRange(use, 1, 8, 1, 9),
			},
		})
	}
	{
		key := tn("K")
		field := prop("key", key)
		body := table(field)
		body.Multiline = true
		alias := luau.NewTypeAlias("T", []luau.TypeGeneric{{Name: "K"}}, body)
		cases = append(cases, positionCase{
			name: "multiline table field excludes indentation",
			ast:  stmts(alias),
			text: "type T<K> = {\n\tkey: K,\n}\n",
			ranges: []Range{
				nodeRange(alias, 0, 0, 2, 1),
				nodeRange(body, 0, 12, 2, 1),
				nodeRange(field, 1, 1, 1, 7),
				nodeRange(key, 1, 6, 1, 7),
			},
		})
	}
	{
		a, b := tn("A"), tn("B")
		body := union(a, b)
		body.Multiline = true
		alias := luau.NewTypeAlias("T", nil, body)
		cases = append(cases, positionCase{
			name: "multiline union starts at the first bar",
			ast:  stmts(alias),
			text: "type T =\n\t| A\n\t| B\n",
			ranges: []Range{
				nodeRange(alias, 0, 0, 2, 4),
				nodeRange(body, 1, 1, 2, 4),
				nodeRange(a, 1, 3, 1, 4),
				nodeRange(b, 2, 3, 2, 4),
			},
		})
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			text, ranges := RenderASTWithPositions(tc.ast)
			if text != tc.text {
				t.Fatalf("text = %q, want %q", text, tc.text)
			}
			if !reflect.DeepEqual(ranges, tc.ranges) {
				t.Fatalf("ranges = %#v, want %#v", ranges, tc.ranges)
			}
			if plain := RenderAST(tc.ast); text != plain {
				t.Fatalf("position text = %q, plain text = %q", text, plain)
			}
		})
	}
}
