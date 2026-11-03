//! Port of `internal/luau/render/golden_test.go`.

use super::dsl::{array, bin, call, id, num, string, un};
use super::{ids, render_both, stmts};
use crate::{AssignmentOperator, Ast, BinaryOperator as B, List, UnaryOperator as U, element};

/// Builds a program shaped like typical transformer output.
fn build_golden_ast(a: &mut Ast) -> List<element::Statement> {
	// local function map(array, callback)
	// 	local result = {}
	// 	for i, v in ipairs(array) do
	// 		result[i] = callback(v)
	// 	end
	// 	return result
	// end
	let (array_id, callback_id) = (a.id("array"), a.id("callback"));
	let result_id = a.id("result");
	let (i_id, v_id) = (a.id("i"), a.id("v"));
	let empty = array(vec![]).build(a);
	let result = a.variable_declaration(result_id, Some(empty.into()));
	let loop_ids = ids(a, &[i_id, v_id]);
	let iterator = call(id("ipairs"), vec![array_id.into()]).build(a);
	let target = a.computed_index(result_id, i_id);
	let value = call(callback_id, vec![v_id.into()]).build(a);
	let assignment = a.assignment(target, AssignmentOperator::Assign, value);
	let loop_body = stmts(a, &[assignment]);
	let loop_node = a.for_statement(loop_ids, iterator, loop_body);
	let ret = a.return_statement(result_id);
	let body = stmts(a, &[result, loop_node, ret]);
	let (name, params) = (a.id("map"), ids(a, &[array_id, callback_id]));
	let map_declaration = a.function_declaration(true, name, params, false, body);

	// local doubled = map({ 1, 2, 3 }, function(n)
	// 	return n * 2
	// end)
	let n_id = a.id("n");
	let product = bin(n_id, B::Multiply, num(2.0)).build(a);
	let ret = a.return_statement(product);
	let (params, body) = (ids(a, &[n_id]), stmts(a, &[ret]));
	let callback = a.function_expression(params, false, body);
	let numbers = array(vec![num(1.0), num(2.0), num(3.0)]).build(a);
	let map_call = call(id("map"), vec![numbers.into(), callback.into()]).build(a);
	let doubled = a.id("doubled");
	let doubled_declaration = a.variable_declaration(doubled, Some(map_call.into()));

	// if #doubled > 0 and doubled[1] == 2 then print("ok") else error("bad") end
	let length = bin(un(U::Length, id("doubled")), B::Greater, num(0.0)).build(a);
	let (doubled, one) = (a.id("doubled"), a.num(1.0));
	let first = a.computed_index(doubled, one);
	let equal = bin(first, B::Equal, num(2.0)).build(a);
	let condition = a.binary(length, B::And, equal);
	let print = call(id("print"), vec![string("ok")]).build(a);
	let print = a.call_statement(print);
	let then_body = stmts(a, &[print]);
	let error = call(id("error"), vec![string("bad")]).build(a);
	let error = a.call_statement(error);
	let else_body = stmts(a, &[error]);
	let check = a.if_statement(condition, then_body, Some(else_body.into()));

	stmts(a, &[map_declaration, doubled_declaration, check])
}

const GOLDEN_WANT: &str = "local function map(array, callback)
	local result = {}
	for i, v in ipairs(array) do
		result[i] = callback(v)
	end
	return result
end
local doubled = map({ 1, 2, 3 }, function(n)
	return n * 2
end)
if #doubled > 0 and doubled[1] == 2 then
	print(\"ok\")
else
	error(\"bad\")
end
";

#[test]
fn golden_program() {
	let mut a = Ast::new();
	let ast = build_golden_ast(&mut a);
	let got = render_both(&a, ast);
	assert_eq!(got, GOLDEN_WANT, "golden mismatch");
	assert!(!got.contains('\r'), "output must never contain carriage returns");
}
