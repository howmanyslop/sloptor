//! `transform_unary.go`: `++` and `--` on component attributes.

use rotor_tsrs::P;
use rotor_tsrs::ast::{Kind, Node, TokenFlags, is_postfix_unary_expression};

use crate::Error;
use crate::transform_component::{is_flamework_attributes_access, new_flamework_attribute_setter_call};
use crate::transform_expressions::{ExpressionTransformResult, transform_flamework_expression_children_with_runtime};
use crate::transform_macro_types::MacroRuntime;
use crate::transform_state::TransformState;

/// `transformFlameworkUnaryExpression`
pub(crate) fn transform_flamework_unary_expression(
	state: &mut TransformState<'_>,
	node: P<Node>,
	runtime: &MacroRuntime,
) -> Result<ExpressionTransformResult, Error> {
	let (operand, operator, postfix) = flamework_unary_parts(node);
	let Some(binary_operator) = flamework_unary_operator(operator) else {
		return transform_flamework_expression_children_with_runtime(state, node, runtime);
	};
	if !is_flamework_attributes_access(state, operand) {
		return transform_flamework_expression_children_with_runtime(state, node, runtime);
	}
	let factory = &state.factory;
	let value = factory.new_binary_expression(
		None,
		operand,
		None,
		factory.new_token(binary_operator),
		factory.new_numeric_literal("1", TokenFlags::None),
	);
	let setter = new_flamework_attribute_setter_call(state, operand, value, postfix)?;
	Ok(ExpressionTransformResult::empty(setter))
}

fn flamework_unary_parts(node: P<Node>) -> (P<Node>, Kind, bool) {
	if is_postfix_unary_expression(node) {
		let unary = node.as_postfix_unary_expression();
		return (unary.operand, unary.operator, true);
	}
	let unary = node.as_prefix_unary_expression();
	(unary.operand, unary.operator, false)
}

const fn flamework_unary_operator(operator: Kind) -> Option<Kind> {
	match operator {
		Kind::PlusPlusToken => Some(Kind::PlusToken),
		Kind::MinusMinusToken => Some(Kind::MinusToken),
		_ => None,
	}
}
