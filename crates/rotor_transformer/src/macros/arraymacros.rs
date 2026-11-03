//! `arraymacros.go`: the `ReadonlyArray` region of `macros/propertyCallMacros.ts`
//! (`makeEveryOrSomeMethod`, `makeEveryMethod`, `makeSomeMethod`, `READONLY_ARRAY_METHODS`).
//!
//! User callbacks receive `(value, index, array)`; the generic-for yields the 1-based key, so the
//! emitted call is `callback(_v, _k - 1, exp)`. `reduce`'s callback receives
//! `callback(_result, exp[_i], _i - 1, exp)` from a numeric for.
//!
//! Nodes are built in Go's evaluation order (arguments left to right, inner constructors first),
//! so every node is adopted, or cloned on a second adoption, exactly where Go's is.

#![expect(
	clippy::needless_pass_by_value,
	reason = "the macro tables fix the `CallMacro` signature, which takes the arguments by value"
)]

use rotor_tsrs::types::TypeCheck;

use super::arg;
use super::stringmacros::arguments_with_defaults;
use crate::access::offset_expr;
use crate::call::convert_to_indexable_expression;
use crate::conditional::is_used_as_statement;
use crate::macromanager::PropertyCallMacro;
use crate::prelude::*;
use crate::types::is_definitely_type;

/// `callback(_v, _k - 1, exp)`
fn call_callback(
	state: &mut State<'_>,
	callback_id: NodeId,
	key_id: NodeId,
	value_id: NodeId,
	expression: NodeId,
) -> NodeId {
	let index = offset_expr(&mut state.ast, key_id, -1.0);
	let args = state.ast.list_from([value_id, index, expression]);
	state.ast.call(callback_id, args)
}

/// `table.create(#expression)`
fn table_create_length(state: &mut State<'_>, expression: NodeId) -> NodeId {
	let callee = state.ast.global_property("table", "create");
	let length = state.ast.unary(UnaryOperator::Length, expression);
	let args = state.ast.list_from([length]);
	state.ast.call(callee, args)
}

/// `table.<name>(args...)`
fn table_call(state: &mut State<'_>, name: &str, args: impl IntoIterator<Item = NodeId>) -> NodeId {
	let callee = state.ast.global_property("table", name);
	let args = state.ast.list_from(args);
	state.ast.call(callee, args)
}

/// `for <ids> in <expression> do <statements> end`
fn generic_for(state: &mut State<'_>, ids: &[NodeId], expression: NodeId, statements: &[NodeId]) -> NodeId {
	let ids = state.ast.list_from(ids.iter().copied());
	let statements = state.ast.list_from(statements.iter().copied());
	state.ast.for_statement(ids, expression, statements)
}

/// `if <condition> then <statements> end`
fn if_then(state: &mut State<'_>, condition: NodeId, statements: &[NodeId]) -> NodeId {
	let statements = state.ast.list_from(statements.iter().copied());
	state.ast.if_statement(condition, statements, None)
}

/// `makeEveryOrSomeMethod`:
///
/// ```lua
/// local _result = <initialState>
/// for _k, _v in exp do
///     if [not] _callback(_v, _k - 1, exp) then
///         _result = <not initialState>
///         break
///     end
/// end
/// ```
fn make_every_or_some_method(
	state: &mut State<'_>,
	expression: NodeId,
	args: &[NodeId],
	initial_state: bool,
) -> NodeId {
	let expression = state.push_to_var_if_complex(expression, "exp");

	let initial = state.ast.bool(initial_state);
	let result_id = state.push_to_var(Some(initial), "result");
	let callback = arg(state, args, 0);
	let callback_id = state.push_to_var_if_non_id(callback, "callback");

	let key_id = state.ast.temp_id("k");
	let value_id = state.ast.temp_id("v");

	let call = call_callback(state, callback_id, key_id, value_id, expression);
	let condition = if initial_state {
		state.ast.unary(UnaryOperator::Not, call)
	} else {
		call
	};
	let flipped = state.ast.bool(!initial_state);
	let assignment = state.ast.assignment(result_id, AssignmentOperator::Assign, flipped);
	let break_statement = state.ast.break_statement();
	let check = if_then(state, condition, &[assignment, break_statement]);
	let loop_statement = generic_for(state, &[key_id, value_id], expression, &[check]);
	state.prereq(loop_statement);

	result_id
}

