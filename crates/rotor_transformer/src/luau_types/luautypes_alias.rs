//! `luautypes_alias.go`: alias placement, exports, cross-file mirrors and naming for the
//! `luauTypes` registry.
//!
//! - A top-level interface, type alias or enum of this file is placed at its declaration.
//! - Everything else is hoisted after the header, in registration order: nested declarations
//!   (flattened `Outer_Inner_Type` names), ambient declarations, types imported from another file
//!   (mirrored under the local import name), and the `Anonymous_N` aliases.
//! - Names are unique within the file: `_2`, `_3`, ... on a collision with another alias, a Luau
//!   builtin or type-function name, a Roblox extern type, or any type-parameter name used in the
//!   file.

use std::collections::{HashMap, HashSet};

use rotor_luau::{NodeData, TypeAlias, TypeFunctionDeclaration, TypeGeneric, TypeReference, is_valid_identifier};
use rotor_tsrs::ast::{self as tsast, ModifierFlags, NodeFlags, SymbolFlags};

use super::luautypes::{AliasPlacement, Builder, EntryId, FidelityLossKind, SymKey, TypeContext, TypePosition, types};
use super::luautypes_builder::is_roblox_symbol;
use super::nativetypes_alias::NATIVE_RESERVED_TYPE_ALIAS_NAMES;
use crate::prelude::*;

/// `luauTypeFunctionNames`: the built-in type functions and the `typeof` spelling.
pub const LUAU_TYPE_FUNCTION_NAMES: &[&str] = &[
	"typeof",
	"keyof",
	"index",
	"rawkeyof",
	"rawget",
	"setmetatable",
	"getmetatable",
	"len",
	"unm",
	"add",
	"sub",
	"mul",
	"div",
	"idiv",
	"pow",
	"mod",
	"concat",
	"lt",
	"le",
];

/// `aliasCommentLossKinds`: the declaration-level losses that earn a comment.
const ALIAS_COMMENT_LOSS_KINDS: &[FidelityLossKind] = &[
	FidelityLossKind::MappedType,
	FidelityLossKind::ConditionalType,
	FidelityLossKind::InferType,
	FidelityLossKind::KeyRemap,
	FidelityLossKind::RecursiveAliasArgs,
];

// -----------------------------------------------------------------------------------------
// Emission
// -----------------------------------------------------------------------------------------

/// `luauDeclarationStatements`: the `type` statement to emit at a top-level interface, type
/// alias or enum declaration of this file, or an empty list.
pub fn luau_declaration_statements(state: &mut State<'_>, node: P<Node>) -> Statements {
	let statements = state.ast.new_list();
	if !state.is_luau_typed {
		return statements;
	}
	let Some(name) = node.name() else {
		return statements;
	};
	let Some(symbol) = state.ck().get_symbol_at_location(name) else {
		return statements;
	};
	if luau_first_declaration(state, symbol) != Some(node) {
		return statements;
	}
	let mut builder = types(state);
	let Some(entry) = builder.alias_for(Some(SymKey::Real(symbol))) else {
		return statements;
	};
	let data = builder.b().entry(entry);
	if data.placement != AliasPlacement::AtDeclaration || data.placed {
		return statements;
	}
	builder.b().entry_mut(entry).placed = true;
	let list = entry_statements(&mut builder, entry);
	state.ast.list_push_list(statements, list);
	statements
}

/// `luauHoistedTypeStatements`: the hoisted alias statements, in registration order.
pub fn luau_hoisted_type_statements(state: &mut State<'_>) -> Statements {
	let statements = state.ast.new_list();
	if state.slices.luau_types.shared.is_none() {
		return statements;
	}
	let mut builder = types(state);
	for entry in builder.aliases() {
		if builder.b().entry(entry).placed {
			continue;
		}
		builder.b().entry_mut(entry).placed = true;
		let list = entry_statements(&mut builder, entry);
		builder.ast().list_push_list(statements, list);
	}
	statements
}

