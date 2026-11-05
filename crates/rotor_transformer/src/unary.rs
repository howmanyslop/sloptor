//! `unary.go`: `expressions/transformUnaryExpression.ts`, the expression positions of `++`/`--`
//! (temps capture the value); statement position lives in [`crate::statements`].

use rotor_tsrs::types::TypeCheck;

use crate::access::validate_not_any_type;
use crate::assignment::transform_writable_expression;
use crate::diagnostics;
use crate::dispatch::transform_expression;
use crate::prelude::*;
use crate::truthiness::create_truthiness_checks;
use crate::types::is_definitely_type;

/// `transformPostfixUnaryExpression` (L13-40): `i++` captures the original value.
///
/// ```text
/// local _original = i
/// i += 1
/// -> _original
/// ```
pub fn transform_postfix_unary_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let expression = node.as_postfix_unary_expression();

	validate_not_any_type(state, expression.operand);

	let writable = transform_writable_expression(state, expression.operand, true);
	let original = state.ast.temp_id("original");

	let declaration = state.ast.variable_declaration(original, Some(writable.into()));
	state.prereq(declaration);

	let operator = match expression.operator {
		Kind::PlusPlusToken => AssignmentOperator::Add,
		Kind::MinusMinusToken => AssignmentOperator::Subtract,
		other => {
			state.ice(format!(
				"transformer: transformPostfixUnaryExpression unknown operator: {}",
				diagnostics::kind_name(other)
			));
			return state.ast.none();
		}
	};
	let one = state.ast.num(1.0);
	let assignment = state.ast.assignment(writable, operator, one);
	state.set_update_operator_source_position(assignment, node);
	state.prereq(assignment);

	original
}

/// `transformPrefixUnaryExpression` (L42-71).
pub fn transform_prefix_unary_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let expression = node.as_prefix_unary_expression();

	validate_not_any_type(state, expression.operand);

	match expression.operator {
		Kind::PlusPlusToken | Kind::MinusMinusToken => {
			// `++i`: prereq the increment, return the writable (the new value).
			let writable = transform_writable_expression(state, expression.operand, true);
			let operator = if expression.operator == Kind::MinusMinusToken {
				AssignmentOperator::Subtract
			} else {
				AssignmentOperator::Add
			};
			let one = state.ast.num(1.0);
			let assignment = state.ast.assignment(writable, operator, one);
			state.set_update_operator_source_position(assignment, node);
			state.prereq(assignment);
			writable
		}
		Kind::PlusToken => {
			state.diags.add(diagnostics::no_unary_plus(node));
			transform_expression(state, expression.operand)
		}
		Kind::MinusToken => {
			let t = state.get_type(expression.operand);
			if !is_definitely_type(state, t, &[TypeCheck::Number]) {
				state.diags.add(diagnostics::no_non_number_unary_minus(node));
			}
			let operand = transform_expression(state, expression.operand);
			state.ast.unary(UnaryOperator::Negate, operand)
		}
		Kind::ExclamationToken => {
			let operand = transform_expression(state, expression.operand);
			let t = state.get_type(expression.operand);
			let checks = create_truthiness_checks(state, operand, expression.operand, t);
			state.ast.unary(UnaryOperator::Not, checks)
		}
		Kind::TildeToken => {
			let callee = state.ast.global_property("bit32", "bnot");
			let operand = transform_expression(state, expression.operand);
			let args = state.ast.list_from([operand]);
			state.ast.call(callee, args)
		}
		other => {
			state.ice(format!(
				"transformer: transformPrefixUnaryExpression unknown operator: {}",
				diagnostics::kind_name(other)
			));
			state.ast.none()
		}
	}
}
