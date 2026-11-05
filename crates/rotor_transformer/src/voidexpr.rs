//! `voidexpr.go`: `expressions/transformVoidExpression.ts`. The operand runs for its side
//! effects in statement form, and the expression is `nil`.

use rotor_tsrs::ast as tsast;

use crate::prelude::*;
use crate::statements::transform_expression_statement_inner;

/// `transformVoidExpression`
pub fn transform_void_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	if let Some(expression) = node.expression() {
		let statements = transform_expression_statement_inner(state, tsast::skip_downwards(expression));
		state.prereq_list(statements);
	}
	state.ast.nil()
}