/// `typeAliasEntry.statements`: the alias's loss comments, then the type function or the alias
/// statement carrying the export marker.
pub fn entry_statements(builder: &mut Builder<'_, '_>, entry: EntryId) -> Statements {
	let statements = builder.ast().new_list();
	for text in loss_comments(builder, entry) {
		let comment = builder.ast().comment(text);
		builder.ast().list_push(statements, comment);
	}
	let (type_function, statement, exported) = {
		let data = builder.b().entry(entry);
		(data.type_function, data.statement, data.exported)
	};
	if let Some(type_function) = type_function {
		if let Some(payload) = builder.ast().get_mut::<TypeFunctionDeclaration>(type_function) {
			payload.export = exported;
		}
		builder.ast().list_push(statements, type_function);
	} else if let Some(statement) = statement {
		if let Some(payload) = builder.ast().get_mut::<TypeAlias>(statement) {
			payload.export = exported;
		}
		builder.ast().list_push(statements, statement);
	}
	statements
}

/// `lossComments`: the declaration-level losses of the alias body as comment texts, one per
/// (replacement, reason) pair with the lost TS types listed once each, in recording order.
pub fn loss_comments(builder: &mut Builder<'_, '_>, entry: EntryId) -> Vec<String> {
	struct Group {
		luau: String,
		reason: String,
		ts_texts: Vec<String>,
	}
	let mut groups: Vec<Group> = Vec::new();
	for loss in &builder.b().entry(entry).losses {
		if !ALIAS_COMMENT_LOSS_KINDS.contains(&loss.kind) {
			continue;
		}
		let found = groups
			.iter()
			.position(|group| group.luau == loss.luau && group.reason == loss.reason);
		let index = found.unwrap_or_else(|| {
			groups.push(Group {
				luau: loss.luau.clone(),
				reason: loss.reason.clone(),
				ts_texts: Vec::new(),
			});
			groups.len() - 1
		});
		let group = &mut groups[index];
		if !group.ts_texts.contains(&loss.ts_text) {
			group.ts_texts.push(loss.ts_text.clone());
		}
	}
	groups
		.iter()
		.map(|group| {
			format!(
				" luauTypes: {} spelled as {}: {}",
				group.ts_texts.join(", "),
				group.luau,
				group.reason
			)
		})
		.collect()
}

/// `luauFirstDeclaration`: the first interface, type alias, enum or class declaration of
/// `symbol` in this file.
pub fn luau_first_declaration(state: &State<'_>, symbol: P<Symbol>) -> Option<P<Node>> {
	symbol.declarations().iter().copied().find(|&declaration| {
		is_alias_declaration(declaration) && tsast::get_source_file_of_node(declaration) == Some(state.source_file)
	})
}

/// `aliasDeclarationOf`: the first interface, type alias, enum or class declaration of
/// `symbol`.
pub fn alias_declaration_of_symbol(symbol: P<Symbol>) -> Option<P<Node>> {
	symbol
		.declarations()
		.iter()
		.copied()
		.find(|&declaration| is_alias_declaration(declaration))
}

/// `isAliasDeclaration`: a declaration that can own a `type` statement.
pub fn is_alias_declaration(node: P<Node>) -> bool {
	tsast::is_interface_declaration(node)
		|| tsast::is_type_alias_declaration(node)
		|| tsast::is_enum_declaration(node)
		|| tsast::is_class_like(node)
}

// -----------------------------------------------------------------------------------------
// Placement and exports
// -----------------------------------------------------------------------------------------

/// `aliasPlacementFor`: at the declaration for a top-level, non-ambient alias declaration of
/// this file; hoisted otherwise, class expressions included.
pub fn alias_placement_for(state: &State<'_>, declaration: P<Node>) -> AliasPlacement {
	if tsast::get_source_file_of_node(declaration) != Some(state.source_file)
		|| !super::luautypes_builder::is_top_level_declaration(declaration)
		|| tsast::has_syntactic_modifier(declaration, ModifierFlags::Ambient)
		|| !is_alias_declaration(declaration)
		|| tsast::is_class_expression(declaration)
	{
		return AliasPlacement::Hoisted;
	}
	AliasPlacement::AtDeclaration
}

/// `isLuauExportedType`: whether a type declared in this file is part of the module's type
/// surface.
pub fn is_luau_exported_type(state: &mut State<'_>, symbol: P<Symbol>) -> bool {
	for &declaration in symbol.declarations() {
		if !is_alias_declaration(declaration) || tsast::get_source_file_of_node(declaration) != Some(state.source_file)
		{
			continue;
		}
		if exported_through_containers(declaration) {
			return true;
		}
	}
	let merged = state.ck().get_merged_symbol(symbol);
	for &statement in state.source_file.statements().nodes() {
		if !tsast::is_export_declaration(statement) || statement.module_specifier().is_some() {
			continue;
		}
		let Some(clause) = statement.as_export_declaration().export_clause else {
			continue;
		};
		if !tsast::is_named_exports(clause) {
			continue;
		}
		for &specifier in clause.as_named_exports().elements.nodes() {
			let Some(target) = state.ck().get_export_specifier_local_target_symbol(specifier) else {
				continue;
			};
			if target == symbol || state.ck().get_merged_symbol(target) == merged {
				return true;
			}
		}
	}
	false
}

