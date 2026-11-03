//! `transform_delete.go`: `delete` of a component attribute.

use rotor_tsrs::P;
use rotor_tsrs::ast::Node;

use crate::Error;
use crate::transform_component::{is_flamework_attributes_access, new_flamework_attribute_setter_call};
use crate::transform_expressions::{ExpressionTransformResult, transform_flamework_expression_children_with_runtime};
use crate::transform_macro_types::MacroRuntime;
use crate::transform_state::TransformState;

/// `transformFlameworkDeleteExpression`
pub(crate) fn transform_flamework_delete_expression(
	state: &mut TransformState<'_>,
	node: P<Node>,
	runtime: &MacroRuntime,
) -> Result<ExpressionTransformResult, Error> {
	let Some(access) = node.expression() else {
		return transform_flamework_expression_children_with_runtime(state, node, runtime);
	};
	if !is_flamework_attributes_access(state, access) {
		return transform_flamework_expression_children_with_runtime(state, node, runtime);
	}
	let undefined = state.factory.new_identifier("undefined");
	let setter = new_flamework_attribute_setter_call(state, access, undefined, false)?;
	Ok(ExpressionTransformResult::empty(setter))
}
