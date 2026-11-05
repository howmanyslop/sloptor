//! Port of `internal/luau/render/types_test.go`.

#![expect(
	clippy::tuple_array_conversions,
	reason = "tests bind sibling nodes in one statement"
)]

use super::dsl::{
	N, alias, array, bin, bool_singleton, build_generics, call, cast, fn_type, fn_type_generic, generic, generic_pack,
	gpack, id, if_expr, intersection, map, nil, num, optional, optional_type, pack, param, paren, prop, prop_with,
	property, qualified, string_singleton, table, tn, type_array, type_ref, typeof_, un, union, variadic,
};
use super::positions::{PositionCase, check_position_cases};
use super::{closed_range, ids, node_range, payload, render_both, render_expr, stmts};
use crate::render::{DEFAULT_TYPE_LINE_WIDTH, Range, layout_types, render_ast};
use crate::{
	Ast, BinaryOperator as B, FunctionDeclaration, FunctionExpression, MethodDeclaration, NodeId, TypeAccess,
	TypeAlias, TypeArray, TypeFunction, TypeFunctionDeclaration, TypeGeneric, TypeOptional, TypeTable, TypeUnion,
	UnaryOperator as U, element,
};

/// Renders `node` as an alias body through both public APIs and returns the
/// body text alone.
fn render_type_text(a: &mut Ast, node: NodeId) -> String {
	let declaration = a.type_alias("T", Vec::new(), Some(node));
	let ast = stmts(a, &[declaration]);
	let got = render_both(a, ast);
	got.strip_prefix("type T = ")
		.and_then(|rest| rest.strip_suffix('\n'))
		.unwrap_or_else(|| panic!("alias rendered as {got:?}"))
		.to_owned()
}

fn run_type_cases(cases: Vec<(&str, N, &str)>) {
	let mut a = Ast::new();
	for (name, node, want) in cases {
		let node = node.build(&mut a);
		assert_eq!(render_type_text(&mut a, node), want, "{name}");
	}
}

fn multiline_table(fields: Vec<N>) -> N {
	let table = table(fields);
	N::from_fn(move |a| {
		let node = table.build(a);
		payload_mut::<TypeTable>(a, node).multiline = true;
		node
	})
}

fn multiline_union(members: Vec<N>) -> N {
	let union = union(members);
	N::from_fn(move |a| {
		let node = union.build(a);
		payload_mut::<TypeUnion>(a, node).multiline = true;
		node
	})
}

fn exported(node: N) -> N {
	N::from_fn(move |a| {
		let node = node.build(a);
		if let Some(alias) = a.get_mut::<TypeAlias>(node) {
			alias.export = true;
		} else if let Some(declaration) = a.get_mut::<TypeFunctionDeclaration>(node) {
			declaration.export = true;
		}
		node
	})
}

fn payload_mut<T: crate::Payload>(a: &mut Ast, node: NodeId) -> &mut T {
	a.get_mut::<T>(node).expect("expected kind")
}

#[test]
fn render_type_atoms() {
	run_type_cases(vec![
		("name", tn("number"), "number"),
		("nil", tn("nil"), "nil"),
		("qualified", qualified("Enum", "Material", None), "Enum.Material"),
		(
			"arguments",
			type_ref("Map", vec![tn("string"), tn("number")]),
			"Map<string, number>",
		),
		(
			"qualified arguments",
			qualified("M", "T", Some(vec![tn("A")])),
			"M.T<A>",
		),
		("empty arguments", type_ref("Foo", vec![]), "Foo"),
		(
			"pack argument",
			type_ref("Foo", vec![pack(vec![tn("A"), tn("B")])]),
			"Foo<(A, B)>",
		),
		(
			"variadic argument",
			type_ref("Foo", vec![variadic(tn("number"))]),
			"Foo<...number>",
		),
		("generic pack argument", type_ref("Foo", vec![gpack("U")]), "Foo<U...>"),
		(
			"trailing generic pack",
			type_ref("List", vec![tn("number"), gpack("Rest")]),
			"List<number, Rest...>",
		),
		(
			"union argument",
			type_ref("Foo", vec![union(vec![tn("A"), tn("B")])]),
			"Foo<A | B>",
		),
		(
			"function argument",
			type_ref("Foo", vec![fn_type(vec![param("", tn("A"))], None, tn("B"))]),
			"Foo<(A) -> B>",
		),
	]);
}

