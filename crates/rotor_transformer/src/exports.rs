//! `exports.go`: the per-file export emission (`transformSourceFile.ts` `handleExports`) and the
//! module export queries.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use rotor_tsrs::ast::{self as tsast, ModifierFlags, NodeFlags, SymbolFlags};
use rotor_tsrs::checker::skip_alias;

use crate::identifier::transform_identifier_defined;
use crate::prelude::*;
use crate::util::{find_statement_ancestor, node_pos};

/// `ExportShape`: which of the four export emission shapes `handleExports` produces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportShape {
	/// No exports at all.
	None,
	/// `export = x`: append `return exports` unless the file ends with the `export =`.
	ExportEquals,
	/// Any `export ... from` or mutable export: `local exports = {}` at the top, assignments,
	/// then `return exports`.
	ExportsTable,
	/// Only immutable exports: `return { key = id, ... }` at the bottom.
	ReturnMap,
}

/// `ChooseExportShape`: export-equals wins outright; then export-from or a mutable export force
/// the exports table; then immutable exports return a map.
#[must_use]
#[expect(clippy::fn_params_excessive_bools, reason = "Go's ChooseExportShape signature")]
pub const fn choose_export_shape(
	has_export_equals: bool,
	has_export_from: bool,
	has_mutable_exports: bool,
	has_immutable_exports: bool,
) -> ExportShape {
	if has_export_equals {
		ExportShape::ExportEquals
	} else if has_export_from || has_mutable_exports {
		ExportShape::ExportsTable
	} else if has_immutable_exports {
		ExportShape::ReturnMap
	} else {
		ExportShape::None
	}
}

struct ExportPair {
	name: String,
	id: NodeId,
	symbol: P<Symbol>,
}

/// `getExportSyntaxAnchor` (`sourcefile.go`): the syntax an export's assignment maps back to.
pub fn get_export_syntax_anchor(export_symbol: P<Symbol>, source_file: P<SourceFile>) -> Option<P<Node>> {
	let in_file = |declaration: P<Node>| tsast::get_source_file_of_node(declaration) == Some(source_file);
	for &declaration in export_symbol.declarations() {
		if !in_file(declaration) {
			continue;
		}
		if tsast::is_export_specifier(declaration) || tsast::is_export_assignment(declaration) {
			return Some(declaration);
		}
		if let Some(statement) = find_statement_ancestor(declaration)
			&& tsast::has_syntactic_modifier(statement, ModifierFlags::Export)
		{
			return Some(statement);
		}
	}
	for &declaration in export_symbol.declarations() {
		if in_file(declaration) {
			return Some(find_statement_ancestor(declaration).unwrap_or(declaration));
		}
	}
	export_symbol.declarations().first().copied()
}

/// `handleExports` (`transformSourceFile.ts` lines 94-189): export information at the end (and,
/// for the exports table, the top) of the file's statements.
pub fn handle_exports(state: &mut State<'_>, source_file: P<SourceFile>, symbol: P<Symbol>, statements: Statements) {
	let ignored_export_symbols = get_ignored_export_symbols(state, source_file);

	let mut has_mutable_exports = false;
	let mut export_pairs = Vec::new();
	if !state.has_export_equals {
		for export_symbol in state.get_module_exports(symbol) {
			if ignored_export_symbols.contains(&export_symbol) {
				continue;
			}
			// ignore prototype exports
			if export_symbol.flags().intersects(SymbolFlags::Prototype) {
				continue;
			}
			// export { default as x } from "./module";
			if is_export_symbol_from_export_from(export_symbol) {
				continue;
			}
			let original_symbol = skip_alias(export_symbol, state.checker);
			// only export values
			if !is_symbol_of_value(original_symbol) {
				continue;
			}
			// export let: reads and writes already compile to exports.x in transformIdentifier;
			// presence forces the exports-table shape
			if is_symbol_mutable(state, original_symbol) {
				has_mutable_exports = true;
				continue;
			}
			// ignore exports in the form of `export declare const x: T;`
			if is_export_symbol_only_from_declare(export_symbol) {
				continue;
			}
			let (name, id) = get_export_pair(state, export_symbol);
			export_pairs.push(ExportPair {
				name,
				id,
				symbol: export_symbol,
			});
		}
	}

	match choose_export_shape(
		state.has_export_equals,
		state.has_export_from,
		has_mutable_exports,
		!export_pairs.is_empty(),
	) {
		ExportShape::ExportEquals => {
			// the local exports variable is created in transformExportAssignment
			let final_statement = source_file.statements().nodes().last().copied();
			let ends_with_export_equals = final_statement.is_some_and(|statement| {
				tsast::is_export_assignment(statement) && statement.as_export_assignment().is_export_equals()
			});
			if !ends_with_export_equals {
				let exports = state.ast.global_id("exports");
				let statement = state.ast.return_statement(exports);
				state.ast.list_push(statements, statement);
			}
		}
		ExportShape::ExportsTable => {
			// `local exports = {}` at the top of the list: this runs before the header is
			// prepended, so it lands right after the header in the final output.
			let exports = state.ast.global_id("exports");
			let fields = state.ast.new_list();
			let table = state.ast.map(fields);
			let declaration = state.ast.variable_declaration(exports, Some(table.into()));
			state.ast.list_unshift(statements, declaration);
			for pair in export_pairs {
				let target = state.ast.global_property("exports", pair.name.as_str());
				let assignment = state.ast.assignment(target, AssignmentOperator::Assign, pair.id);
				if let Some(anchor) = get_export_syntax_anchor(pair.symbol, source_file) {
					let start = state.original_source_position(Some(anchor), false);
					state.set_source_position(assignment, start);
					let end = state.original_source_position(Some(anchor), true);
					state.set_source_end_position(assignment, end);
				}
				state.ast.list_push(statements, assignment);
			}
			let exports = state.ast.global_id("exports");
			let statement = state.ast.return_statement(exports);
			state.ast.list_push(statements, statement);
		}
		ExportShape::ReturnMap => {
			let fields = state.ast.new_list();
			for pair in export_pairs {
				let key = state.ast.str(pair.name.as_str());
				let field = state.ast.map_field(key, pair.id);
				if let Some(anchor) = get_export_syntax_anchor(pair.symbol, source_file) {
					state.set_node_source_position(field, anchor);
				}
				state.ast.list_push(fields, field);
			}
			let map = state.ast.map(fields);
			let statement = state.ast.return_statement(map);
			state.ast.list_push(statements, statement);
		}
		ExportShape::None => {}
	}
}

