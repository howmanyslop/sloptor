//! `transform_reflection.go`: constructor parameter and implemented-type IDs, and
//! `Reflect.defineMetadata` statements.

use rotor_tsrs::P;
use rotor_tsrs::ast::{Kind, Node, NodeFactory, NodeFlags, TokenFlags, is_constructor_declaration};

use crate::Error;
use crate::plan::FilePlan;
use crate::transform_macro_types::alloc;
use crate::transform_reflection_metadata::metadata_for_class;
use crate::transform_reflection_methods::reflected_parameter_ids;
use crate::transform_state::TransformState;
use crate::transform_symbols::node_uid;

/// `analyzeConstructorParameters`
pub(crate) fn analyze_constructor_parameters(
	state: &mut TransformState<'_>,
	_plan: &FilePlan,
	node: P<Node>,
) -> Result<Vec<String>, Error> {
	if !metadata_for_class(state, node).requested("flamework:parameters") {
		return Ok(Vec::new());
	}
	for &member in node.members() {
		if is_constructor_declaration(member) {
			return reflected_parameter_ids(state, member.parameters());
		}
	}
	Ok(Vec::new())
}

/// `analyzeImplementedTypes`
pub(crate) fn analyze_implemented_types(
	state: &mut TransformState<'_>,
	_plan: &FilePlan,
	node: P<Node>,
) -> Result<Vec<String>, Error> {
	if !metadata_for_class(state, node).requested("flamework:implements") {
		return Ok(Vec::new());
	}
	let mut identifiers = Vec::new();
	let Some(clauses) = node.as_class_declaration().heritage_clauses() else {
		return Ok(identifiers);
	};
	for &clause in clauses.nodes() {
		let heritage = clause.as_heritage_clause();
		if heritage.token() != Kind::ImplementsKeyword {
			continue;
		}
		for &implemented in heritage.types().nodes() {
			identifiers.push(node_uid(state, implemented)?);
		}
	}
	Ok(identifiers)
}

/// `metadataCall`
pub(crate) struct MetadataCall {
	pub(crate) class_name: String,
	pub(crate) key: String,
	pub(crate) value: P<Node>,
}

/// `newMetadataStatement`: `Reflect.defineMetadata(Class, "key", value)`.
pub(crate) fn new_metadata_statement(factory: &NodeFactory, call: &MetadataCall) -> P<Node> {
	new_reflect_call(
		factory,
		"defineMetadata",
		vec![
			factory.new_identifier(alloc(&call.class_name)),
			factory.new_string_literal(alloc(&call.key), TokenFlags::None),
			call.value,
		],
	)
}

/// `newReflectCall`
pub(crate) fn new_reflect_call(factory: &NodeFactory, method: &str, arguments: Vec<P<Node>>) -> P<Node> {
	let access = factory.new_element_access_expression(
		factory.new_identifier("Reflect"),
		None,
		factory.new_string_literal(alloc(method), TokenFlags::None),
		NodeFlags::None,
	);
	let call = factory.new_call_expression(access, None, None, factory.new_node_list(arguments), NodeFlags::None);
	factory.new_expression_statement(call)
}

/// `stringArray`
pub(crate) fn string_array(factory: &NodeFactory, values: &[String]) -> P<Node> {
	let elements = values
		.iter()
		.map(|value| factory.new_string_literal(alloc(value), TokenFlags::None))
		.collect();
	expression_array(factory, elements)
}

/// `expressionArray`
pub(crate) fn expression_array(factory: &NodeFactory, values: Vec<P<Node>>) -> P<Node> {
	factory.new_array_literal_expression(factory.new_node_list(values), false)
}
