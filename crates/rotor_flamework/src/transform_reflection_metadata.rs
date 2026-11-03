//! `transform_reflection_metadata.go`: `@metadata` tokens of nodes, decorators, classes and
//! members.

use rotor_tsrs::P;
use rotor_tsrs::ast::{
	Kind, Node, SymbolFlags, get_implements_heritage_clause_elements, get_property_name_for_property_name_node,
	get_source_file_of_node, is_call_expression, is_class_like, is_decorator, is_source_file, is_statement,
};

use crate::metadata::Metadata;
use crate::transform_decorators::is_flamework_decorator;
use crate::transform_macro_types::jsdoc_tags;
use crate::transform_state::TransformState;

/// `metadataForNode`: the node's own `@metadata` tags.
pub(crate) fn metadata_for_node(_state: &mut TransformState<'_>, node: P<Node>) -> Metadata {
	let mut metadata = Metadata::new(Vec::<String>::new());
	let file = get_source_file_of_node(node)
		.map(|file| -> &'static rotor_tsrs::ast::SourceFile { file.as_node().as_source_file() });
	for jsdoc in node.jsdoc(file) {
		for tag in jsdoc_tags(*jsdoc) {
			if tag.kind() != Kind::JSDocUnknownTag || tag.tag_name().text() != "metadata" {
				continue;
			}
			let text: String = tag.comments().iter().map(|comment| comment.text()).collect();
			metadata = merge_metadata(&metadata, &Metadata::parse_text(&text));
		}
	}
	metadata
}

/// `metadataForDecorator`: the metadata of the decorator's declarations, up to their
/// statements.
pub(crate) fn metadata_for_decorator(state: &mut TransformState<'_>, identifier: P<Node>) -> Metadata {
	let mut metadata = Metadata::new(Vec::<String>::new());
	let Some(mut symbol) = state.ck().get_symbol_at_location(identifier) else {
		return metadata;
	};
	if symbol.flags().intersects(SymbolFlags::Alias) {
		symbol = state.ck().get_aliased_symbol(symbol);
	}
	for &declaration in symbol.declarations() {
		let mut current = Some(declaration);
		while let Some(node) = current {
			if is_source_file(node) {
				break;
			}
			let node_metadata = metadata_for_node(state, node);
			metadata = merge_metadata(&metadata, &node_metadata);
			if is_statement(node) {
				break;
			}
			current = node.parent();
		}
	}
	metadata
}

/// `mergeMetadata`
pub(crate) fn merge_metadata(left: &Metadata, right: &Metadata) -> Metadata {
	let mut tokens = left.tokens();
	tokens.extend(right.tokens());
	Metadata::new(tokens)
}

/// `metadataForClass`: the decorated class's metadata and its implemented interfaces'.
pub(crate) fn metadata_for_class(state: &mut TransformState<'_>, node: P<Node>) -> Metadata {
	let mut metadata = metadata_for_decorated_node(state, node);
	for &implemented in get_implements_heritage_clause_elements(node) {
		let Some(expression) = heritage_entity(implemented) else {
			continue;
		};
		let Some(mut symbol) = state.ck().get_symbol_at_location(expression) else {
			continue;
		};
		if symbol.flags().intersects(SymbolFlags::Alias) {
			symbol = state.ck().get_aliased_symbol(symbol);
		}
		for &declaration in symbol.declarations() {
			let declaration_metadata = metadata_for_node(state, declaration);
			metadata = merge_metadata(&metadata, &declaration_metadata);
		}
	}
	metadata
}

/// `metadataForMember`: the decorated member's metadata and the implemented interfaces'
/// member of the same name.
pub(crate) fn metadata_for_member(state: &mut TransformState<'_>, member: P<Node>) -> Metadata {
	let mut metadata = metadata_for_decorated_node(state, member);
	let (Some(name), Some(parent)) = (member.name(), member.parent()) else {
		return metadata;
	};
	if !is_class_like(parent) {
		return metadata;
	}
	let name = get_property_name_for_property_name_node(name);
	for &implemented in get_implements_heritage_clause_elements(parent) {
		let Some(expression) = heritage_entity(implemented) else {
			continue;
		};
		let Some(mut symbol) = state.ck().get_symbol_at_location(expression) else {
			continue;
		};
		if symbol.flags().intersects(SymbolFlags::Alias) {
			symbol = state.ck().get_aliased_symbol(symbol);
		}
		let Some(interface_member) = symbol
			.members()
			.and_then(|members| rotor_tsrs::ast::SymbolTable::get(&members, &name))
		else {
			continue;
		};
		for &declaration in interface_member.declarations() {
			let declaration_metadata = metadata_for_node(state, declaration);
			metadata = merge_metadata(&metadata, &declaration_metadata);
		}
	}
	metadata
}

fn metadata_for_decorated_node(state: &mut TransformState<'_>, node: P<Node>) -> Metadata {
	let mut metadata = metadata_for_node(state, node);
	let Some(modifiers) = node.modifiers() else {
		return metadata;
	};
	for &modifier in modifiers.nodes() {
		if !is_decorator(modifier) || !is_flamework_decorator(state, modifier) {
			continue;
		}
		let Some(mut expression) = modifier.expression() else {
			continue;
		};
		if is_call_expression(expression)
			&& let Some(callee) = expression.expression()
		{
			expression = callee;
		}
		let decorator_metadata = crate::transform_reflection_metadata::metadata_for_decorator(state, expression);
		metadata = merge_metadata(&metadata, &decorator_metadata);
	}
	metadata
}

/// An `implements` element's entity: Go's `ExpressionWithTypeArguments.Expression`, which
/// TypeScript 7.1 parses as a `TypeReference` whose `typeName` this returns.
pub(crate) fn heritage_entity(element: P<Node>) -> Option<P<Node>> {
	if rotor_tsrs::ast::is_type_reference_node(element) {
		return Some(element.as_type_reference_node().type_name);
	}
	element.expression()
}
