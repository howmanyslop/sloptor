//! `bindingarray.go`: the array-pattern destructuring transforms
//! (`nodes/binding/transformArrayBindingPattern.ts`,
//! `nodes/binding/transformArrayAssignmentPattern.ts`) and the optimized multi-local and
//! multi-assign paths of `transformVariableStatement.ts` (L57-99) and
//! `transformBinaryExpression.ts` (L36-111).

use rotor_luau::{ComputedIndexExpression, PropertyAccessExpression};
use rotor_tsrs::ast as tsast;

use super::binding::get_accessor_for_binding_type;
use super::bindingobject::{transform_object_assignment_pattern, transform_object_binding_pattern};
use super::spreaddestruct::get_spread_destructor_for_type;
use crate::access::validate_not_any_type;
use crate::assignment::transform_writable_expression;
use crate::diagnostics::{self, kind_name};
use crate::identifier::{transform_identifier_defined, validate_identifier};
use crate::prelude::*;
use crate::shared::{is_omitted_binding_element, transform_initializer};
use crate::statements::transform_variable;

// -------------------------------------------------------------------------------------------------
// Binding patterns: const/let declarations and parameters
// -------------------------------------------------------------------------------------------------

/// `transformArrayBindingPattern`
pub fn transform_array_binding_pattern(state: &mut State<'_>, binding_pattern: P<Node>, parent_id: NodeId) {
	validate_not_any_type(state, binding_pattern);

	let pattern_type = state.get_type(binding_pattern);
	let (accessor, mut binding_state) = get_accessor_for_binding_type(state, binding_pattern, pattern_type, parent_id);
	let destructor = get_spread_destructor_for_type(state, pattern_type, &binding_state);
	for (index, &element) in binding_pattern.as_binding_pattern().elements.nodes().iter().enumerate() {
		if is_omitted_binding_element(element) {
			accessor(state, parent_id, index, &mut binding_state, true);
		} else {
			let binding_element = element.as_binding_element();
			let value = if binding_element.dot_dot_dot_token().is_some() {
				let Some(destructor) = destructor else {
					state
						.diags
						.add(diagnostics::no_nested_spreads_in_assignment_patterns(element));
					return;
				};
				destructor(state, parent_id, index, &mut binding_state)
			} else {
				accessor(state, parent_id, index, &mut binding_state, false)
			};
			let Some(name) = binding_element.name() else {
				state.ice("transformer: transformArrayBindingPattern: binding element without a name");
				return;
			};
			if tsast::is_identifier(name) {
				let id = transform_variable(state, name, Some(value));
				if let Some(initializer) = binding_element.initializer() {
					let statement = transform_initializer(state, id, initializer);
					state.prereq(statement);
				}
			} else {
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
}

/// `transformOptimizedArrayBindingPattern` (`transformVariableStatement.ts` L57-99): the single
/// multi-local `local a, b = <rhs...>`, used when the right side is a `LuaTuple` call (direct
/// unpack) or a literal array (members inlined) and no bound identifier is hoisted. This path
/// bypasses `transformVariable`, so there is no export-let or hoist handling per element
/// (upstream relies on optimized patterns being local-only; ported verbatim). Defaults and
/// nested destructures are emitted after the declaration.
pub fn transform_optimized_array_binding_pattern(
	state: &mut State<'_>,
	binding_pattern: P<Node>,
	rhs: NodeOrList,
) -> Statements {
	state.capture_statements(|state| {
		let ids = state.ast.new_list::<element::AnyIdentifier>();
		let statements = state.capture_statements(|state| {
			for &element in binding_pattern.as_binding_pattern().elements.nodes() {
				if is_omitted_binding_element(element) {
					let id = state.ast.temp_id("");
					state.ast.list_push(ids, id);
				} else {
					let binding_element = element.as_binding_element();
					if binding_element.dot_dot_dot_token().is_some() {
						state
							.diags
							.add(diagnostics::no_nested_spreads_in_assignment_patterns(element));
						return;
					}
					let Some(name) = binding_element.name() else {
						state.ice("transformer: transformOptimizedArrayBindingPattern: binding element without a name");
						return;
					};
					if tsast::is_identifier(name) {
						validate_identifier(state, name);
						let id = transform_identifier_defined(state, name);
						state.ast.list_push(ids, id);
						if let Some(initializer) = binding_element.initializer() {
							let statement = transform_initializer(state, id, initializer);
							state.prereq(statement);
						}
					} else {
						let id = state.ast.temp_id("binding");
						state.ast.list_push(ids, id);
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
		});
		if state.ast.list_is_empty(ids) {
			// upstream assert
			state.ice("transformer: transformOptimizedArrayBindingPattern: no ids");
			return;
		}
		let declaration = state.ast.variable_declaration(ids, Some(rhs));
		state.prereq(declaration);
		state.prereq_list(statements);
	})
}

// -------------------------------------------------------------------------------------------------
// Assignment patterns: `[a, b] = exp`
// -------------------------------------------------------------------------------------------------

/// `transformArrayAssignmentPattern`
pub fn transform_array_assignment_pattern(state: &mut State<'_>, assignment_pattern: P<Node>, parent_id: NodeId) {
	let pattern_type = state.ck().get_type_of_assignment_pattern(assignment_pattern);
	let (accessor, mut binding_state) =
		get_accessor_for_binding_type(state, assignment_pattern, pattern_type, parent_id);
	let destructor = get_spread_destructor_for_type(state, pattern_type, &binding_state);
	for (index, &original) in assignment_pattern
		.as_array_literal_expression()
		.elements
		.nodes()
		.iter()
		.enumerate()
	{
		let mut element = original;
		if tsast::is_omitted_expression(element) {
			accessor(state, parent_id, index, &mut binding_state, true);
		} else {
			let mut initializer = None;
			if tsast::is_binary_expression(element) {
				let binary = element.as_binary_expression();
				initializer = Some(tsast::skip_downwards(binary.right()));
				element = tsast::skip_downwards(binary.left);
			}

			let (mut value, value_prereqs) = if tsast::is_spread_element(element) {
				let spread_expression = element.as_spread_element().expression;
				let Some(destructor) = destructor.filter(|_| {
					!tsast::is_object_literal_expression(spread_expression)
						&& !tsast::is_array_literal_expression(spread_expression)
				}) else {
					state
						.diags
						.add(diagnostics::no_nested_spreads_in_assignment_patterns(element));
					continue;
				};
				let captured = state.capture(|state| destructor(state, parent_id, index, &mut binding_state));
				element = spread_expression;
				captured
			} else {
				state.capture(|state| accessor(state, parent_id, index, &mut binding_state, false))
			};
			if tsast::is_identifier(element)
				|| tsast::is_element_access_expression(element)
				|| tsast::is_property_access_expression(element)
			{
				let mut id = transform_writable_expression(state, element, false);
				if state.ast.list_is_non_empty(value_prereqs) || initializer.is_some() {
					id = pin_writable_target(state, id);
				}
				state.prereq_list(value_prereqs);
				if let Some(initializer) = initializer {
					let binding = state.push_to_var(Some(value), "binding");
					let statement = transform_initializer(state, binding, initializer);
					state.prereq(statement);
					value = binding;
				}
				let assignment = state.ast.assignment(id, AssignmentOperator::Assign, value);
				state.prereq(assignment);
			} else if tsast::is_array_literal_expression(element) {
				state.prereq_list(value_prereqs);
				let id = state.push_to_var(Some(value), "binding");
				if let Some(initializer) = initializer {
					let statement = transform_initializer(state, id, initializer);
					state.prereq(statement);
				}
				transform_array_assignment_pattern(state, element, id);
			} else if tsast::is_object_literal_expression(element) {
				state.prereq_list(value_prereqs);
				let id = state.push_to_var(Some(value), "binding");
				if let Some(initializer) = initializer {
					let statement = transform_initializer(state, id, initializer);
					state.prereq(statement);
				}
				transform_object_assignment_pattern(state, element, id);
			} else {
				// upstream assert
				state.ice(format!(
					"transformer: transformArrayAssignmentPattern invalid element: {}",
					kind_name(element.kind())
				));
			}
		}
	}
}

/// Pins a property or element access target's base (and a non-primitive index) so the value's
/// prereqs and the default cannot change what it writes to.
fn pin_writable_target(state: &mut State<'_>, id: NodeId) -> NodeId {
	if let Some(target) = state.ast.get::<PropertyAccessExpression>(id) {
		let (expression, name) = (target.expression, target.name.clone());
		let base = state.push_to_var(Some(expression), "exp");
		return state.ast.property_access(base, name);
	}
	if let Some(&ComputedIndexExpression { expression, index }) = state.ast.get::<ComputedIndexExpression>(id) {
		let base = state.push_to_var(Some(expression), "exp");
		let mut key = index;
		if !state.ast.kind(key).is_simple_primitive() {
			key = state.push_to_var(Some(key), "index");
		}
		return state.ast.computed_index(base, key);
	}
	id
}

/// `transformOptimizedArrayAssignmentPattern` (`transformBinaryExpression.ts` L36-111): the
/// multi-assign `a, b = <rhs...>` (the swap `[a, b] = [b, a]` becomes `a, b = b, a`), for a
/// `LuaTuple` call or a literal array in statement position. There is no hoist gate here
/// (assignment targets are already declared). Emission order: `local _t1, _t2` for nested
/// patterns, the target prereqs, the multi-assignment, then defaults and nested destructures.
pub fn transform_optimized_array_assignment_pattern(
	state: &mut State<'_>,
	assignment_pattern: P<Node>,
	rhs: NodeOrList,
) {
	let variables = state.ast.new_list::<element::AnyIdentifier>();
	let writes = state.ast.new_list::<element::WritableExpression>();
	let writes_prereqs = state.ast.new_list::<element::Statement>();
	let statements = state.capture_statements(|state| {
		for &original in assignment_pattern.as_array_literal_expression().elements.nodes() {
			let mut element = original;
			if tsast::is_omitted_expression(element) {
				let id = state.ast.temp_id("");
				state.ast.list_push(writes, id);
			} else if tsast::is_spread_element(element) {
				state
					.diags
					.add(diagnostics::no_nested_spreads_in_assignment_patterns(element));
			} else {
				let mut initializer = None;
				if tsast::is_binary_expression(element) {
					let binary = element.as_binary_expression();
					initializer = Some(tsast::skip_downwards(binary.right()));
					element = tsast::skip_downwards(binary.left);
				}

				if tsast::is_identifier(element)
					|| tsast::is_element_access_expression(element)
					|| tsast::is_property_access_expression(element)
				{
					let mut id = None;
					let id_prereqs = state.capture_statements(|state| {
						id = Some(transform_writable_expression(state, element, true));
					});
					let id = id.unwrap_or_else(|| state.ast.none());
					state.ast.list_push_list(writes_prereqs, id_prereqs);
					state.ast.list_push(writes, id);
					if let Some(initializer) = initializer {
						let statement = transform_initializer(state, id, initializer);
						state.prereq(statement);
					}
				} else if tsast::is_array_literal_expression(element) {
					let id = state.ast.temp_id("binding");
					state.ast.list_push(variables, id);
					state.ast.list_push(writes, id);
					if let Some(initializer) = initializer {
						let statement = transform_initializer(state, id, initializer);
						state.prereq(statement);
					}
					transform_array_assignment_pattern(state, element, id);
				} else if tsast::is_object_literal_expression(element) {
					let id = state.ast.temp_id("binding");
					state.ast.list_push(variables, id);
					state.ast.list_push(writes, id);
					if let Some(initializer) = initializer {
						let statement = transform_initializer(state, id, initializer);
						state.prereq(statement);
					}
					transform_object_assignment_pattern(state, element, id);
				} else {
					// upstream assert
					state.ice(format!(
						"transformer: transformOptimizedArrayAssignmentPattern invalid element: {}",
						kind_name(element.kind())
					));
				}
			}
		}
	});
	if state.ast.list_is_non_empty(variables) {
		let declaration = state.ast.variable_declaration(variables, None);
		state.prereq(declaration);
	}
	state.prereq_list(writes_prereqs);
	if state.ast.list_is_empty(writes) {
		// upstream assert
		state.ice("transformer: transformOptimizedArrayAssignmentPattern: no writes");
		return;
	}
	let assignment = state.ast.assignment(writes, AssignmentOperator::Assign, rhs);
	state.prereq(assignment);
	state.prereq_list(statements);
}