/// `exportedThroughContainers`: `declaration` is exported from every container up to the
/// source file.
fn exported_through_containers(declaration: P<Node>) -> bool {
	let mut node = declaration;
	loop {
		let Some(parent) = node.parent() else {
			return false;
		};
		let container = if tsast::is_module_block(parent) {
			let Some(outer) = parent.parent() else {
				return false;
			};
			outer
		} else {
			parent
		};
		let exported = tsast::has_syntactic_modifier(node, ModifierFlags::Export)
			|| container.flags().intersects(NodeFlags::ExportContext);
		if !exported {
			return false;
		}
		if tsast::is_source_file(parent) {
			return true;
		}
		if !tsast::is_module_block(parent) || !tsast::is_module_declaration(container) {
			return false;
		}
		node = container;
	}
}

// -----------------------------------------------------------------------------------------
// Naming
// -----------------------------------------------------------------------------------------

/// `luauAliasBaseName`: the name an alias wants before uniqueness.
pub fn luau_alias_base_name(state: &mut State<'_>, symbol: P<Symbol>) -> String {
	let mut name = symbol.name().to_owned();
	if name == tsast::INTERNAL_SYMBOL_NAME_CLASS {
		name = super::luautypes_class::class_expression_alias_name(state, symbol);
	}
	if name == tsast::INTERNAL_SYMBOL_NAME_DEFAULT {
		"Default".clone_into(&mut name);
		for &declaration in symbol.declarations() {
			if let Some(declaration_name) = declaration.name()
				&& tsast::is_identifier(declaration_name)
			{
				declaration_name.text().clone_into(&mut name);
				break;
			}
		}
	}
	if let Some(declaration) = luau_first_declaration(state, symbol) {
		let mut names = enclosing_declaration_names(declaration);
		names.push(name);
		name = names.join("_");
	} else if let Some(local) = types(state).imported_type_name(symbol) {
		name = local;
	} else {
		let mut names = namespace_chain_names(symbol);
		names.push(name);
		name = names.join("_");
	}
	if !is_valid_identifier(&name) {
		return "Alias".to_owned();
	}
	name
}

/// `enclosingDeclarationNames`: the names of the namespaces, functions and classes that
/// enclose `declaration`, outermost first.
fn enclosing_declaration_names(declaration: P<Node>) -> Vec<String> {
	let mut names = Vec::new();
	let mut parent = declaration.parent();
	while let Some(current) = parent {
		parent = current.parent();
		if !tsast::is_module_declaration(current) && !tsast::is_function_like(current) && !tsast::is_class_like(current)
		{
			continue;
		}
		let Some(name) = current.name() else {
			continue;
		};
		if !tsast::is_identifier(name) {
			continue;
		}
		names.insert(0, name.text().to_owned());
	}
	names
}

/// `namespaceChainNames`: the non-module namespaces a foreign symbol is declared under,
/// outermost first.
fn namespace_chain_names(symbol: P<Symbol>) -> Vec<String> {
	let mut names = Vec::new();
	let mut parent = symbol.parent();
	while let Some(current) = parent {
		let flags = current.flags();
		if !flags.intersects(SymbolFlags::Module)
			|| current.is_external_module()
			|| (flags.intersects(SymbolFlags::ValueModule) && is_source_file_symbol(current))
		{
			break;
		}
		names.insert(0, current.name().to_owned());
		parent = current.parent();
	}
	names
}

/// `isSourceFileSymbol`: the module symbol of a source file.
fn is_source_file_symbol(symbol: P<Symbol>) -> bool {
	symbol
		.declarations()
		.iter()
		.any(|&declaration| tsast::is_source_file(declaration))
}

/// `uniqueLuauTypeName` (Go keeps it for its tests only).
#[cfg(test)]
pub fn unique_luau_type_name(state: &mut State<'_>, base: &str) -> String {
	types(state).unique_name(base)
}

