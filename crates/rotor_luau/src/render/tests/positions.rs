//! Port of `internal/luau/render/positions_test.go`.

use std::collections::HashMap;

use super::dsl::{bin, call, id};
use super::{closed_range, head, ids, node_range, payload, stmts};
use crate::render::{Range, RenderState, render, render_ast, render_ast_with_positions};
use crate::{
	AssignmentOperator, Ast, BinaryExpression, BinaryOperator as B, CallExpression, FunctionExpression, IfExpression,
	IfStatement, List, MapField, NodeId, NodeOrList, Position, RepeatStatement, ReturnStatement, VariableDeclaration,
	WhileStatement, element,
};

pub(super) struct PositionCase {
	pub(super) name: &'static str,
	pub(super) ast: List<element::Statement>,
	pub(super) text: &'static str,
	pub(super) ranges: Vec<Range>,
}

pub(super) fn check_position_cases(a: &Ast, cases: Vec<PositionCase>) {
	for case in cases {
		let (text, ranges) = render_ast_with_positions(a, case.ast).expect("render_ast_with_positions");
		assert_eq!(text, case.text, "{}: text", case.name);
		assert_eq!(ranges, case.ranges, "{}: ranges", case.name);
		assert_eq!(
			render_ast(a, case.ast).expect("render_ast"),
			text,
			"{}: position text differs from plain text",
			case.name
		);
	}
}

fn callee(a: &Ast, call: NodeId) -> NodeId {
	payload::<CallExpression>(a, call).expression
}

fn returned(a: &Ast, statement: NodeId) -> NodeId {
	match payload::<ReturnStatement>(a, statement).expression {
		NodeOrList::Node(node) => node,
		NodeOrList::List(_) => panic!("return of a list"),
	}
}

fn call_statement(a: &mut Ast, name: &'static str) -> (NodeId, NodeId) {
	let call = call(id(name), vec![]).build(a);
	(call, a.call_statement(call))
}

#[test]
fn render_ast_with_positions_cases() {
	let mut a = Ast::new();
	let (call, statement) = call_statement(&mut a, "f");
	let body = stmts(&mut a, &[statement]);
	let block = a.do_statement(body);
	let ast = stmts(&mut a, &[block]);
	let mut cases = vec![PositionCase {
		name: "do and call",
		ast,
		text: "do\n\tf()\nend\n",
		ranges: vec![
			closed_range(block, (0, 0), (2, 3), (2, 0)),
			node_range(statement, 1, 1, 1, 4),
			node_range(call, 1, 1, 1, 4),
			node_range(callee(&a, call), 1, 1, 1, 2),
		],
	}];
	cases.extend(position_cases(&mut a));
	check_position_cases(&a, cases);
}

#[test]
fn operator_positions() {
	let mut a = Ast::new();
	let sum = bin(id("a"), B::Add, id("b")).build(&mut a);
	let and = bin(id("a"), B::And, id("b")).build(&mut a);
	let or = bin(and, B::Or, id("c")).build(&mut a);
	let (x, one) = (a.id("x"), a.num(1.0));
	let assignment = a.assignment(x, AssignmentOperator::Add, one);
	let inner = bin(id("a"), B::Add, id("b")).build(&mut a);
	let outer = bin(inner, B::Multiply, id("c")).build(&mut a);
	let x = a.id("x");
	let first = a.variable_declaration(x, Some(sum.into()));
	let x = a.id("x");
	let second = a.variable_declaration(x, Some(or.into()));
	let ret = a.return_statement(outer);
	let ast = stmts(&mut a, &[first, second, assignment, ret]);
	let (text, ranges) = render_ast_with_positions(&a, ast).expect("render");
	assert_eq!(
		text,
		"local x = a + b\nlocal x = a and b or c\nx += 1\nreturn (a + b) * c\n"
	);
	assert_eq!(text, render_ast(&a, ast).expect("render"));
	let mut want: HashMap<NodeId, Position> = HashMap::from([
		(sum, Position::new(0, 12)),
		(and, Position::new(1, 12)),
		(or, Position::new(1, 18)),
		(assignment, Position::new(2, 2)),
		(inner, Position::new(3, 10)),
		(outer, Position::new(3, 15)),
	]);
	for range in ranges {
		if let Some(position) = want.remove(&range.node) {
			assert_eq!(range.operator, Some(position), "{} operator", a.kind(range.node));
		} else {
			assert_eq!(
				range.operator,
				None,
				"{} has unexpected operator marker",
				a.kind(range.node)
			);
		}
	}
	assert!(want.is_empty(), "missing ranges: {want:?}");
}

