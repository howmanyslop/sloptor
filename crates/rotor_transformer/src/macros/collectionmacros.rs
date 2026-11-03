//! `collectionmacros.go`: the `Set`, `Map` and `Promise` region of
//! `macros/propertyCallMacros.ts` (`READONLY_SET_MAP_SHARED_METHODS`, `SET_MAP_SHARED_METHODS`,
//! `READONLY_SET_METHODS`, `SET_METHODS`, `READONLY_MAP_METHODS`, `MAP_METHODS`,
//! `PROMISE_METHODS`). Go's `mergeMacroLists` composition is spelled out in each table.
//!
//! `Set.forEach(cb)` calls `cb(value, value, set)` from `for _v in exp do`; `Map.forEach(cb)`
//! calls `cb(value, key, map)` from `for _k, _v in exp do`.

#![expect(
	clippy::needless_pass_by_value,
	reason = "the macro tables fix the `CallMacro` signature, which takes the arguments by value"
)]

use super::arg;
use super::arraymacros2::clear;
use crate::call::convert_to_indexable_expression;
use crate::conditional::is_used_as_statement;
use crate::macromanager::PropertyCallMacro;
use crate::prelude::*;

/// `expression[index]`, parenthesizing a non-indexable base.
fn index_into(state: &mut State<'_>, expression: NodeId, index: NodeId) -> NodeId {
	let indexable = convert_to_indexable_expression(&mut state.ast, expression);
	state.ast.computed_index(indexable, index)
}

/// `nil` when the call's value is used, nothing in statement position.
fn nil_if_used(state: &mut State<'_>, node: P<Node>) -> NodeId {
	if is_used_as_statement(node) {
		state.ast.none()
	} else {
		state.ast.nil()
	}
}

/// `isEmpty`: `next(exp) == nil`.
fn is_empty(state: &mut State<'_>, _node: P<Node>, expression: NodeId, _args: Vec<NodeId>) -> NodeId {
	let next = state.ast.global_id("next");
	let args = state.ast.list_from([expression]);
	let call = state.ast.call(next, args);
	let nil = state.ast.nil();
	state.ast.binary(call, BinaryOperator::Equal, nil)
}

/// `size`: there is no `#` for hash parts, so `local _size = 0` counts in `for _ in exp do`.
fn size(state: &mut State<'_>, _node: P<Node>, expression: NodeId, _args: Vec<NodeId>) -> NodeId {
	let zero = state.ast.num(0.0);
	let size_id = state.push_to_var(Some(zero), "size");
	let id = state.ast.temp_id("");
	let ids = state.ast.list_from([id]);
	let one = state.ast.num(1.0);
	let increment = state.ast.assignment(size_id, AssignmentOperator::Add, one);
	let statements = state.ast.list_from([increment]);
	let loop_statement = state.ast.for_statement(ids, expression, statements);
	state.prereq(loop_statement);
	size_id
}

/// `has`: `exp[key] ~= nil`.
fn has(state: &mut State<'_>, _node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let key = arg(state, &args, 0);
	let element = index_into(state, expression, key);
	let nil = state.ast.nil();
	state.ast.binary(element, BinaryOperator::NotEqual, nil)
}

/// `delete`: the key temp first; when the result is used, the base goes through
/// `pushToVarIfNonId` (not `IfComplex`) and `local _valueExisted = exp[_value] ~= nil` snapshots
/// existence before the clear.
fn delete(state: &mut State<'_>, node: P<Node>, mut expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let key = arg(state, &args, 0);
	let key = state.push_to_var_if_complex(key, "value");
	let value_is_used = !is_used_as_statement(node);
	let value_existed_id = if value_is_used {
		expression = state.push_to_var_if_non_id(expression, "exp");
		let element = index_into(state, expression, key);
		let nil = state.ast.nil();
		let existed = state.ast.binary(element, BinaryOperator::NotEqual, nil);
		Some(state.push_to_var(Some(existed), "valueExisted"))
	} else {
		None
	};

	let slot = index_into(state, expression, key);
	let nil = state.ast.nil();
	let clear = state.ast.assignment(slot, AssignmentOperator::Assign, nil);
	state.prereq(clear);

	match value_existed_id {
		Some(value_existed_id) => value_existed_id,
		None => state.ast.none(),
	}
}

