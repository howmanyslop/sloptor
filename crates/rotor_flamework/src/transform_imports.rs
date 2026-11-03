//! `transform_imports.go`: the `Reflect` import and import deduplication.

use std::collections::HashSet;

use rotor_tsrs::P;
use rotor_tsrs::ast::{
	Kind, Node, NodeFactory, SourceFile, TokenFlags, get_source_file_of_node, is_import_declaration, is_named_imports,
	is_string_literal,
};

pub(crate) const FLAMEWORK_CORE_MODULE: &str = "@flamework/core";

fn new_reflect_import(factory: &NodeFactory) -> P<Node> {
	let reflect = factory.new_identifier("Reflect");
	let specifier = factory.new_import_specifier(false, None, reflect);
	let named_imports = factory.new_named_imports(factory.new_node_list(vec![specifier]));
	let clause = factory.new_import_clause(Kind::Unknown, None, Some(named_imports));
	factory.new_import_declaration(
		None,
		Some(clause),
		factory.new_string_literal(FLAMEWORK_CORE_MODULE, TokenFlags::None),
		None,
	)
}

/// `prependFlameworkReflectImport`: the `Reflect` import (unless the file has one), then the
/// generated imports, then the authored ones, deduplicated, ahead of the other statements.
pub(crate) fn prepend_flamework_reflect_import(factory: &NodeFactory, source_file: P<SourceFile>) -> P<SourceFile> {
	let all = source_file.statements().nodes();
	let mut generated_imports = Vec::new();
	let mut source_imports = Vec::new();
	let mut statements = Vec::with_capacity(all.len());
	for &statement in all {
		if !is_import_declaration(statement) {
			statements.push(statement);
			continue;
		}
		if statement.pos().is_synthetic() {
			generated_imports.push(statement);
		} else {
			source_imports.push(statement);
		}
	}
	let mut imports = Vec::with_capacity(generated_imports.len() + source_imports.len() + 1);
	if flamework_reflect_import(Some(source_file)).is_none() {
		imports.push(new_reflect_import(factory));
	}
	imports.extend(generated_imports);
	imports.extend(source_imports);
	let mut imports = deduplicate_named_imports(imports);
	let first_original = all.iter().copied().find(|statement| !statement.pos().is_synthetic());
	if let (Some(&first_import), Some(first)) = (imports.first(), first_original)
		&& first_import != first
	{
		first_import.set_loc(first.loc());
		if is_import_declaration(first) {
			if let Some(slot) = imports.iter_mut().find(|import| **import == first) {
				*slot = factory.deep_clone_node(Some(first)).unwrap_or(first);
			}
		} else if let Some(slot) = statements.iter_mut().find(|statement| **statement == first) {
			*slot = factory.deep_clone_node(Some(first)).unwrap_or(first);
		}
	}
	imports.extend(statements);
	factory
		.update_source_file(
			source_file.as_node(),
			factory.new_node_list(imports),
			source_file.end_of_file_token,
		)
		.as_source_file_p()
}

fn deduplicate_named_imports(imports: Vec<P<Node>>) -> Vec<P<Node>> {
	let mut seen = HashSet::with_capacity(imports.len());
	imports
		.into_iter()
		.filter(|import| {
			let key = named_import_key(*import);
			key.is_empty() || seen.insert(key)
		})
		.collect()
}

/// `namedImportKey`: the module and each specifier of a plain named import, or `""`.
pub(crate) fn named_import_key(node: P<Node>) -> String {
	let declaration = node.as_import_declaration();
	let Some(clause_node) = declaration.import_clause() else {
		return String::new();
	};
	if !is_string_literal(declaration.module_specifier()) {
		return String::new();
	}
	let clause = clause_node.as_import_clause();
	let Some(named) = clause.named_bindings().filter(|bindings| is_named_imports(*bindings)) else {
		return String::new();
	};
	if clause_node.name().is_some() {
		return String::new();
	}
	let mut key = declaration.module_specifier().text().to_owned();
	for specifier_node in named.elements() {
		key.push('|');
		if specifier_node.is_type_only() {
			key.push_str("type:");
		}
		let name = specifier_node.name().map_or("", |name| name.text());
		key.push_str(specifier_node.property_name().map_or(name, |property| property.text()));
		key.push(':');
		key.push_str(name);
	}
	key
}

/// `reflectIdentifierForNode`: the local name of the file's `Reflect` import, or `Reflect`.
pub(crate) fn reflect_identifier_for_node(node: Option<P<Node>>) -> String {
	node.and_then(|node| flamework_reflect_import(get_source_file_of_node(node)))
		.unwrap_or_else(|| "Reflect".to_owned())
}

/// `flameworkReflectImport`: the local name of a value import of `Reflect` from
/// `@flamework/core`.
pub(crate) fn flamework_reflect_import(source_file: Option<P<SourceFile>>) -> Option<String> {
	for &statement in source_file?.statements().nodes() {
		if !is_import_declaration(statement) {
			continue;
		}
		let declaration = statement.as_import_declaration();
		if !is_string_literal(declaration.module_specifier())
			|| declaration.module_specifier().text() != FLAMEWORK_CORE_MODULE
		{
			continue;
		}
		let Some(clause_node) = declaration.import_clause() else {
			continue;
		};
		let clause = clause_node.as_import_clause();
		if clause.phase_modifier() == Kind::TypeKeyword {
			continue;
		}
		let Some(named) = clause.named_bindings().filter(|bindings| is_named_imports(*bindings)) else {
			continue;
		};
		for specifier in named.elements() {
			if specifier.is_type_only() {
				continue;
			}
			let name = specifier.name().map_or("", |name| name.text());
			let exported = specifier.property_name().map_or(name, |property| property.text());
			if exported == "Reflect" {
				return Some(name.to_owned());
			}
		}
	}
	None
}
