//! `varargsoptimization.go`: a rest parameter that is only indexed, sized, spread into a call or
//! iterated keeps its values in `...` instead of packing them into a table.
//!
//! `VarArgsData`, `createVarArgsLengthSelect` and `registerOptimizableVarArgsForFunction` live in
//! `control/mod.rs` and `crate::shared`.

use rotor_tsrs::ast::{self as tsast, FunctionFlags, skip_upwards};

use super::VarArgsData;
use crate::prelude::*;
use crate::refwalker::for_each_symbol_reference;
use crate::util::is_identifier;

/// `analyzeVarArgsOptimization`: counts the uses of the rest parameter `param` of `func_node`
/// and decides whether each one can read `...` directly.
pub fn analyze_var_args_optimization(state: &mut State<'_>, param: P<Node>, func_node: P<Node>) -> VarArgsData {
	let mut data = VarArgsData::default();
	let parameter = param.as_parameter_declaration();
	if parameter.dot_dot_dot_token().is_none() || !is_identifier(parameter.name) {
		return data;
	}

	data.parameter_index = func_node.parameters().iter().position(|&candidate| candidate == param);
	if data.parameter_index.is_none() || tsast::get_function_flags(Some(func_node)).intersects(FunctionFlags::Generator)
	{
		return data;
	}

	let body = match func_node.kind() {
		Kind::FunctionDeclaration
		| Kind::FunctionExpression
		| Kind::ArrowFunction
		| Kind::MethodDeclaration
		| Kind::Constructor => func_node.body(),
		_ => return data,
	};
	let Some(body) = body else {
		return data;
	};

	if state.ck().get_symbol_at_location(parameter.name).is_none() {
		return data;
	}

	// ForEachSymbolReference also matches shorthand properties (`{ args }`), which resolve to the
	// property symbol; missing them would drop the `local args = { ... }` capture while the
	// reference still emits `args`.
	data.is_optimizable = true;
	for_each_symbol_reference(state.checker, parameter.name, body, &mut |node| {
		let reference = skip_upwards(node);
		let Some(parent) = reference.parent() else {
			data.is_optimizable = false;
			return true;
		};
		if is_var_args_reference_inside_nested_function(reference, func_node)
			|| is_var_args_reference_inside_try(reference, func_node)
		{
			data.is_optimizable = false;
			return true;
		}

		if tsast::is_element_access_expression(parent) && parent.as_element_access_expression().expression == reference
		{
			if tsast::is_assignment_target(parent) {
				data.is_optimizable = false;
				return true;
			}
			return false;
		}

		if tsast::is_spread_element(parent) && parent.expression() == Some(reference) {
			if parent.parent().is_some_and(tsast::is_array_literal_expression) {
				data.is_optimizable = false;
				return true;
			}
			return false;
		}

		if tsast::is_for_of_statement(parent) && parent.as_for_in_or_of_statement().expression == reference {
			let initializer = parent.as_for_in_or_of_statement().initializer;
			if tsast::is_variable_declaration_list(initializer) {
				let declarations = initializer.as_variable_declaration_list().declarations.nodes();
				if !declarations
					.first()
					.is_some_and(|declaration| is_identifier(declaration.as_variable_declaration().name))
				{
					data.is_optimizable = false;
					return true;
				}
			} else if !is_identifier(initializer) {
				data.is_optimizable = false;
				return true;
			}
			data.for_of_access_count += 1;
			return false;
		}

		if tsast::is_property_access_expression(parent)
			&& parent.as_property_access_expression().expression == reference
		{
			if parent.as_property_access_expression().name.text() == "size"
				&& let Some(call) = parent.parent()
				&& tsast::is_call_expression(call)
				&& call.as_call_expression().expression == parent
			{
				data.size_access_count += 1;
				return false;
			}
			data.is_optimizable = false;
			return true;
		}

		data.is_optimizable = false;
		true
	});
	data
}

/// `isVarArgsReferenceInsideNestedFunction`
fn is_var_args_reference_inside_nested_function(node: P<Node>, function_node: P<Node>) -> bool {
	let mut current = skip_upwards(node).parent();
	while let Some(candidate) = current
		&& candidate != function_node
	{
		if tsast::is_function_like_declaration(candidate) {
			return true;
		}
		current = skip_upwards(candidate).parent();
	}
	false
}

/// `isVarArgsReferenceInsideTry`
fn is_var_args_reference_inside_try(node: P<Node>, function_node: P<Node>) -> bool {
	let mut current = skip_upwards(node).parent();
	while let Some(candidate) = current
		&& candidate != function_node
	{
		if tsast::is_try_statement(candidate) {
			return true;
		}
		current = skip_upwards(candidate).parent();
	}
	false
}
