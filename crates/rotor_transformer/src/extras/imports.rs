//! `imports.go`: `statements/transformImportDeclaration.ts`, `transformImportEqualsDeclaration.ts`,
//! `transformExportDeclaration.ts` and `transformExportAssignment.ts`.

use rotor_tsrs::ast::{self as tsast, SymbolFlags, get_source_file_of_node};
use rotor_tsrs::checker::skip_alias;

use super::importexpr::{create_import_expression, get_source_file_from_module_specifier};
use crate::call::convert_to_indexable_expression;
use crate::diagnostics;
use crate::dispatch::transform_expression;
use crate::exports::{get_original_symbol_of_node, is_symbol_mutable, is_symbol_of_value};
use crate::identifier::{transform_identifier, validate_identifier};
use crate::prelude::*;
use crate::statements::transform_variable;

/// `cleanModuleName` (`util/cleanModuleName.ts`): every non-word character becomes `_`. Go's
/// `\W` is ASCII, and matches one whole rune at a time.
fn clean_module_name(name: &str) -> String {
	name.chars()
		.map(|character| {
			if character.is_ascii_alphanumeric() || character == '_' {
				character
			} else {
				'_'
			}
		})
		.collect()
}

/// The temp a module import is hoisted into: the cleaned last segment of the specifier.
fn module_temp_name(module_specifier: P<Node>) -> String {
	let text = module_specifier.text();
	clean_module_name(text.rsplit('/').next().unwrap_or(text))
}

/// `lazyImportExp` (`Shared/classes/Lazy.ts`): the `TS.import` call is only built when some
/// binding survives elision; [`LazyImportExp::set`] overrides the memo with the hoisted temp.
struct LazyImportExp {
	node: P<Node>,
	module_specifier: P<Node>,
	value: Option<NodeId>,
}

impl LazyImportExp {
	const fn new(node: P<Node>, module_specifier: P<Node>) -> Self {
		Self {
			node,
			module_specifier,
			value: None,
		}
	}

	fn get(&mut self, state: &mut State<'_>) -> NodeId {
		if let Some(value) = self.value {
			return value;
		}
		let source_file = get_source_file_of_node(self.node).unwrap_or(state.source_file);
		let value = create_import_expression(state, source_file, self.module_specifier);
		self.value = Some(value);
		value
	}

	const fn set(&mut self, value: NodeId) {
		self.value = Some(value);
	}
}

/// `isReferencedAliasValue`: the elision predicate for the default name and each named element.
/// `declaration` is the node the emit resolver is asked about (the import clause or specifier);
/// `name` is the bound identifier.
pub(super) fn is_referenced_alias_value(state: &mut State<'_>, declaration: P<Node>, name: P<Node>) -> bool {
	let symbol = get_original_symbol_of_node(state, name);
	if state.is_referenced_alias_declaration(declaration) && symbol.is_none_or(is_symbol_of_value) {
		return true;
	}
	if symbol.is_some_and(|symbol| symbol.flags().intersects(SymbolFlags::ConstEnum)) {
		return false;
	}
	is_imported_name_referenced_as_value(state, name)
}

/// `isImportedNameReferencedAsValue`: whether any identifier of the file other than `name` reads
/// the imported binding as a value.
pub(super) fn is_imported_name_referenced_as_value(state: &mut State<'_>, name: P<Node>) -> bool {
	if imported_name_used_as_jsx_factory(state, name) {
		return true;
	}
	let alias = state.ck().get_symbol_at_location(name);
	let target = alias.map(|alias| skip_alias(alias, state.checker));
	let file = state.source_file;
	file.as_node()
		.for_each_child(&mut |child| visit_value_reference(state, child, name, alias, target))
}

