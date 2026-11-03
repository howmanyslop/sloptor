//! `transform_binary.go`: assignments to component attributes.

use rotor_tsrs::P;
use rotor_tsrs::ast::{Kind, Node};

use crate::Error;
use crate::transform_component::{is_flamework_attributes_access, new_flamework_attribute_setter_call};
use crate::transform_expressions::{
	ExpressionTransformResult, transform_flamework_expression_children_with_runtime,
	transform_flamework_expression_with_runtime,
};
use crate::transform_macro_types::MacroRuntime;
use crate::transform_state::TransformState;

/// `flameworkAssignmentOperators`: each assignment operator's binary operator.
const fn flamework_assignment_operator(kind: Kind) -> Option<Kind> {
	Some(match kind {
		Kind::EqualsToken => Kind::EqualsToken,
		Kind::BarEqualsToken => Kind::BarToken,
		Kind::PlusEqualsToken => Kind::PlusToken,
		Kind::MinusEqualsToken => Kind::MinusToken,
		Kind::CaretEqualsToken => Kind::CaretToken,
		Kind::SlashEqualsToken => Kind::SlashToken,
		Kind::BarBarEqualsToken => Kind::BarBarToken,
		Kind::PercentEqualsToken => Kind::PercentToken,
		Kind::AsteriskEqualsToken => Kind::AsteriskToken,
		Kind::AmpersandEqualsToken => Kind::AmpersandToken,
		Kind::QuestionQuestionEqualsToken => Kind::QuestionQuestionToken,
		Kind::AsteriskAsteriskEqualsToken => Kind::AsteriskAsteriskToken,
		Kind::LessThanLessThanEqualsToken => Kind::LessThanLessThanToken,
		Kind::AmpersandAmpersandEqualsToken => Kind::AmpersandAmpersandToken,
		Kind::GreaterThanGreaterThanEqualsToken => Kind::GreaterThanGreaterThanToken,
		Kind::GreaterThanGreaterThanGreaterThanEqualsToken => Kind::GreaterThanGreaterThanGreaterThanToken,
		_ => return None,
	})
}

/// `transformFlameworkBinaryExpression`
pub(crate) fn transform_flamework_binary_expression(
	state: &mut TransformState<'_>,
	node: P<Node>,
	runtime: &MacroRuntime,
) -> Result<ExpressionTransformResult, Error> {
	let binary = node.as_binary_expression();
	let Some(operator) = flamework_assignment_operator(binary.operator_token().kind()) else {
		return transform_flamework_expression_children_with_runtime(state, node, runtime);
	};
	if !is_flamework_attributes_access(state, binary.left()) {
		return transform_flamework_expression_children_with_runtime(state, node, runtime);
	}
	let right = transform_flamework_expression_with_runtime(state, binary.right(), runtime)?;
	let value = if operator == Kind::EqualsToken {
		right.expression
	} else {
		state.factory.new_binary_expression(
			None,
			binary.left(),
			None,
			state.factory.new_token(operator),
			right.expression,
		)
	};
	let setter = new_flamework_attribute_setter_call(state, binary.left(), value, false)?;
	// The macro that produced `value` may have requested imports and hoisted statements.
	Ok(ExpressionTransformResult {
		expression: setter,
		prerequisites: right.prerequisites,
		imports: right.imports,
	})
}
