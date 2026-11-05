//! `parameters.go`: `nodes/transformParameters.ts`. `transformInitializer` is in
//! `crate::shared`.

use rotor_tsrs::ast as tsast;

use super::varargsoptimization::analyze_var_args_optimization;
use super::{TransformedParameters, create_var_args_length_select};
use crate::diagnostics;
use crate::expressions::{transform_array_binding_pattern, transform_object_binding_pattern};
use crate::identifier::{transform_identifier_defined, validate_identifier};
use crate::ismethod::is_method;
use crate::luau_types::{
	get_luau_binding_element_type, get_luau_parameter_type, get_luau_self_type, get_native_parameter_type,
	luau_default_refinement,
};
use crate::prelude::*;
use crate::shared::{is_omitted_binding_element, transform_initializer};
use crate::spreadhelpers::array_like_expression_contains_spread;
use crate::util::is_identifier;

/// `transformParameters` (`transformParameters.ts` L53-124): the Luau parameter list, the
/// body-head statements (rest capture, defaults, destructuring), and whether the parameter list
/// ends in `...`.
///
/// - methods get an explicit leading `self` parameter (`isMethod`, checker);
/// - a TS `this` parameter emits nothing (its only effect is via `isMethod`);
/// - rest `...args` sets `has_dot_dot_dot` and prepends `local args = { ... }`;
/// - defaults run before destructuring, via `transformInitializer`.
///
/// With luauTypes on, `self`, every parameter and every `_param` temp get the builder's type and
/// a defaulted parameter is re-bound to its non-nil type right after its default; a `//!native`
/// file keeps the native annotations instead.
pub fn transform_parameters(state: &mut State<'_>, node: P<Node>) -> TransformedParameters {
	let parameters = state.ast.new_list();
	let statements = state.ast.new_list();
	let mut has_dot_dot_dot = false;
	let mut var_args_data = None;

	if is_method(state, node) {
		let self_id = state.ast.global_id("self");
		if state.luau_typed_signatures() {
			let self_type = get_luau_self_type(state, node);
			state.ast.set_binding_type(self_id, self_type);
		}
		state.ast.list_push(parameters, self_id);
	}

	for &parameter_node in node.parameters() {
		let parameter = parameter_node.as_parameter_declaration();
		let name = parameter.name;

		if tsast::is_this_identifier(name) {
			continue;
		}

		let is_rest = parameter.dot_dot_dot_token().is_some();
		if is_rest && tsast::is_array_binding_pattern(name) && !array_like_expression_contains_spread(name) {
			// `...[a, b]: [A, B]` flattens the pattern elements into real parameters (no `...`
			// capture).
			let parameter_statements =
				state.capture_statements(|state| optimize_array_spread_parameter(state, parameters, name));
			state.set_statement_list_source_positions(parameter_node, &[parameter_statements]);
			state.ast.list_push_list(statements, parameter_statements);
			continue;
		}

		let param_id = if is_identifier(name) {
			let id = transform_identifier_defined(state, name);
			validate_identifier(state, name);
			id
		} else {
			state.ast.temp_id("param")
		};

		if is_rest {
			has_dot_dot_dot = true;
			let mut data = analyze_var_args_optimization(state, parameter_node, node);
			if data.is_optimizable {
				if data.size_access_count + data.for_of_access_count > 1 {
					let length_id = state.ast.temp_id("args_length");
					data.length_id = Some(length_id);
					let select = create_var_args_length_select(&mut state.ast);
					let declaration = state.ast.variable_declaration(length_id, Some(select.into()));
					state.ast.list_push(statements, declaration);
				}
			} else {
				let var_args = state.ast.var_args();
				let members = state.ast.list_from([var_args]);
				let array = state.ast.array(members);
				let declaration = state.ast.variable_declaration(param_id, Some(array.into()));
				state.ast.list_push(statements, declaration);
			}
			var_args_data = Some(data);
		} else {
			state.ast.list_push(parameters, param_id);
			if state.luau_typed_signatures() {
				let parameter_type = get_luau_parameter_type(state, parameter_node);
				state.ast.set_binding_type(param_id, parameter_type);
			} else if state.is_native && is_identifier(name) {
				let parameter_type = get_native_parameter_type(state, parameter_node);
				state.ast.set_binding_type(param_id, parameter_type);
			}
		}

		if let Some(initializer) = parameter_node.initializer() {
			let statement = transform_initializer(state, param_id, initializer);
			state.ast.list_push(statements, statement);
			if state.luau_typed_signatures()
				&& let Some(refinement) = luau_default_refinement(state, name, parameter_node, param_id)
			{
				state.ast.list_push(statements, refinement);
			}
		}

		// destructuring
		if !is_identifier(name) {
			let destructuring = state.capture_statements(|state| {
				if tsast::is_array_binding_pattern(name) {
					transform_array_binding_pattern(state, name, param_id);
				} else {
					transform_object_binding_pattern(state, name, param_id);
				}
			});
			state.ast.list_push_list(statements, destructuring);
		}
		state.set_statement_list_source_positions(parameter_node, &[statements]);
	}

	TransformedParameters {
		parameters,
		statements,
		has_dot_dot_dot,
		var_args_data,
	}
}