/// `isEmpty`: `#expression == 0`.
fn is_empty(state: &mut State<'_>, _node: P<Node>, expression: NodeId, _args: Vec<NodeId>) -> NodeId {
	let length = state.ast.unary(UnaryOperator::Length, expression);
	let zero = state.ast.num(0.0);
	state.ast.binary(length, BinaryOperator::Equal, zero)
}

/// `join`: the separator defaults to `", "`. `table.concat` errors on elements that are not
/// strings or numbers, so those arrays get a `tostring()` copy first.
fn join(state: &mut State<'_>, node: P<Node>, mut expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let default_separator = state.ast.str(", ");
	let args = arguments_with_defaults(state, args, &[default_separator]);
	// the ELEMENT type of the array: node.expression.expression is the base, not the call
	let Some(base) = node.as_call_expression().expression.expression() else {
		state.ice("transformer: ReadonlyArray.join: the callee has no base expression");
		return state.ast.none();
	};
	let base_type = state.get_type(base);
	let number_type = state.ck().get_number_type();
	let index_type = state.ck().get_index_type_of_type(base_type, number_type);

	// table.concat only works on string and number types, so call tostring() otherwise
	if let Some(index_type) = index_type
		&& !is_definitely_type(state, index_type, &[TypeCheck::String, TypeCheck::Number])
	{
		expression = state.push_to_var_if_complex(expression, "exp");
		let create = table_create_length(state, expression);
		let id = state.push_to_var(Some(create), "result");
		let key_id = state.ast.temp_id("k");
		let value_id = state.ast.temp_id("v");
		let target = state.ast.computed_index(id, key_id);
		let tostring = state.ast.global_id("tostring");
		let tostring_args = state.ast.list_from([value_id]);
		let value = state.ast.call(tostring, tostring_args);
		let assignment = state.ast.assignment(target, AssignmentOperator::Assign, value);
		let loop_statement = generic_for(state, &[key_id, value_id], expression, &[assignment]);
		state.prereq(loop_statement);

		expression = id;
	}

	let separator = arg(state, &args, 0);
	table_call(state, "concat", [expression, separator])
}

/// `move`: `table.move(exp, sourceStart + 1, sourceEnd + 1, dest + 1[, target])`; the target is
/// an array, not an index, so it is not offset.
fn move_macro(state: &mut State<'_>, _node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let mut move_args = vec![expression];
	for index in 0..3 {
		let argument = arg(state, &args, index);
		move_args.push(offset_expr(&mut state.ast, argument, 1.0));
	}
	if let Some(&target) = args.get(3) {
		move_args.push(target);
	}
	table_call(state, "move", move_args)
}

/// `table.find(exp, value[, fromIndex + 1])`
fn find_call(state: &mut State<'_>, expression: NodeId, args: &[NodeId]) -> NodeId {
	let mut find_args = vec![expression, arg(state, args, 0)];
	if let Some(&from_index) = args.get(1) {
		find_args.push(offset_expr(&mut state.ast, from_index, 1.0));
	}
	table_call(state, "find", find_args)
}

/// `includes`: `table.find(exp, value[, fromIndex + 1]) ~= nil`.
fn includes(state: &mut State<'_>, _node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let find = find_call(state, expression, &args);
	let nil = state.ast.nil();
	state.ast.binary(find, BinaryOperator::NotEqual, nil)
}

/// `indexOf`: `(table.find(exp, value[, fromIndex + 1]) or 0) - 1`.
fn index_of(state: &mut State<'_>, _node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let find = find_call(state, expression, &args);
	let zero = state.ast.num(0.0);
	let found = state.ast.binary(find, BinaryOperator::Or, zero);
	offset_expr(&mut state.ast, found, -1.0)
}

/// `every`
fn every(state: &mut State<'_>, _node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	make_every_or_some_method(state, expression, &args, true)
}

