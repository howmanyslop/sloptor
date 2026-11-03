//! `methodassignment.go`: `util/validateMethodAssignment.ts`. Assigning a function where the
//! contextual or inherited type expects the opposite method-ness would make the call convention
//! silently disagree, so upstream raises `expectedMethodGotFunction` / `expectedFunctionGotMethod`.
//!
//! `getAllSuperTypeNodes` and `validateHeritageClause` live in Go's `classes.go`; they port this
//! same upstream file, so they are here.

use rotor_tsrs::ast::{self as tsast, INTERNAL_SYMBOL_NAME_MISSING};
use rotor_tsrs::checker::ContextFlags;

use crate::access::is_valid_method_index_without_call;
use crate::diagnostics;
use crate::ismethod::{is_method, is_method_from_type};
use crate::prelude::*;
use crate::types::walk_types;

/// `hasCallSignatures` (L8-14).
fn has_call_signatures(state: &mut State<'_>, t: P<Type>) -> bool {
	let mut result = false;
	walk_types(state, t, &mut |state, leaf| {
		result = result || !state.ck().get_call_signatures(leaf).is_empty();
	});
	result
}

/// `validateTypes` (L16-27): method-ness is compared only when both types are callable.
pub fn validate_types(state: &mut State<'_>, node: P<Node>, base_type: P<Type>, assignment_type: P<Type>) {
	if has_call_signatures(state, base_type) && has_call_signatures(state, assignment_type) {
		let assignment_is_method = is_method_from_type(state, node, assignment_type);
		if is_method_from_type(state, node, base_type) != assignment_is_method {
			if assignment_is_method {
				state.diags.add(diagnostics::expected_method_got_function(node));
			} else {
				state.diags.add(diagnostics::expected_function_got_method(node));
			}
		}
	}
}

/// `validateMethodExpression`: an expression's method-ness against its contextual type.
pub fn validate_method_expression(state: &mut State<'_>, node: P<Node>) {
	let Some(parent) = tsast::skip_upwards(node).parent() else {
		return;
	};
	if tsast::is_property_assignment(parent) || tsast::is_shorthand_property_assignment(parent) {
		return;
	}
	let t = state.get_type(node);
	if !has_call_signatures(state, t) {
		return;
	}
	let Some(contextual_type) = state
		.ck()
		.get_contextual_type(node, ContextFlags::None)
		.filter(|&contextual_type| contextual_type != t)
	else {
		return;
	};
	let expression = tsast::skip_downwards(node);
	if tsast::is_function_expression(expression)
		&& is_method(state, expression) == is_method_from_type(state, node, contextual_type)
	{
		return;
	}
	let is_access = tsast::is_property_access_expression(expression) || tsast::is_element_access_expression(expression);
	if is_access
		&& !is_valid_method_index_without_call(state, tsast::skip_upwards(node))
		&& is_method_from_type(state, node, t)
	{
		return;
	}
	validate_types(state, node, t, contextual_type);
}

/// `validateObjectLiteralElement` (L29-35).
fn validate_object_literal_element(state: &mut State<'_>, node: P<Node>) {
	let t = state.get_type(node);
	if let Some(contextual_type) = state
		.ck()
		.get_contextual_type_for_object_literal_element(node, ContextFlags::None)
		&& contextual_type != t
	{
		validate_types(state, node, t, contextual_type);
	}
}

/// `validateHeritageClause` (L37-46): a class element against the same-named property of one
/// heritage type.
pub fn validate_heritage_clause(state: &mut State<'_>, node: P<Node>, type_node: P<Node>) {
	let Some(name) = node.name() else {
		return;
	};
	let name = tsast::get_property_name_for_property_name_node(name);
	if name == INTERNAL_SYMBOL_NAME_MISSING {
		// upstream returns undefined for late-bound computed names
		return;
	}

	let t = state.get_type(node);
	let heritage_type = state.get_type(type_node);
	let Some(property_type) = state.ck().get_type_of_property_of_type(heritage_type, &name) else {
		return;
	};

	validate_types(state, node, t, property_type);
}

/// `validateSpread` (L48-61): per property of the spread expression's type, against the
/// contextual type's same-named property.
fn validate_spread(state: &mut State<'_>, node: P<Node>) {
	let Some(expression) = node.expression() else {
		return;
	};
	let t = state.get_type(expression);
	let Some(contextual_type) = state.ck().get_contextual_type(expression, ContextFlags::None) else {
		return;
	};

	for &property in state.ck().get_properties_of_type(t) {
		let base_property_type = state.ck().get_type_of_property_of_type(t, property.name());
		let assignment_property_type = state
			.ck()
			.get_type_of_property_of_type(contextual_type, property.name());
		if let (Some(base), Some(assignment)) = (base_property_type, assignment_property_type) {
			validate_types(state, node, base, assignment);
		}
	}
}

/// `getAllSuperTypeNodes` (`classes.go`): the `extends` and `implements` nodes of a class or
/// interface.
#[must_use]
pub fn get_all_super_type_nodes(node: P<Node>) -> Vec<P<Node>> {
	if tsast::is_interface_declaration(node) {
		return tsast::get_heritage_elements(node, Kind::ExtendsKeyword).to_vec();
	}
	if tsast::is_class_like(node) {
		let mut result: Vec<P<Node>> = tsast::get_class_extends_heritage_element(node).into_iter().collect();
		result.extend_from_slice(tsast::get_implements_heritage_clause_elements(node));
		return result;
	}
	Vec::new()
}

/// `validateMethodAssignment` (L63-77).
pub fn validate_method_assignment(state: &mut State<'_>, node: P<Node>) {
	if tsast::is_class_element(node)
		&& let Some(parent) = node.parent()
		&& tsast::is_class_like(parent)
		&& node.name().is_some()
	{
		for type_node in get_all_super_type_nodes(parent) {
			validate_heritage_clause(state, node, type_node);
		}
		return;
	}
	if tsast::is_object_literal_element(node) {
		if tsast::is_spread_assignment(node) {
			if !node.expression().is_some_and(tsast::is_object_literal_expression) {
				validate_spread(state, node);
			}
		} else {
			validate_object_literal_element(state, node);
		}
	}
}
