//! `arrayappendoptimization.go`: with `optimizedArrayAppends` on, `arr.push(x)` inside the loop
//! right after `const arr = []` writes through a loop-carried length counter instead of
//! `table.insert`. The plan types and `getArrayAppendTarget` are in core (`crate::state`).

use std::collections::HashMap;

use rotor_tsrs::ast::{self as tsast, ModifierFlags, skip_downwards};
use rotor_tsrs::checker::{TypeChecker, TypeFlags};
use rotor_tsrs::types::{self as tstypes, TypeCheck};

use crate::prelude::*;
use crate::state::{ArrayAppendPlan, ArrayAppendTarget};
use crate::types::{check, get_first_construct_symbol, get_first_defined_symbol};
use crate::util::{is_ancestor_of, is_identifier, node_pos};

/// `analyzeArrayAppendStatements`: the plans for every loop that directly follows a fresh empty
/// array definition, keyed by the loop statement.
pub fn analyze_array_append_statements(
	state: &mut State<'_>,
	statements: &[P<Node>],
) -> HashMap<P<Node>, ArrayAppendPlan> {
	let mut plans = HashMap::new();
	if !state.optimized_array_appends {
		return plans;
	}

	for index in 1..statements.len() {
		let loop_statement = statements[index];
		let Some(definition) = get_fresh_array_definition(state, statements[index - 1]) else {
			continue;
		};

		if let Some(plan) = create_array_append_plan(state, definition, loop_statement) {
			plans.insert(loop_statement, plan);
		}
	}
	plans
}

/// `createArrayAppendPlan`: every reference to `definition` must be either before the
/// definition's function boundary or an optimizable `push` inside the loop body.
fn create_array_append_plan(
	state: &mut State<'_>,
	definition: P<Node>,
	loop_statement: P<Node>,
) -> Option<ArrayAppendPlan> {
	let body = get_array_append_loop_body(loop_statement)?;

	let target = ArrayAppendTarget {
		length_id: state.ast.temp_id(format!("{}Length", definition.text())),
	};
	let mut calls = HashMap::new();
	let definition_function = tsast::find_ancestor(definition, tsast::is_function_like);
	let search_container = match definition_function {
		Some(function) => function,
		None => tsast::get_source_file_of_node(definition)?.as_node(),
	};

	let is_unsafe =
		for_each_symbol_reference_with_state(state, definition, search_container, &mut |state, reference| {
			if !is_ancestor_of(loop_statement, reference) {
				return node_pos(reference) < node_pos(definition)
					|| tsast::find_ancestor(reference, tsast::is_function_like) != definition_function;
			}

			let Some(call) = get_optimizable_array_push_call(state, body, reference) else {
				return true;
			};
			if !call.as_call_expression().arguments.nodes().is_empty() {
				calls.insert(call, target);
			}
			false
		});
	if is_unsafe || calls.is_empty() {
		return None;
	}
	Some(ArrayAppendPlan { calls, target })
}

