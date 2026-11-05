//! `bindingobject.go`: the object-pattern destructuring transforms
//! (`nodes/binding/transformObjectBindingPattern.ts` and
//! `nodes/binding/transformObjectAssignmentPattern.ts`).

use rotor_tsrs::ast as tsast;
use rotor_tsrs::types::TypeCheck;

use super::binding::object_accessor;
use super::bindingarray::{transform_array_assignment_pattern, transform_array_binding_pattern};
use super::spreaddestruct::spread_destructure_object;
use crate::access::validate_not_any_type;
use crate::assignment::transform_writable_expression;
use crate::diagnostics::{self, kind_name};
use crate::prelude::*;
use crate::shared::transform_initializer;
use crate::statements::transform_variable;
use crate::types::is_possibly_type;

// -------------------------------------------------------------------------------------------------
// Binding patterns: `const { a, b: c } = exp`
// -------------------------------------------------------------------------------------------------

/// `transformObjectBindingPattern`
pub fn transform_object_binding_pattern(state: &mut State<'_>, binding_pattern: P<Node>, parent_id: NodeId) {
	validate_not_any_type(state, binding_pattern);
	let elements = binding_pattern.as_binding_pattern().elements.nodes();
	let mut pre_spread_names = Vec::with_capacity(elements.len());
	for &element in elements {
		let binding_element = element.as_binding_element();
		let Some(name) = binding_element.name() else {
			state.ice("transformer: transformObjectBindingPattern: binding element without a name");
			return;
		};
		let prop = binding_element.property_name();
		let is_spread = binding_element.dot_dot_dot_token().is_some();
		if tsast::is_identifier(name) {
			let value = if is_spread {
				spread_destructure_object(state, parent_id, &pre_spread_names)
			} else {
				let name_or_prop = prop.unwrap_or(name);
				let pattern_type = state.get_type(binding_pattern);
				object_accessor(state, parent_id, pattern_type, name_or_prop)
			};
			pre_spread_names.push(value);
			if is_spread {
				let pattern_type = state.get_type(binding_pattern);
				if is_possibly_type(state, pattern_type, &[TypeCheck::Roblox]) {
					state.diags.add(diagnostics::no_rest_spreading_of_roblox_types(element));
					continue;
				}
			}
			let id = transform_variable(state, name, Some(value));
			if let Some(initializer) = binding_element.initializer() {
				let statement = transform_initializer(state, id, initializer);
				state.prereq(statement);
			}
		} else {
			let Some(prop) = prop else {
				// upstream assert
				state.ice("transformer: transformObjectBindingPattern: nested pattern without property name");
				return;
			};
			if is_spread {
				state.ice("transformer: transformObjectBindingPattern: nested spread pattern");
				return;
			}
			let pattern_type = state.get_type(binding_pattern);
			let value = object_accessor(state, parent_id, pattern_type, prop);
			pre_spread_names.push(value);
			let id = state.push_to_var(Some(value), "binding");
			if let Some(initializer) = binding_element.initializer() {
				let statement = transform_initializer(state, id, initializer);
				state.prereq(statement);
			}
			if tsast::is_array_binding_pattern(name) {
				transform_array_binding_pattern(state, name, id);
			} else {
				transform_object_binding_pattern(state, name, id);
			}
		}
	}
}

// -------------------------------------------------------------------------------------------------
// Assignment patterns: `({ a, b: target } = exp)`
// -------------------------------------------------------------------------------------------------

