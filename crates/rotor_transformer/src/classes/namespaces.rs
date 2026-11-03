//! `namespaces.go`: `statements/transformModuleDeclaration.ts`.

use std::collections::HashMap;

use rotor_tsrs::ast::{self as tsast, ModifierFlags};
use rotor_tsrs::checker::skip_alias;

use super::enums::{add_multiple_definitions_diagnostic, has_multiple_definitions};
use crate::diagnostics;
use crate::exports::{is_symbol_mutable, is_symbol_of_value};
use crate::identifier::{transform_identifier_defined, validate_identifier};
use crate::prelude::*;
use crate::statementlist::{ExportInfo, transform_statement_list};

/// `isDeclarationOfNamespace`: `transformModuleDeclaration.ts` (L16-30), the merge filter. A
/// declaration counts toward the namespace value definitions iff it has no `declare` modifier and
/// is an instantiated module, a function declaration WITH a body, or a class declaration.
fn is_declaration_of_namespace(declaration: P<Node>) -> bool {
	if tsast::has_syntactic_modifier(declaration, ModifierFlags::Ambient) {
		return false;
	}

	(tsast::is_module_declaration(declaration) && tsast::is_instantiated_module(declaration, false))
		|| (tsast::is_function_declaration(declaration) && declaration.body().is_some())
		|| tsast::is_class_declaration(declaration)
}

/// `getValueDeclarationStatement`: `transformModuleDeclaration.ts` (L32-44). For an export
/// symbol, the STATEMENT that declares its value, skipping function overload signatures, type
/// aliases, interfaces and `declare`d statements.
fn get_value_declaration_statement(symbol: P<Symbol>) -> Option<P<Node>> {
	for &declaration in symbol.declarations() {
		if let Some(statement) = tsast::find_ancestor(declaration, tsast::is_statement) {
			if tsast::is_function_declaration(statement) && statement.body().is_none() {
				continue;
			}
			if tsast::is_type_alias_declaration(statement) {
				continue;
			}
			if tsast::is_interface_declaration(statement) {
				continue;
			}
			if tsast::has_syntactic_modifier(statement, ModifierFlags::Ambient) {
				continue;
			}
			return Some(statement);
		}
	}
	None
}

/// `transformNamespace`: `transformModuleDeclaration.ts` (L46-122). `local X = {}` (or `X = {}`
/// when hoisted) and a do-block. With value exports the do-block opens with
/// `local _container = X`; non-mutable value exports are appended after their declaring
/// statement by the statement-list `ExportInfo` mapping, while mutable ones (`export let`) are
/// excluded: `SetModuleIDBySymbol(containerId)` routes them through `_container.<name>`. `body` is
/// a `ModuleBlock`, or a nested `ModuleDeclaration` for dotted `namespace A.B`.
fn transform_namespace(state: &mut State<'_>, name: P<Node>, body: P<Node>) -> Statements {
	let Some(symbol) = state.ck().get_symbol_at_location(name) else {
		state.ice("transformer: transformNamespace: no symbol"); // upstream assert
		return state.ast.new_list();
	};

	validate_identifier(state, name);

	let name_exp = transform_identifier_defined(state, name);

	let statements = state.ast.new_list();
	let do_statements = state.ast.new_list();

	let container_id = state.ast.temp_id("container");
	state.set_module_id_by_symbol(symbol, container_id);

	let empty_fields = state.ast.new_list();
	let empty = state.ast.map(empty_fields);
	let declaration = if state.is_hoisted.get(&symbol).copied().unwrap_or(false) {
		state.ast.assignment(name_exp, AssignmentOperator::Assign, empty)
	} else {
		state.ast.variable_declaration(name_exp, Some(empty.into()))
	};
	state.ast.list_push(statements, declaration);

	let module_exports = state.get_module_exports(symbol);
	if !module_exports.is_empty() {
		let declaration = state.ast.variable_declaration(container_id, Some(name_exp.into()));
		state.ast.list_push(do_statements, declaration);
	}

	if tsast::is_module_block(body) {
		let mut exports_map: HashMap<P<Node>, Vec<String>> = HashMap::new();
		for &export_symbol in &module_exports {
			let original_symbol = skip_alias(export_symbol, state.checker);
			if is_symbol_of_value(original_symbol)
				&& !is_symbol_mutable(state, original_symbol)
				&& let Some(value_declaration_statement) = get_value_declaration_statement(export_symbol)
			{
				exports_map
					.entry(value_declaration_statement)
					.or_default()
					.push(export_symbol.name().to_owned());
			}
		}
		let export_info = ExportInfo {
			id: container_id,
			mapping: exports_map,
		};
		let list = transform_statement_list(state, Some(body), body.statements(), Some(&export_info));
		state.ast.list_push_list(do_statements, list);
	} else if let (Some(inner_name), Some(inner_body)) = (body.name(), body.body()) {
		// dotted `namespace A.B { ... }`: tsgo parses the tail as a nested ModuleDeclaration (with
		// an implicit reparsed export modifier)
		let inner = transform_namespace(state, inner_name, inner_body);
		state.ast.list_push_list(do_statements, inner);
		let target = state.ast.property_access(container_id, inner_name.text());
		let value = transform_identifier_defined(state, inner_name);
		let assignment = state.ast.assignment(target, AssignmentOperator::Assign, value);
		state.ast.list_push(do_statements, assignment);
	} else {
		state.ice("transformer: transformNamespace: nested module has no name or body");
	}

	let block = state.ast.do_statement(do_statements);
	state.ast.list_push(statements, block);

	statements
}

/// `transformModuleDeclaration`: `transformModuleDeclaration.ts` (L124-146). Type-only
/// namespaces emit nothing; merging is banned (reported once per symbol); `declare module "X"`
/// and `declare namespace` are filtered by the dispatch-level declare skip.
pub fn transform_module_declaration(state: &mut State<'_>, node: P<Node>) -> Statements {
	// type-only namespace
	if !tsast::is_instantiated_module(node, false) {
		return state.ast.new_list();
	}

	let Some(name) = node.name() else {
		state.ice("transformer: transformModuleDeclaration: no name");
		return state.ast.new_list();
	};

	// disallow merging
	let symbol = state.ck().get_symbol_at_location(name);
	if let Some(symbol) = symbol
		&& has_multiple_definitions(symbol, is_declaration_of_namespace)
	{
		add_multiple_definitions_diagnostic(state, symbol, diagnostics::no_namespace_merging(node));
		return state.ast.new_list();
	}

	// ts.StringLiteral is only in the case of `declare module "X"`? Should be filtered out above
	if tsast::is_string_literal(name) {
		state.ice("transformer: transformModuleDeclaration: string literal name"); // upstream assert
		return state.ast.new_list();
	}
	let Some(body) = node.body().filter(|&body| !tsast::is_identifier(body)) else {
		state.ice("transformer: transformModuleDeclaration: unexpected body"); // upstream assert
		return state.ast.new_list();
	};
	transform_namespace(state, name, body)
}
