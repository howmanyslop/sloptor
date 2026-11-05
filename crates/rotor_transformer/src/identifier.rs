//! `identifier.go`: `expressions/transformIdentifier.ts`.

use rotor_tsrs::ast::{self as tsast, ModifierFlags, SymbolFlags};

use crate::diagnostics;
use crate::exports::is_symbol_mutable;
use crate::macros;
use crate::prelude::*;
use crate::types::get_first_construct_symbol;
use crate::util::{index_of_node, is_ancestor_of};

/// The shorthand-aware symbol lookup shared by [`transform_identifier`] and
/// [`transform_identifier_defined`]: an identifier inside a shorthand property assignment
/// resolves through `getShorthandAssignmentValueSymbol`.
fn identifier_symbol(state: &mut State<'_>, node: P<Node>) -> Option<P<Symbol>> {
	let symbol = match node.parent() {
		Some(parent) if tsast::is_shorthand_property_assignment(parent) => {
			state.ck().get_shorthand_assignment_value_symbol(Some(parent))
		}
		_ => state.ck().get_symbol_at_location(node),
	};
	if symbol.is_none() {
		// tsgo returns nil where TypeScript returns unknownSymbol. Upstream never sees that:
		// the pre-emit diagnostics skip the file first. [flamework] noSemanticDiagnostics skips
		// that precheck, so emit the identifier text instead of asserting.
		if state.skip_semantic_diagnostics {
			return None;
		}
		let message = missing_identifier_symbol(node);
		state.ice(message);
	}
	symbol
}

fn missing_identifier_symbol(node: P<Node>) -> String {
	let name = rotor_support::gostr::quote(node.text());
	match tsast::get_source_file_of_node(node) {
		Some(file) if !file.file_name().is_empty() => {
			format!(
				"transformer: identifier {name} has no symbol\n    at {}",
				file.file_name()
			)
		}
		_ => format!("transformer: identifier {name} has no symbol"),
	}
}

/// `TransformIdentifier` (`transformIdentifier.ts` L111-176): an identifier in a use position.
pub fn transform_identifier(state: &mut State<'_>, node: P<Node>) -> NodeId {
	// Synthetic nodes have no parents or symbols, so skip the symbol logic. JSX entity names
	// from getJsxFactoryEntity are synthetic and end up here.
	if node.parent().is_none() || node.pos().is_synthetic() {
		return state.ast.id(node.text());
	}

	let Some(symbol) = identifier_symbol(state, node) else {
		return state.ast.id(node.text());
	};

	if state.ck().is_undefined_symbol(symbol) {
		return state.ast.nil();
	} else if state.ck().is_arguments_symbol(symbol) {
		state.diags.add(diagnostics::no_arguments(node));
	} else if is_global_this_symbol(symbol) {
		state.diags.add(diagnostics::no_global_this(node));
	}

	// Identifier macros (upstream L132-135): `Promise` -> `TS.Promise`. Constructor-macro `new`
	// expressions never reach here, but their non-macro fallback transforms the callee.
	if let Some(entry) = state.macros().get_identifier_macro(symbol) {
		return (entry.macro_fn)(state, node);
	}

	// rotor extension: a bare `$env` outside a call or access has no runtime value.
	if node.text() == "$env"
		&& let Some(env_symbol) = macros::env_macro_symbol(state)
		&& symbol == env_symbol
	{
		state.diags.add(diagnostics::rotor_env_bad_usage(node));
		return state.ast.none();
	}

	// rotor extension: bare $nameof / $keys / $file outside a call have no runtime value.
	match node.text() {
		"$nameof" if macros::nameof_macro_symbol(state) == Some(symbol) => {
			state.diags.add(diagnostics::rotor_nameof_bad_usage(node));
			return state.ast.none();
		}
		"$keys" if macros::keys_macro_symbol(state) == Some(symbol) => {
			state.diags.add(diagnostics::rotor_keys_bad_usage(node));
			return state.ast.none();
		}
		"$file" if macros::file_macro_symbol(state) == Some(symbol) => {
			state.diags.add(diagnostics::rotor_file_bad_usage(node));
			return state.ast.none();
		}
		_ => {}
	}

	// Constructor-macro misuse (upstream L137-150).
	if let Some(construct_symbol) = get_first_construct_symbol(state, node)
		&& state.macros().get_constructor_macro(construct_symbol).is_some()
	{
		let class_like = node
			.parent()
			.and_then(|parent| parent.parent())
			.and_then(|grandparent| grandparent.parent());
		let is_class_extends_node = class_like.is_some_and(|class_like| {
			tsast::is_class_like(class_like)
				&& tsast::get_class_extends_heritage_element(class_like)
					.is_some_and(|extends| extends.expression() == Some(node))
		});
		if is_class_extends_node {
			state.diags.add(diagnostics::no_macro_extends(node));
		} else {
			state.diags.add(diagnostics::no_constructor_macro_without_new(node));
		}
	}

	// Call-macro misuse outside call position (upstream L152-159).
	if let Some(parent) = tsast::skip_upwards(node).parent() {
		let is_direct_callee =
			tsast::is_call_expression(parent) && tsast::skip_downwards(parent.as_call_expression().expression) == node;
		if !is_direct_callee && state.macros().get_call_macro(symbol).is_some() {
			state.diags.add(diagnostics::no_index_without_call(node));
			return state.ast.none();
		}
	}

	// `export let` indirection (upstream L161-171): reads of mutable exported symbols compile
	// to `exports.<name>`.
	if let Some(value_declaration) = symbol.value_declaration()
		&& tsast::get_source_file_of_node(value_declaration) == tsast::get_source_file_of_node(node)
		&& tsast::find_ancestor(value_declaration, |candidate| {
			tsast::is_module_declaration(candidate) && !is_namespace(candidate)
		})
		.is_none()
	{
		let export_access = state.get_module_id_property_access(symbol);
		if let Some(export_access) = export_access
			&& is_symbol_mutable(state, symbol)
		{
			return export_access;
		}
	}

	check_identifier_hoist(state, node, symbol);

	transform_identifier_defined(state, node)
}