impl Builder<'_, '_> {
	/// `uniqueName`: a free alias name from `base`: `base` itself, else `base_2`, `base_3`, ...
	pub fn unique_name(&mut self, base: &str) -> String {
		let name = if is_valid_identifier(base) { base } else { "Alias" };
		let mut suffix = 1_u32;
		loop {
			let candidate = if suffix > 1 {
				format!("{name}_{suffix}")
			} else {
				name.to_owned()
			};
			if is_valid_identifier(&candidate) && !self.type_name_taken(&candidate) {
				self.b().alias_names.insert(candidate.clone());
				return candidate;
			}
			suffix += 1;
		}
	}

	/// `typeNameTaken`: `candidate` cannot name a new alias.
	fn type_name_taken(&mut self, candidate: &str) -> bool {
		if self.b().alias_names.contains(candidate) {
			return true;
		}
		if NATIVE_RESERVED_TYPE_ALIAS_NAMES.contains(&candidate) || LUAU_TYPE_FUNCTION_NAMES.contains(&candidate) {
			return true;
		}
		if self.type_parameter_names().contains(candidate) {
			return true;
		}
		self.s
			.ck()
			.get_global_symbol(candidate, SymbolFlags::Type, None)
			.is_some_and(is_roblox_symbol)
	}

	/// `typeParameterNames`: every type-parameter name declared anywhere in this file, collected
	/// once.
	fn type_parameter_names(&mut self) -> &HashSet<String> {
		if self.b().reserved_type_parameters.is_none() {
			let mut names = HashSet::new();
			let root = self.s.source_file.as_node();
			collect_type_parameter_names(root, &mut names);
			self.b().reserved_type_parameters = Some(names);
		}
		self.b().reserved_type_parameters.get_or_insert_with(HashSet::new)
	}

	/// `importedTypeName`: the local name this file imports `symbol` under.
	pub fn imported_type_name(&mut self, symbol: P<Symbol>) -> Option<String> {
		if self.b().imported_names.is_none() {
			let (names, namespaces) = self.scan_imports();
			self.b().imported_names = Some(names);
			self.b().imported_namespaces = namespaces;
		}
		if let Some(name) = self.b().imported_names.as_ref().and_then(|names| names.get(&symbol)) {
			return Some(name.clone());
		}
		let merged = self.s.ck().get_merged_symbol(symbol);
		if let Some(name) = self.b().imported_names.as_ref().and_then(|names| names.get(&merged)) {
			return Some(name.clone());
		}
		if let Some(parent) = symbol.parent() {
			let merged_parent = self.s.ck().get_merged_symbol(parent);
			if let Some(prefix) = self.b().imported_namespaces.get(&merged_parent) {
				return Some(format!("{prefix}_{}", symbol.name()));
			}
		}
		None
	}

	/// `scanImports`: every symbol this file imports to its local name, and every namespace
	/// import's module to its local name.
	fn scan_imports(&mut self) -> (HashMap<P<Symbol>, String>, HashMap<P<Symbol>, String>) {
		let mut names = HashMap::new();
		let mut namespaces = HashMap::new();
		let statements = self.s.source_file.statements().nodes();
		for &statement in statements {
			match statement.kind() {
				Kind::ImportEqualsDeclaration => self.bind_import(statement.name(), &mut names),
				Kind::ImportDeclaration => {
					let Some(clause) = statement.as_import_declaration().import_clause else {
						continue;
					};
					self.bind_import(clause.name(), &mut names);
					let Some(bindings) = clause.as_import_clause().named_bindings else {
						continue;
					};
					if tsast::is_namespace_import(bindings) {
						self.bind_import(bindings.name(), &mut namespaces);
					} else if tsast::is_named_imports(bindings) {
						for &specifier in bindings.as_named_imports().elements.nodes() {
							self.bind_import(specifier.name(), &mut names);
						}
					}
				}
				_ => {}
			}
		}
		(names, namespaces)
	}

	fn bind_import(&mut self, local: Option<P<Node>>, into: &mut HashMap<P<Symbol>, String>) {
		let Some(local) = local.filter(|&local| tsast::is_identifier(local)) else {
			return;
		};
		let Some(alias) = self.s.ck().get_symbol_at_location(local) else {
			return;
		};
		if !alias.flags().intersects(SymbolFlags::Alias) {
			return;
		}
		let target = self.s.ck().get_aliased_symbol(alias);
		if target == alias {
			return;
		}
		into.entry(target).or_insert_with(|| local.text().to_owned());
	}