/// `transformObjectAssignmentPattern` (`transformObjectAssignmentPattern.ts` L14-90), over an
/// object literal left side. The pattern type comes from the checker's
/// `getTypeOfAssignmentPattern`, not `getType`. Shorthand defaults arrive through
/// `objectAssignmentInitializer` (`{ a = 1 }`); property-assignment defaults as binary
/// initializers (`{ a: b = 1 }`). A nested-literal spread reports
/// `noNestedSpreadsInAssignmentPatterns` and skips that property.
pub fn transform_object_assignment_pattern(state: &mut State<'_>, assignment_pattern: P<Node>, parent_id: NodeId) {
	let properties = assignment_pattern.as_object_literal_expression().properties.nodes();
	let mut pre_spread_names = Vec::with_capacity(properties.len());
	for &property in properties {
		if tsast::is_shorthand_property_assignment(property) {
			let shorthand = property.as_shorthand_property_assignment();
			let name = shorthand.name();
			let pattern_type = state.ck().get_type_of_assignment_pattern(assignment_pattern);
			let value = object_accessor(state, parent_id, pattern_type, name);
			pre_spread_names.push(value);
			let object_assignment_initializer = shorthand.object_assignment_initializer();
			let id = transform_writable_expression(state, name, object_assignment_initializer.is_some());
			let assignment = state.ast.assignment(id, AssignmentOperator::Assign, value);
			state.prereq(assignment);
			if !state.ast.kind(id).is_any_identifier() {
				// upstream assert
				state.ice("transformer: transformObjectAssignmentPattern: shorthand target is not an identifier");
				return;
			}
			if let Some(initializer) = object_assignment_initializer {
				let statement = transform_initializer(state, id, initializer);
				state.prereq(statement);
			}
		} else if tsast::is_spread_assignment(property) {
			let value = spread_destructure_object(state, parent_id, &pre_spread_names);
			let expression = property.as_spread_assignment().expression;
			if tsast::is_object_literal_expression(expression) || tsast::is_array_literal_expression(expression) {
				state
					.diags
					.add(diagnostics::no_nested_spreads_in_assignment_patterns(property));
				continue;
			}
			if !tsast::is_identifier(expression) {
				state.ice(format!(
					"transformer: transformObjectAssignmentPattern unexpected expression type: {}",
					kind_name(expression.kind())
				));
				return;
			}
			let id = transform_writable_expression(state, expression, true);
			let assignment = state.ast.assignment(id, AssignmentOperator::Assign, value);
			state.prereq(assignment);
		} else if tsast::is_property_assignment(property) {
			let property_assignment = property.as_property_assignment();
			let name = property_assignment.name();
			let mut init = property_assignment.initializer();
			let mut initializer = None;
			if tsast::is_binary_expression(init) {
				let binary = init.as_binary_expression();
				initializer = Some(tsast::skip_downwards(binary.right()));
				init = tsast::skip_downwards(binary.left);
			}

			let pattern_type = state.ck().get_type_of_assignment_pattern(assignment_pattern);
			let value = object_accessor(state, parent_id, pattern_type, name);
			pre_spread_names.push(value);
			if tsast::is_identifier(init)
				|| tsast::is_element_access_expression(init)
				|| tsast::is_property_access_expression(init)
			{
				let id = transform_writable_expression(state, init, initializer.is_some());
				let assignment = state.ast.assignment(id, AssignmentOperator::Assign, value);
				state.prereq(assignment);
				if let Some(initializer) = initializer {
					let statement = transform_initializer(state, id, initializer);
					state.prereq(statement);
				}
			} else if tsast::is_array_literal_expression(init) {
				let id = state.push_to_var(Some(value), "binding");
				if let Some(initializer) = initializer {
					let statement = transform_initializer(state, id, initializer);
					state.prereq(statement);
				}
				if !tsast::is_identifier(name) {
					// upstream assert
					state.ice(
						"transformer: transformObjectAssignmentPattern: nested array pattern with non-identifier name",
					);
					return;
				}
				transform_array_assignment_pattern(state, init, id);
			} else if tsast::is_object_literal_expression(init) {
				let id = state.push_to_var(Some(value), "binding");
				if let Some(initializer) = initializer {
					let statement = transform_initializer(state, id, initializer);
					state.prereq(statement);
				}
				transform_object_assignment_pattern(state, init, id);
			} else {
				// upstream assert
				state.ice(format!(
					"transformer: transformObjectAssignmentPattern invalid initializer: {}",
					kind_name(init.kind())
				));
				return;
			}
		} else {
			// upstream assert
			state.ice(format!(
				"transformer: transformObjectAssignmentPattern invalid property: {}",
				kind_name(property.kind())
			));
			return;
		}
	}
}
