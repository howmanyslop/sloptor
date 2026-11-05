//! `async.go`: `expressions/transformAwaitExpression.ts`, `expressions/transformYieldExpression.ts`
//! and `util/wrapStatementsAsGenerator.ts`.

use rotor_luau::{Array, VariableDeclaration};
use rotor_tsrs::ast::skip_downwards;

use crate::call::convert_to_indexable_expression;
use crate::conditional::is_used_as_statement;
use crate::dispatch::transform_expression;
use crate::prelude::*;

/// `transformAwaitExpression` (L7-9), the entire transform: `await x` is `TS.await(x)`. The
/// wrapper is never elided for non-promise operands.
pub fn transform_await_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let await_ = state.runtime_lib(Some(node), "await");
	let expression = transform_expression(state, skip_downwards(node.as_await_expression().expression));
	let args = state.ast.list_from([expression]);
	state.ast.call(await_, args)
}

/// `coroutine.yield`
fn coroutine_yield(state: &mut State<'_>) -> NodeId {
	state.ast.global_property("coroutine", "yield")
}

/// `transformYieldExpression` (L8-54):
///
/// - bare `yield` is `coroutine.yield()` (zero args, not nil);
/// - `yield v` is `coroutine.yield(v)`, whose result is the resume arguments;
/// - `yield* g` lowers to a generic for over the inner generator's bare `.next`, re-yielding each
///   value; used as a value, the inner generator's return value (`_result.value` at `done`) is
///   captured in `_returnValue` before `break`. `_result` is created before `_returnValue`.
pub fn transform_yield_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let yield_ = node.as_yield_expression();
	let Some(inner) = yield_.expression else {
		let callee = coroutine_yield(state);
		let args = state.ast.new_list();
		return state.ast.call(callee, args);
	};

	let expression = transform_expression(state, inner);
	if yield_.asterisk_token.is_some() {
		let loop_id = state.ast.temp_id("result");

		let break_statement = state.ast.break_statement();
		let finalizer = state.ast.list_from([break_statement]);
		let evaluated = if is_used_as_statement(node) {
			state.ast.none()
		} else {
			let return_value = state.push_to_var(None, "returnValue");
			let value = state.ast.property_access(loop_id, "value");
			let assignment = state.ast.assignment(return_value, AssignmentOperator::Assign, value);
			state.ast.list_unshift(finalizer, assignment);
			return_value
		};

		let done = state.ast.property_access(loop_id, "done");
		let check = state.ast.if_statement(done, finalizer, None);
		let callee = coroutine_yield(state);
		let value = state.ast.property_access(loop_id, "value");
		let args = state.ast.list_from([value]);
		let call = state.ast.call(callee, args);
		let reyield = state.ast.call_statement(call);
		let body = state.ast.list_from([check, reyield]);
		let iterable = convert_to_indexable_expression(&mut state.ast, expression);
		let next = state.ast.property_access(iterable, "next");
		let ids = state.ast.list_from([loop_id]);
		let loop_statement = state.ast.for_statement(ids, next, body);
		state.prereq(loop_statement);

		return evaluated;
	}
	let callee = coroutine_yield(state);
	let args = state.ast.list_from([expression]);
	state.ast.call(callee, args)
}

/// `local args = { ... }`: the var-args packing statement that stays outside the generator.
fn is_var_args_packing_statement(state: &State<'_>, statement: NodeId) -> bool {
	let Some(declaration) = state.ast.get::<VariableDeclaration>(statement) else {
		return false;
	};
	let Some(NodeOrList::Node(right)) = declaration.right else {
		return false;
	};
	let Some(array) = state.ast.get::<Array>(right) else {
		return false;
	};
	let members = state.ast.list_to_vec(array.members);
	matches!(members[..], [member] if state.ast.kind(member) == LuauKind::VarArgsLiteral)
}

/// `wrapStatementsAsGenerator` (`util/wrapStatementsAsGenerator.ts`): the body becomes
/// `return TS.generator(function() ... end)`, keeping var-args packing outside.
pub fn wrap_statements_as_generator(state: &mut State<'_>, node: P<Node>, statements: Statements) -> Statements {
	let outer_statements = state.ast.new_list();
	let generator_statements = state.ast.new_list();
	for statement in state.ast.list_to_vec(statements) {
		if is_var_args_packing_statement(state, statement) {
			state.ast.list_push(outer_statements, statement);
		} else {
			state.ast.list_push(generator_statements, statement);
		}
	}
	let generator = state.runtime_lib(Some(node), "generator");
	let params = state.ast.new_list();
	let function = state.ast.function_expression(params, false, generator_statements);
	let args = state.ast.list_from([function]);
	let call = state.ast.call(generator, args);
	let return_statement = state.ast.return_statement(call);
	state.ast.list_push(outer_statements, return_statement);
	outer_statements
}
