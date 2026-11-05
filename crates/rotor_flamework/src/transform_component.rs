//! `transform_component.go`: component attribute access and the attribute setter.

use rotor_tsrs::P;
use rotor_tsrs::ast::{
	Kind, Node, NodeFactory, NodeFlags, Symbol, SymbolFlags, TokenFlags, get_source_file_of_node,
	is_element_access_expression, is_identifier, is_property_access_expression,
};

use crate::Error;
use crate::transform::source_may_need_flamework_access_rewrite;
use crate::transform_class_diagnostics_constraints::collect_node_metadata;
use crate::transform_macro_types::invalid_macro;
use crate::transform_state::TransformState;

pub(crate) const COMPONENT_ATTRIBUTES_METADATA: &str = "intrinsic-component-attributes";
pub(crate) const COMPONENT_INSTANCE_METADATA: &str = "intrinsic-component-instance";
pub(crate) const ATTRIBUTE_SETTER_IDENTIFIER: &str = "SYMBOL_ATTRIBUTE_SETTER";
const ATTRIBUTE_SETTER_MODULE: &str = "@flamework/components/out/baseComponent";

/// `isFlameworkAttributesAccess`: `x.attributes.y` (or `[...]`) on a component.
pub(crate) fn is_flamework_attributes_access(state: &mut TransformState<'_>, node: P<Node>) -> bool {
	if !is_property_access_expression(node) && !is_element_access_expression(node) {
		return false;
	}
	if !get_source_file_of_node(node).is_some_and(|file| source_may_need_flamework_access_rewrite(state, file)) {
		return false;
	}
	let Some(expression) = node.expression() else {
		return false;
	};
	let Some(mut symbol) = state.ck().get_symbol_at_location(expression) else {
		return false;
	};
	if symbol.flags().intersects(SymbolFlags::Alias) {
		symbol = state.ck().get_aliased_symbol(symbol);
	}
	flamework_symbol_requests_metadata(state, Some(symbol), COMPONENT_ATTRIBUTES_METADATA)
}

/// `flameworkSymbolRequestsMetadata`
pub(crate) fn flamework_symbol_requests_metadata(
	state: &mut TransformState<'_>,
	symbol: Option<P<Symbol>>,
	requested: &str,
) -> bool {
	let Some(declaration) = symbol.and_then(|symbol| symbol.value_declaration()) else {
		return false;
	};
	collect_node_metadata(state, declaration).requested(requested)
}

fn flamework_access_index(state: &TransformState<'_>, access: P<Node>) -> P<Node> {
	if is_property_access_expression(access) {
		let name = access.name().map_or("", |name| name.text());
		return state.factory.new_string_literal(name, TokenFlags::None);
	}
	access.as_element_access_expression().argument_expression
}

fn flamework_attribute_receiver(access: P<Node>) -> Option<P<Node>> {
	let container = access.expression()?;
	if !is_property_access_expression(container) && !is_element_access_expression(container) {
		return None;
	}
	container.expression()
}

/// `newFlameworkAttributeSetterCall`: `receiver[SYMBOL_ATTRIBUTE_SETTER](key, value[, true])`.
pub(crate) fn new_flamework_attribute_setter_call(
	state: &TransformState<'_>,
	access: P<Node>,
	value: P<Node>,
	postfix: bool,
) -> Result<P<Node>, Error> {
	let Some(receiver) = flamework_attribute_receiver(access) else {
		return Err(invalid_macro(access, "assignments not supported with direct access"));
	};
	let factory = &state.factory;
	let setter = factory.new_element_access_expression(
		receiver,
		None,
		factory.new_identifier(ATTRIBUTE_SETTER_IDENTIFIER),
		NodeFlags::None,
	);
	let mut arguments = vec![flamework_access_index(state, access), value];
	if postfix {
		arguments.push(factory.new_token(Kind::TrueKeyword));
	}
	Ok(factory.new_call_expression(setter, None, None, factory.new_node_list(arguments), NodeFlags::None))
}

/// `expressionUsesAttributeSetter`
pub(crate) fn expression_uses_attribute_setter(node: P<Node>) -> bool {
	if is_identifier(node) && node.text() == ATTRIBUTE_SETTER_IDENTIFIER {
		return true;
	}
	node.iter_children().into_iter().any(expression_uses_attribute_setter)
}

/// `newAttributeSetterImport`
pub(crate) fn new_attribute_setter_import(factory: &NodeFactory) -> P<Node> {
	let identifier = factory.new_identifier(ATTRIBUTE_SETTER_IDENTIFIER);
	let specifier = factory.new_import_specifier(false, None, identifier);
	let named_imports = factory.new_named_imports(factory.new_node_list(vec![specifier]));
	let clause = factory.new_import_clause(Kind::Unknown, None, Some(named_imports));
	factory.new_import_declaration(
		None,
		Some(clause),
		factory.new_string_literal(ATTRIBUTE_SETTER_MODULE, TokenFlags::None),
		None,
	)
}
