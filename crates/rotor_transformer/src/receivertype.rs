//! `receivertype.go`: whether a `this` type can change the call convention.

use rotor_tsrs::checker::{TypeFlags, conditional_root_check_type, conditional_root_node};

use crate::prelude::*;
use crate::types::get_type_arguments;

fn contains_instantiable_type(t: P<Type>) -> bool {
	if t.is_union() || t.is_intersection() {
		return t.types().iter().any(|&member| contains_instantiable_type(member));
	}
	t.flags().intersects(TypeFlags::Instantiable)
}

/// `canChangeReceiverConvention`: a generic `this` type that could become `void`.
pub fn can_change_receiver_convention(state: &mut State<'_>, t: P<Type>) -> bool {
	if !contains_instantiable_type(t) {
		return false;
	}
	if t.is_union()
		&& t.types().iter().any(|&member| {
			!member.flags().intersects(TypeFlags::Void | TypeFlags::Never) && !contains_instantiable_type(member)
		}) {
		return false;
	}
	if t.flags().intersects(TypeFlags::Conditional) {
		let conditional = t.as_conditional_type();
		let mut check_type = conditional_root_check_type(t);
		let mut extends_type = conditional.extends_type();
		if let (Some(check), Some(extends)) = (check_type, extends_type)
			&& check.is_tuple_type()
			&& extends.is_tuple_type()
		{
			let check_elements = get_type_arguments(state, check);
			let extends_elements = get_type_arguments(state, extends);
			if check_elements.len() == 1 && extends_elements.len() == 1 {
				check_type = Some(check_elements[0]);
				extends_type = Some(extends_elements[0]);
			}
		}
		if let Some(root) = conditional_root_node(t) {
			let conditional_node = root.as_conditional_type_node();
			let true_type = state.ck().get_type_from_type_node(conditional_node.true_type);
			let removes_void = true_type.flags().intersects(TypeFlags::Never)
				&& Some(state.ck().get_type_from_type_node(conditional_node.false_type)) == check_type
				&& extends_type.is_some_and(|extends| {
					let void_type = state.ck().get_void_type();
					state.ck().is_type_assignable_to(void_type, extends)
				});
			if removes_void {
				return false;
			}
		}
	}
	let constraint = state.ck().get_base_constraint_of_type(t);
	constraint.is_none_or(|constraint| {
		let void_type = state.ck().get_void_type();
		state.ck().is_type_assignable_to(void_type, constraint)
	})
}
