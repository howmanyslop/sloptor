//! `arraymacros2.go`: the `Array` region of `macros/propertyCallMacros.ts` (`ARRAY_METHODS`).

#![expect(
	clippy::needless_pass_by_value,
	reason = "the macro tables fix the `CallMacro` signature, which takes the arguments by value"
)]

use super::arg;
use crate::access::offset_expr;
use crate::call::convert_to_indexable_expression;
use crate::conditional::is_used_as_statement;
use crate::macromanager::PropertyCallMacro;
use crate::prelude::*;

/// `table.<name>(args...)`
fn table_call(state: &mut State<'_>, name: &str, args: impl IntoIterator<Item = NodeId>) -> NodeId {
	let callee = state.ast.global_property("table", name);
	let args = state.ast.list_from(args);
	state.ast.call(callee, args)
}

/// `table.<name>(args...)` as a prereq statement.
fn prereq_table_call(state: &mut State<'_>, name: &str, args: impl IntoIterator<Item = NodeId>) {
	let call = table_call(state, name, args);
	let statement = state.ast.call_statement(call);
	state.prereq(statement);
}

/// `expression[index]`, parenthesizing a non-indexable base.
fn index_into(state: &mut State<'_>, expression: NodeId, index: NodeId) -> NodeId {
	let indexable = convert_to_indexable_expression(&mut state.ast, expression);
	state.ast.computed_index(indexable, index)
}

/// `push`: no arguments is `#exp` even as a statement (so the call does not disappear);
/// otherwise one `table.insert(exp, arg)` per argument, or the array append plan's
/// `_length += 1; exp[_length] = arg`. A used value is the new length.
fn push(state: &mut State<'_>, node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	// for `a.push()` always emit luau.unary so the call doesn't disappear in emit
	if args.is_empty() {
		return state.ast.unary(UnaryOperator::Length, expression);
	}

	let expression = state.push_to_var_if_complex(expression, "exp");
	if let Some(target) = state.get_array_append_target(node) {
		for &argument in &args {
			let one = state.ast.num(1.0);
			let increment = state.ast.assignment(target.length_id, AssignmentOperator::Add, one);
			state.prereq(increment);
			let slot = index_into(state, expression, target.length_id);
			let store = state.ast.assignment(slot, AssignmentOperator::Assign, argument);
			state.prereq(store);
		}
		if !is_used_as_statement(node) {
			return state.push_to_var(Some(target.length_id), "length");
		}
		return state.ast.none();
	}

	for &argument in &args {
		prereq_table_call(state, "insert", [expression, argument]);
	}

	if !is_used_as_statement(node) {
		return state.ast.unary(UnaryOperator::Length, expression);
	}
	state.ast.none()
}

/// `pop`: in statement position the one-liner `exp[#exp] = nil`; a used value snapshots
/// `local _length = #exp` and `local _result = exp[_length]` first.
fn pop(state: &mut State<'_>, node: P<Node>, expression: NodeId, _args: Vec<NodeId>) -> NodeId {
	let expression = state.push_to_var_if_complex(expression, "exp");

	let mut length_exp = state.ast.unary(UnaryOperator::Length, expression);

	let return_value_is_used = !is_used_as_statement(node);
	let ret_value = if return_value_is_used {
		length_exp = state.push_to_var(Some(length_exp), "length");
		let element = index_into(state, expression, length_exp);
		Some(state.push_to_var(Some(element), "result"))
	} else {
		None
	};

	let slot = index_into(state, expression, length_exp);
	let nil = state.ast.nil();
	let clear = state.ast.assignment(slot, AssignmentOperator::Assign, nil);
	state.prereq(clear);

	match ret_value {
		Some(ret_value) => ret_value,
		None => state.ast.none(),
	}
}

/// `shift`: `table.remove(exp, 1)`.
fn shift(state: &mut State<'_>, _node: P<Node>, expression: NodeId, _args: Vec<NodeId>) -> NodeId {
	let one = state.ast.num(1.0);
	table_call(state, "remove", [expression, one])
}

