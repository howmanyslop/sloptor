//! Port of `internal/luau/render/expressions_test.go`.

#![expect(
	clippy::tuple_array_conversions,
	reason = "tests bind sibling nodes in one statement"
)]

use super::dsl::{array, bin, call, id, if_expr, num, paren, property, un};
use super::{exprs, render_expr};
use crate::{Ast, BinaryOperator as B, NodeId, UnaryOperator as U, element};

fn check_cases(a: &mut Ast, cases: Vec<(NodeId, &str)>) {
	for (node, want) in cases {
		assert_eq!(render_expr(a, node), want);
	}
}

#[test]
fn render_literals() {
	let mut a = Ast::new();
	let cases = vec![
		(a.nil(), "nil"),
		(a.bool(true), "true"),
		(a.bool(false), "false"),
		(a.var_args(), "..."),
		(a.num(5.0), "5"),
		(a.number_literal("1_000"), "1_000"),
		(a.number_literal("0xFF"), "0xFF"),
		(a.str("hi"), "\"hi\""),
		(a.str("say \"hi\""), "'say \"hi\"'"),
		(a.str("both \" and '"), "[[both \" and ']]"),
		(a.str("multi\nline"), "[[multi\nline]]"),
	];
	check_cases(&mut a, cases);
}

#[test]
fn render_indexing() {
	let mut a = Ast::new();
	let cases = vec![
		(property(id("a"), "b").build(&mut a), "a.b"),
		(property(id("a"), "not valid").build(&mut a), "a[\"not valid\"]"),
		(
			{
				let (x, b) = (a.id("a"), a.str("b"));
				a.computed_index(x, b)
			},
			"a.b",
		),
		(
			{
				let (x, one) = (a.id("a"), a.num(1.0));
				a.computed_index(x, one)
			},
			"a[1]",
		),
		(call(id("f"), vec![num(1.0), num(2.0)]).build(&mut a), "f(1, 2)"),
		(
			{
				let (object, args) = (a.id("obj"), a.new_list::<element::Expression>());
				a.method_call("m", object, args)
			},
			"obj:m()",
		),
	];
	check_cases(&mut a, cases);
}

#[test]
fn render_operators() {
	let mut a = Ast::new();
	let cases = vec![
		(
			bin(bin(id("a"), B::Add, id("b")), B::Multiply, id("c")).build(&mut a),
			"(a + b) * c",
		),
		(
			bin(id("a"), B::Subtract, bin(id("b"), B::Subtract, id("c"))).build(&mut a),
			"a - (b - c)",
		),
		(un(U::Not, id("a")).build(&mut a), "not a"),
		(un(U::Negate, un(U::Negate, id("a"))).build(&mut a), "- -a"),
		(un(U::Length, id("t")).build(&mut a), "#t"),
		(if_expr(id("c"), num(1.0), num(2.0)).build(&mut a), "if c then 1 else 2"),
	];
	check_cases(&mut a, cases);
}

#[test]
fn render_parenthesized() {
	let mut a = Ast::new();
	// Parentheses around simple expressions are dropped.
	let simple = paren(id("x")).build(&mut a);
	assert_eq!(render_expr(&mut a, simple), "x");
	let sum = paren(bin(id("a"), B::Add, id("b"))).build(&mut a);
	assert_eq!(render_expr(&mut a, sum), "(a + b)");
}

#[test]
fn render_tables() {
	let mut a = Ast::new();
	let empty = array(vec![]).build(&mut a);
	assert_eq!(render_expr(&mut a, empty), "{}", "empty array");
	let pair = array(vec![num(1.0), num(2.0)]).build(&mut a);
	assert_eq!(render_expr(&mut a, pair), "{ 1, 2 }", "array");
	let member = a.str("a");
	let members = exprs(&mut a, &[member]);
	let set = a.set(members);
	assert_eq!(render_expr(&mut a, set), "{\n\t[\"a\"] = true,\n}", "set");
	let (foo, one) = (a.str("foo"), a.num(1.0));
	let first = a.map_field(foo, one);
	let (two, three) = (a.num(2.0), a.num(3.0));
	let second = a.map_field(two, three);
	let fields = a.list_from::<element::MapField>([first, second]);
	let map = a.map(fields);
	assert_eq!(render_expr(&mut a, map), "{\n\tfoo = 1,\n\t[2] = 3,\n}", "map");
}

#[test]
fn render_mixed_table() {
	let mut a = Ast::new();
	let empty = a.new_list::<element::Node>();
	let empty = a.mixed_table(empty);
	assert_eq!(render_expr(&mut a, empty), "{}", "empty mixed table");
	let one = a.num(1.0);
	let (foo, two) = (a.str("foo"), a.num(2.0));
	let first = a.map_field(foo, two);
	let (three, four) = (a.num(3.0), a.num(4.0));
	let second = a.map_field(three, four);
	let fields = a.list_from::<element::Node>([one, first, second]);
	let table = a.mixed_table(fields);
	assert_eq!(render_expr(&mut a, table), "{\n\t1,\n\tfoo = 2,\n\t[3] = 4,\n}");
}

#[test]
fn render_if_expression_elseif_chain() {
	let mut a = Ast::new();
	let node = if_expr(id("a"), num(1.0), if_expr(id("b"), num(2.0), num(3.0))).build(&mut a);
	assert_eq!(render_expr(&mut a, node), "if a then 1 elseif b then 2 else 3");
}

#[test]
#[expect(
	clippy::literal_string_with_formatting_args,
	reason = "the Go case renders literal braces"
)]
fn render_interpolated_string() {
	let mut a = Ast::new();
	let (first, b, last) = (
		a.interpolated_string_part("a"),
		a.id("b"),
		a.interpolated_string_part(" {c} "),
	);
	let parts = a.list_from::<element::Node>([first, b, last]);
	let node = a.interpolated_string(parts);
	assert_eq!(render_expr(&mut a, node), "`a{b} \\{c\\} `");
}

#[test]
fn render_interpolated_string_with_table() {
	// Table expressions are wrapped in parentheses inside the braces, since
	// `{{}}` is invalid Luau.
	let mut a = Ast::new();
	let part = a.interpolated_string_part("x=");
	let table = array(vec![num(1.0)]).build(&mut a);
	let parts = a.list_from::<element::Node>([part, table]);
	let node = a.interpolated_string(parts);
	assert_eq!(render_expr(&mut a, node), "`x={({ 1 })}`");
}

#[test]
fn render_function_expression() {
	let mut a = Ast::new();
	let (params, body) = (
		a.new_list::<element::AnyIdentifier>(),
		a.new_list::<element::Statement>(),
	);
	let empty = a.function_expression(params, false, body);
	assert_eq!(render_expr(&mut a, empty), "function() end");
	let x = a.id("x");
	let ret = a.return_statement(x);
	let body = a.list_from::<element::Statement>([ret]);
	let param = a.id("x");
	let params = a.list_from::<element::AnyIdentifier>([param]);
	let function = a.function_expression(params, true, body);
	assert_eq!(render_expr(&mut a, function), "function(x, ...)\n\treturn x\nend");
}