/// `ForEachSymbolReference` (`refwalker.go`) with the state in reach of the callback, which
/// needs the checker between references. The walk itself is the core one: document order,
/// definition skipped, a text prefilter, then symbol identity or the shorthand value symbol.
fn for_each_symbol_reference_with_state(
	state: &mut State<'_>,
	definition: P<Node>,
	search_container: P<Node>,
	on_ref: &mut dyn FnMut(&mut State<'_>, P<Node>) -> bool,
) -> bool {
	let Some(symbol) = state.ck().get_symbol_at_location(definition) else {
		return false;
	};
	visit_references(state, search_container, definition, symbol, on_ref)
}

fn visit_references(
	state: &mut State<'_>,
	node: P<Node>,
	definition: P<Node>,
	symbol: P<Symbol>,
	on_ref: &mut dyn FnMut(&mut State<'_>, P<Node>) -> bool,
) -> bool {
	if is_identifier(node) {
		if node == definition || node.text() != definition.text() {
			return false;
		}
		let mut checker = TypeChecker::new(state.checker);
		let mut matches = checker.get_symbol_at_location(node) == Some(symbol);
		if !matches
			&& let Some(parent) = node.parent()
			&& tsast::is_shorthand_property_assignment(parent)
		{
			matches = checker.get_shorthand_assignment_value_symbol(Some(parent)) == Some(symbol);
		}
		if matches {
			return on_ref(state, node);
		}
		return false;
	}
	node.for_each_child(&mut |child| visit_references(state, child, definition, symbol, on_ref))
}

/// `getFreshArrayDefinition`: the name of `const x = []` (or `new Array()`), not exported.
fn get_fresh_array_definition(state: &mut State<'_>, statement: P<Node>) -> Option<P<Node>> {
	if !tsast::is_variable_statement(statement) || tsast::has_syntactic_modifier(statement, ModifierFlags::Export) {
		return None;
	}

	let declaration_list = statement.as_variable_statement().declaration_list;
	if !tsast::is_var_const(declaration_list) {
		return None;
	}
	let [declaration] = declaration_list.as_variable_declaration_list().declarations.nodes() else {
		return None;
	};

	let name = declaration.as_variable_declaration().name;
	if !is_identifier(name) || !is_fresh_empty_array(state, declaration.initializer()) {
		return None;
	}
	Some(name)
}

/// `isFreshEmptyArray`: `[]`, or an argument-less `new` of a constructor macro whose type is an
/// array.
fn is_fresh_empty_array(state: &mut State<'_>, initializer: Option<P<Node>>) -> bool {
	let Some(initializer) = initializer else {
		return false;
	};
	let initializer = skip_downwards(initializer);
	if tsast::is_array_literal_expression(initializer) {
		return initializer.as_array_literal_expression().elements.nodes().is_empty();
	}
	if !tsast::is_new_expression(initializer) {
		return false;
	}

	let new_expression = initializer.as_new_expression();
	if new_expression
		.arguments
		.is_some_and(|arguments| !arguments.nodes().is_empty())
	{
		return false;
	}
	let Some(construct_symbol) = get_first_construct_symbol(state, new_expression.expression) else {
		return false;
	};
	if state.macros().get_constructor_macro(construct_symbol).is_none() {
		return false;
	}
	let t = state.get_type(initializer);
	check(state, TypeCheck::Array, t)
}

/// `getArrayAppendLoopBody`: the body of a non-`for-in` iteration statement, through labels.
fn get_array_append_loop_body(mut node: P<Node>) -> Option<P<Node>> {
	while tsast::is_labeled_statement(node) {
		node = node.as_labeled_statement().statement;
	}
	if tsast::is_for_in_statement(node) || !tsast::is_iteration_statement(node, false) {
		return None;
	}
	Some(node.statement())
}

/// `isNullType`: `TypeFlagsNull`.
fn is_null_type(t: P<Type>) -> bool {
	t.flags().intersects(TypeFlags::Null)
}

/// `IsPossiblyType(s, t, IsUndefinedType, isNullType)`: Go's predicate list takes a
/// file-local check the shared `TypeCheck` enum has no variant for, so the walk is spelled out
/// with the same steps.
fn is_possibly_undefined_or_null(state: &mut State<'_>, t: P<Type>) -> bool {
	let t = state.ck().get_base_constraint_of_type(t).unwrap_or(t);
	is_possibly_undefined_or_null_inner(state, t)
}

fn is_possibly_undefined_or_null_inner(state: &mut State<'_>, t: P<Type>) -> bool {
	if t.flags().intersects(TypeFlags::UnionOrIntersection) {
		return t
			.types()
			.iter()
			.any(|&member| is_possibly_undefined_or_null_inner(state, member));
	}
	if tstypes::is_class_or_interface(t)
		&& tstypes::get_recursive_base_types(state, t)
			.into_iter()
			.any(|base| is_possibly_undefined_or_null_inner(state, base))
	{
		return true;
	}
	if t.flags().intersects(TypeFlags::TypeVariable | TypeFlags::AnyOrUnknown) {
		return true;
	}
	if tstypes::is_defined_type(state, t) {
		// two callbacks, so `defined` never rules the list out
		return true;
	}
	check(state, TypeCheck::Undefined, t) || is_null_type(t)
}

/// `getOptimizableArrayPushCall`: the `x.push(...)` call `reference` is the receiver of, when it
/// sits in `body` outside any nested function, is not optional, resolves to the `Array.push`
/// macro and has no spread or possibly-nil argument.
fn get_optimizable_array_push_call(state: &mut State<'_>, body: P<Node>, reference: P<Node>) -> Option<P<Node>> {
	if !is_ancestor_of(body, reference) {
		return None;
	}
	let mut ancestor = reference.parent();
	while let Some(candidate) = ancestor
		&& candidate != body
	{
		if tsast::is_function_like(candidate) {
			return None;
		}
		ancestor = candidate.parent();
	}

	let property_access_node = reference.parent()?;
	if !tsast::is_property_access_expression(property_access_node) {
		return None;
	}
	let property_access = property_access_node.as_property_access_expression();
	if property_access.expression != reference
		|| property_access_node.question_dot_token().is_some()
		|| property_access.name.text() != "push"
	{
		return None;
	}

	let call_node = property_access_node.parent()?;
	if !tsast::is_call_expression(call_node) {
		return None;
	}
	let call = call_node.as_call_expression();
	if skip_downwards(call.expression) != property_access_node || call_node.question_dot_token().is_some() {
		return None;
	}

	let callee_type = state.get_type(call.expression);
	let call_type = state.ck().get_non_optional_type(callee_type);
	let call_symbol = get_first_defined_symbol(state, call_type)?;
	let entry = state.get_property_call_macro(call_symbol)?;
	if entry.name() != "Array.push" {
		return None;
	}

	for &argument in call.arguments.nodes() {
		if tsast::is_spread_element(argument) {
			return None;
		}
		let t = state.get_type(argument);
		if is_possibly_undefined_or_null(state, t) {
			return None;
		}
	}
	Some(call_node)
}