#[test]
fn render_type_singletons() {
	run_type_cases(vec![
		("string", string_singleton("up"), "\"up\""),
		("escapes", string_singleton("a\"b\\c\nd\re\tf"), r#""a\"b\\c\nd\re\tf""#),
		("control", string_singleton("\x01\x7f"), r#""\x01\x7F""#),
		("unicode", string_singleton("😀"), "\"😀\""),
		("true", bool_singleton(true), "true"),
		("false", bool_singleton(false), "false"),
		(
			"singleton union",
			union(vec![string_singleton("r"), string_singleton("w")]),
			"\"r\" | \"w\"",
		),
	]);
}

#[test]
fn render_type_optional() {
	let unit = || fn_type(vec![param("a", tn("number"))], None, pack(vec![]));
	run_type_cases(vec![
		("name", optional(tn("number")), "number?"),
		("union", optional(union(vec![tn("A"), tn("B")])), "(A | B)?"),
		(
			"intersection",
			optional(intersection(vec![tn("A"), tn("B")])),
			"(A & B)?",
		),
		("function", optional(unit()), "((a: number) -> ())?"),
		("optional", optional(optional(tn("A"))), "(A?)?"),
		("array", optional(type_array(tn("number"))), "{ number }?"),
		(
			"table",
			optional(table(vec![prop("a", tn("number"))])),
			"{ a: number }?",
		),
		("typeof", optional(typeof_(id("x"))), "typeof(x)?"),
		("singleton", optional(string_singleton("a")), "\"a\"?"),
		("normalized optional", optional_type(optional(tn("A"))), "A?"),
		("normalized nil", optional_type(tn("nil")), "nil"),
		("normalized any", optional_type(tn("any")), "any"),
		("normalized unknown", optional_type(tn("unknown")), "unknown"),
		(
			"normalized qualified",
			optional_type(qualified("M", "any", None)),
			"M.any?",
		),
		("normalized name", optional_type(tn("number")), "number?"),
		(
			"normalized union",
			optional_type(union(vec![tn("A"), tn("B")])),
			"(A | B)?",
		),
	]);
}

#[test]
fn render_type_union_intersection() {
	let predicate = || fn_type(vec![param("a", tn("number"))], None, tn("boolean"));
	let method = || fn_type(vec![param("self", tn("T"))], None, pack(vec![]));
	run_type_cases(vec![
		("union", union(vec![tn("A"), tn("B"), tn("C")]), "A | B | C"),
		("union of optional", union(vec![optional(tn("A")), tn("B")]), "A? | B"),
		(
			"union of function",
			union(vec![predicate(), tn("nil")]),
			"((a: number) -> boolean) | nil",
		),
		(
			"union of intersection",
			union(vec![intersection(vec![tn("A"), tn("B")]), tn("C")]),
			"(A & B) | C",
		),
		(
			"union of union",
			union(vec![union(vec![tn("A"), tn("B")]), tn("C")]),
			"(A | B) | C",
		),
		(
			"union of tables",
			union(vec![
				table(vec![prop("ok", bool_singleton(true))]),
				table(vec![prop("ok", bool_singleton(false))]),
			]),
			"{ ok: true } | { ok: false }",
		),
		("intersection", intersection(vec![tn("A"), tn("B")]), "A & B"),
		(
			"intersection of optional",
			intersection(vec![optional(tn("A")), tn("B")]),
			"(A?) & B",
		),
		(
			"intersection of union",
			intersection(vec![union(vec![tn("A"), tn("B")]), tn("C")]),
			"(A | B) & C",
		),
		(
			"intersection of intersection",
			intersection(vec![intersection(vec![tn("A"), tn("B")]), tn("C")]),
			"(A & B) & C",
		),
		(
			"intersection of function",
			intersection(vec![method(), table(vec![prop("x", tn("number"))])]),
			"((self: T) -> ()) & { x: number }",
		),
		(
			"intersection of tables",
			intersection(vec![
				table(vec![prop("x", tn("number"))]),
				table(vec![prop("y", tn("number"))]),
			]),
			"{ x: number } & { y: number }",
		),
		(
			"overloads",
			intersection(vec![predicate(), method()]),
			"((a: number) -> boolean) & ((self: T) -> ())",
		),
	]);
}

#[test]
fn render_type_tables() {
	let read_array = N::from_fn(|a| {
		let element = a.type_name("number");
		a.alloc(TypeArray {
			access: TypeAccess::Read,
			element,
		})
	});
	run_type_cases(vec![
		("empty", table(vec![]), "{}"),
		(
			"props",
			table(vec![prop("a", tn("number")), prop("b", optional(tn("string")))]),
			"{ a: number, b: string? }",
		),
		(
			"quoted key",
			table(vec![prop("kebab-case", tn("number"))]),
			"{ [\"kebab-case\"]: number }",
		),
		(
			"keyword key",
			table(vec![prop("end", tn("boolean"))]),
			"{ [\"end\"]: boolean }",
		),
		(
			"escaped key",
			table(vec![prop("say \"hi\"", tn("number"))]),
			r#"{ ["say \"hi\""]: number }"#,
		),
		(
			"read write",
			table(vec![
				prop_with(TypeAccess::Read, "x", tn("number")),
				prop_with(TypeAccess::Write, "y", tn("string")),
			]),
			"{ read x: number, write y: string }",
		),
		(
			"read prop named read",
			table(vec![prop_with(TypeAccess::Read, "read", tn("number"))]),
			"{ read read: number }",
		),
		(
			"indexer",
			table(vec![super::dsl::indexer(TypeAccess::None, tn("string"), tn("number"))]),
			"{ [string]: number }",
		),
		(
			"read indexer",
			table(vec![super::dsl::indexer(TypeAccess::Read, tn("string"), tn("number"))]),
			"{ read [string]: number }",
		),
		(
			"indexer and prop",
			table(vec![
				super::dsl::indexer(TypeAccess::None, tn("number"), tn("string")),
				prop("n", tn("number")),
			]),
			"{ [number]: string, n: number }",
		),
		(
			"union key",
			table(vec![super::dsl::indexer(
				TypeAccess::None,
				union(vec![tn("A"), tn("B")]),
				tn("number"),
			)]),
			"{ [A | B]: number }",
		),
		(
			"function field",
			table(vec![prop(
				"f",
				fn_type(vec![param("self", tn("Foo"))], None, tn("number")),
			)]),
			"{ f: (self: Foo) -> number }",
		),
		(
			"union field",
			table(vec![prop("u", union(vec![tn("A"), tn("B")]))]),
			"{ u: A | B }",
		),
		("array", type_array(tn("number")), "{ number }"),
		("nested array", type_array(type_array(tn("number"))), "{ { number } }"),
		("read array", read_array, "{ read number }"),
		("array of union", type_array(union(vec![tn("A"), tn("B")])), "{ A | B }"),
		(
			"array of function",
			type_array(fn_type(vec![], None, pack(vec![]))),
			"{ () -> () }",
		),
	]);
}

#[test]
#[expect(clippy::too_many_lines, reason = "one entry per Go case")]
fn render_type_functions() {
	let unit = || pack(vec![]);
	run_type_cases(vec![
		("unit", fn_type(vec![], None, unit()), "() -> ()"),
		(
			"unnamed",
			fn_type(
				vec![param("", tn("number")), param("", tn("string"))],
				None,
				tn("boolean"),
			),
			"(number, string) -> boolean",
		),
		(
			"named",
			fn_type(
				vec![param("a", tn("number")), param("b", optional(tn("string")))],
				None,
				tn("boolean"),
			),
			"(a: number, b: string?) -> boolean",
		),
		(
			"mixed names",
			fn_type(vec![param("a", tn("number")), param("", tn("string"))], None, unit()),
			"(a: number, string) -> ()",
		),
		(
			"generic pack",
			fn_type_generic(
				vec![generic("T"), generic_pack("U")],
				vec![param("value", tn("T"))],
				Some(gpack("U")),
				gpack("U"),
			),
			"<T, U...>(value: T, U...) -> U...",
		),
		(
			"variadic",
			fn_type(vec![], Some(variadic(tn("number"))), unit()),
			"(...number) -> ()",
		),
		(
			"variadic return",
			fn_type(vec![], None, variadic(tn("any"))),
			"() -> ...any",
		),
		(
			"curried",
			fn_type(
				vec![param("n", tn("number"))],
				None,
				fn_type(vec![param("s", tn("string"))], None, tn("boolean")),
			),
			"(n: number) -> (s: string) -> boolean",
		),
		("generic pack return", fn_type(vec![], None, gpack("T")), "() -> T..."),
		(
			"pack return",
			fn_type(vec![], None, pack(vec![tn("number"), tn("string")])),
			"() -> (number, string)",
		),
		(
			"pack return variadic tail",
			fn_type(vec![], None, pack(vec![tn("A"), variadic(tn("B"))])),
			"() -> (A, ...B)",
		),
		(
			"pack return generic tail",
			fn_type(vec![], None, pack(vec![tn("A"), gpack("U")])),
			"() -> (A, U...)",
		),
		(
			"param and variadic",
			fn_type(
				vec![param("x", tn("number"))],
				Some(variadic(tn("string"))),
				variadic(tn("any")),
			),
			"(x: number, ...string) -> ...any",
		),
		(
			"generic function param",
			fn_type_generic(
				vec![generic("A")],
				vec![param("", fn_type(vec![param("", tn("A"))], None, unit()))],
				None,
				unit(),
			),
			"<A>((A) -> ()) -> ()",
		),
		(
			"union return",
			fn_type(vec![], None, union(vec![tn("A"), tn("B")])),
			"() -> (A | B)",
		),
		(
			"intersection return",
			fn_type(vec![], None, intersection(vec![tn("A"), tn("B")])),
			"() -> (A & B)",
		),
		("optional return", fn_type(vec![], None, optional(tn("A"))), "() -> A?"),
		(
			"optional union return",
			fn_type(vec![], None, optional(union(vec![tn("A"), tn("B")]))),
			"() -> (A | B)?",
		),
		(
			"optional function return",
			fn_type(vec![], None, optional(fn_type(vec![], None, tn("T")))),
			"() -> (() -> T)?",
		),
		(
			"union param",
			fn_type(vec![param("a", union(vec![tn("A"), tn("B")]))], None, unit()),
			"(a: A | B) -> ()",
		),
		(
			"variadic union",
			fn_type(vec![], Some(variadic(union(vec![tn("A"), tn("B")]))), unit()),
			"(...(A | B)) -> ()",
		),
		(
			"variadic intersection",
			fn_type(vec![], Some(variadic(intersection(vec![tn("A"), tn("B")]))), unit()),
			"(...(A & B)) -> ()",
		),
		(
			"variadic optional",
			fn_type(vec![], Some(variadic(optional(tn("A")))), unit()),
			"(...A?) -> ()",
		),
		(
			"variadic function",
			fn_type(vec![], Some(variadic(fn_type(vec![], None, unit()))), unit()),
			"(...() -> ()) -> ()",
		),
	]);
}

#[test]
fn render_type_typeof() {
	let setmetatable = || call(id("setmetatable"), vec![map(vec![]), id("Foo")]);
	run_type_cases(vec![
		("call", typeof_(setmetatable()), "typeof(setmetatable({}, Foo))"),
		("binary", typeof_(bin(id("a"), B::Add, id("b"))), "typeof(a + b)"),
		("cast", typeof_(cast(id("x"), tn("Foo"))), "typeof(x :: Foo)"),
	]);
}

#[test]
fn render_cast_expression() {
	let setmetatable = || call(id("setmetatable"), vec![map(vec![]), id("Foo")]);
	let cases: Vec<(&str, N, &str)> = vec![
		("cast", cast(id("x"), tn("number")), "x :: number"),
		(
			"double cast",
			cast(cast(setmetatable(), tn("any")), tn("Foo")),
			"(setmetatable({}, Foo) :: any) :: Foo",
		),
		(
			"binary operand",
			cast(bin(id("x"), B::Add, id("y")), tn("number")),
			"(x + y) :: number",
		),
		(
			"unary operand",
			cast(un(U::Not, id("x")), tn("boolean")),
			"(not x) :: boolean",
		),
		(
			"if operand",
			cast(if_expr(id("c"), id("x"), id("y")), tn("T")),
			"(if c then x else y) :: T",
		),
		(
			"left of less than",
			bin(cast(id("x"), tn("T")), B::Less, id("y")),
			"(x :: T) < y",
		),
		(
			"right of less than",
			bin(id("x"), B::Less, cast(id("y"), tn("T"))),
			"x < y :: T",
		),
		(
			"left of plus",
			bin(cast(id("x"), tn("number")), B::Add, num(1.0)),
			"x :: number + 1",
		),
		(
			"left of less equal",
			bin(cast(id("x"), tn("number")), B::LessEqual, num(1.0)),
			"x :: number <= 1",
		),
		(
			"under unary",
			un(U::Negate, cast(id("x"), tn("number"))),
			"-x :: number",
		),
		("union type", cast(id("x"), union(vec![tn("A"), tn("B")])), "x :: A | B"),
		("optional type", cast(id("x"), optional(tn("T"))), "x :: T?"),
		(
			"function type",
			cast(id("x"), fn_type(vec![], None, pack(vec![]))),
			"x :: () -> ()",
		),
		(
			"parenthesized then indexed",
			property(paren(cast(id("x"), tn("T"))), "y"),
			"(x :: T).y",
		),
		("argument", call(id("f"), vec![cast(id("x"), tn("T"))]), "f(x :: T)"),
		("table member", array(vec![cast(id("x"), tn("T"))]), "{ x :: T }"),
	];
	let mut a = Ast::new();
	for (name, node, want) in cases {
		let node = node.build(&mut a);
		assert_eq!(render_expr(&mut a, node), want, "{name}");
	}
}

#[test]
fn render_cast_statement_ending() {
	// A cast never ends with an indexable expression, so no `;` is needed
	// before a statement that starts with `(`.
	let mut a = Ast::new();
	let x = a.id("a");
	let value = cast(id("b"), tn("T")).build(&mut a);
	let first = a.variable_declaration(x, Some(value.into()));
	let target = property(paren(bin(id("c"), B::Add, id("d"))), "e").build(&mut a);
	let f = a.id("f");
	let second = a.assignment(target, crate::AssignmentOperator::Assign, f);
	let ast = stmts(&mut a, &[first, second]);
	assert_eq!(render_both(&a, ast), "local a = b :: T\n(c + d).e = f\n");
}

fn type_function_declaration(name: &'static str, params: Vec<&'static str>, dots: bool, body: Vec<N>) -> N {
	N::from_fn(move |a| {
		let params: Vec<NodeId> = params.into_iter().map(|param| a.id(param)).collect();
		let params = ids(a, &params);
		let body: Vec<NodeId> = body.into_iter().map(|statement| statement.build(a)).collect();
		let body = stmts(a, &body);
		a.type_function_declaration(name, params, dots, body)
	})
}

fn ret(value: N) -> N {
	N::from_fn(move |a| {
		let value = value.build(a);
		a.return_statement(value)
	})
}

fn local(name: &'static str, value: Option<N>) -> N {
	N::from_fn(move |a| {
		let left = a.id(name);
		let value = value.map(|value| value.build(a).into());
		a.variable_declaration(left, value)
	})
}

fn local_function(name: &'static str, body: Vec<N>) -> N {
	N::from_fn(move |a| {
		let name = a.id(name);
		let params = a.new_list::<element::AnyIdentifier>();
		let body: Vec<NodeId> = body.into_iter().map(|statement| statement.build(a)).collect();
		let body = stmts(a, &body);
		a.function_declaration(true, name, params, false, body)
	})
}

fn run_statement_cases(cases: Vec<(&str, Vec<N>, &str)>) {
	let mut a = Ast::new();
	for (name, statements, want) in cases {
		let statements: Vec<NodeId> = statements
			.into_iter()
			.map(|statement| statement.build(&mut a))
			.collect();
		let ast = stmts(&mut a, &statements);
		assert_eq!(render_both(&a, ast), want, "{name}");
	}
}

#[test]
#[expect(clippy::too_many_lines, reason = "one entry per Go case")]
fn render_type_statements() {
	let set_body = N::from_fn(|a| {
		let generics = build_generics(a, vec![generic("a")]);
		let declaration = a.type_alias("Tree", generics, None);
		let body = table(vec![
			prop("data", tn("a")),
			prop("children", type_array(type_ref("Tree", vec![tn("a")]))),
		])
		.build(a);
		a.set_type_alias_body(declaration, body);
		declaration
	});
	run_statement_cases(vec![
		(
			"structured alias",
			vec![alias(
				"Node",
				vec![generic("K"), generic("V")],
				Some(table(vec![prop("key", tn("K")), prop("value", tn("V"))])),
			)],
			"type Node<K, V> = { key: K, value: V }\n",
		),
		(
			"export",
			vec![exported(alias("Foo", vec![], Some(tn("number"))))],
			"export type Foo = number\n",
		),
		(
			"defaults",
			vec![exported(alias(
				"Fn",
				vec![
					generic("T").default(tn("number")),
					generic_pack("U").default(variadic(tn("string"))),
				],
				Some(fn_type(vec![param("", tn("T"))], Some(gpack("U")), pack(vec![]))),
			))],
			"export type Fn<T = number, U... = ...string> = (T, U...) -> ()\n",
		),
		(
			"default referencing earlier parameter",
			vec![alias(
				"Pair",
				vec![generic("T"), generic("U").default(tn("T"))],
				Some(table(vec![prop("first", tn("T")), prop("second", tn("U"))])),
			)],
			"type Pair<T, U = T> = { first: T, second: U }\n",
		),
		(
			"empty pack default",
			vec![alias(
				"Returns",
				vec![generic_pack("T").default(pack(vec![]))],
				Some(fn_type(vec![], None, gpack("T"))),
			)],
			"type Returns<T... = ()> = () -> T...\n",
		),
		(
			"generic packs",
			vec![alias(
				"Callback",
				vec![generic_pack("Args"), generic_pack("Rets")],
				Some(table(vec![prop(
					"f",
					fn_type(vec![], Some(gpack("Args")), gpack("Rets")),
				)])),
			)],
			"type Callback<Args..., Rets...> = { f: (Args...) -> Rets... }\n",
		),
		(
			"set body",
			vec![set_body],
			"type Tree<a> = { data: a, children: { Tree<a> } }\n",
		),
		(
			"type function",
			vec![exported(type_function_declaration(
				"Keys",
				vec!["t"],
				false,
				vec![local("result", Some(map(vec![]))), ret(id("result"))],
			))],
			"export type function Keys(t)\n\tlocal result = {}\n\treturn result\nend\n",
		),
		(
			"type function with varargs",
			vec![type_function_declaration(
				"Partial",
				vec!["T"],
				true,
				vec![ret(id("T"))],
			)],
			"type function Partial(T, ...)\n\treturn T\nend\n",
		),
		(
			"alias inside a function body",
			vec![local_function(
				"f",
				vec![alias("Local", vec![], Some(tn("number"))), ret(nil())],
			)],
			"local function f()\n\ttype Local = number\n\treturn nil\nend\n",
		),
		(
			"type function inside a do block",
			vec![N::from_fn(|a| {
				let declaration = type_function_declaration("F", vec![], false, vec![ret(id("types"))]).build(a);
				let body = stmts(a, &[declaration]);
				a.do_statement(body)
			})],
			"do\n\ttype function F()\n\t\treturn types\n\tend\nend\n",
		),
	]);
}

fn annotated(name: &'static str, ty: N) -> N {
	N::from_fn(move |a| {
		let identifier = a.id(name);
		let ty = ty.build(a);
		a.set_binding_type(identifier, Some(ty));
		identifier
	})
}

/// A function declaration with an empty body, then `edit` applied.
fn declaration(
	localize: bool,
	params: Vec<N>,
	dots: bool,
	edit: impl FnOnce(&mut Ast, &mut FunctionDeclaration) + 'static,
) -> N {
	N::from_fn(move |a| {
		let name = a.id("f");
		let params: Vec<NodeId> = params.into_iter().map(|param| param.build(a)).collect();
		let params = ids(a, &params);
		let body = a.new_list::<element::Statement>();
		let node = a.function_declaration(localize, name, params, dots, body);
		let mut payload = payload::<FunctionDeclaration>(a, node).clone();
		edit(a, &mut payload);
		*payload_mut::<FunctionDeclaration>(a, node) = payload;
		node
	})
}

#[test]
#[expect(clippy::too_many_lines, reason = "one entry per Go case")]
fn render_typed_declarations() {
	run_statement_cases(vec![
		(
			"generic function with typed varargs",
			vec![declaration(
				true,
				vec![annotated("a", tn("K"))],
				true,
				|a, declaration| {
					declaration.generics = vec![TypeGeneric::new("K"), TypeGeneric::new("V")];
					declaration.var_args_type = Some(a.type_name("V"));
					declaration.return_type = Some(a.type_name("V"));
				},
			)],
			"local function f<K, V>(a: K, ...: V): V\nend\n",
		),
		(
			"generic pack varargs",
			vec![declaration(false, vec![], true, |a, declaration| {
				declaration.generics = vec![TypeGeneric::pack("T")];
				declaration.var_args_type = Some(a.type_generic_pack("T"));
			})],
			"function f<T...>(...: T...)\nend\n",
		),
		(
			"untyped varargs",
			vec![declaration(true, vec![id("a")], true, |_, _| {})],
			"local function f(a, ...)\nend\n",
		),
		(
			"annotated locals",
			vec![N::from_fn(|a| {
				let x = annotated("a", tn("number")).build(a);
				let y = annotated("b", optional(tn("string"))).build(a);
				let left = ids(a, &[x, y]);
				let (one, nil) = (a.num(1.0), a.nil());
				let right = super::exprs(a, &[one, nil]);
				a.variable_declaration(left, Some(right.into()))
			})],
			"local a: number, b: string? = 1, nil\n",
		),
		(
			"annotated local without value",
			vec![N::from_fn(|a| {
				let x = annotated("x", type_array(tn("number"))).build(a);
				a.variable_declaration(x, None)
			})],
			"local x: { number }\n",
		),
		(
			"annotated temp local",
			vec![N::from_fn(|a| {
				let temp = a.temp_id("binding");
				let ty = a.type_name("Foo");
				a.set_binding_type(temp, Some(ty));
				let v = a.id("v");
				a.variable_declaration(temp, Some(v.into()))
			})],
			"local _binding: Foo = v\n",
		),
		(
			"mixed annotated list",
			vec![N::from_fn(|a| {
				let x = a.id("a");
				let y = annotated("b", tn("number")).build(a);
				let left = ids(a, &[x, y]);
				let (one, two) = (a.num(1.0), a.num(2.0));
				let right = super::exprs(a, &[one, two]);
				a.variable_declaration(left, Some(right.into()))
			})],
			"local a, b: number = 1, 2\n",
		),
		(
			"assignment never renders annotations",
			vec![N::from_fn(|a| {
				let x = annotated("x", tn("number")).build(a);
				let one = a.num(1.0);
				a.assignment(x, crate::AssignmentOperator::Assign, one)
			})],
			"x = 1\n",
		),
		(
			"uses never render annotations",
			vec![N::from_fn(|a| {
				let x = annotated("x", tn("number")).build(a);
				let sum = a.binary(x, B::Add, x);
				a.return_statement(sum)
			})],
			"return x + x\n",
		),
		(
			"function expression",
			vec![N::from_fn(|a| {
				let x = annotated("x", tn("T")).build(a);
				let params = ids(a, &[x]);
				let returned = ret(id("x")).build(a);
				let body = stmts(a, &[returned]);
				let expression = a.function_expression(params, false, body);
				let return_type = a.type_name("T");
				let payload = payload_mut::<FunctionExpression>(a, expression);
				payload.generics = vec![TypeGeneric::new("T")];
				payload.return_type = Some(return_type);
				let f = a.id("f");
				a.variable_declaration(f, Some(expression.into()))
			})],
			"local f = function<T>(x: T): T\n\treturn x\nend\n",
		),
		(
			"empty function expression with annotated temp",
			vec![N::from_fn(|a| {
				let temp = a.temp_id("param");
				let ty = a.type_name("Foo");
				a.set_binding_type(temp, Some(ty));
				let (params, body) = (ids(a, &[temp]), a.new_list::<element::Statement>());
				let expression = a.function_expression(params, false, body);
				let f = a.id("f");
				a.variable_declaration(f, Some(expression.into()))
			})],
			"local f = function(_param: Foo) end\n",
		),
		(
			"method",
			vec![N::from_fn(|a| {
				let class = a.id("Class");
				let x = annotated("x", tn("number")).build(a);
				let params = ids(a, &[x]);
				let returned = ret(id("x")).build(a);
				let body = stmts(a, &[returned]);
				let method = a.method_declaration(class, "method", params, false, body);
				let return_type = optional(tn("number")).build(a);
				payload_mut::<MethodDeclaration>(a, method).return_type = Some(return_type);
				method
			})],
			"function Class:method(x: number): number?\n\treturn x\nend\n",
		),
		(
			"declared union return stays bare",
			vec![declaration(true, vec![], false, |a, declaration| {
				declaration.return_type =
					Some(union(vec![type_array(tn("string")), type_array(tn("number"))]).build(a));
			})],
			"local function f(): { string } | { number }\nend\n",
		),
		(
			"declared pack return",
			vec![declaration(true, vec![], false, |a, declaration| {
				declaration.return_type = Some(pack(vec![tn("number"), tn("string")]).build(a));
			})],
			"local function f(): (number, string)\nend\n",
		),
		(
			"declared function return",
			vec![declaration(true, vec![], false, |a, declaration| {
				declaration.return_type = Some(fn_type(vec![param("a", tn("number"))], None, tn("boolean")).build(a));
			})],
			"local function f(): (a: number) -> boolean\nend\n",
		),
		(
			"declared variadic return",
			vec![declaration(true, vec![], false, |a, declaration| {
				declaration.return_type = Some(variadic(tn("any")).build(a));
			})],
			"local function f(): ...any\nend\n",
		),
		(
			"cast at construction",
			vec![local(
				"self",
				Some(cast(
					cast(call(id("setmetatable"), vec![map(vec![]), id("Foo")]), tn("any")),
					tn("Foo"),
				)),
			)],
			"local self = (setmetatable({}, Foo) :: any) :: Foo\n",
		),
	]);
}

#[test]
fn render_multiline_types() {
	let ok_true = || table(vec![prop("ok", bool_singleton(true))]);
	let ok_false = || table(vec![prop("ok", bool_singleton(false))]);
	let multiline_ok_true = || multiline_table(vec![prop("ok", bool_singleton(true))]);
	run_statement_cases(vec![
		(
			"table",
			vec![alias(
				"T",
				vec![],
				Some(multiline_table(vec![prop("a", tn("number")), prop("b", tn("string"))])),
			)],
			"type T = {\n\ta: number,\n\tb: string,\n}\n",
		),
		(
			"empty table never breaks",
			vec![alias("T", vec![], Some(multiline_table(vec![])))],
			"type T = {}\n",
		),
		(
			"nested tables",
			vec![alias(
				"T",
				vec![],
				Some(multiline_table(vec![
					prop("inner", multiline_table(vec![prop("a", tn("number"))])),
					prop("flat", table(vec![prop("b", tn("number"))])),
				])),
			)],
			"type T = {\n\tinner: {\n\t\ta: number,\n\t},\n\tflat: { b: number },\n}\n",
		),
		(
			"table inside a function body",
			vec![local_function(
				"f",
				vec![alias("T", vec![], Some(multiline_table(vec![prop("a", tn("number"))])))],
			)],
			"local function f()\n\ttype T = {\n\t\ta: number,\n\t}\nend\n",
		),
		(
			"table in a local annotation",
			vec![N::from_fn(|a| {
				let x = annotated("x", multiline_table(vec![prop("a", tn("number"))])).build(a);
				let v = a.id("v");
				a.variable_declaration(x, Some(v.into()))
			})],
			"local x: {\n\ta: number,\n} = v\n",
		),
		(
			"table in a single-line union",
			vec![alias(
				"T",
				vec![],
				Some(union(vec![multiline_table(vec![prop("a", tn("number"))]), tn("nil")])),
			)],
			"type T = {\n\ta: number,\n} | nil\n",
		),
		(
			"union alias body",
			vec![alias("R", vec![], Some(multiline_union(vec![ok_true(), ok_false()])))],
			"type R =\n\t| { ok: true }\n\t| { ok: false }\n",
		),
		(
			"exported union alias body",
			vec![exported(alias(
				"R",
				vec![generic("T")],
				Some(multiline_union(vec![tn("T"), tn("nil")])),
			))],
			"export type R<T> =\n\t| T\n\t| nil\n",
		),
		(
			"union member parenthesization survives",
			vec![alias(
				"R",
				vec![],
				Some(multiline_union(vec![
					fn_type(vec![], None, pack(vec![])),
					optional(tn("A")),
				])),
			)],
			"type R =\n\t| (() -> ())\n\t| A?\n",
		),
		(
			"union of multiline tables",
			vec![alias(
				"R",
				vec![],
				Some(multiline_union(vec![multiline_ok_true(), ok_false()])),
			)],
			"type R =\n\t| {\n\t\tok: true,\n\t}\n\t| { ok: false }\n",
		),
		(
			"union alias inside a function body",
			vec![local_function(
				"f",
				vec![alias("R", vec![], Some(multiline_union(vec![ok_true(), ok_false()])))],
			)],
			"local function f()\n\ttype R =\n\t\t| { ok: true }\n\t\t| { ok: false }\nend\n",
		),
	]);
}

fn alias_statements(a: &mut Ast, name: &'static str, body: N) -> crate::List<element::Statement> {
	let declaration = alias(name, vec![], Some(body)).build(a);
	stmts(a, &[declaration])
}

#[test]
#[expect(clippy::too_many_lines, reason = "one block per Go subtest")]
fn layout_types_cases() {
	// 24 columns.
	let flat = || table(vec![prop("a", tn("number")), prop("b", tn("string"))]);
	{
		// Table threshold.
		let mut a = Ast::new();
		let ast = alias_statements(&mut a, "T", flat());
		layout_types(&mut a, ast, 24).expect("layout");
		assert_eq!(render_both(&a, ast), "type T = { a: number, b: string }\n", "width 24");
		layout_types(&mut a, ast, 23).expect("layout");
		assert_eq!(
			render_both(&a, ast),
			"type T = {\n\ta: number,\n\tb: string,\n}\n",
			"width 23"
		);
		// Widening again clears the flag: the pass is a function of width.
		layout_types(&mut a, ast, DEFAULT_TYPE_LINE_WIDTH).expect("layout");
		assert_eq!(
			render_both(&a, ast),
			"type T = { a: number, b: string }\n",
			"default width"
		);
	}
	{
		// Children before parents; 35 columns.
		let mut a = Ast::new();
		let ast = alias_statements(&mut a, "T", table(vec![prop("inner", flat())]));
		layout_types(&mut a, ast, 30).expect("layout");
		assert_eq!(
			render_both(&a, ast),
			"type T = {\n\tinner: { a: number, b: string },\n}\n",
			"width 30"
		);
		layout_types(&mut a, ast, 20).expect("layout");
		assert_eq!(
			render_both(&a, ast),
			"type T = {\n\tinner: {\n\t\ta: number,\n\t\tb: string,\n\t},\n}\n",
			"width 20"
		);
	}
	{
		// Alias body union; 28 columns.
		let mut a = Ast::new();
		let body = union(vec![
			table(vec![prop("ok", bool_singleton(true))]),
			table(vec![prop("ok", bool_singleton(false))]),
		]);
		let ast = alias_statements(&mut a, "R", body);
		layout_types(&mut a, ast, 28).expect("layout");
		assert_eq!(
			render_both(&a, ast),
			"type R = { ok: true } | { ok: false }\n",
			"width 28"
		);
		layout_types(&mut a, ast, 27).expect("layout");
		assert_eq!(
			render_both(&a, ast),
			"type R =\n\t| { ok: true }\n\t| { ok: false }\n",
			"width 27"
		);
		layout_types(&mut a, ast, 10).expect("layout");
		assert_eq!(
			render_both(&a, ast),
			"type R =\n\t| {\n\t\tok: true,\n\t}\n\t| {\n\t\tok: false,\n\t}\n",
			"width 10"
		);
	}
	{
		// A union outside an alias body stays single-line.
		let mut a = Ast::new();
		let x = annotated("x", union(vec![tn("A"), table(vec![prop("a", tn("number"))])])).build(&mut a);
		let v = a.id("v");
		let declaration = a.variable_declaration(x, Some(v.into()));
		let ast = stmts(&mut a, &[declaration]);
		layout_types(&mut a, ast, 1).expect("layout");
		assert_eq!(render_both(&a, ast), "local x: A | {\n\ta: number,\n} = v\n");
	}
	{
		// A nested union is not an alias body.
		let mut a = Ast::new();
		let ast = alias_statements(&mut a, "T", table(vec![prop("u", union(vec![tn("A"), tn("B")]))]));
		layout_types(&mut a, ast, 1).expect("layout");
		assert_eq!(render_both(&a, ast), "type T = {\n\tu: A | B,\n}\n");
	}
	{
		// Types in every annotation slot.
		let mut a = Ast::new();
		let p = annotated("a", table(vec![prop("p", tn("number"))])).build(&mut a);
		let returned = ret(cast(id("v"), table(vec![prop("c", tn("number"))]))).build(&mut a);
		let (name, params, body) = (a.id("f"), ids(&mut a, &[p]), stmts(&mut a, &[returned]));
		let declaration = a.function_declaration(true, name, params, true, body);
		let var_args = table(vec![prop("v", tn("number"))]).build(&mut a);
		let return_type = table(vec![prop("r", tn("number"))]).build(&mut a);
		let payload = payload_mut::<FunctionDeclaration>(&mut a, declaration);
		payload.var_args_type = Some(var_args);
		payload.return_type = Some(return_type);
		let ast = stmts(&mut a, &[declaration]);
		layout_types(&mut a, ast, 1).expect("layout");
		let want = "local function f(a: {\n\tp: number,\n}, ...: {\n\tv: number,\n}): {\n\tr: number,\n}\n\
			\treturn v :: {\n\t\tc: number,\n\t}\nend\n";
		assert_eq!(render_both(&a, ast), want);
	}
	{
		// Width zero clears flags.
		let mut a = Ast::new();
		let ast = alias_statements(
			&mut a,
			"T",
			multiline_table(vec![prop("a", tn("number")), prop("b", tn("string"))]),
		);
		layout_types(&mut a, ast, 0).expect("layout");
		assert_eq!(render_both(&a, ast), "type T = { a: number, b: string }\n");
	}
	{
		// An empty table never breaks.
		let mut a = Ast::new();
		let ast = alias_statements(&mut a, "T", table(vec![]));
		layout_types(&mut a, ast, 1).expect("layout");
		assert_eq!(render_both(&a, ast), "type T = {}\n");
	}
	{
		// Width counts characters: 12 characters, 15 bytes.
		let mut a = Ast::new();
		let ast = alias_statements(&mut a, "T", table(vec![prop("e", string_singleton("😀"))]));
		layout_types(&mut a, ast, 12).expect("layout");
		assert_eq!(render_both(&a, ast), "type T = { e: \"😀\" }\n");
	}
	{
		// render_ast never breaks on its own.
		let mut a = Ast::new();
		let names = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi";
		let fields = names
			.split_whitespace()
			.map(|name| prop(Box::leak(name.to_owned().into_boxed_str()), tn("number")))
			.collect();
		let ast = alias_statements(&mut a, "Wide", table(fields));
		let got = render_both(&a, ast);
		assert!(
			got.matches('\n').count() == 1 && got.len() >= 150,
			"wide alias must stay on one line: {got:?}"
		);
	}
	{
		// Temporaries inside typeof.
		let mut a = Ast::new();
		let temp = a.temp_id("instance");
		let v = a.id("v");
		let declaration = a.variable_declaration(temp, Some(v.into()));
		let type_alias = alias("T", vec![], Some(table(vec![prop("self", typeof_(temp))]))).build(&mut a);
		let ast = stmts(&mut a, &[declaration, type_alias]);
		layout_types(&mut a, ast, 1).expect("layout");
		assert_eq!(
			render_both(&a, ast),
			"local _instance = v\ntype T = {\n\tself: typeof(_instance),\n}\n"
		);
	}
}

#[test]
#[expect(clippy::too_many_lines, reason = "one entry per Go case")]
fn render_type_panics() {
	let raw_optional_nil = N::from_fn(|a| a.alloc(TypeOptional { inner: None }));
	let raw_function_without_return = N::from_fn(|a| {
		a.alloc(TypeFunction {
			generics: Vec::new(),
			parameters: Vec::new(),
			var_args: None,
			ret: None,
		})
	});
	let default_on_function_type = N::from_fn(|a| {
		let generics = build_generics(a, vec![generic("T").default(tn("number"))]);
		let empty = a.new_list::<element::TypeNode>();
		let ret = a.type_pack(empty);
		a.type_function(generics, Vec::new(), None, ret)
	});
	let cases: Vec<(&str, N)> = vec![
		("invalid type name", alias("T", vec![], Some(tn("not valid")))),
		("keyword type name", alias("T", vec![], Some(tn("end")))),
		("typeof as a name", alias("T", vec![], Some(tn("typeof")))),
		(
			"invalid module",
			alias("T", vec![], Some(qualified("not valid", "T", None))),
		),
		("nil type child", alias("T", vec![], Some(raw_optional_nil))),
		("optional pack", alias("T", vec![], Some(optional(pack(vec![tn("A")]))))),
		(
			"optional variadic",
			alias("T", vec![], Some(optional(variadic(tn("A"))))),
		),
		("empty union", alias("T", vec![], Some(union(vec![])))),
		("empty intersection", alias("T", vec![], Some(intersection(vec![])))),
		(
			"two indexers",
			alias(
				"T",
				vec![],
				Some(table(vec![
					super::dsl::indexer(TypeAccess::None, tn("string"), tn("number")),
					super::dsl::indexer(TypeAccess::None, tn("number"), tn("number")),
				])),
			),
		),
		(
			"function without return",
			alias("T", vec![], Some(raw_function_without_return)),
		),
		(
			"invalid parameter name",
			alias(
				"T",
				vec![],
				Some(fn_type(vec![param("not valid", tn("A"))], None, pack(vec![]))),
			),
		),
		(
			"varargs that is not a pack",
			alias("T", vec![], Some(fn_type(vec![], Some(tn("A")), pack(vec![])))),
		),
		(
			"varargs explicit pack",
			alias(
				"T",
				vec![],
				Some(fn_type(vec![], Some(pack(vec![tn("A")])), pack(vec![]))),
			),
		),
		(
			"pack inside a pack",
			alias(
				"T",
				vec![],
				Some(fn_type(vec![], None, pack(vec![pack(vec![tn("A")])]))),
			),
		),
		(
			"variadic not last",
			alias(
				"T",
				vec![],
				Some(fn_type(vec![], None, pack(vec![variadic(tn("A")), tn("B")]))),
			),
		),
		(
			"generic pack not last",
			alias(
				"T",
				vec![],
				Some(fn_type(vec![], None, pack(vec![gpack("U"), tn("B")]))),
			),
		),
		("variadic of a pack", alias("T", vec![], Some(variadic(gpack("T"))))),
		(
			"invalid generic pack name",
			alias("T", vec![], Some(gpack("not valid"))),
		),
		(
			"generic before pack",
			alias("T", vec![generic_pack("U"), generic("T")], Some(tn("T"))),
		),
		(
			"invalid generic name",
			alias("T", vec![generic("not valid")], Some(tn("T"))),
		),
		(
			"default on a function type",
			alias("T", vec![], Some(default_on_function_type)),
		),
		(
			"default on a function declaration",
			declaration(true, vec![], false, |a, declaration| {
				let default = a.type_name("number");
				declaration.generics = vec![TypeGeneric::new("T").with_default(default)];
			}),
		),
		(
			"pack default on a type parameter",
			alias("T", vec![generic("T").default(pack(vec![]))], Some(tn("T"))),
		),
		(
			"type default on a pack parameter",
			alias("T", vec![generic_pack("T").default(tn("number"))], Some(tn("T"))),
		),
		(
			"missing default after a defaulted parameter",
			alias(
				"T",
				vec![generic("T").default(tn("number")), generic("U")],
				Some(tn("T")),
			),
		),
		("alias without body", alias("T", vec![], None)),
		(
			"alias with a pack body",
			alias("T", vec![], Some(pack(vec![tn("A"), tn("B")]))),
		),
		("invalid alias name", alias("not valid", vec![], Some(tn("A")))),
		(
			"invalid type function name",
			type_function_declaration("not valid", vec![], false, vec![]),
		),
	];
	let mut a = Ast::new();
	for (name, statement) in cases {
		let statement = statement.build(&mut a);
		let ast = stmts(&mut a, &[statement]);
		assert!(render_ast(&a, ast).is_err(), "{name}: must fail");
	}
}

#[test]
#[expect(clippy::too_many_lines, reason = "one entry per Go case")]
fn type_positions() {
	let mut a = Ast::new();
	let mut cases = Vec::new();
	{
		let x = a.id("x");
		let number = a.type_name("number");
		let optional = a.type_optional(number);
		a.set_binding_type(x, Some(optional));
		let one = a.num(1.0);
		let declaration = a.variable_declaration(x, Some(one.into()));
		let ast = stmts(&mut a, &[declaration]);
		cases.push(PositionCase {
			name: "annotated local",
			ast,
			text: "local x: number? = 1\n",
			ranges: vec![
				node_range(declaration, 0, 0, 0, 20),
				node_range(x, 0, 6, 0, 7),
				node_range(optional, 0, 9, 0, 16),
				node_range(number, 0, 9, 0, 15),
				node_range(one, 0, 19, 0, 20),
			],
		});
	}
	{
		let (x, y) = (a.type_name("A"), a.type_name("B"));
		let members = a.list_from::<element::TypeNode>([x, y]);
		let inner = a.type_union(members);
		let optional = a.type_optional(inner);
		let declaration = a.type_alias("T", Vec::new(), Some(optional));
		let ast = stmts(&mut a, &[declaration]);
		cases.push(PositionCase {
			name: "parentheses belong to the parenthesized type",
			ast,
			text: "type T = (A | B)?\n",
			ranges: vec![
				node_range(declaration, 0, 0, 0, 17),
				node_range(optional, 0, 9, 0, 17),
				node_range(inner, 0, 9, 0, 16),
				node_range(x, 0, 10, 0, 11),
				node_range(y, 0, 14, 0, 15),
			],
		});
	}
	{
		let (x, number) = (a.id("x"), a.type_name("number"));
		let cast = a.cast(x, number);
		let returned = a.return_statement(cast);
		let ast = stmts(&mut a, &[returned]);
		cases.push(PositionCase {
			name: "cast operator",
			ast,
			text: "return x :: number\n",
			ranges: vec![
				node_range(returned, 0, 0, 0, 18),
				Range {
					operator: Some(crate::Position::new(0, 9)),
					..node_range(cast, 0, 7, 0, 18)
				},
				node_range(x, 0, 7, 0, 8),
				node_range(number, 0, 12, 0, 18),
			],
		});
	}
	{
		let (param, used) = (a.id("t"), a.id("t"));
		let returned = a.return_statement(used);
		let (params, body) = (ids(&mut a, &[param]), stmts(&mut a, &[returned]));
		let declaration = a.type_function_declaration("F", params, false, body);
		let ast = stmts(&mut a, &[declaration]);
		cases.push(PositionCase {
			name: "type function closing",
			ast,
			text: "type function F(t)\n\treturn t\nend\n",
			ranges: vec![
				closed_range(declaration, (0, 0), (2, 3), (2, 0)),
				node_range(param, 0, 16, 0, 17),
				node_range(returned, 1, 1, 1, 9),
				node_range(used, 1, 8, 1, 9),
			],
		});
	}
	{
		let key = a.type_name("K");
		let field = a.type_table_prop(TypeAccess::None, "key", key);
		let fields = a.list_from::<element::TypeTableField>([field]);
		let body = a.type_table(fields);
		payload_mut::<TypeTable>(&mut a, body).multiline = true;
		let declaration = a.type_alias("T", vec![TypeGeneric::new("K")], Some(body));
		let ast = stmts(&mut a, &[declaration]);
		cases.push(PositionCase {
			name: "multiline table field excludes indentation",
			ast,
			text: "type T<K> = {\n\tkey: K,\n}\n",
			ranges: vec![
				node_range(declaration, 0, 0, 2, 1),
				node_range(body, 0, 12, 2, 1),
				node_range(field, 1, 1, 1, 7),
				node_range(key, 1, 6, 1, 7),
			],
		});
	}
	{
		let (x, y) = (a.type_name("A"), a.type_name("B"));
		let members = a.list_from::<element::TypeNode>([x, y]);
		let body = a.type_union(members);
		payload_mut::<TypeUnion>(&mut a, body).multiline = true;
		let declaration = a.type_alias("T", Vec::new(), Some(body));
		let ast = stmts(&mut a, &[declaration]);
		cases.push(PositionCase {
			name: "multiline union starts at the first bar",
			ast,
			text: "type T =\n\t| A\n\t| B\n",
			ranges: vec![
				node_range(declaration, 0, 0, 2, 4),
				node_range(body, 1, 1, 2, 4),
				node_range(x, 1, 3, 1, 4),
				node_range(y, 2, 3, 2, 4),
			],
		});
	}
	check_position_cases(&a, cases);
}
