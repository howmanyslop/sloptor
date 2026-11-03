//! `bindingiterator.go`: the iterator-protocol destructuring state shared by the `Set`, `Map`,
//! `SharedTable`, generator and object accessors and by the rest destructor.

use crate::prelude::*;

/// `bindingState`: per-pattern accessor progress. `matcher` is the string accessor's
/// `string.gmatch` temp; `iterator` is set for the iterator-protocol accessors (Go's
/// `cursor`/`consumed`/`done`/`step` fields).
#[derive(Debug, Default)]
pub struct BindingState {
	/// The string accessor's matcher temp.
	pub(crate) matcher: Option<NodeId>,
	/// The iterator-protocol state; Go's non-nil `step`.
	pub(crate) iterator: Option<IteratorState>,
}

/// The iterator-protocol half of Go's `bindingState`.
#[derive(Clone, Copy, Debug)]
pub struct IteratorState {
	/// The `_done` temp.
	pub(crate) done: NodeId,
	/// Which `step` closure Go builds.
	pub(crate) step: Step,
}

/// Go's `step` closures.
#[derive(Clone, Copy, Debug)]
pub enum Step {
	/// `collectionBindingState`: `next(parent, cursor)`, skipping consumed keys.
	Collection {
		/// The destructured collection.
		parent_id: NodeId,
		/// The `_key` temp.
		cursor: NodeId,
		/// The `_consumed` temp.
		consumed: NodeId,
		/// Whether entries are `{ key, value }` pairs.
		is_map: bool,
	},
	/// `generatorBindingState`: `next()` results.
	Generator {
		/// The `_next` temp.
		next: NodeId,
	},
}

/// `collectionBindingState`: `local _key`, `local _consumed = {}`, `local _done = false`.
pub fn collection_binding_state(state: &mut State<'_>, parent_id: NodeId, is_map: bool) -> BindingState {
	let cursor = state.push_to_var(None, "key");
	let members = state.ast.new_list();
	let empty = state.ast.array(members);
	let consumed = state.push_to_var(Some(empty), "consumed");
	let false_literal = state.ast.bool(false);
	let done = state.push_to_var(Some(false_literal), "done");
	BindingState {
		matcher: None,
		iterator: Some(IteratorState {
			done,
			step: Step::Collection {
				parent_id,
				cursor,
				consumed,
				is_map,
			},
		}),
	}
}

/// `generatorBindingState`: `local _next = parent.next`, `local _done = false`.
pub fn generator_binding_state(state: &mut State<'_>, parent_id: NodeId) -> BindingState {
	let next_access = state.ast.property_access(parent_id, "next");
	let next = state.push_to_var(Some(next_access), "next");
	let false_literal = state.ast.bool(false);
	let done = state.push_to_var(Some(false_literal), "done");
	BindingState {
		matcher: None,
		iterator: Some(IteratorState {
			done,
			step: Step::Generator { next },
		}),
	}
}

/// One step of the iterator protocol: emits the advance and returns `(value, done)`.
pub fn step(state: &mut State<'_>, step: Step, omitted: bool) -> (NodeId, NodeId) {
	match step {
		Step::Collection {
			parent_id,
			cursor,
			consumed,
			is_map,
		} => {
			let member = (is_map && !omitted).then(|| state.push_to_var(None, "value"));
			let value = match member {
				Some(member) => {
					let members = state.ast.list_from([cursor, member]);
					state.ast.array(members)
				}
				None => cursor,
			};
			let advance = |state: &mut State<'_>| {
				let target: NodeOrList = match member {
					Some(member) => state.ast.list_from::<element::AnyIdentifier>([cursor, member]).into(),
					None => cursor.into(),
				};
				let next = state.ast.global_id("next");
				let args = state.ast.list_from([parent_id, cursor]);
				let call = state.ast.call(next, args);
				state.ast.assignment(target, AssignmentOperator::Assign, call)
			};
			// A computed destination can delete the cursor; recover without repeating consumed keys.
			let nil = state.ast.nil();
			let cursor_present = state.ast.binary(cursor, BinaryOperator::NotEqual, nil);
			let read = state.ast.computed_index(parent_id, cursor);
			let nil = state.ast.nil();
			let entry_missing = state.ast.binary(read, BinaryOperator::Equal, nil);
			let condition = state.ast.binary(cursor_present, BinaryOperator::And, entry_missing);
			let nil = state.ast.nil();
			let reset = state.ast.assignment(cursor, AssignmentOperator::Assign, nil);
			let reset = state.ast.list_from([reset]);
			let recover = state.ast.if_statement(condition, reset, None);
			state.prereq(recover);
			let first = advance(state);
			state.prereq(first);
			let nil = state.ast.nil();
			let cursor_present = state.ast.binary(cursor, BinaryOperator::NotEqual, nil);
			let seen = state.ast.computed_index(consumed, cursor);
			let condition = state.ast.binary(cursor_present, BinaryOperator::And, seen);
			let again = advance(state);
			let body = state.ast.list_from([again]);
			let skip = state.ast.while_statement(condition, body);
			state.prereq(skip);
			let nil = state.ast.nil();
			let done = state.ast.binary(cursor, BinaryOperator::Equal, nil);
			(value, done)
		}
		Step::Generator { next } => {
			let args = state.ast.new_list();
			let call = state.ast.call(next, args);
			let result = state.push_to_var(Some(call), "result");
			let value = state.ast.property_access(result, "value");
			let done = state.ast.property_access(result, "done");
			(value, done)
		}
	}
}

/// `iteratorBindingAccessor`: `if not _done then <step> _done = <done>; if not _done then
/// <record and bind> end end`.
pub fn iterator_binding_accessor(
	state: &mut State<'_>,
	_: NodeId,
	_: usize,
	binding_state: &mut BindingState,
	omitted: bool,
) -> NodeId {
	let Some(iterator) = binding_state.iterator else {
		state.ice("transformer: iteratorBindingAccessor: no iterator state");
		return state.ast.none();
	};
	let result = (!omitted).then(|| state.push_to_var(None, "value"));
	let body = state.capture_statements(|state| {
		let (value, done) = step(state, iterator.step, omitted);
		let assignment = state.ast.assignment(iterator.done, AssignmentOperator::Assign, done);
		state.prereq(assignment);
		let active = state.ast.new_list();
		if let Step::Collection { cursor, consumed, .. } = iterator.step {
			let slot = state.ast.computed_index(consumed, cursor);
			let true_literal = state.ast.bool(true);
			let record = state.ast.assignment(slot, AssignmentOperator::Assign, true_literal);
			state.ast.list_push(active, record);
		}
		if let Some(result) = result {
			let bind = state.ast.assignment(result, AssignmentOperator::Assign, value);
			state.ast.list_push(active, bind);
		}
		if state.ast.list_is_non_empty(active) {
			let not_done = state.ast.unary(UnaryOperator::Not, iterator.done);
			let statement = state.ast.if_statement(not_done, active, None);
			state.prereq(statement);
		}
	});
	let not_done = state.ast.unary(UnaryOperator::Not, iterator.done);
	let statement = state.ast.if_statement(not_done, body, None);
	state.prereq(statement);
	match result {
		Some(result) => result,
		None => state.ast.none(),
	}
}
