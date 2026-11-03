//! `delete.go`: `expressions/transformDeleteExpression.ts`. The access transforms emit the
//! `= nil` when they see a delete parent; this node keeps prereq order and the `true` value.

use crate::conditional::is_used_as_statement;
use crate::dispatch::transform_expression;
use crate::prelude::*;

/// `transformDeleteExpression`
pub fn transform_delete_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	if let Some(expression) = node.expression() {
		transform_expression(state, expression);
	}
	if !is_used_as_statement(node) {
		return state.ast.bool(true);
	}
	state.ast.none()
}
