//! `pointer.go`: `util/pointer.ts`'s inline-until-disabled table builders.

use rotor_luau::{Array, Map};

use crate::prelude::*;

/// `MapPointer`: `value` is a map literal while the table is built inline, and a temporary after
/// [`disable_map_inline`] materializes it into `local <name> = { ... }`.
#[derive(Clone, Debug)]
pub struct MapPointer {
	/// The temp's name hint.
	pub name: String,
	/// A `Map` node or a temporary identifier.
	pub value: NodeId,
}

/// `ArrayPointer`: the same mechanics with an array literal.
#[derive(Clone, Debug)]
pub struct ArrayPointer {
	/// The temp's name hint.
	pub name: String,
	/// An `Array` node or a temporary identifier.
	pub value: NodeId,
}

/// `CreateMapPointer`: starts as an empty inline map literal.
pub fn create_map_pointer(state: &mut State<'_>, name: &str) -> MapPointer {
	let fields = state.ast.new_list();
	MapPointer {
		name: name.to_owned(),
		value: state.ast.map(fields),
	}
}

/// `CreateArrayPointer`
pub fn create_array_pointer(state: &mut State<'_>, name: &str) -> ArrayPointer {
	let members = state.ast.new_list();
	ArrayPointer {
		name: name.to_owned(),
		value: state.ast.array(members),
	}
}

/// `AssignToMapPointer`: pushes a field while the pointer is inline, else emits
/// `ptr[left] = right`.
pub fn assign_to_map_pointer(state: &mut State<'_>, pointer: &MapPointer, left: NodeId, right: NodeId) {
	if let Some(map) = state.ast.get::<Map>(pointer.value) {
		let fields = map.fields;
		let field = state.ast.map_field(left, right);
		state.ast.list_push(fields, field);
	} else {
		let index = state.ast.computed_index(pointer.value, left);
		let assignment = state.ast.assignment(index, AssignmentOperator::Assign, right);
		state.prereq(assignment);
	}
}

/// `DisableMapInline`: pins a still-inline map to `local <name> = { ...so far }`.
pub fn disable_map_inline(state: &mut State<'_>, pointer: &mut MapPointer) {
	if state.ast.get::<Map>(pointer.value).is_some() {
		pointer.value = state.push_to_var(Some(pointer.value), &pointer.name);
	}
}

/// `DisableArrayInline`
pub fn disable_array_inline(state: &mut State<'_>, pointer: &mut ArrayPointer) {
	if state.ast.get::<Array>(pointer.value).is_some() {
		pointer.value = state.push_to_var(Some(pointer.value), &pointer.name);
	}
}
