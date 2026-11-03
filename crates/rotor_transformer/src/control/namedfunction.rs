//! `namedfunction.go`: named function expressions lower to local function declarations, the
//! only Luau form that carries a debug name into a traceback.

use rotor_tsrs::ast::{self as tsast, ModifierFlags, NodeFlags};
use rotor_tsrs::checker::skip_alias;

use super::functions::{TransformedFunctionBody, set_function_signature, transform_function_body};
use crate::diagnostics;
use crate::extras::wrap_statements_as_generator;
use crate::identifier::{transform_identifier_defined, validate_identifier};
use crate::prelude::*;
use crate::statements::check_variable_hoist;
use crate::util::is_identifier;

/// `emitNamedFunctionExpression`: lowers a named function expression to a function declaration.
/// A generator body is wrapped the same way as an anonymous function expression. An async
/// expression keeps that declaration and then assigns `name = TS.async(name)`: `local function
/// name` is `local name; name = function() ... end`, so the body already closes over the local,
/// and the write points it at the wrapper.
pub fn emit_named_function_expression(
	state: &mut State<'_>,
	node: P<Node>,
	name: NodeId,
	localize: bool,
	name_replacement: Option<NodeId>,
) -> Statements {
	let mut body = transform_named_function_expression_body(state, node, name_replacement);

	let is_async = tsast::has_syntactic_modifier(node, ModifierFlags::Async);
	if node.as_function_expression().asterisk_token().is_some() {
		if is_async {
			state.diags.add(diagnostics::no_async_generator_functions(node));
		}
		body.statements = wrap_statements_as_generator(state, node, body.statements);
	}

	let declaration =
		state
			.ast
			.function_declaration(localize, name, body.parameters, body.has_dot_dot_dot, body.statements);
	set_function_signature(
		&mut state.ast,
		declaration,
		body.generics,
		body.return_type,
		body.var_args_type,
	);
	let statements = state.ast.list_from([declaration]);
	state.set_node_source_position(declaration, node);
	if is_async {
		let runtime_async = state.runtime_lib(Some(node), "async");
		let args = state.ast.list_from([name]);
		let call = state.ast.call(runtime_async, args);
		let assignment = state.ast.assignment(name, AssignmentOperator::Assign, call);
		state.ast.list_push(statements, assignment);
	}
	statements
}

/// `transformNamedFunctionExpressionBody`: the body with the name's symbol forced unhoisted (and
/// renamed to `replacement`, if given), both restored afterwards.
fn transform_named_function_expression_body(
	state: &mut State<'_>,
	node: P<Node>,
	replacement: Option<NodeId>,
) -> TransformedFunctionBody {
	let name_symbol = if let Some(name_node) = node.name() {
		state.ck().get_symbol_at_location(name_node)
	} else {
		state.ice("transformer: named FunctionExpression has no name");
		None
	};
	let Some(name_symbol) = name_symbol else {
		state.ice("transformer: FunctionExpression name has no symbol");
		return transform_function_body(state, node);
	};
	let previous_hoist = state.is_hoisted.insert(name_symbol, false);
	let previous_replacement = match replacement {
		Some(replacement) => state.symbol_to_id.insert(name_symbol, replacement),
		None => state.symbol_to_id.get(&name_symbol).copied(),
	};
	let body = transform_function_body(state, node);
	match previous_hoist {
		Some(was_hoisted) => state.is_hoisted.insert(name_symbol, was_hoisted),
		None => state.is_hoisted.remove(&name_symbol),
	};
	match previous_replacement {
		Some(previous) => state.symbol_to_id.insert(name_symbol, previous),
		None => state.symbol_to_id.remove(&name_symbol),
	};
	body
}

/// `transformMatchingNamedFunctionConst`: `const f = function f() {}` emits one
/// `local function f`. `None` when the statement is any other shape.
pub fn transform_matching_named_function_const(state: &mut State<'_>, node: P<Node>) -> Option<Statements> {
	if tsast::has_syntactic_modifier(node, ModifierFlags::Export) {
		return None;
	}

	let declaration_list = node.as_variable_statement().declaration_list;
	let flags = declaration_list.flags();
	if !flags.intersects(NodeFlags::Const) || flags.intersects(NodeFlags::Using) {
		return None;
	}

	let [declaration] = declaration_list.as_variable_declaration_list().declarations.nodes() else {
		return None;
	};
	let binding_name = declaration.as_variable_declaration().name;
	let initializer = declaration.initializer()?;
	if !is_identifier(binding_name) || !tsast::is_function_expression(initializer) {
		return None;
	}

	let function_name = initializer.name()?;
	if function_name.text() != binding_name.text() {
		return None;
	}

	validate_identifier(state, binding_name);
	let Some(binding_symbol) = state.ck().get_symbol_at_location(binding_name) else {
		state.ice("transformer: matching named function const has no binding symbol");
		return None;
	};
	if is_exported_local_symbol(state, binding_symbol) {
		return None;
	}
	check_variable_hoist(state, binding_name, binding_symbol);
	let name = transform_identifier_defined(state, binding_name);
	let localize = !state.is_hoisted.get(&binding_symbol).copied().unwrap_or(false);
	Some(emit_named_function_expression(state, initializer, name, localize, None))
}

/// `isExportedLocalSymbol`: some export of the file's module resolves to `local_symbol`.
fn is_exported_local_symbol(state: &mut State<'_>, local_symbol: P<Symbol>) -> bool {
	let source_file = state.source_file.as_node();
	let Some(module_symbol) = state.ck().get_symbol_at_location(source_file) else {
		state.ice("transformer: source file has no module symbol");
		return false;
	};
	state
		.get_module_exports(module_symbol)
		.into_iter()
		.any(|export_symbol| skip_alias(export_symbol, state.checker) == local_symbol)
}