fn visit_value_reference(
	state: &mut State<'_>,
	node: P<Node>,
	name: P<Node>,
	alias: Option<P<Symbol>>,
	target: Option<P<Symbol>>,
) -> bool {
	if tsast::is_identifier(node) {
		if node == name || node.text() != name.text() {
			return false;
		}
		// IsPartOfTypeNode is false for the left side of a QualifiedName (`A` in `A.B`), so judge
		// the identifier by the outermost qualified name it belongs to. PropertyAccessExpression
		// is not climbed: `A.b` in an expression is a real value use.
		let mut type_name = node;
		while let Some(parent) = type_name.parent()
			&& tsast::is_qualified_name(parent)
		{
			type_name = parent;
		}
		if tsast::is_part_of_type_node(type_name) || tsast::is_part_of_type_query(type_name) {
			return false;
		}
		let Some(alias) = alias else {
			return true;
		};
		let reference = state.ck().get_symbol_at_location(node);
		if references_alias(state, reference, alias, target) {
			return true;
		}
		if reference.is_none() {
			// Under noSemanticDiagnostics the checker may fail to resolve a value use (e.g. the
			// base of a property access). Treat an unresolvable value identifier as a reference
			// rather than drop a runtime binding.
			return true;
		}
		if let Some(parent) = node.parent()
			&& tsast::is_shorthand_property_assignment(parent)
		{
			let reference = state.ck().get_shorthand_assignment_value_symbol(Some(parent));
			if references_alias(state, reference, alias, target) {
				return true;
			}
		}
		return false;
	}
	node.for_each_child(&mut |child| visit_value_reference(state, child, name, alias, target))
}

/// `reference == alias || reference != nil && SkipAlias(reference) == target`
fn references_alias(
	state: &mut State<'_>,
	reference: Option<P<Symbol>>,
	alias: P<Symbol>,
	target: Option<P<Symbol>>,
) -> bool {
	reference == Some(alias) || reference.is_some_and(|reference| Some(skip_alias(reference, state.checker)) == target)
}

fn imported_name_used_as_jsx_factory(state: &mut State<'_>, name: P<Node>) -> bool {
	if !file_contains_jsx(state.source_file.as_node()) {
		return false;
	}
	let factory = state.ck().get_jsx_factory_entity(name);
	if factory.is_some_and(|factory| tsast::get_first_identifier(factory).text() == name.text()) {
		return true;
	}
	let fragment = state.ck().get_jsx_fragment_factory_entity(name);
	fragment.is_some_and(|fragment| tsast::get_first_identifier(fragment).text() == name.text())
}

fn file_contains_jsx(node: P<Node>) -> bool {
	fn visit(child: P<Node>) -> bool {
		if tsast::is_jsx_element(child) || tsast::is_jsx_self_closing_element(child) || tsast::is_jsx_fragment(child) {
			return true;
		}
		child.for_each_child(&mut visit)
	}
	node.for_each_child(&mut visit)
}

/// `countImportExpUses` (L13-37): how many bindings read the import. Namespace imports count
/// unconditionally.
fn count_import_exp_uses(state: &mut State<'_>, import_clause: P<Node>) -> usize {
	let clause = import_clause.as_import_clause();
	let mut uses = 0;

	if let Some(name) = clause.name
		&& is_referenced_alias_value(state, import_clause, name)
	{
		uses += 1;
	}

	if let Some(named_bindings) = clause.named_bindings {
		if tsast::is_namespace_import(named_bindings) {
			uses += 1;
		} else {
			for &element in named_bindings.elements() {
				if let Some(name) = element.name()
					&& is_referenced_alias_value(state, element, name)
				{
					uses += 1;
				}
			}
		}
	}

	uses
}

