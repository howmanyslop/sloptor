//! `transform_access.go`: key-obfuscated property access.

use rotor_tsrs::P;
use rotor_tsrs::ast::{
	Node, TokenFlags, get_source_file_of_node, is_as_expression, is_element_access_expression,
	is_property_access_expression, is_string_literal,
};
use rotor_tsrs::checker::LiteralValue;

use crate::transform::source_may_need_flamework_access_rewrite;
use crate::transform_expressions::{
	ExpressionTransformResult, transform_flamework_expression_children_with_runtime,
	transform_flamework_expression_with_runtime,
};
use crate::transform_macro_types::{MacroRuntime, alloc, macro_error};
use crate::transform_state::TransformState;
use crate::{Error, ErrorKind};

/// `transformFlameworkAccessExpression`
pub(crate) fn transform_flamework_access_expression(
	state: &mut TransformState<'_>,
	node: P<Node>,
	runtime: &MacroRuntime,
) -> Result<ExpressionTransformResult, Error> {
	if !get_source_file_of_node(node).is_some_and(|file| source_may_need_flamework_access_rewrite(state, file)) {
		return transform_flamework_expression_children_with_runtime(state, node, runtime);
	}
	let Some(receiver) = node.expression() else {
		return transform_flamework_expression_children_with_runtime(state, node, runtime);
	};
	let receiver_type = state.ck().get_type_at_location(receiver);
	let hash_type = state
		.ck()
		.get_type_of_property_of_type(receiver_type, "_flamework_key_obfuscation")
		.filter(|hash_type| hash_type.is_string_literal());
	let Some(hash_type) = hash_type else {
		return transform_flamework_expression_children_with_runtime(state, node, runtime);
	};

	let Some(name) = flamework_access_name(node) else {
		if is_element_access_expression(node)
			&& is_as_expression(node.as_element_access_expression().argument_expression)
		{
			return transform_flamework_expression_children_with_runtime(state, node, runtime);
		}
		return Err(macro_error(
			Some(node),
			"This object has key obfuscation enabled and must be accessed directly.".to_owned(),
			ErrorKind::DynamicObfuscatedAccess,
			Vec::new(),
		));
	};

	let receiver_result = transform_flamework_expression_with_runtime(state, receiver, runtime)?;
	let Some(LiteralValue::String(context)) = hash_type.as_literal_type().value() else {
		return Err(Error::kinded(
			ErrorKind::DynamicObfuscatedAccess,
			": obfuscation context is not text",
		));
	};
	let obfuscated = if state.project.config.obfuscation {
		state.hash_string(name, context).map_err(|error| {
			error.wrap(&format!(
				"obfuscate Flamework key {}",
				rotor_support::gostr::quote(name)
			))
		})?
	} else {
		name.to_owned()
	};
	let factory = &state.factory;
	let literal = factory.new_string_literal(name, TokenFlags::None);
	let index = factory.new_as_expression(
		factory.new_string_literal(alloc(&obfuscated), TokenFlags::None),
		factory.new_literal_type_node(literal),
	);
	Ok(ExpressionTransformResult {
		expression: factory.new_element_access_expression(
			receiver_result.expression,
			node.question_dot_token(),
			index,
			node.flags(),
		),
		prerequisites: receiver_result.prerequisites,
		imports: receiver_result.imports,
	})
}

fn flamework_access_name(node: P<Node>) -> Option<&'static str> {
	if is_property_access_expression(node) {
		return node.name().map(|name| name.text());
	}
	let argument = node.as_element_access_expression().argument_expression;
	is_string_literal(argument).then(|| argument.text())
}