/// `getExportPair`: `[exportName, luauId]`. `export { a as b }` gives `("b", <a>)`; otherwise the
/// symbol's name and an identifier, a default-exported named function or class using its
/// declared name.
pub fn get_export_pair(state: &mut State<'_>, export_symbol: P<Symbol>) -> (String, NodeId) {
	let declaration = export_symbol.declarations().first().copied();
	if let Some(declaration) = declaration
		&& tsast::is_export_specifier(declaration)
	{
		let name_node = declaration.property_name().or_else(|| declaration.name());
		let export_name = declaration.name().map_or("", |name| name.text());
		let id = match name_node {
			Some(name_node) => transform_identifier_defined(state, name_node),
			None => state.ast.none(),
		};
		return (export_name.to_owned(), id);
	}
	let mut name = export_symbol.name();
	if name == "default"
		&& let Some(declaration) = declaration
		&& (tsast::is_function_declaration(declaration) || tsast::is_class_declaration(declaration))
		&& let Some(declaration_name) = declaration.name()
	{
		name = declaration_name.text();
	}
	(export_symbol.name().to_owned(), state.ast.id(name))
}

/// `isExportSymbolFromExportFrom`: a declaration is a specifier of an `export ... from`.
fn is_export_symbol_from_export_from(export_symbol: P<Symbol>) -> bool {
	export_symbol.declarations().iter().any(|&declaration| {
		tsast::is_export_specifier(declaration)
			&& declaration
				.parent()
				.and_then(|parent| parent.parent())
				.is_some_and(|export_declaration| {
					tsast::is_export_declaration(export_declaration) && export_declaration.module_specifier().is_some()
				})
	})
}

/// `getIgnoredExportSymbols`: everything `export * from "./m"` re-exports, and the namespace id
/// of `export * as ns from "./m"`.
fn get_ignored_export_symbols(state: &mut State<'_>, source_file: P<SourceFile>) -> HashSet<P<Symbol>> {
	let mut ignored = HashSet::new();
	for &statement in source_file.statements().nodes() {
		if !tsast::is_export_declaration(statement) {
			continue;
		}
		let export_declaration = statement.as_export_declaration();
		let Some(module_specifier) = export_declaration.module_specifier() else {
			continue;
		};
		match export_declaration.export_clause() {
			None => {
				// export * from "./module";
				if let Some(module_symbol) = get_original_symbol_of_node(state, module_specifier) {
					ignored.extend(state.get_module_exports(module_symbol));
				}
			}
			Some(clause) if tsast::is_namespace_export(clause) => {
				// export * as id from "./module";
				if let Some(name) = clause.name()
					&& let Some(id_symbol) = state.ck().get_symbol_at_location(name)
				{
					ignored.insert(id_symbol);
				}
			}
			Some(_) => {}
		}
	}
	ignored
}

/// `isExportSymbolOnlyFromDeclare`: every declaration's statement has `declare`.
fn is_export_symbol_only_from_declare(export_symbol: P<Symbol>) -> bool {
	let declarations = export_symbol.declarations();
	!declarations.is_empty()
		&& declarations.iter().all(|&declaration| {
			find_statement_ancestor(declaration)
				.is_some_and(|statement| tsast::has_syntactic_modifier(statement, ModifierFlags::Ambient))
		})
}