#[expect(clippy::too_many_lines, reason = "one block per Go position case")]
fn position_cases(a: &mut Ast) -> Vec<PositionCase> {
	let mut cases = Vec::new();
	{
		let (call, statement) = call_statement(a, "f");
		let body = stmts(a, &[statement]);
		let live = a.id("live");
		let loop_node = a.while_statement(live, body);
		let body = stmts(a, &[loop_node]);
		let done = a.id("done");
		let repeat = a.repeat_statement(done, body);
		let body = stmts(a, &[repeat]);
		let block = a.do_statement(body);
		let ast = stmts(a, &[block]);
		cases.push(PositionCase {
			name: "nested blocks and until",
			ast,
			text: "do\n\trepeat\n\t\twhile live do\n\t\t\tf()\n\t\tend\n\tuntil done\nend\n",
			ranges: vec![
				closed_range(block, (0, 0), (6, 3), (6, 0)),
				closed_range(repeat, (1, 1), (5, 11), (5, 1)),
				closed_range(loop_node, (2, 2), (4, 5), (4, 2)),
				node_range(payload::<WhileStatement>(a, loop_node).condition, 2, 8, 2, 12),
				node_range(statement, 3, 3, 3, 6),
				node_range(call, 3, 3, 3, 6),
				node_range(callee(a, call), 3, 3, 3, 4),
				node_range(payload::<RepeatStatement>(a, repeat).condition, 5, 7, 5, 11),
			],
		});
	}
	{
		let p = a.id("p");
		let ret = a.return_statement(p);
		let param = a.id("p");
		let (params, body) = (ids(a, &[param]), stmts(a, &[ret]));
		let function = a.function_expression(params, false, body);
		let call = call(id("f"), vec![function.into()]).build(a);
		let x = a.id("x");
		let declaration = a.variable_declaration(x, Some(call.into()));
		let ast = stmts(a, &[declaration]);
		let left = payload::<VariableDeclaration>(a, declaration)
			.left
			.node()
			.expect("node");
		let parameter = head(a, payload::<FunctionExpression>(a, function).parameters);
		cases.push(PositionCase {
			name: "function argument",
			ast,
			text: "local x = f(function(p)\n\treturn p\nend)\n",
			ranges: vec![
				node_range(declaration, 0, 0, 2, 4),
				node_range(left, 0, 6, 0, 7),
				node_range(call, 0, 10, 2, 4),
				node_range(callee(a, call), 0, 10, 0, 11),
				closed_range(function, (0, 12), (2, 3), (2, 0)),
				node_range(parameter, 0, 21, 0, 22),
				node_range(ret, 1, 1, 1, 9),
				node_range(returned(a, ret), 1, 8, 1, 9),
			],
		});
	}
	{
		let (call, statement) = call_statement(a, "f");
		let condition = |a: &mut Ast, name| a.id(name);
		let (c, empty, else_body) = (
			condition(a, "c"),
			a.new_list::<element::Statement>(),
			stmts(a, &[statement]),
		);
		let last = a.if_statement(c, empty, Some(else_body.into()));
		let (b, empty) = (condition(a, "b"), a.new_list::<element::Statement>());
		let link = a.if_statement(b, empty, Some(last.into()));
		let (first_condition, empty) = (condition(a, "a"), a.new_list::<element::Statement>());
		let first = a.if_statement(first_condition, empty, Some(link.into()));
		let body = stmts(a, &[first]);
		let block = a.do_statement(body);
		let ast = stmts(a, &[block]);
		let condition_of = |a: &Ast, node| payload::<IfStatement>(a, node).condition;
		cases.push(PositionCase {
			name: "elseif links contain later links and else body",
			ast,
			text: "do\n\tif a then\n\telseif b then\n\telseif c then\n\telse\n\t\tf()\n\tend\nend\n",
			ranges: vec![
				closed_range(block, (0, 0), (7, 3), (7, 0)),
				closed_range(first, (1, 1), (6, 4), (6, 1)),
				node_range(condition_of(a, first), 1, 4, 1, 5),
				closed_range(link, (2, 1), (6, 4), (6, 1)),
				node_range(condition_of(a, link), 2, 8, 2, 9),
				closed_range(last, (3, 1), (6, 4), (6, 1)),
				node_range(condition_of(a, last), 3, 8, 3, 9),
				node_range(statement, 5, 2, 5, 5),
				node_range(call, 5, 2, 5, 5),
				node_range(callee(a, call), 5, 2, 5, 3),
			],
		});
	}
	{
		let (params, body) = (
			a.new_list::<element::AnyIdentifier>(),
			a.new_list::<element::Statement>(),
		);
		let function = a.function_expression(params, false, body);
		let f = a.id("f");
		let declaration = a.variable_declaration(f, Some(function.into()));
		let ast = stmts(a, &[declaration]);
		let left = payload::<VariableDeclaration>(a, declaration)
			.left
			.node()
			.expect("node");
		cases.push(PositionCase {
			name: "empty inline function",
			ast,
			text: "local f = function() end\n",
			ranges: vec![
				node_range(declaration, 0, 0, 0, 24),
				node_range(left, 0, 6, 0, 7),
				closed_range(function, (0, 10), (0, 24), (0, 21)),
			],
		});
	}
	{
		let comment = a.comment("first\r\n😀");
		let body = stmts(a, &[comment]);
		let block = a.do_statement(body);
		let ast = stmts(a, &[block]);
		cases.push(PositionCase {
			name: "multiline CRLF comment",
			ast,
			text: "do\n\t--[[\n\t\tfirst\n\t\t😀\n\t]]\nend\n",
			ranges: vec![
				closed_range(block, (0, 0), (5, 3), (5, 0)),
				node_range(comment, 1, 1, 4, 3),
			],
		});
	}
	{
		let (string, number) = (a.str("a\r\n😀"), a.num(2.0));
		let call = call(id("f"), vec![string.into(), number.into()]).build(a);
		let statement = a.call_statement(call);
		let ast = stmts(a, &[statement]);
		cases.push(PositionCase {
			name: "long bracket string",
			ast,
			text: "f([[a\r\n😀]], 2)\n",
			ranges: vec![
				node_range(statement, 0, 0, 1, 8),
				node_range(call, 0, 0, 1, 8),
				node_range(callee(a, call), 0, 0, 0, 1),
				node_range(string, 0, 2, 1, 4),
				node_range(number, 1, 6, 1, 7),
			],
		});
	}
	{
		let (call, first) = call_statement(a, "f");
		let binary = bin(id("g"), B::Add, id("h")).build(a);
		let parens = a.parenthesized(binary);
		let empty = a.new_list::<element::Expression>();
		let next_call = a.call(parens, empty);
		let next = a.call_statement(next_call);
		let ast = stmts(a, &[first, next]);
		let BinaryExpression { left, right, .. } = payload::<BinaryExpression>(a, binary).clone();
		cases.push(PositionCase {
			name: "semicolon belongs to statement",
			ast,
			text: "f();\n(g + h)()\n",
			ranges: vec![
				node_range(first, 0, 0, 0, 4),
				node_range(call, 0, 0, 0, 3),
				node_range(callee(a, call), 0, 0, 0, 1),
				node_range(next, 1, 0, 1, 9),
				node_range(next_call, 1, 0, 1, 9),
				node_range(parens, 1, 0, 1, 7),
				Range {
					operator: Some(Position::new(1, 3)),
					..node_range(binary, 1, 1, 1, 6)
				},
				node_range(left, 1, 1, 1, 2),
				node_range(right, 1, 5, 1, 6),
			],
		});
	}
	{
		let (string, x) = (a.str("😀"), a.id("x"));
		let call = call(id("f"), vec![string.into(), x.into()]).build(a);
		let statement = a.call_statement(call);
		let ast = stmts(a, &[statement]);
		cases.push(PositionCase {
			name: "non-BMP generated columns",
			ast,
			text: "f(\"😀\", x)\n",
			ranges: vec![
				node_range(statement, 0, 0, 0, 10),
				node_range(call, 0, 0, 0, 10),
				node_range(callee(a, call), 0, 0, 0, 1),
				node_range(string, 0, 2, 0, 6),
				node_range(x, 0, 8, 0, 9),
			],
		});
	}
	{
		let (string, x) = (a.str("a\rb"), a.id("x"));
		let call = call(id("f"), vec![string.into(), x.into()]).build(a);
		let statement = a.call_statement(call);
		let ast = stmts(a, &[statement]);
		cases.push(PositionCase {
			name: "bare CR does not break a generated line",
			ast,
			text: "f(\"a\rb\", x)\n",
			ranges: vec![
				node_range(statement, 0, 0, 0, 11),
				node_range(call, 0, 0, 0, 11),
				node_range(callee(a, call), 0, 0, 0, 1),
				node_range(string, 0, 2, 0, 7),
				node_range(x, 0, 9, 0, 10),
			],
		});
	}
	{
		let (call, statement) = call_statement(a, "f");
		let ast = a.new_list::<element::Statement>();
		// The same statement twice: a list built by pushing, not by adoption,
		// so both links hold the same node.
		a.list_push(ast, statement);
		a.list_push(ast, statement);
		let callee = callee(a, call);
		cases.push(PositionCase {
			name: "same node rendered more than once",
			ast,
			text: "f()\nf()\n",
			ranges: vec![
				node_range(statement, 0, 0, 0, 3),
				node_range(call, 0, 0, 0, 3),
				node_range(callee, 0, 0, 0, 1),
				node_range(statement, 1, 0, 1, 3),
				node_range(call, 1, 0, 1, 3),
				node_range(callee, 1, 0, 1, 1),
			],
		});
	}
	{
		let index = call(id("key"), vec![]).build(a);
		let value = call(id("value"), vec![]).build(a);
		let field = a.map_field(index, value);
		let fields = a.list_from::<element::MapField>([field]);
		let table = a.map(fields);
		let ret = a.return_statement(table);
		let ast = stmts(a, &[ret]);
		cases.push(PositionCase {
			name: "map field index precedes value",
			ast,
			text: "return {\n\t[key()] = value(),\n}\n",
			ranges: vec![
				node_range(ret, 0, 0, 2, 1),
				node_range(table, 0, 7, 2, 1),
				node_range(field, 1, 1, 1, 18),
				node_range(index, 1, 2, 1, 7),
				node_range(callee(a, index), 1, 2, 1, 5),
				node_range(value, 1, 11, 1, 18),
				node_range(callee(a, value), 1, 11, 1, 16),
			],
		});
	}
	{
		let (key, one) = (a.str("x"), a.num(1.0));
		let field = a.map_field(key, one);
		let fields = a.list_from::<element::MapField>([field]);
		let table = a.map(fields);
		let ret = a.return_statement(table);
		let ast = stmts(a, &[ret]);
		let MapField { index, value } = payload::<MapField>(a, field).clone();
		cases.push(PositionCase {
			name: "identifier map field key",
			ast,
			text: "return {\n\tx = 1,\n}\n",
			ranges: vec![
				node_range(ret, 0, 0, 2, 1),
				node_range(table, 0, 7, 2, 1),
				node_range(field, 1, 1, 1, 6),
				node_range(index, 1, 1, 1, 2),
				node_range(value, 1, 5, 1, 6),
			],
		});
	}
	{
		let (b, two, three) = (a.id("b"), a.num(2.0), a.num(3.0));
		let alternative = a.if_expression(b, two, three);
		let (condition, one) = (a.id("a"), a.num(1.0));
		let expression = a.if_expression(condition, one, alternative);
		let ret = a.return_statement(expression);
		let ast = stmts(a, &[ret]);
		let outer = payload::<IfExpression>(a, expression).clone();
		let inner = payload::<IfExpression>(a, alternative).clone();
		cases.push(PositionCase {
			name: "elseif expression link",
			ast,
			text: "return if a then 1 elseif b then 2 else 3\n",
			ranges: vec![
				node_range(ret, 0, 0, 0, 41),
				node_range(expression, 0, 7, 0, 41),
				node_range(outer.condition, 0, 10, 0, 11),
				node_range(outer.expression, 0, 17, 0, 18),
				node_range(alternative, 0, 19, 0, 41),
				node_range(inner.condition, 0, 26, 0, 27),
				node_range(inner.expression, 0, 33, 0, 34),
				node_range(inner.alternative, 0, 40, 0, 41),
			],
		});
	}
	cases
}

#[test]
fn render_map_field_fallback_temp_order() {
	let mut a = Ast::new();
	let (index, value) = (a.temp_id("key"), a.temp_id("value"));
	let field = a.map_field(index, value);
	let mut state = RenderState::new();
	// Standalone rendering keeps its historical fallback names: the map
	// field's value receives a name before its index, although the index
	// appears first.
	for _ in 0..2 {
		assert_eq!(
			render(&mut state, &a, field).expect("render"),
			"[_1] = _0",
			"standalone field"
		);
	}
}
