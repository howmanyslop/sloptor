//! `transform_call.go`: call expressions.

use rotor_tsrs::P;
use rotor_tsrs::ast::{Node, is_super_call};

use crate::Error;
use crate::transform::call_may_be_flamework_macro;
use crate::transform_call_prefilter::{callee_may_resolve_to_flamework_macro, pass_through_flamework_call};
use crate::transform_expressions::{
	ExpressionTransformResult, macro_transform_changed, macro_transform_imports,
	transform_flamework_expression_children_with_runtime,
};
use crate::transform_macro_types::MacroRuntime;
use crate::transform_macros::transform_flamework_call;
use crate::transform_state::TransformState;

/// `transformFlameworkCallExpression`
pub(crate) fn transform_flamework_call_expression(
	state: &mut TransformState<'_>,
	node: P<Node>,
	runtime: &MacroRuntime,
) -> Result<ExpressionTransformResult, Error> {
	if is_super_call(node) || !call_may_be_flamework_macro(state, node) {
		return transform_flamework_expression_children_with_runtime(state, node, runtime);
	}
	let result = if callee_may_resolve_to_flamework_macro(state, node) {
		// Go returns early when the checker resolves no signature; tsrs always resolves one.
		state.ck().get_resolved_signature(node);
		transform_flamework_call(state, node, runtime)?
	} else {
		pass_through_flamework_call(state, node)
	};
	finish_macro_transform(state, node, result, runtime)
}

/// The shared tail of call and `new` transforms: a changed macro result replaces the node,
/// else the result's children are visited with its prerequisites and imports first.
pub(crate) fn finish_macro_transform(
	state: &mut TransformState<'_>,
	node: P<Node>,
	result: crate::transform_macro_types::MacroTransformResult,
	runtime: &MacroRuntime,
) -> Result<ExpressionTransformResult, Error> {
	let expression = result.expression.unwrap_or(node);
	if macro_transform_changed(node, result.expression) {
		let imports = macro_transform_imports(state, node, &result);
		return Ok(ExpressionTransformResult {
			expression,
			prerequisites: result.prerequisites,
			imports,
		});
	}
	let mut children = transform_flamework_expression_children_with_runtime(state, expression, runtime)?;
	let mut prerequisites = result.prerequisites.clone();
	prerequisites.append(&mut children.prerequisites);
	children.prerequisites = prerequisites;
	let mut imports = macro_transform_imports(state, node, &result);
	imports.append(&mut children.imports);
	children.imports = imports;
	Ok(children)
}