/// `TransformIdentifierDefined` (`transformIdentifier.ts` L14-28): an identifier in a defining
/// position, renamed through [`State::symbol_to_id`].
pub fn transform_identifier_defined(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let Some(symbol) = identifier_symbol(state, node) else {
		return state.ast.id(node.text());
	};
	if let Some(&replacement) = state.symbol_to_id.get(&symbol) {
		return replacement;
	}
	state.ast.id(node.text())
}

/// `getAncestorWhichIsChildOf`: the ancestor of `node` that is a direct child of `parent`.
fn get_ancestor_which_is_child_of(parent: P<Node>, mut node: P<Node>) -> Option<P<Node>> {
	while let Some(current_parent) = node.parent()
		&& current_parent != parent
	{
		node = current_parent;
	}
	node.parent()?;
	Some(node)
}

/// `getDeclarationFromImport`: `symbol.valueDeclaration` does not point to imports.
pub fn get_declaration_from_import(symbol: P<Symbol>) -> Option<P<Node>> {
	symbol
		.declarations()
		.iter()
		.copied()
		.find(|&declaration| tsast::find_ancestor(declaration, tsast::is_any_import_syntax).is_some())
}

/// `checkIdentifierHoist` (`transformIdentifier.ts` L47-109): records that a symbol must be
/// pre-declared when it is referenced at or before its declaring statement in the same block.
fn check_identifier_hoist(state: &mut State<'_>, node: P<Node>, symbol: P<Symbol>) {
	if state.is_hoisted.contains_key(&symbol) {
		return;
	}

	let Some(declaration) = symbol
		.value_declaration()
		.or_else(|| get_declaration_from_import(symbol))
	else {
		return;
	};

	// parameters cannot be hoisted
	if tsast::find_ancestor_kind(declaration, Kind::Parameter).is_some()
		|| tsast::is_shorthand_property_assignment(declaration)
	{
		return;
	}

	// class expressions can self refer
	if tsast::is_class_like(declaration) && is_ancestor_of(declaration, node) {
		return;
	}

	let Some(declaration_statement) = tsast::find_ancestor(declaration, tsast::is_statement) else {
		return;
	};
	if tsast::is_for_statement(declaration_statement)
		|| tsast::is_for_of_statement(declaration_statement)
		|| tsast::is_try_statement(declaration_statement)
	{
		return;
	}

	let Some(parent) = declaration_statement.parent().filter(|&parent| is_block_like(parent)) else {
		return;
	};

	let Some(sibling) = get_ancestor_which_is_child_of(parent, node).filter(|&sibling| tsast::is_statement(sibling))
	else {
		return;
	};

	let statements = parent.statements();
	let declaration_index = index_of_node(statements, declaration_statement);
	let sibling_index = index_of_node(statements, sibling);

	if sibling_index > declaration_index {
		return;
	}

	if sibling_index == declaration_index {
		// non-async function declarations, class declarations, and variable statements can
		// self refer
		if (tsast::is_function_declaration(declaration_statement)
			&& !tsast::has_syntactic_modifier(declaration_statement, ModifierFlags::Async))
			|| tsast::is_class_declaration(declaration_statement)
			|| (tsast::is_variable_statement(declaration_statement)
				&& tsast::find_ancestor(node, |candidate| {
					tsast::is_statement(candidate) || tsast::is_function_like_declaration(candidate)
				}) == Some(declaration_statement))
		{
			return;
		}
	}

	state.hoists_by_statement.entry(sibling).or_default().push(node);
	state.is_hoisted.insert(symbol, true);
}

/// `isBlockLike` (`typeGuards.ts`): the kinds whose `Statements()` tsgo accepts.
#[must_use]
pub fn is_block_like(node: P<Node>) -> bool {
	matches!(
		node.kind(),
		Kind::SourceFile | Kind::Block | Kind::ModuleBlock | Kind::CaseClause | Kind::DefaultClause
	)
}

/// `isNamespace` (`typeGuards.ts`): a module declaration written with `namespace`.
#[must_use]
pub fn is_namespace(node: P<Node>) -> bool {
	tsast::is_module_declaration(node) && node.as_module_declaration().keyword() == Kind::NamespaceKeyword
}

/// `isGlobalThisSymbol`: the checker-synthesized `globalThis` symbol, matched structurally.
pub fn is_global_this_symbol(symbol: P<Symbol>) -> bool {
	symbol.name() == "globalThis" && symbol.flags().intersects(SymbolFlags::Module) && symbol.declarations().is_empty()
}

/// `ValidateIdentifier` (`statementlist.go`; `util/validateIdentifier.ts`): reserved-word and
/// invalid-identifier diagnostics.
pub fn validate_identifier(state: &mut State<'_>, node: P<Node>) {
	let text = node.text();
	if !rotor_luau::is_valid_identifier(text) {
		state.diags.add(diagnostics::no_invalid_identifier(node));
	} else if rotor_luau::is_reserved_identifier(text) {
		state.diags.add(diagnostics::no_reserved_identifier(node));
	}
}
