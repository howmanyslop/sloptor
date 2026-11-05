//! Port of `internal/luau/render/fallback_order_test.go`.

#![expect(
	clippy::tuple_array_conversions,
	reason = "tests bind sibling nodes in one statement"
)]

use super::dsl::{bin, call, id, nil};
use super::{closed_range, head, ids, node_range, payload, returned_of, stmts, tail};
use crate::render::{Range, RenderState, render, render_ast, render_ast_with_positions};
use crate::{
	AssignmentOperator, Ast, BinaryOperator as B, CallExpression, ForStatement, FunctionExpression, NodeId, element,
};

fn renderers(a: &Ast, loop_statement: NodeId, list: crate::List<element::Statement>) -> [(&'static str, String); 3] {
	[
		("RenderAST", render_ast(a, list).expect("render_ast")),
		(
			"RenderASTWithPositions",
			render_ast_with_positions(a, list).expect("render_ast_with_positions").0,
		),
		(
			"Render",
			render(&mut RenderState::new(), a, loop_statement).expect("render"),
		),
	]
}

#[test]
fn render_for_iterator_fallback_temp_order() {
	let mut a = Ast::new();
	let callback = |a: &mut Ast, authored_local: bool, fallback: f64| {
		let temp = a.temp_id("condition");
		let body = a.new_list::<element::Statement>();
		if authored_local {
			let (local, ten) = (a.id("_0"), a.num(10.0));
			let declaration = a.variable_declaration(local, Some(ten.into()));
			a.list_push(body, declaration);
		}
		let input = a.id("input");
		let declaration = a.variable_declaration(temp, Some(input.into()));
		a.list_push(body, declaration);
		let condition = bin(temp, B::Equal, nil()).build(a);
		let value = a.num(fallback);
		let assignment = a.assignment(temp, AssignmentOperator::Assign, value);
		let then_body = stmts(a, &[assignment]);
		let statement = a.if_statement(condition, then_body, None);
		a.list_push(body, statement);
		let n = a.id("n");
		let declaration = a.variable_declaration(n, Some(temp.into()));
		a.list_push(body, declaration);
		let mut result = a.id("n");
		if authored_local {
			result = bin(id("_0"), B::Add, result).build(a);
		}
		let ret = a.return_statement(result);
		a.list_push(body, ret);
		let params = a.new_list::<element::AnyIdentifier>();
		a.function_expression(params, false, body)
	};
	let (key, value) = (callback(&mut a, true, 1.0), callback(&mut a, false, 2.0));
	let field = a.map_field(key, value);
	let fields = a.list_from::<element::MapField>([field]);
	let table = a.map(fields);
	let (k, v) = (a.id("key"), a.id("value"));
	let loop_ids = ids(&mut a, &[k, v]);
	let print = call(id("print"), vec![call(id("key"), vec![]), call(id("value"), vec![])]).build(&mut a);
	let print = a.call_statement(print);
	let body = stmts(&mut a, &[print]);
	let loop_statement = a.for_statement(loop_ids, table, body);
	let list = stmts(&mut a, &[loop_statement]);
	// The legacy renderer evaluates the value callback before the key
	// callback. Its fallback _0 belongs to the value; the key must use _1 so
	// that the authored local _0 survives and key() returns 11 when input is
	// nil.
	let want = "for key, value in {\n\
		\t[function()\n\t\tlocal _0 = 10\n\t\tlocal _1 = input\n\
		\t\tif _1 == nil then\n\t\t\t_1 = 1\n\t\tend\n\
		\t\tlocal n = _1\n\t\treturn _0 + n\n\tend] = function()\n\
		\t\tlocal _0 = input\n\t\tif _0 == nil then\n\t\t\t_0 = 2\n\t\tend\n\
		\t\tlocal n = _0\n\t\treturn n\n\tend,\n} do\n\tprint(key(), value())\nend\n";
	for (name, got) in renderers(&a, loop_statement, list) {
		assert_eq!(got, want, "{name}: iterator callback names changed");
	}
}

#[test]
fn render_nested_for_iterator_fallback_positions() {
	let mut a = Ast::new();
	let callback = |a: &mut Ast, value: NodeId| {
		let ret = a.return_statement(value);
		let (params, body) = (a.new_list::<element::AnyIdentifier>(), stmts(a, &[ret]));
		a.function_expression(params, false, body)
	};
	let inner_key_temp = a.temp_id("innerKey");
	let inner_key = callback(&mut a, inner_key_temp);
	let inner_value_temp = a.temp_id("innerValue");
	let inner_value = callback(&mut a, inner_value_temp);
	let inner_field = a.map_field(inner_key, inner_value);
	let key_temp = a.temp_id("key");
	let key = callback(&mut a, key_temp);
	let inner_fields = a.list_from::<element::MapField>([inner_field]);
	let inner_map = a.map(inner_fields);
	let ret = a.return_statement(inner_map);
	let parameter = a.temp_id("parameter");
	let (params, body) = (ids(&mut a, &[parameter]), stmts(&mut a, &[ret]));
	let value = a.function_expression(params, false, body);
	let field = a.map_field(key, value);
	let fields = a.list_from::<element::MapField>([field]);
	let outer_map = a.map(fields);
	let first = a.temp_id("first");
	let iterator = call(id("iter"), vec![first.into(), outer_map.into()]).build(&mut a);
	let nested = a.id("nested");
	let (nested_ids, last_temp, empty) = (
		ids(&mut a, &[nested]),
		a.temp_id("last"),
		a.new_list::<element::Statement>(),
	);
	let nested_loop = a.for_statement(nested_ids, last_temp, empty);
	let k = a.id("key");
	let (loop_ids, body) = (ids(&mut a, &[k]), stmts(&mut a, &[nested_loop]));
	let loop_statement = a.for_statement(loop_ids, iterator, body);
	let want = "for key in iter(_0, {\n\t[function()\n\t\treturn _4\n\tend] = function(_1)\n\
		\t\treturn {\n\t\t\t[function()\n\t\t\t\treturn _3\n\t\t\tend] = function()\n\
		\t\t\t\treturn _2\n\t\t\tend,\n\t\t}\n\tend,\n}) do\n\tfor nested in _5 do\n\tend\nend\n";
	let list = stmts(&mut a, &[loop_statement]);
	assert_eq!(render_ast(&a, list).expect("render"), want, "nested iterator");
	assert_eq!(
		render(&mut RenderState::new(), &a, loop_statement).expect("render"),
		want,
		"standalone nested iterator"
	);
	let (text, ranges) = render_ast_with_positions(&a, list).expect("render");
	assert_eq!(text, want, "positioned nested iterator");

	let function = |node| payload::<FunctionExpression>(&a, node).clone();
	let call_payload = payload::<CallExpression>(&a, iterator).clone();
	let loop_payload = payload::<ForStatement>(&a, loop_statement).clone();
	let nested_payload = payload::<ForStatement>(&a, nested_loop).clone();
	let first_statement = |node| head(&a, function(node).statements);
	// Ranges remain in emitted key-before-value order, even though fallback
	// allocation visits each value first. Every rendered occurrence is
	// recorded.
	let want_ranges: Vec<Range> = vec![
		closed_range(loop_statement, (0, 0), (15, 3), (15, 0)),
		node_range(head(&a, loop_payload.ids), 0, 4, 0, 7),
		node_range(iterator, 0, 11, 12, 2),
		node_range(call_payload.expression, 0, 11, 0, 15),
		node_range(head(&a, call_payload.args), 0, 16, 0, 18),
		node_range(tail(&a, call_payload.args), 0, 20, 12, 1),
		node_range(field, 1, 1, 11, 4),
		closed_range(key, (1, 2), (3, 4), (3, 1)),
		node_range(first_statement(key), 2, 2, 2, 11),
		node_range(returned_of(&a, first_statement(key)), 2, 9, 2, 11),
		closed_range(value, (3, 8), (11, 4), (11, 1)),
		node_range(head(&a, function(value).parameters), 3, 17, 3, 19),
		node_range(first_statement(value), 4, 2, 10, 3),
		node_range(returned_of(&a, first_statement(value)), 4, 9, 10, 3),
		node_range(inner_field, 5, 3, 9, 6),
		closed_range(inner_key, (5, 4), (7, 6), (7, 3)),
		node_range(first_statement(inner_key), 6, 4, 6, 13),
		node_range(returned_of(&a, first_statement(inner_key)), 6, 11, 6, 13),
		closed_range(inner_value, (7, 10), (9, 6), (9, 3)),
		node_range(first_statement(inner_value), 8, 4, 8, 13),
		node_range(returned_of(&a, first_statement(inner_value)), 8, 11, 8, 13),
		closed_range(nested_loop, (13, 1), (14, 4), (14, 1)),
		node_range(head(&a, nested_payload.ids), 13, 5, 13, 11),
		node_range(nested_payload.expression, 13, 15, 13, 17),
	];
	assert_eq!(ranges, want_ranges, "ranges");
}

#[test]
fn render_fallback_order_across_map_callbacks() {
	let mut a = Ast::new();
	let callback = |a: &mut Ast| {
		let item = a.id("item");
		let (loop_ids, iterator, empty) = (
			ids(a, &[item]),
			a.temp_id("iterator"),
			a.new_list::<element::Statement>(),
		);
		let loop_statement = a.for_statement(loop_ids, iterator, empty);
		let (params, body) = (a.new_list::<element::AnyIdentifier>(), stmts(a, &[loop_statement]));
		a.function_expression(params, false, body)
	};
	// The callbacks are scoped, but each nested iterator still uses fallback
	// names. The map's value must allocate before its key across those
	// subtrees.
	let (key, value) = (callback(&mut a), callback(&mut a));
	let field = a.map_field(key, value);
	let fields = a.list_from::<element::MapField>([field]);
	let table = a.map(fields);
	let ret = a.return_statement(table);
	let list = stmts(&mut a, &[ret]);
	let want = "return {\n\t[function()\n\t\tfor item in _1 do\n\t\tend\n\
		\tend] = function()\n\t\tfor item in _0 do\n\t\tend\n\tend,\n}\n";
	for (name, got) in renderers(&a, ret, list) {
		assert_eq!(got, want, "{name}: map callback iterators");
	}
}