/// `some`
fn some(state: &mut State<'_>, _node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	make_every_or_some_method(state, expression, &args, false)
}

/// `nil` when the call's value is used, nothing in statement position.
fn nil_if_used(state: &mut State<'_>, node: P<Node>) -> NodeId {
	if is_used_as_statement(node) {
		state.ast.none()
	} else {
		state.ast.nil()
	}
}

/// `forEach`: `for _k, _v in exp do callback(_v, _k - 1, exp) end`.
fn for_each(state: &mut State<'_>, node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let expression = state.push_to_var_if_complex(expression, "exp");

	let callback = arg(state, &args, 0);
	let callback_id = state.push_to_var_if_non_id(callback, "callback");
	let key_id = state.ast.temp_id("k");
	let value_id = state.ast.temp_id("v");
	let call = call_callback(state, callback_id, key_id, value_id, expression);
	let call_statement = state.ast.call_statement(call);
	let loop_statement = generic_for(state, &[key_id, value_id], expression, &[call_statement]);
	state.prereq(loop_statement);

	nil_if_used(state, node)
}

/// `map`: `local _newValue = table.create(#exp)`, then `_newValue[_k] = callback(_v, _k - 1, exp)`.
fn map(state: &mut State<'_>, _node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let expression = state.push_to_var_if_complex(expression, "exp");
	let create = table_create_length(state, expression);
	let new_value_id = state.push_to_var(Some(create), "newValue");
	let callback = arg(state, &args, 0);
	let callback_id = state.push_to_var_if_non_id(callback, "callback");
	let key_id = state.ast.temp_id("k");
	let value_id = state.ast.temp_id("v");
	let target = state.ast.computed_index(new_value_id, key_id);
	let call = call_callback(state, callback_id, key_id, value_id, expression);
	let assignment = state.ast.assignment(target, AssignmentOperator::Assign, call);
	let loop_statement = generic_for(state, &[key_id, value_id], expression, &[assignment]);
	state.prereq(loop_statement);

	new_value_id
}

/// `mapFiltered`: declares `newValue`, `callback`, then `length` (byte-relevant order); keeps
/// only non-nil callback results.
fn map_filtered(state: &mut State<'_>, _node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let expression = state.push_to_var_if_complex(expression, "exp");

	let members = state.ast.new_list();
	let empty = state.ast.array(members);
	let new_value_id = state.push_to_var(Some(empty), "newValue");
	let callback = arg(state, &args, 0);
	let callback_id = state.push_to_var_if_non_id(callback, "callback");
	let zero = state.ast.num(0.0);
	let length_id = state.push_to_var(Some(zero), "length");
	let key_id = state.ast.temp_id("k");
	let value_id = state.ast.temp_id("v");
	let result_id = state.ast.temp_id("result");
	let call = call_callback(state, callback_id, key_id, value_id, expression);
	let declaration = state.ast.variable_declaration(result_id, Some(call.into()));
	let nil = state.ast.nil();
	let condition = state.ast.binary(result_id, BinaryOperator::NotEqual, nil);
	let one = state.ast.num(1.0);
	let increment = state.ast.assignment(length_id, AssignmentOperator::Add, one);
	let target = state.ast.computed_index(new_value_id, length_id);
	let store = state.ast.assignment(target, AssignmentOperator::Assign, result_id);
	let check = if_then(state, condition, &[increment, store]);
	let loop_statement = generic_for(state, &[key_id, value_id], expression, &[declaration, check]);
	state.prereq(loop_statement);

	new_value_id
}

