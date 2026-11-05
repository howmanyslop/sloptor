//! `spreaddestruct.go`: the rest element of array and object destructuring.

use rotor_luau::{ComputedIndexExpression, PropertyAccessExpression};
use rotor_tsrs::types::TypeCheck;

use super::bindingiterator::{BindingState, step};
use super::num_from_index;
use crate::prelude::*;
use crate::types::is_definitely_type;

/// `spreadDestructor`: the value of a rest element at `index`.
pub type SpreadDestructor = fn(&mut State<'_>, NodeId, usize, &mut BindingState) -> NodeId;

/// `getSpreadDestructorForType`: arrays slice with `table.move`; iterator-protocol patterns
/// drain the iterator; anything else has no rest support.
pub fn get_spread_destructor_for_type(
	state: &mut State<'_>,
	t: P<Type>,
	binding_state: &BindingState,
) -> Option<SpreadDestructor> {
	if is_definitely_type(state, t, &[TypeCheck::Array]) {
		return Some(spread_destructure_array);
	}
	if binding_state.iterator.is_some() {
		return Some(spread_destructure_iterator);
	}
	None
}

/// `spreadDestructureArray`: `table.move(parent, index + 1, #parent, 1, {})`.
fn spread_destructure_array(state: &mut State<'_>, parent_id: NodeId, index: usize, _: &mut BindingState) -> NodeId {
	let table_move = state.ast.global_property("table", "move");
	let start = num_from_index(&mut state.ast, index, 1);
	let length = state.ast.unary(UnaryOperator::Length, parent_id);
	let one = state.ast.num(1.0);
	let members = state.ast.new_list();
	let destination = state.ast.array(members);
	let args = state.ast.list_from([parent_id, start, length, one, destination]);
	state.ast.call(table_move, args)
}

/// `spreadDestructureIterator`: `local _rest = {}`, `local _length = 0`, then a
/// `while not _done do` loop appending every remaining value.
fn spread_destructure_iterator(state: &mut State<'_>, _: NodeId, _: usize, binding_state: &mut BindingState) -> NodeId {
	let Some(iterator) = binding_state.iterator else {
		state.ice("transformer: spreadDestructureIterator: no iterator state");
		return state.ast.none();
	};
	let members = state.ast.new_list();
	let empty = state.ast.array(members);
	let rest = state.push_to_var(Some(empty), "rest");
	let zero = state.ast.num(0.0);
	let length = state.push_to_var(Some(zero), "length");
	let body = state.capture_statements(|state| {
		let (value, done) = step(state, iterator.step, false);
		let assignment = state.ast.assignment(iterator.done, AssignmentOperator::Assign, done);
		state.prereq(assignment);
		let break_statement = state.ast.break_statement();
		let break_list = state.ast.list_from([break_statement]);
		let check = state.ast.if_statement(iterator.done, break_list, None);
		state.prereq(check);
		let one = state.ast.num(1.0);
		let increment = state.ast.assignment(length, AssignmentOperator::Add, one);
		state.prereq(increment);
		let slot = state.ast.computed_index(rest, length);
		let append = state.ast.assignment(slot, AssignmentOperator::Assign, value);
		state.prereq(append);
	});
	let not_done = state.ast.unary(UnaryOperator::Not, iterator.done);
	let loop_statement = state.ast.while_statement(not_done, body);
	state.prereq(loop_statement);
	rest
}

/// `spreadDestructureObject`: copies every key of `parent_id` not named before the rest element
/// into `local _rest = {}`, skipping the `_extracted` set.
pub fn spread_destructure_object(state: &mut State<'_>, parent_id: NodeId, pre_spread_names: &[NodeId]) -> NodeId {
	let extracted_members = state.ast.new_list::<element::Expression>();
	for &expression in pre_spread_names {
		if let Some(access) = state.ast.get::<PropertyAccessExpression>(expression) {
			let name = access.name.clone();
			let key = state.ast.str(name);
			state.ast.list_push(extracted_members, key);
		} else if let Some(&ComputedIndexExpression { index, .. }) =
			state.ast.get::<ComputedIndexExpression>(expression)
		{
			state.ast.list_push(extracted_members, index);
		} else {
			state.ice("transformer: spreadDestructureObject: unknown expression type");
			return state.ast.none();
		}
	}
	let set = state.ast.set(extracted_members);
	let extracted = state.push_to_var(Some(set), "extracted");
	let fields = state.ast.new_list();
	let map = state.ast.map(fields);
	let rest = state.push_to_var(Some(map), "rest");
	let key_id = state.ast.temp_id("k");
	let value_id = state.ast.temp_id("v");
	let ids = state.ast.list_from([key_id, value_id]);
	let is_extracted = state.ast.computed_index(extracted, key_id);
	let condition = state.ast.unary(UnaryOperator::Not, is_extracted);
	let slot = state.ast.computed_index(rest, key_id);
	let copy = state.ast.assignment(slot, AssignmentOperator::Assign, value_id);
	let copy = state.ast.list_from([copy]);
	let check = state.ast.if_statement(condition, copy, None);
	let body = state.ast.list_from([check]);
	let loop_statement = state.ast.for_statement(ids, parent_id, body);
	state.prereq(loop_statement);
	rest
}