/// `getOriginalSymbolOfNode` (`util/getOriginalSymbolOfNode.ts`).
pub fn get_original_symbol_of_node(state: &mut State<'_>, node: P<Node>) -> Option<P<Symbol>> {
	let symbol = state.ck().get_symbol_at_location(node)?;
	Some(skip_alias(symbol, state.checker))
}

/// `isSymbolOfValue` (`util/isSymbolOfValue.ts`): a runtime value, not a type or const enum.
#[must_use]
pub fn is_symbol_of_value(symbol: P<Symbol>) -> bool {
	symbol.flags().intersects(SymbolFlags::Value) && !symbol.flags().intersects(SymbolFlags::ConstEnum)
}

/// `IsSymbolMutable` (`util/isSymbolMutable.ts`), cached pass-wide: the value declaration is a
/// parameter or sits in a `let` declaration list.
pub fn is_symbol_mutable(state: &mut State<'_>, id_symbol: P<Symbol>) -> bool {
	if let Some(&cached) = state.multi.is_defined_as_let_cache.get(&id_symbol) {
		return cached;
	}
	let result = id_symbol.value_declaration().is_some_and(|declaration| {
		if declaration.kind() == Kind::Parameter {
			return true;
		}
		tsast::find_ancestor(declaration, tsast::is_variable_declaration_list)
			.is_some_and(|list| list.flags().intersects(NodeFlags::Let))
	});
	state.multi.is_defined_as_let_cache.insert(id_symbol, result);
	result
}

impl State<'_> {
	/// `GetModuleExports`, cached pass-wide.
	///
	/// DIVERGENCE (Go's): tsgo's export table is a Go map, so rotor reconstructs the binder's
	/// insertion order: function declarations first (`bindEachFunctionsFirst`), in source
	/// order, then everything else in source order. Declaration-less symbols sort last by name.
	pub fn get_module_exports(&mut self, module_symbol: P<Symbol>) -> Vec<P<Symbol>> {
		if let Some(cached) = self.multi.get_module_exports_cache.get(&module_symbol) {
			return cached.clone();
		}
		let mut exports = self.ck().get_exports_of_module(module_symbol);
		exports.sort_by(|&left, &right| compare_exports(left, right));
		self.multi
			.get_module_exports_cache
			.insert(module_symbol, exports.clone());
		exports
	}

	/// `GetModuleExportsAliasMap`, cached pass-wide: original (alias-skipped) export symbol to
	/// the name it is exported under.
	pub fn get_module_exports_alias_map(&mut self, module_symbol: P<Symbol>) -> &HashMap<P<Symbol>, String> {
		if !self
			.multi
			.get_module_exports_alias_map_cache
			.contains_key(&module_symbol)
		{
			let mut alias_map = HashMap::new();
			for export_symbol in self.get_module_exports(module_symbol) {
				let original_symbol = skip_alias(export_symbol, self.checker);
				let declaration = export_symbol.declarations().first().copied();
				let name = match declaration {
					Some(declaration) if tsast::is_export_specifier(declaration) => {
						declaration.name().map_or("", |name| name.text())
					}
					_ => export_symbol.name(),
				};
				alias_map.insert(original_symbol, name.to_owned());
			}
			self.multi
				.get_module_exports_alias_map_cache
				.insert(module_symbol, alias_map);
		}
		&self.multi.get_module_exports_alias_map_cache[&module_symbol]
	}
}

/// `GetModuleExports`'s `sort.SliceStable` comparator.
fn compare_exports(left: P<Symbol>, right: P<Symbol>) -> Ordering {
	match (export_sort_key(left), export_sort_key(right)) {
		(Some(_), None) => Ordering::Less,
		(None, Some(_)) => Ordering::Greater,
		(None, None) => left.name().cmp(right.name()),
		(Some(left_key), Some(right_key)) => left_key
			.cmp(&right_key)
			// Final tiebreaker: deterministic even when two symbols share a position.
			.then_with(|| left.name().cmp(right.name())),
	}
}

/// `exportSortKey`: `(file, pass, pos)`. Pass 0 holds a symbol with a function declaration
/// (the binder's functions-first pass), pass 1 everything else at its first declaration.
fn export_sort_key(symbol: P<Symbol>) -> Option<(String, u8, i64)> {
	let function = symbol
		.declarations()
		.iter()
		.copied()
		.find(|&declaration| tsast::is_function_declaration(declaration));
	let (declaration, pass) = match function {
		Some(declaration) => (declaration, 0),
		None => (
			symbol
				.declarations()
				.first()
				.copied()
				.or_else(|| symbol.value_declaration())?,
			1,
		),
	};
	let file = tsast::get_source_file_of_node(declaration).map_or_else(String::new, |file| file.file_name().to_owned());
	Some((file, pass, node_pos(declaration)))
}
