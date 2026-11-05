//! `transform_new.go`: `new` expressions.

use rotor_tsrs::P;
use rotor_tsrs::ast::Node;

use crate::Error;
use crate::transform::call_may_be_flamework_macro;
use crate::transform_call::finish_macro_transform;
use crate::transform_expressions::{ExpressionTransformResult, transform_flamework_expression_children_with_runtime};
use crate::transform_macro_types::MacroRuntime;
use crate::transform_macros::transform_flamework_call;
use crate::transform_state::TransformState;

/// `transformFlameworkNewExpression`
pub(crate) fn transform_flamework_new_expression(
	state: &mut TransformState<'_>,
	node: P<Node>,
	runtime: &MacroRuntime,
) -> Result<ExpressionTransformResult, Error> {
	// Go also skips a call with no resolved signature; tsrs always resolves one.
	if !call_may_be_flamework_macro(state, node) {
		return transform_flamework_expression_children_with_runtime(state, node, runtime);
	}
	let result = transform_flamework_call(state, node, runtime)?;
	finish_macro_transform(state, node, result, runtime)
}