/// `optimizeArraySpreadParameter` (`transformParameters.ts` L16-51): parameters in the form
/// `...[a, b, c]: [A, B, C]` become just `(a, b, c)`. An omitted element gets a placeholder temp
/// parameter; a rest element raises `noNestedSpreadsInAssignmentPatterns` and aborts the pattern;
/// nested patterns get a `_param` temp destructured in the body (initializer first, then the
/// recursion). Callers capture the prereqs into the body-head statements.
fn optimize_array_spread_parameter(
	state: &mut State<'_>,
	parameters: List<element::AnyIdentifier>,
	binding_pattern: P<Node>,
) {
	for &element in binding_pattern.as_binding_pattern().elements.nodes() {
		if is_omitted_binding_element(element) {
			let placeholder = state.ast.temp_id("");
			state.ast.list_push(parameters, placeholder);
			continue;
		}
		let binding_element = element.as_binding_element();
		if binding_element.dot_dot_dot_token().is_some() {
			state
				.diags
				.add(diagnostics::no_nested_spreads_in_assignment_patterns(element));
			return;
		}
		let Some(name) = element.name() else {
			continue;
		};
		if is_identifier(name) {
			let param_id = transform_identifier_defined(state, name);
			validate_identifier(state, name);
			state.ast.list_push(parameters, param_id);
			if state.luau_typed_signatures() {
				let element_type = get_luau_binding_element_type(state, element);
				state.ast.set_binding_type(param_id, element_type);
			}
			if let Some(initializer) = element.initializer() {
				let statement = transform_initializer(state, param_id, initializer);
				state.prereq(statement);
				prereq_luau_default_refinement(state, name, element, param_id);
			}
		} else {
			let param_id = state.ast.temp_id("param");
			state.ast.list_push(parameters, param_id);
			if state.luau_typed_signatures() {
				let element_type = get_luau_binding_element_type(state, element);
				state.ast.set_binding_type(param_id, element_type);
			}
			if let Some(initializer) = element.initializer() {
				let statement = transform_initializer(state, param_id, initializer);
				state.prereq(statement);
				prereq_luau_default_refinement(state, name, element, param_id);
			}
			if tsast::is_array_binding_pattern(name) {
				transform_array_binding_pattern(state, name, param_id);
			} else {
				transform_object_binding_pattern(state, name, param_id);
			}
		}
	}
}

/// `prereqLuauDefaultRefinement`: prereqs the luauTypes re-binding of a defaulted flattened
/// parameter (see `luauDefaultRefinement`).
fn prereq_luau_default_refinement(state: &mut State<'_>, name: P<Node>, owner: P<Node>, param_id: NodeId) {
	if !state.luau_typed_signatures() {
		return;
	}
	if let Some(refinement) = luau_default_refinement(state, name, owner, param_id) {
		state.prereq(refinement);
	}
}
