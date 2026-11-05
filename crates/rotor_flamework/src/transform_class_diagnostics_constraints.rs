//! `transform_class_diagnostics_constraints.go`: node metadata with `{@link constraint}`
//! types, and the diagnostics for classes that do not satisfy them.

use std::collections::HashSet;

use rotor_tsrs::P;
use rotor_tsrs::ast::{
	Node, SymbolFlags, get_implements_heritage_clause_elements, get_source_file_of_node, is_call_expression,
	is_class_declaration, is_class_element, is_class_expression, is_jsdoc_link, is_jsdoc_link_code,
	is_jsdoc_link_plain, is_source_file,
};
use rotor_tsrs::checker::Type;
use rotor_tsrs::diagnostics as messages;

use crate::transform_analysis_discovery::DiscoveredClassState;
use crate::transform_diagnostics::{Diagnostic, diagnostic_file_name, new_message_diagnostic, text_pos};
use crate::transform_macro_types::jsdoc_tags;
use crate::transform_reflection_metadata::heritage_entity;
use crate::transform_state::TransformState;
use crate::transform_symbols::declaration_from_node;

/// `linkedConstraint`
pub(crate) struct LinkedConstraint {
	target: P<Type>,
	trace: P<Node>,
}

/// `classNodeMetadata`
#[derive(Default)]
pub(crate) struct ClassNodeMetadata {
	flags: HashSet<String>,
	constraints: Vec<LinkedConstraint>,
}

impl ClassNodeMetadata {
	pub(crate) fn requested(&self, name: &str) -> bool {
		if self.flags.contains(&format!("~{name}")) {
			return false;
		}
		self.flags.contains(name) || self.flags.contains("*")
	}
}

/// `collectNodeMetadata`: the node's metadata, its decorators' and its implemented types'.
pub(crate) fn collect_node_metadata(state: &mut TransformState<'_>, node: P<Node>) -> ClassNodeMetadata {
	let mut metadata = ClassNodeMetadata::default();
	let mut visited = HashSet::new();
	collect_node_metadata_into(state, Some(node), &mut metadata, &mut visited);
	metadata
}

fn collect_node_metadata_into(
	state: &mut TransformState<'_>,
	node: Option<P<Node>>,
	metadata: &mut ClassNodeMetadata,
	visited: &mut HashSet<P<Node>>,
) {
	let Some(node) = node else {
		return;
	};
	if !visited.insert(node) {
		return;
	}
	parse_jsdoc_metadata(state, node, metadata);
	for decorator in node.decorators() {
		let Some(mut expression) = decorator.expression() else {
			continue;
		};
		if is_call_expression(expression)
			&& let Some(callee) = expression.expression()
		{
			expression = callee;
		}
		let declaration = declaration_from_node(state, expression);
		collect_node_metadata_into(state, declaration, metadata, visited);
	}
	if is_class_declaration(node) || is_class_expression(node) {
		for &implemented in get_implements_heritage_clause_elements(node) {
			let declaration = heritage_entity(implemented).and_then(|entity| declaration_from_node(state, entity));
			collect_node_metadata_into(state, declaration, metadata, visited);
		}
	}
	if is_class_element(node)
		&& let (Some(name), Some(parent)) = (node.name(), node.parent())
		&& (is_class_declaration(parent) || is_class_expression(parent))
	{
		for &implemented in get_implements_heritage_clause_elements(parent) {
			let Some(entity) = heritage_entity(implemented) else {
				continue;
			};
			let Some(mut symbol) = state.ck().get_symbol_at_location(entity) else {
				continue;
			};
			if symbol.flags().intersects(SymbolFlags::Alias) {
				symbol = state.ck().get_aliased_symbol(symbol);
			}
			let Some(member) = symbol
				.members()
				.and_then(|members| rotor_tsrs::ast::SymbolTable::get(&members, name.text()))
			else {
				continue;
			};
			for &declaration in member.declarations() {
				collect_node_metadata_into(state, Some(declaration), metadata, visited);
			}
		}
	}
}

