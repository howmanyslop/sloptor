//! `logicalassignment.go`: `nodes/transformLogicalOrCoalescingAssignmentExpression.ts`.

use crate::assignment::transform_writable_expression;
use crate::dispatch::transform_expression;
use crate::logical::tag_condition_operator;
use crate::prelude::*;
use crate::truthiness::create_truthiness_checks;

/// `transformCoalescingAssignmentExpression` (L8-36): `a ??= b` is a pure
/// `if a == nil then a = b end`, with no truthiness checks and the right side's prereqs inside
/// the if. Identifier and id-rooted targets need no temps.
fn transform_coalescing_assignment_expression(
	state: &mut State<'_>,
	left: P<Node>,
	right: P<Node>,
	operator: P<Node>,
) -> NodeId {
	let writable = transform_writable_expression(state, left, true);
	let (value, value_prereqs) = state.capture(|state| transform_expression(state, right));

	let if_statements = state.ast.new_list();
	state.ast.list_push_list(if_statements, value_prereqs);
	let assignment = state.ast.assignment(writable, AssignmentOperator::Assign, value);
	state.ast.list_push(if_statements, assignment);

	let nil = state.ast.nil();
	let condition = state.ast.binary(writable, BinaryOperator::Equal, nil);
	let condition = tag_condition_operator(state, condition, Some(operator));
	let else_body = state.ast.new_list::<element::Statement>();
	let statement = state.ast.if_statement(condition, if_statements, Some(else_body.into()));
	state.prereq(statement);

	writable
}

/// `transformLogicalAndAssignmentExpression` (L38-76) and
/// `transformLogicalOrAssignmentExpression` (L78-116): capture a `_condition` temp from the
/// writable, truthiness-test the original writable (not the temp), assign the right side to the
/// temp inside the if, then write the temp back unconditionally (a redundant self-assignment
/// when the branch was not taken; upstream quirk, ported verbatim). `||=` negates the test.
fn transform_logical_and_or_assignment_expression(
	state: &mut State<'_>,
	left: P<Node>,
	right: P<Node>,
	operator: P<Node>,
	negate: bool,
) -> NodeId {
	let writable = transform_writable_expression(state, left, true);
	let (value, value_prereqs) = state.capture(|state| transform_expression(state, right));

	let condition_id = state.push_to_var(Some(writable), "condition");

	let if_statements = state.ast.new_list();
	state.ast.list_push_list(if_statements, value_prereqs);
	let assignment = state.ast.assignment(condition_id, AssignmentOperator::Assign, value);
	state.ast.list_push(if_statements, assignment);

	let left_type = state.get_type(left);
	let mut condition = create_truthiness_checks(state, writable, left, left_type);
	if negate {
		condition = state.ast.unary(UnaryOperator::Not, condition);
	}
	let condition = tag_condition_operator(state, condition, Some(operator));
	let else_body = state.ast.new_list::<element::Statement>();
	let statement = state.ast.if_statement(condition, if_statements, Some(else_body.into()));
	state.prereq(statement);

	let write_back = state.ast.assignment(writable, AssignmentOperator::Assign, condition_id);
	state.prereq(write_back);

	writable
}

/// `transformLogicalOrCoalescingAssignmentExpression` (L118-130): operator-token dispatch; the
/// returned writable is the expression's value (a re-read of the target).
pub fn transform_logical_or_coalescing_assignment_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let expression = node.as_binary_expression();
	let (left, right, operator) = (expression.left, expression.right(), expression.operator_token);
	match operator.kind() {
		Kind::QuestionQuestionEqualsToken => transform_coalescing_assignment_expression(state, left, right, operator),
		Kind::AmpersandAmpersandEqualsToken => {
			transform_logical_and_or_assignment_expression(state, left, right, operator, false)
		}
		_ => transform_logical_and_or_assignment_expression(state, left, right, operator, true),
	}
}

/// `transformLogicalOrCoalescingAssignmentExpressionStatement` (L132-137): statement position
/// discards the writable and keeps only the prereqs.
pub fn transform_logical_or_coalescing_assignment_expression_statement(
	state: &mut State<'_>,
	node: P<Node>,
) -> Statements {
	state.capture_statements(|state| {
		transform_logical_or_coalescing_assignment_expression(state, node);
	})
}