/// `transformImportDeclaration` (`statements/transformImportDeclaration.ts` L39-131).
pub fn transform_import_declaration(state: &mut State<'_>, node: P<Node>) -> Statements {
	let declaration = node.as_import_declaration();

	// no emit for type only
	let import_clause = declaration.import_clause;
	if import_clause.is_some_and(|clause| clause.is_type_only()) {
		return state.ast.new_list();
	}

	let statements = state.ast.new_list();

	let module_specifier = declaration.module_specifier;
	if !tsast::is_string_literal(module_specifier) {
		state.ice("transformer: transformImportDeclaration: module specifier is not a string literal");
		return statements;
	}
	let mut import_exp = LazyImportExp::new(node, module_specifier);

	if let Some(import_clause) = import_clause {
		let clause = import_clause.as_import_clause();

		// Detect if we need to push to a new var or not (L53-65).
		let uses = count_import_exp_uses(state, import_clause);
		if uses > 1 {
			let id = state.ast.temp_id(module_temp_name(module_specifier));
			let value = import_exp.get(state);
			let declaration = state.ast.variable_declaration(id, Some(value.into()));
			state.ast.list_push(statements, declaration);
			import_exp.set(id);
		}

		// Default import logic (L68-88): `.default` when the target module has a real `default`
		// export, else the whole module table.
		if let Some(import_clause_name) = clause.name
			&& is_referenced_alias_value(state, import_clause, import_clause_name)
		{
			let module_file = get_source_file_from_module_specifier(state, module_specifier);
			let module_symbol = module_file.and_then(|file| state.ck().get_symbol_at_location(file.as_node()));
			let has_default_export = module_symbol.is_some_and(|module_symbol| {
				state
					.get_module_exports(module_symbol)
					.iter()
					.any(|export| export.name() == "default")
			});
			let captured = state.capture_statements(|state| {
				let mut value = import_exp.get(state);
				if has_default_export {
					value = state.ast.property_access(value, "default");
				}
				transform_variable(state, import_clause_name, Some(value));
			});
			state.ast.list_push_list(statements, captured);
		}

		if let Some(named_bindings) = clause.named_bindings {
			if tsast::is_namespace_import(named_bindings) {
				// Namespace import logic (L93-99): the whole module table, never elided.
				let captured = state.capture_statements(|state| {
					let value = import_exp.get(state);
					if let Some(name) = named_bindings.name() {
						transform_variable(state, name, Some(value));
					}
				});
				state.ast.list_push_list(statements, captured);
			} else {
				// Named elements import logic (L102-118): `import { greet as g }` reads `greet`
				// and binds `g`.
				for &element in named_bindings.elements() {
					let Some(name) = element.name() else {
						continue;
					};
					if is_referenced_alias_value(state, element, name) {
						let property_name = element.property_name().unwrap_or(name);
						let captured = state.capture_statements(|state| {
							let module = import_exp.get(state);
							let value = state.ast.property_access(module, property_name.text());
							transform_variable(state, name, Some(value));
						});
						state.ast.list_push_list(statements, captured);
					}
				}
			}
		}
	}

	// Ensure we emit something (L122-128): `import "./x"` emits a bare call; under
	// verbatimModuleSyntax a fully-elided clause still emits it.
	if import_clause.is_none()
		|| (state.program.options().verbatim_module_syntax.is_true() && state.ast.list_is_empty(statements))
	{
		let expression = import_exp.get(state);
		if state.ast.kind(expression) == LuauKind::CallExpression {
			let call = state.ast.call_statement(expression);
			state.ast.list_push(statements, call);
		}
	}

	statements
}

/// `transformImportEqualsDeclaration` (`statements/transformImportEqualsDeclaration.ts`
/// L10-44).
pub fn transform_import_equals_declaration(state: &mut State<'_>, node: P<Node>) -> Statements {
	let declaration = node.as_import_equals_declaration();
	let module_reference = declaration.module_reference;
	let name = declaration.name;

	if tsast::is_external_module_reference(module_reference) {
		// `import x = require("./y")`: an eager import, the whole module table.
		let expression = module_reference.as_external_module_reference().expression();
		if !tsast::is_string_literal(expression) {
			state.ice("transformer: transformImportEqualsDeclaration: module reference is not a string literal");
			return state.ast.new_list();
		}
		let source_file = get_source_file_of_node(node).unwrap_or(state.source_file);
		let import_exp = create_import_expression(state, source_file, expression);

		let statements = state.ast.new_list();

		let Some(alias_symbol) = state.ck().get_symbol_at_location(name) else {
			state.ice("transformer: transformImportEqualsDeclaration: no alias symbol");
			return statements;
		};
		if is_symbol_of_value(skip_alias(alias_symbol, state.checker))
			|| is_imported_name_referenced_as_value(state, name)
		{
			let captured = state.capture_statements(|state| {
				transform_variable(state, name, Some(import_exp));
			});
			state.ast.list_push_list(statements, captured);
		}

		// ensure we emit something
		if state.program.options().verbatim_module_syntax.is_true()
			&& state.ast.list_is_empty(statements)
			&& state.ast.kind(import_exp) == LuauKind::CallExpression
		{
			let call = state.ast.call_statement(import_exp);
			state.ast.list_push(statements, call);
		}

		return statements;
	}

	// Identifier | QualifiedName (issue #1895): plain aliasing of a namespace path.
	state.capture_statements(|state| {
		let value = transform_entity_name(state, module_reference);
		transform_variable(state, name, Some(value));
	})
}

