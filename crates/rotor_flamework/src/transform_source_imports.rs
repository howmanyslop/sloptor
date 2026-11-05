//! `transform_source_imports.go`: merging generated named imports into one per module.

use std::collections::{HashMap, HashSet};

use rotor_tsrs::P;
use rotor_tsrs::ast::{Node, NodeFactory, SourceFile, is_import_declaration, is_named_imports, is_string_literal};

use crate::transform_imports::named_import_key;

#[expect(
	clippy::too_many_lines,
	reason = "Go's deduplicateSourceImports, statement for statement"
)]
/// `deduplicateSourceImports`: generated named imports merge into the first import of their
/// module and phase; specifiers an authored import already supplies, and repeats, are dropped.
pub(crate) fn deduplicate_source_imports(
	factory: &NodeFactory,
	original: P<SourceFile>,
	source_file: P<SourceFile>,
) -> P<SourceFile> {
	let mut authored_counts: HashMap<String, usize> = HashMap::new();
	for &statement in original.statements().nodes() {
		let key = source_named_import_key(statement);
		if !key.is_empty() {
			*authored_counts.entry(key).or_insert(0) += 1;
		}
	}
	let current = source_file.statements().nodes();
	let mut authored: HashSet<P<Node>> = HashSet::with_capacity(authored_counts.len());
	for &statement in current.iter().rev() {
		let key = source_named_import_key(statement);
		if key.is_empty() {
			continue;
		}
		let Some(count) = authored_counts.get_mut(&key).filter(|count| **count > 0) else {
			continue;
		};
		*count -= 1;
		authored.insert(statement);
	}
	let mut authored_bindings: HashMap<String, HashSet<String>> = HashMap::new();
	for statement in &authored {
		let group_key = source_named_import_group_key(*statement);
		if group_key.is_empty() {
			continue;
		}
		let bindings = authored_bindings.entry(group_key).or_default();
		let Some(clause) = statement.as_import_declaration().import_clause() else {
			continue;
		};
		if let Some(named) = clause.as_import_clause().named_bindings() {
			for element in named.elements() {
				bindings.insert(source_import_specifier_key(*element));
			}
		}
	}

	let mut positions: HashMap<String, usize> = HashMap::new();
	let mut bases: HashMap<String, P<Node>> = HashMap::new();
	let mut elements: HashMap<String, Vec<P<Node>>> = HashMap::new();
	let mut seen: HashSet<String> = HashSet::new();
	let mut statements: Vec<P<Node>> = Vec::with_capacity(current.len());
	let mut changed = false;
	for &statement in current {
		if authored.contains(&statement) || !is_import_declaration(statement) {
			statements.push(statement);
			continue;
		}
		let declaration = statement.as_import_declaration();
		let Some(clause_node) = declaration
			.import_clause()
			.filter(|_| is_string_literal(declaration.module_specifier()))
		else {
			statements.push(statement);
			continue;
		};
		let clause = clause_node.as_import_clause();
		let Some(named) = clause
			.named_bindings()
			.filter(|bindings| is_named_imports(*bindings) && clause_node.name().is_none())
		else {
			statements.push(statement);
			continue;
		};
		let key = source_named_import_group_key(statement);
		let existing = positions.get(&key).copied();
		changed |= existing.is_some();
		let position = existing.unwrap_or_else(|| {
			let position = statements.len();
			positions.insert(key.clone(), position);
			bases.insert(key.clone(), statement);
			statements.push(statement);
			position
		});
		for &element in named.elements() {
			let specifier_key = source_import_specifier_key(element);
			if authored_bindings
				.get(&key)
				.is_some_and(|bindings| bindings.contains(&specifier_key))
			{
				changed = true;
				continue;
			}
			let name = element.name().map_or("", |name| name.text());
			let exported = element.property_name().map_or(name, |property| property.text());
			let element_key = format!("{key}|{}|{exported}|{name}", element.is_type_only());
			if !seen.insert(element_key) {
				changed = true;
				continue;
			}
			let mut element = element;
			if element.property_name().is_none() {
				changed = true;
				element = factory.new_import_specifier(
					element.is_type_only(),
					Some(factory.new_identifier(exported)),
					factory.new_identifier(name),
				);
			}
			elements.entry(key.clone()).or_default().push(element);
		}
		let Some(&base) = bases.get(&key) else {
			continue;
		};
		let base_declaration = base.as_import_declaration();
		let Some(base_clause_node) = base_declaration.import_clause() else {
			continue;
		};
		let base_clause = base_clause_node.as_import_clause();
		let Some(base_named) = base_clause.named_bindings() else {
			continue;
		};
		let named = factory.update_named_imports(
			base_named,
			factory.new_node_list(elements.get(&key).cloned().unwrap_or_default()),
		);
		let updated_clause = factory.update_import_clause(
			base_clause_node,
			base_clause.phase_modifier(),
			base_clause_node.name(),
			Some(named),
		);
		statements[position] = factory.update_import_declaration(
			base,
			base.modifiers(),
			Some(updated_clause),
			base_declaration.module_specifier(),
			base_declaration.attributes(),
		);
	}
	if !changed {
		return source_file;
	}
	factory
		.update_source_file(
			source_file.as_node(),
			factory.new_node_list(statements),
			source_file.end_of_file_token,
		)
		.as_source_file_p()
}

/// Go's `%d` of an `ast.Kind`.
const fn kind_number(kind: rotor_tsrs::ast::Kind) -> i32 {
	kind as i32
}

fn source_named_import_key(node: P<Node>) -> String {
	if !is_import_declaration(node) {
		return String::new();
	}
	let Some(clause_node) = node.as_import_declaration().import_clause() else {
		return String::new();
	};
	let clause = clause_node.as_import_clause();
	if clause_node.name().is_some() || !clause.named_bindings().is_some_and(is_named_imports) {
		return String::new();
	}
	format!("{}|{}", kind_number(clause.phase_modifier()), named_import_key(node))
}

fn source_named_import_group_key(node: P<Node>) -> String {
	if source_named_import_key(node).is_empty() {
		return String::new();
	}
	let declaration = node.as_import_declaration();
	let phase = declaration
		.import_clause
		.map_or(0, |clause| kind_number(clause.as_import_clause().phase_modifier()));
	format!("{}|{phase}", declaration.module_specifier().text())
}

fn source_import_specifier_key(node: P<Node>) -> String {
	let name = node.name().map_or("", |name| name.text());
	let exported = node.property_name().map_or(name, |property| property.text());
	format!("{}|{exported}|{name}", node.is_type_only())
}