	// -------------------------------------------------------------------------------------
	// Generics with defaults
	// -------------------------------------------------------------------------------------

	/// `aliasGenerics`: an alias's declared generics completed with their TS defaults, built in
	/// the alias's own scope.
	pub fn alias_generics(&mut self, entry: EntryId, declaration: P<Node>) -> Vec<TypeGeneric> {
		let (mut generics, generic_indexes, params) = {
			let data = self.b().entry(entry);
			(data.generics.clone(), data.generic_indexes.clone(), data.params.clone())
		};
		for (index, &parameter_index) in generic_indexes.iter().enumerate() {
			let Some(parameter) = params[parameter_index] else {
				continue;
			};
			let Some(default_type) = self.s.ck().get_default_from_type_parameter(parameter) else {
				continue;
			};
			let node = self
				.build(default_type, TypeContext::at(TypePosition::Nested, Some(declaration)))
				.node;
			let later: HashSet<String> = generics[index..].iter().map(|generic| generic.name.clone()).collect();
			if type_node_references_name(&self.s.ast, node, &later) {
				continue;
			}
			generics[index].default = Some(node);
		}
		drop_leading_defaults(generics)
	}
}

fn collect_type_parameter_names(node: P<Node>, names: &mut HashSet<String>) {
	node.for_each_child(&mut |child| {
		if child.kind() == Kind::TypeParameter
			&& let Some(name) = child.name()
		{
			names.insert(name.text().to_owned());
		}
		collect_type_parameter_names(child, names);
		false
	});
}

/// `dropLeadingDefaults`: clears the default of every generic before the last generic without
/// one, so defaults are trailing as Luau requires.
pub fn drop_leading_defaults(mut generics: Vec<TypeGeneric>) -> Vec<TypeGeneric> {
	let last = generics
		.iter()
		.rposition(|generic| generic.default.is_none())
		.unwrap_or(0);
	for generic in &mut generics[..last] {
		generic.default = None;
	}
	generics
}

/// `typeNodeReferencesName`: `node` contains a bare reference to one of `names`.
pub fn type_node_references_name(ast: &Ast, node: NodeId, names: &HashSet<String>) -> bool {
	let mut found = false;
	walk_type_node(ast, Some(node), &mut |child| {
		if let Some(reference) = ast.get::<TypeReference>(child)
			&& reference.module.is_empty()
			&& names.contains(&reference.name)
		{
			found = true;
		}
	});
	found
}

/// `walkTypeNode`: calls `visit` on `node` and every type node under it.
pub fn walk_type_node(ast: &Ast, node: Option<NodeId>, visit: &mut dyn FnMut(NodeId)) {
	let Some(node) = node else {
		return;
	};
	visit(node);
	let walk_list = |list: Option<List<element::TypeNode>>, visit: &mut dyn FnMut(NodeId)| {
		if let Some(list) = list {
			for child in ast.list_to_vec(list) {
				walk_type_node(ast, Some(child), visit);
			}
		}
	};
	match ast.data(node) {
		NodeData::TypeReference(reference) => walk_list(reference.arguments, visit),
		NodeData::TypeOptional(optional) => walk_type_node(ast, optional.inner, visit),
		NodeData::TypeUnion(union) => walk_list(Some(union.members), visit),
		NodeData::TypeIntersection(intersection) => walk_list(Some(intersection.members), visit),
		NodeData::TypeTable(table) => {
			for field in ast.list_to_vec(table.fields) {
				match ast.data(field) {
					NodeData::TypeTableProp(prop) => walk_type_node(ast, Some(prop.value), visit),
					NodeData::TypeTableIndexer(indexer) => {
						walk_type_node(ast, Some(indexer.key), visit);
						walk_type_node(ast, Some(indexer.value), visit);
					}
					_ => {}
				}
			}
		}
		NodeData::TypeArray(array) => walk_type_node(ast, Some(array.element), visit),
		NodeData::TypeFunction(function) => {
			for generic in &function.generics {
				walk_type_node(ast, generic.default, visit);
			}
			for parameter in &function.parameters {
				walk_type_node(ast, Some(parameter.ty), visit);
			}
			walk_type_node(ast, function.var_args, visit);
			walk_type_node(ast, function.ret, visit);
		}
		NodeData::TypePack(pack) => walk_list(Some(pack.elements), visit),
		NodeData::TypeVariadic(variadic) => walk_type_node(ast, Some(variadic.inner), visit),
		_ => {}
	}
}