/// `unshift`: `table.insert(exp, 1, arg)` per argument in reverse order; a used value is the new
/// length.
fn unshift(state: &mut State<'_>, node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let expression = state.push_to_var_if_complex(expression, "exp");

	for &argument in args.iter().rev() {
		let one = state.ast.num(1.0);
		prereq_table_call(state, "insert", [expression, one, argument]);
	}

	if !is_used_as_statement(node) {
		return state.ast.unary(UnaryOperator::Length, expression);
	}
	state.ast.none()
}

/// `insert`: `table.insert(exp, index + 1, value)`.
fn insert(state: &mut State<'_>, _node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let index = arg(state, &args, 0);
	let index = offset_expr(&mut state.ast, index, 1.0);
	let value = arg(state, &args, 1);
	table_call(state, "insert", [expression, index, value])
}

/// `remove`: `table.remove(exp, index + 1)`.
fn remove(state: &mut State<'_>, _node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let index = arg(state, &args, 0);
	let index = offset_expr(&mut state.ast, index, 1.0);
	table_call(state, "remove", [expression, index])
}

/// `unorderedRemove`: the index temp comes before the base temp (upstream order);
/// `local _value = exp[_index]` is declared even when the value is unused; the swap-and-clear
/// runs under `if _value ~= nil then`.
fn unordered_remove(state: &mut State<'_>, node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let index = arg(state, &args, 0);
	let index = offset_expr(&mut state.ast, index, 1.0);
	let index_exp = state.push_to_var_if_complex(index, "index");

	let expression = state.push_to_var_if_complex(expression, "exp");

	let length = state.ast.unary(UnaryOperator::Length, expression);
	let length_id = state.push_to_var(Some(length), "length");

	let value_is_used = !is_used_as_statement(node);
	let element = index_into(state, expression, index_exp);
	let value_id = state.push_to_var(Some(element), "value");

	let nil = state.ast.nil();
	let condition = state.ast.binary(value_id, BinaryOperator::NotEqual, nil);
	let slot = index_into(state, expression, index_exp);
	let last = index_into(state, expression, length_id);
	let swap = state.ast.assignment(slot, AssignmentOperator::Assign, last);
	let last_slot = index_into(state, expression, length_id);
	let nil = state.ast.nil();
	let clear = state.ast.assignment(last_slot, AssignmentOperator::Assign, nil);
	let statements = state.ast.list_from([swap, clear]);
	let check = state.ast.if_statement(condition, statements, None);
	state.prereq(check);

	if value_is_used { value_id } else { state.ast.none() }
}

/// `sort`: `table.sort(exp[, compareFn])`; a used value is the array itself.
fn sort(state: &mut State<'_>, node: P<Node>, mut expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let value_is_used = !is_used_as_statement(node);
	if value_is_used {
		expression = state.push_to_var_if_complex(expression, "exp");
	}

	prereq_table_call(state, "sort", std::iter::once(expression).chain(args));

	if value_is_used { expression } else { state.ast.none() }
}

/// `clear`: `table.clear(exp)`; a used value is `nil`. Shared with `Set.clear`/`Map.clear`.
pub(super) fn clear(state: &mut State<'_>, node: P<Node>, expression: NodeId, _args: Vec<NodeId>) -> NodeId {
	prereq_table_call(state, "clear", [expression]);
	if !is_used_as_statement(node) {
		return state.ast.nil();
	}
	state.ast.none()
}

/// `arrayMethods` (`ARRAY_METHODS`).
pub(super) const ARRAY_METHODS: &[(&str, PropertyCallMacro)] = &[
	("push", push),
	("pop", pop),
	("shift", shift),
	("unshift", unshift),
	("insert", insert),
	("remove", remove),
	("unorderedRemove", unordered_remove),
	("sort", sort),
	("clear", clear),
];