/// `filterUndefined`: a generic-for with one id finds the max index, then a numeric for copies
/// the non-nil values.
fn filter_undefined(state: &mut State<'_>, _node: P<Node>, expression: NodeId, _args: Vec<NodeId>) -> NodeId {
	let expression = state.push_to_var_if_complex(expression, "exp");

	let zero = state.ast.num(0.0);
	let length_id = state.push_to_var(Some(zero), "length");
	let index_id1 = state.ast.temp_id("i");
	let condition = state.ast.binary(index_id1, BinaryOperator::Greater, length_id);
	let assignment = state.ast.assignment(length_id, AssignmentOperator::Assign, index_id1);
	let check = if_then(state, condition, &[assignment]);
	let loop_statement = generic_for(state, &[index_id1], expression, &[check]);
	state.prereq(loop_statement);

	let members = state.ast.new_list();
	let empty = state.ast.array(members);
	let result_id = state.push_to_var(Some(empty), "result");
	let zero = state.ast.num(0.0);
	let result_length_id = state.push_to_var(Some(zero), "resultLength");
	let index_id2 = state.ast.temp_id("i");
	let value_id = state.ast.temp_id("v");
	let start = state.ast.num(1.0);
	let indexable = convert_to_indexable_expression(&mut state.ast, expression);
	let element = state.ast.computed_index(indexable, index_id2);
	let declaration = state.ast.variable_declaration(value_id, Some(element.into()));
	let nil = state.ast.nil();
	let condition = state.ast.binary(value_id, BinaryOperator::NotEqual, nil);
	let one = state.ast.num(1.0);
	let increment = state.ast.assignment(result_length_id, AssignmentOperator::Add, one);
	let target = state.ast.computed_index(result_id, result_length_id);
	let store = state.ast.assignment(target, AssignmentOperator::Assign, value_id);
	let check = if_then(state, condition, &[increment, store]);
	let statements = state.ast.list_from([declaration, check]);
	let loop_statement = state.ast.numeric_for(index_id2, start, length_id, None, statements);
	state.prereq(loop_statement);

	result_id
}

/// `callback(_v, _i - 1, exp) == true`
fn callback_is_true(
	state: &mut State<'_>,
	callback_id: NodeId,
	key_id: NodeId,
	value_id: NodeId,
	expression: NodeId,
) -> NodeId {
	let call = call_callback(state, callback_id, key_id, value_id, expression);
	let true_value = state.ast.bool(true);
	state.ast.binary(call, BinaryOperator::Equal, true_value)
}

/// `filter`: like `mapFiltered`, but the condition is the explicit `callback(...) == true` and the
/// kept value is the element itself.
fn filter(state: &mut State<'_>, _node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let expression = state.push_to_var_if_complex(expression, "exp");

	let members = state.ast.new_list();
	let empty = state.ast.array(members);
	let new_value_id = state.push_to_var(Some(empty), "newValue");
	let callback = arg(state, &args, 0);
	let callback_id = state.push_to_var_if_non_id(callback, "callback");
	let zero = state.ast.num(0.0);
	let length_id = state.push_to_var(Some(zero), "length");
	let key_id = state.ast.temp_id("k");
	let value_id = state.ast.temp_id("v");
	let condition = callback_is_true(state, callback_id, key_id, value_id, expression);
	let one = state.ast.num(1.0);
	let increment = state.ast.assignment(length_id, AssignmentOperator::Add, one);
	let target = state.ast.computed_index(new_value_id, length_id);
	let store = state.ast.assignment(target, AssignmentOperator::Assign, value_id);
	let check = if_then(state, condition, &[increment, store]);
	let loop_statement = generic_for(state, &[key_id, value_id], expression, &[check]);
	state.prereq(loop_statement);

	new_value_id
}