/// `transformEntityName` (`nodes/transformEntityName.ts`).
pub(super) fn transform_entity_name(state: &mut State<'_>, node: P<Node>) -> NodeId {
	if tsast::is_identifier(node) {
		validate_identifier(state, node);
		return transform_identifier(state, node);
	}
	transform_qualified_name(state, node)
}

fn transform_qualified_name(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let qualified = node.as_qualified_name();
	let left = transform_entity_name(state, qualified.left);
	let left = convert_to_indexable_expression(&mut state.ast, left);
	state.ast.property_access(left, qualified.right.text())
}

// ---------------------------------------------------------------------------------------------
// Export-from declarations (statements/transformExportDeclaration.ts)
// ---------------------------------------------------------------------------------------------

/// `isExportSpecifierValue` (L9-24): not type-only, and either referenced or (fallback) the
/// skipped alias is a value.
fn is_export_specifier_value(state: &mut State<'_>, element: P<Node>) -> bool {
	if element.as_export_specifier().is_type_only {
		return false;
	}

	if state.is_referenced_alias_declaration(element) {
		return true;
	}

	let Some(name) = element.name() else {
		return false;
	};
	state
		.ck()
		.get_symbol_at_location(name)
		.is_some_and(|alias_symbol| is_symbol_of_value(skip_alias(alias_symbol, state.checker)))
}

/// `countExportFromUses` (upstream `countImportExpUses`, L26-38).
fn count_export_from_uses(state: &mut State<'_>, export_clause: Option<P<Node>>) -> usize {
	if let Some(export_clause) = export_clause
		&& tsast::is_named_exports(export_clause)
	{
		let mut uses = 0;
		for &element in export_clause.elements() {
			if is_export_specifier_value(state, element) {
				uses += 1;
			}
		}
		return uses;
	}
	1
}

/// `transformExportFrom` (L40-123). Every assignment lands at statement position.
fn transform_export_from(state: &mut State<'_>, node: P<Node>) -> Statements {
	let declaration = node.as_export_declaration();
	let statements = state.ast.new_list();
	let Some(module_specifier) = declaration
		.module_specifier
		.filter(|&specifier| tsast::is_string_literal(specifier))
	else {
		state.ice("transformer: transformExportFrom: module specifier is not a string literal");
		return statements;
	};

	let export_clause = declaration.export_clause;
	let source_file = get_source_file_of_node(node).unwrap_or(state.source_file);

	// Detect if we need to push to a new var or not (L49-62).
	let uses = count_export_from_uses(state, export_clause);
	let import_exp = match uses {
		0 => return statements,
		1 => create_import_expression(state, source_file, module_specifier),
		_ => {
			let id = state.ast.temp_id(module_temp_name(module_specifier));
			let value = create_import_expression(state, source_file, module_specifier);
			let hoisted = state.ast.variable_declaration(id, Some(value.into()));
			state.ast.list_push(statements, hoisted);
			id
		}
	};

	let module_id = state.get_module_id_from_node(node);
	if let Some(export_clause) = export_clause {
		if tsast::is_named_exports(export_clause) {
			// export { a, b, c } from "./module";
			for &element in export_clause.elements() {
				if is_export_specifier_value(state, element) {
					let Some(name) = element.name() else {
						continue;
					};
					let property_name = element.property_name().unwrap_or(name);
					let left = state.ast.property_access(module_id, name.text());
					let right = state.ast.property_access(import_exp, property_name.text());
					let assignment = state.ast.assignment(left, AssignmentOperator::Assign, right);
					state.ast.list_push(statements, assignment);
				}
			}
		} else {
			// export * as foo from "./module";
			let name = export_clause.name().map_or("", |name| name.text());
			let left = state.ast.property_access(module_id, name);
			let assignment = state.ast.assignment(left, AssignmentOperator::Assign, import_exp);
			state.ast.list_push(statements, assignment);
		}
	} else {
		// export * from "./module";
		let key_id = state.ast.temp_id("k");
		let value_id = state.ast.temp_id("v");
		let target = state.ast.computed_index(module_id, key_id);
		let assignment = state.ast.assignment(target, AssignmentOperator::Assign, value_id);
		let body = state.ast.list_from([assignment]);
		let ids = state.ast.list_from([key_id, value_id]);
		// importExp may be `nil` in .d.ts files, so default to `{}`; boolean `or` is safe, because
		// importExp can only be a table if not `nil`.
		let fields = state.ast.new_list();
		let empty = state.ast.map(fields);
		let iterable = state.ast.binary(import_exp, BinaryOperator::Or, empty);
		let loop_statement = state.ast.for_statement(ids, iterable, body);
		state.ast.list_push(statements, loop_statement);
	}

	state.has_export_from = true;

	statements
}

