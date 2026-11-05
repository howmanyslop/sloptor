//! `conditional.go`: `expressions/transformConditionalExpression.ts` and
//! `util/isUsedAsStatement.ts`.

use rotor_tsrs::ast as tsast;

use crate::dispatch::transform_expression;
use crate::prelude::*;
use crate::statements::wrap_expression_statement;
use crate::truthiness::create_truthiness_checks;

/// `isUsedAsStatement`: the expression (through skip-upwards wrappers) sits in statement
/// position: an expression statement, a for initializer or incrementor, or a delete used as a
/// statement.
#[must_use]
pub fn is_used_as_statement(expression: P<Node>) -> bool {
	let child = tsast::skip_upwards(expression);
	let Some(parent) = child.parent() else {
		return false;
	};

	if tsast::is_expression_statement(parent) {
		return true;
	}

	// part of a for statement definition, except as the condition
	if tsast::is_for_statement(parent) && parent.as_for_statement().condition != Some(child) {
		return true;
	}

	tsast::is_delete_expression(parent) && is_used_as_statement(parent)
}

/// `transformConditionalExpression`: `a ? b : c`. Statement position becomes an if statement; a
/// prereq-free value inlines to `if a then b else c`; otherwise an undeclared `_result` temp is
/// assigned in both branches.
pub fn transform_conditional_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let conditional = node.as_conditional_expression();
	let condition = transform_expression(state, conditional.condition);
	let (when_true, when_true_prereqs) = state.capture(|state| transform_expression(state, conditional.when_true));
	let (when_false, when_false_prereqs) = state.capture(|state| transform_expression(state, conditional.when_false));

	if is_used_as_statement(node) {
		let true_statements = wrap_expression_statement(&mut state.ast, when_true);
		state.ast.list_push_list(when_true_prereqs, true_statements);
		let false_statements = wrap_expression_statement(&mut state.ast, when_false);
		state.ast.list_push_list(when_false_prereqs, false_statements);
		let t = state.get_type(conditional.condition);
		let checks = create_truthiness_checks(state, condition, conditional.condition, t);
		let statement = state
			.ast
			.if_statement(checks, when_true_prereqs, Some(when_false_prereqs.into()));
		state.prereq(statement);
		return state.ast.none();
	}

	if state.ast.list_is_empty(when_true_prereqs) && state.ast.list_is_empty(when_false_prereqs) {
		let t = state.get_type(conditional.condition);
		let checks = create_truthiness_checks(state, condition, conditional.condition, t);
		return state.ast.if_expression(checks, when_true, when_false);
	}

	let temp_id = state.ast.temp_id("result");
	let declaration = state.ast.variable_declaration(temp_id, None);
	state.prereq(declaration);

	let true_assignment = state.ast.assignment(temp_id, AssignmentOperator::Assign, when_true);
	state.ast.list_push(when_true_prereqs, true_assignment);
	let false_assignment = state.ast.assignment(temp_id, AssignmentOperator::Assign, when_false);
	state.ast.list_push(when_false_prereqs, false_assignment);

	let t = state.get_type(conditional.condition);
	let checks = create_truthiness_checks(state, condition, conditional.condition, t);
	let statement = state
		.ast
		.if_statement(checks, when_true_prereqs, Some(when_false_prereqs.into()));
	state.prereq(statement);

	temp_id
}