/// `reduce`: without an initial value, an empty array raises a runtime `error(...)`, the result
/// is seeded from `exp[1]` and the numeric for starts at 2; with one, the result is seeded from it
/// and the loop starts at 1. The callback is pushed with `pushToVar` (not `IfNonId`) after the
/// result temp, and the step of 1 is omitted.
fn reduce(state: &mut State<'_>, _node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let expression = state.push_to_var_if_complex(expression, "exp");

	let mut start = state.ast.num(1.0);
	let end = state.ast.unary(UnaryOperator::Length, expression);
	let step = 1.0;

	let length_exp = state.ast.unary(UnaryOperator::Length, expression);

	// if there was no initialValue supplied
	let result_id = if args.len() < 2 {
		let zero = state.ast.num(0.0);
		let condition = state.ast.binary(length_exp, BinaryOperator::Equal, zero);
		let error = state.ast.global_id("error");
		let message = state
			.ast
			.str("Attempted to call `ReadonlyArray.reduce()` on an empty array without an initialValue.");
		let error_args = state.ast.list_from([message]);
		let call = state.ast.call(error, error_args);
		let call_statement = state.ast.call_statement(call);
		let check = if_then(state, condition, &[call_statement]);
		state.prereq(check);
		let indexable = convert_to_indexable_expression(&mut state.ast, expression);
		let first = state.ast.computed_index(indexable, start);
		let result_id = state.push_to_var(Some(first), "result");
		start = offset_expr(&mut state.ast, start, step);
		result_id
	} else {
		let initial_value = arg(state, &args, 1);
		state.push_to_var(Some(initial_value), "result")
	};
	let callback = arg(state, &args, 0);
	let callback_id = state.push_to_var(Some(callback), "callback");

	let iterator_id = state.ast.temp_id("i");
	let indexable = convert_to_indexable_expression(&mut state.ast, expression);
	let element = state.ast.computed_index(indexable, iterator_id);
	let index = offset_expr(&mut state.ast, iterator_id, -1.0);
	let call_args = state.ast.list_from([result_id, element, index, expression]);
	let call = state.ast.call(callback_id, call_args);
	let assignment = state.ast.assignment(result_id, AssignmentOperator::Assign, call);
	let statements = state.ast.list_from([assignment]);
	// step === 1 -> omitted
	let loop_statement = state.ast.numeric_for(iterator_id, start, end, None, statements);
	state.prereq(loop_statement);

	result_id
}

/// `find`: a valueless `local _result`, assigned the first element whose
/// `callback(_v, _i - 1, exp) == true`.
fn find(state: &mut State<'_>, _node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let expression = state.push_to_var_if_complex(expression, "exp");

	let callback = arg(state, &args, 0);
	let callback_id = state.push_to_var_if_non_id(callback, "callback");
	let loop_id = state.ast.temp_id("i");
	let value_id = state.ast.temp_id("v");
	let result_id = state.push_to_var(None, "result");

	let condition = callback_is_true(state, callback_id, loop_id, value_id, expression);
	let assignment = state.ast.assignment(result_id, AssignmentOperator::Assign, value_id);
	let break_statement = state.ast.break_statement();
	let check = if_then(state, condition, &[assignment, break_statement]);
	let loop_statement = generic_for(state, &[loop_id, value_id], expression, &[check]);
	state.prereq(loop_statement);

	result_id
}

/// `findIndex`: like `find`, but `local _result = -1` and the stored value is the 0-based index.
fn find_index(state: &mut State<'_>, _node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let expression = state.push_to_var_if_complex(expression, "exp");

	let callback = arg(state, &args, 0);
	let callback_id = state.push_to_var_if_non_id(callback, "callback");
	let loop_id = state.ast.temp_id("i");
	let value_id = state.ast.temp_id("v");
	let minus_one = state.ast.num(-1.0);
	let result_id = state.push_to_var(Some(minus_one), "result");

	let condition = callback_is_true(state, callback_id, loop_id, value_id, expression);
	let index = offset_expr(&mut state.ast, loop_id, -1.0);
	let assignment = state.ast.assignment(result_id, AssignmentOperator::Assign, index);
	let break_statement = state.ast.break_statement();
	let check = if_then(state, condition, &[assignment, break_statement]);
	let loop_statement = generic_for(state, &[loop_id, value_id], expression, &[check]);
	state.prereq(loop_statement);

	result_id
}

/// `readonlyArrayMethods` (`READONLY_ARRAY_METHODS`).
pub(super) const READONLY_ARRAY_METHODS: &[(&str, PropertyCallMacro)] = &[
	("isEmpty", is_empty),
	("join", join),
	("move", move_macro),
	("includes", includes),
	("indexOf", index_of),
	("every", every),
	("some", some),
	("forEach", for_each),
	("map", map),
	("mapFiltered", map_filtered),
	("filterUndefined", filter_undefined),
	("filter", filter),
	("reduce", reduce),
	("find", find),
	("findIndex", find_index),
];
