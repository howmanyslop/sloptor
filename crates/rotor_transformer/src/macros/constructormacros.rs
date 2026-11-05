//! `constructormacros.go`: `macros/constructorMacros.ts`.

use rotor_tsrs::ast as tsast;

use crate::dispatch::transform_expression;
use crate::ensureorder::ensure_transform_order;
use crate::macromanager::ConstructorMacro;
use crate::prelude::*;
use crate::util::nodes;

/// `wrapWeak`: `setmetatable(<macro result>, { __mode = "k" })`. `__mode = "k"` for both
/// `WeakSet` and `WeakMap` (upstream quirk: weak keys only).
fn wrap_weak(state: &mut State<'_>, node: P<Node>, macro_fn: ConstructorMacro) -> NodeId {
	let callee = state.ast.global_id("setmetatable");
	let value = macro_fn(state, node);
	let mode = state.ast.str("__mode");
	let k = state.ast.str("k");
	let field = state.ast.map_field(mode, k);
	let fields = state.ast.list_from([field]);
	let metatable = state.ast.map(fields);
	let args = state.ast.list_from([value, metatable]);
	state.ast.call(callee, args)
}

/// `WeakSetConstructor`
pub(super) fn weak_set_constructor(state: &mut State<'_>, node: P<Node>) -> NodeId {
	wrap_weak(state, node, set_constructor)
}

/// `WeakMapConstructor`
pub(super) fn weak_map_constructor(state: &mut State<'_>, node: P<Node>) -> NodeId {
	wrap_weak(state, node, map_constructor)
}

/// `arrayConstructorMacro`: `table.create(<args>)` with arguments, else `{}`.
pub(super) fn array_constructor(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let arguments = nodes(node.as_new_expression().arguments());
	if !arguments.is_empty() {
		let args = ensure_transform_order(state, arguments);
		let callee = state.ast.global_property("table", "create");
		let args = state.ast.list_from(args);
		return state.ast.call(callee, args);
	}
	let members = state.ast.new_list();
	state.ast.array(members)
}

/// `arrayLiteralHasSpread`
fn array_literal_has_spread(array_literal: P<Node>) -> bool {
	array_literal
		.as_array_literal_expression()
		.elements
		.nodes()
		.iter()
		.any(|&element| tsast::is_spread_element(element))
}

/// `setConstructorMacro`: no arguments is `{}`; a spread-free TS array literal is a set literal;
/// anything else fills `local set = {}` with a prereq loop. The decision is on the TS AST.
pub(super) fn set_constructor(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let Some(&argument) = nodes(node.as_new_expression().arguments()).first() else {
		let members = state.ast.new_list();
		return state.ast.set(members);
	};
	if tsast::is_array_literal_expression(argument) && !array_literal_has_spread(argument) {
		let elements = ensure_transform_order(state, argument.as_array_literal_expression().elements.nodes());
		let members = state.ast.list_from(elements);
		return state.ast.set(members);
	}
	let members = state.ast.new_list();
	let empty = state.ast.set(members);
	let id = state.push_to_var(Some(empty), "set");
	let value_id = state.ast.temp_id("v");
	let key_id = state.ast.temp_id("");
	let ids = state.ast.list_from([key_id, value_id]);
	let iterable = transform_expression(state, argument);
	let target = state.ast.computed_index(id, value_id);
	let true_value = state.ast.bool(true);
	let assignment = state.ast.assignment(target, AssignmentOperator::Assign, true_value);
	let statements = state.ast.list_from([assignment]);
	let loop_statement = state.ast.for_statement(ids, iterable, statements);
	state.prereq(loop_statement);
	id
}

/// `mapConstructorMacro`: no arguments is `{}`; a transformed argument that is a Luau array of
/// arrays is a map literal; anything else fills `local map = {}` with a prereq loop. The
/// decision is on the transformed Luau AST.
pub(super) fn map_constructor(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let Some(&argument) = nodes(node.as_new_expression().arguments()).first() else {
		let fields = state.ast.new_list();
		return state.ast.map(fields);
	};
	let transformed = transform_expression(state, argument);
	let members = state
		.ast
		.get::<rotor_luau::Array>(transformed)
		.map(|array| array.members)
		.filter(|&members| {
			state
				.ast
				.list_every(members, |member| state.ast.kind(member) == LuauKind::Array)
		});
	if let Some(members) = members {
		let fields = state.ast.new_list();
		for member in state.ast.list_to_vec(members) {
			// each member array always has 2 members, due to map constructor typing
			let entry = state
				.ast
				.get::<rotor_luau::Array>(member)
				.map(|array| state.ast.list_to_vec(array.members));
			let (Some(&key), Some(&value)) = (
				entry.as_ref().and_then(|entry| entry.first()),
				entry.as_ref().and_then(|entry| entry.get(1)),
			) else {
				state.ice("transformer: MapConstructor entry without 2 elements");
				return state.ast.none();
			};
			let field = state.ast.map_field(key, value);
			state.ast.list_push(fields, field);
		}
		return state.ast.map(fields);
	}
	let fields = state.ast.new_list();
	let empty = state.ast.map(fields);
	let id = state.push_to_var(Some(empty), "map");
	let value_id = state.ast.temp_id("v");
	let key_id = state.ast.temp_id("");
	let ids = state.ast.list_from([key_id, value_id]);
	let one = state.ast.num(1.0);
	let key = state.ast.computed_index(value_id, one);
	let target = state.ast.computed_index(id, key);
	let two = state.ast.num(2.0);
	let value = state.ast.computed_index(value_id, two);
	let assignment = state.ast.assignment(target, AssignmentOperator::Assign, value);
	let statements = state.ast.list_from([assignment]);
	let loop_statement = state.ast.for_statement(ids, transformed, statements);
	state.prereq(loop_statement);
	id
}