/// `transformExportDeclaration` (`statements/transformExportDeclaration.ts` L125-133): type-only
/// and plain `export { x }` emit nothing here; export-from goes through `transformExportFrom`.
pub fn transform_export_declaration(state: &mut State<'_>, node: P<Node>) -> Statements {
	let declaration = node.as_export_declaration();
	if declaration.is_type_only {
		return state.ast.new_list();
	}
	if declaration.module_specifier.is_some() {
		return transform_export_from(state, node);
	}
	state.ast.new_list()
}

// ---------------------------------------------------------------------------------------------
// Export assignment (statements/transformExportAssignment.ts)
// ---------------------------------------------------------------------------------------------

/// `transformExportEquals` (L10-27): the final statement returns the value directly; otherwise
/// it lands in `local exports = <expr>`.
fn transform_export_equals(state: &mut State<'_>, node: P<Node>, expression: P<Node>) -> Statements {
	state.has_export_equals = true;

	let result = state.ast.new_list();
	let Some(source_file) = get_source_file_of_node(node) else {
		// An export assignment always comes from a real tree, so this is unreachable, but never
		// emit a half-file if one ever detaches.
		return result;
	};
	let final_statement = source_file.statements().nodes().last().copied();
	if final_statement == Some(node) {
		let value = transform_expression(state, expression);
		let statement = state.ast.return_statement(value);
		state.ast.list_push(result, statement);
	} else {
		let module_id = state.get_module_id_from_node(node);
		let value = transform_expression(state, expression);
		let statement = state.ast.variable_declaration(module_id, Some(value.into()));
		state.ast.list_push(result, statement);
	}
	result
}

/// `transformExportDefault` (L29-43): `local default = <expr>`.
fn transform_export_default(state: &mut State<'_>, expression: P<Node>) -> Statements {
	let statements = state.ast.new_list();

	let (value, prereqs) = state.capture(|state| transform_expression(state, expression));
	state.ast.list_push_list(statements, prereqs);
	let default = state.ast.id("default");
	let declaration = state.ast.variable_declaration(default, Some(value.into()));
	state.ast.list_push(statements, declaration);

	statements
}

/// `transformExportAssignment` (`statements/transformExportAssignment.ts` L45-60).
pub fn transform_export_assignment(state: &mut State<'_>, node: P<Node>) -> Statements {
	let Some(expression) = node.expression() else {
		state.ice("transformer: transformExportAssignment: no expression");
		return state.ast.new_list();
	};

	let symbol = state.ck().get_symbol_at_location(expression);
	if let Some(symbol) = symbol
		&& is_symbol_mutable(state, symbol)
	{
		state.diags.add(diagnostics::no_export_assignment_let(node));
	}

	// type-only `export =` emits nothing
	if let Some(symbol) = symbol
		&& !is_symbol_of_value(skip_alias(symbol, state.checker))
	{
		return state.ast.new_list();
	}

	if node.as_export_assignment().is_export_equals {
		return transform_export_equals(state, node, expression);
	}
	transform_export_default(state, expression)
}