fn parse_jsdoc_metadata(state: &mut TransformState<'_>, node: P<Node>, metadata: &mut ClassNodeMetadata) {
	let file = get_source_file_of_node(node)
		.map(|file| -> &'static rotor_tsrs::ast::SourceFile { file.as_node().as_source_file() });
	let mut current = Some(node);
	while let Some(node) = current {
		if is_source_file(node) {
			break;
		}
		for jsdoc in node.jsdoc(file) {
			for tag in jsdoc_tags(*jsdoc) {
				if tag.tag_name().text() != "metadata" {
					continue;
				}
				for &comment in tag.comments() {
					if is_jsdoc_link(comment) || is_jsdoc_link_code(comment) || is_jsdoc_link_plain(comment) {
						parse_linked_metadata(state, comment, metadata);
						continue;
					}
					for token in comment.text().split_whitespace() {
						metadata.flags.insert(token.to_owned());
					}
				}
			}
		}
		current = node.parent();
	}
}

fn parse_linked_metadata(state: &mut TransformState<'_>, link: P<Node>, metadata: &mut ClassNodeMetadata) {
	let Some(name) = link.name() else {
		return;
	};
	if link.text().trim() != "constraint" {
		return;
	}
	let Some(mut symbol) = state.ck().get_symbol_at_location(name) else {
		return;
	};
	if symbol.flags().intersects(SymbolFlags::Alias) {
		symbol = state.ck().get_aliased_symbol(symbol);
	}
	let target = if symbol
		.flags()
		.intersects(SymbolFlags::TypeAlias | SymbolFlags::Interface | SymbolFlags::Class)
	{
		state.ck().get_declared_type_of_symbol(symbol)
	} else {
		state.ck().get_type_at_location(name)
	};
	metadata.constraints.push(LinkedConstraint { target, trace: link });
}

/// `constraintDiagnostics`: TS2344 for each linked constraint the class does not satisfy.
pub(crate) fn constraint_diagnostics(state: &mut TransformState<'_>, class: &DiscoveredClassState) -> Vec<Diagnostic> {
	let metadata = collect_node_metadata(state, class.node);
	let Some(name) = class.node.name() else {
		return Vec::new();
	};
	if metadata.constraints.is_empty() {
		return Vec::new();
	}
	let Some(symbol) = state.ck().get_symbol_at_location(name) else {
		return Vec::new();
	};
	let source = state.ck().get_declared_type_of_symbol(symbol);
	let mut result = Vec::with_capacity(metadata.constraints.len());
	for constraint in &metadata.constraints {
		if state.ck().is_type_assignable_to(source, constraint.target) {
			continue;
		}
		let source_text = state.ck().type_to_string(source);
		let target_text = state.ck().type_to_string(constraint.target);
		let mut diagnostic = new_message_diagnostic(
			get_source_file_of_node(class.node),
			text_pos(name.pos()),
			text_pos(name.end()),
			&messages::TYPE_0_DOES_NOT_SATISFY_THE_CONSTRAINT_1,
			&[&source_text, &target_text],
		);
		diagnostic.add_related_info(new_message_diagnostic(
			get_source_file_of_node(constraint.trace),
			text_pos(constraint.trace.pos()),
			text_pos(constraint.trace.end()),
			&messages::X_0_IS_DECLARED_HERE,
			&["The constraint"],
		));
		result.push(diagnostic);
	}
	result
}

/// `orderClassDiagnostics`: by file, position, then text.
pub(crate) fn order_class_diagnostics(input: &[Diagnostic]) -> Vec<Diagnostic> {
	let mut ordered = input.to_vec();
	ordered.sort_by(|left, right| {
		diagnostic_file_name(left)
			.cmp(&diagnostic_file_name(right))
			.then_with(|| left.pos().cmp(&right.pos()))
			.then_with(|| left.string().cmp(right.string()))
	});
	ordered
}