/// `ReadonlySet.forEach`: `for _v in exp do callback(_v, _v, exp) end`.
fn set_for_each(state: &mut State<'_>, node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let expression = state.push_to_var_if_complex(expression, "exp");

	let callback = arg(state, &args, 0);
	let callback_id = state.push_to_var_if_non_id(callback, "callback");
	let value_id = state.ast.temp_id("v");
	let ids = state.ast.list_from([value_id]);
	let call_args = state.ast.list_from([value_id, value_id, expression]);
	let call = state.ast.call(callback_id, call_args);
	let call_statement = state.ast.call_statement(call);
	let statements = state.ast.list_from([call_statement]);
	let loop_statement = state.ast.for_statement(ids, expression, statements);
	state.prereq(loop_statement);

	nil_if_used(state, node)
}

/// `Set.add`: `exp[value] = true`; a used value is the set itself (for chaining).
fn add(state: &mut State<'_>, node: P<Node>, mut expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let value_is_used = !is_used_as_statement(node);
	if value_is_used {
		expression = state.push_to_var_if_complex(expression, "exp");
	}
	let value = arg(state, &args, 0);
	let slot = index_into(state, expression, value);
	let true_value = state.ast.bool(true);
	let assignment = state.ast.assignment(slot, AssignmentOperator::Assign, true_value);
	state.prereq(assignment);
	if value_is_used { expression } else { state.ast.none() }
}

/// `ReadonlyMap.forEach`: `for _k, _v in exp do callback(_v, _k, exp) end`.
fn map_for_each(state: &mut State<'_>, node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let expression = state.push_to_var_if_complex(expression, "exp");

	let callback = arg(state, &args, 0);
	let callback_id = state.push_to_var_if_non_id(callback, "callback");
	let key_id = state.ast.temp_id("k");
	let value_id = state.ast.temp_id("v");
	let ids = state.ast.list_from([key_id, value_id]);
	let call_args = state.ast.list_from([value_id, key_id, expression]);
	let call = state.ast.call(callback_id, call_args);
	let call_statement = state.ast.call_statement(call);
	let statements = state.ast.list_from([call_statement]);
	let loop_statement = state.ast.for_statement(ids, expression, statements);
	state.prereq(loop_statement);

	nil_if_used(state, node)
}

/// `ReadonlyMap.get`: `exp[key]`.
fn get(state: &mut State<'_>, _node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let key = arg(state, &args, 0);
	index_into(state, expression, key)
}

/// `Map.set`: `exp[key] = value`; a used value is the map itself (for chaining).
fn set(state: &mut State<'_>, node: P<Node>, mut expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let key_exp = arg(state, &args, 0);
	let value_exp = arg(state, &args, 1);
	let value_is_used = !is_used_as_statement(node);
	if value_is_used {
		expression = state.push_to_var_if_complex(expression, "exp");
	}
	let slot = index_into(state, expression, key_exp);
	let assignment = state.ast.assignment(slot, AssignmentOperator::Assign, value_exp);
	state.prereq(assignment);
	if value_is_used { expression } else { state.ast.none() }
}

/// `Promise.then`: `exp:andThen(...)` (`then` is a Luau keyword, and the runtime class names it
/// `andThen`).
fn then(state: &mut State<'_>, _node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let base = convert_to_indexable_expression(&mut state.ast, expression);
	let args = state.ast.list_from(args);
	state.ast.method_call("andThen", base, args)
}

/// `readonlySetMethods` (`READONLY_SET_METHODS`): the shared readonly methods plus `forEach`.
pub(super) const READONLY_SET_METHODS: &[(&str, PropertyCallMacro)] = &[
	("isEmpty", is_empty),
	("size", size),
	("has", has),
	("forEach", set_for_each),
];

/// `setMethods` (`SET_METHODS`): the shared mutable methods plus `add`.
pub(super) const SET_METHODS: &[(&str, PropertyCallMacro)] = &[("delete", delete), ("clear", clear), ("add", add)];

/// `readonlyMapMethods` (`READONLY_MAP_METHODS`): the shared readonly methods plus `forEach` and
/// `get`.
pub(super) const READONLY_MAP_METHODS: &[(&str, PropertyCallMacro)] = &[
	("isEmpty", is_empty),
	("size", size),
	("has", has),
	("forEach", map_for_each),
	("get", get),
];

/// `mapMethods` (`MAP_METHODS`): the shared mutable methods plus `set`.
pub(super) const MAP_METHODS: &[(&str, PropertyCallMacro)] = &[("delete", delete), ("clear", clear), ("set", set)];

/// `promiseMethods` (`PROMISE_METHODS`).
pub(super) const PROMISE_METHODS: &[(&str, PropertyCallMacro)] = &[("then", then)];
