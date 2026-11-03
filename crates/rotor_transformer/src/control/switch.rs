//! `switch.go`: `statements/transformSwitchStatement.ts`.

use rotor_tsrs::ast::{self as tsast, SymbolFlags, skip_downwards};

use crate::call::expression_might_mutate;
use crate::dispatch::transform_expression;
use crate::prelude::*;
use crate::statementlist::{create_hoist_declaration, transform_statement_list};

/// A clause index as a Luau number.
fn index_number(ast: &mut Ast, index: usize) -> NodeId {
	ast.num(f64::from(u32::try_from(index).unwrap_or(u32::MAX)))
}

/// `transformCaseClauseExpression`: a clause condition and the statements needed to evaluate its
/// value once. When the previous clause can fall through:
///
/// - the case expression produced prereqs: they must only run when not already falling through,
///   so they are wrapped in `if not _fallthrough then <prereqs>; _fallthrough = _exp == (<case>)
///   end` and the clause condition becomes just `_fallthrough`;
/// - no prereqs: condition = `_fallthrough or _exp == (<case>)`.
fn transform_case_clause_expression(
	state: &mut State<'_>,
	case_clause_expression: P<Node>,
	switch_expression: NodeId,
	fall_through_flag_id: Option<NodeId>,
	can_fall_through_to: bool,
) -> (NodeId, Statements) {
	let (mut expression, mut prereq_statements) =
		state.capture(|state| transform_expression(state, case_clause_expression));

	if expression_might_mutate(state, expression, Some(case_clause_expression)) {
		let case_value_id = state.ast.temp_id("caseValue");
		let declaration = state.ast.variable_declaration(case_value_id, Some(expression.into()));
		state.ast.list_push(prereq_statements, declaration);
		expression = case_value_id;
	}

	let mut condition = state.ast.binary(switch_expression, BinaryOperator::Equal, expression);

	if can_fall_through_to && let Some(fall_through_flag_id) = fall_through_flag_id {
		if state.ast.list_is_non_empty(prereq_statements) {
			let no_fall_through_condition = state.ast.unary(UnaryOperator::Not, fall_through_flag_id);

			let assignment = state
				.ast
				.assignment(fall_through_flag_id, AssignmentOperator::Assign, condition);
			if let Some(clause) = case_clause_expression.parent() {
				state.set_node_source_position(assignment, clause);
			}
			state.ast.list_push(prereq_statements, assignment);

			let guard = state
				.ast
				.if_statement(no_fall_through_condition, prereq_statements, None);
			prereq_statements = state.ast.list_from([guard]);

			condition = fall_through_flag_id;
		} else {
			condition = state.ast.binary(fall_through_flag_id, BinaryOperator::Or, condition);
		}
	}

	(condition, prereq_statements)
}

/// `caseClauseResult`: `transformCaseClause`'s return object (L105-109).
struct CaseClauseResult {
	can_fall_through_from: bool,
	prereqs: Statements,
	clause_statements: Statements,
}

/// `transformCaseClause` (L54-110). `can_fall_through_from` is true when the body is empty or does
/// not end in a final statement; when it can fall through and the next clause is a case clause,
/// `_fallthrough = true` is appended.
///
/// `clause_statements` is `[hoist declaration?, if]`: the hoist declaration (`local x` for
/// case-locals referenced from sibling clauses, registered by `checkVariableHoist` during the
/// body transform and keyed on this clause) lands before the `if`, making the variables visible
/// to later clauses.
fn transform_case_clause(
	state: &mut State<'_>,
	node: P<Node>,
	switch_expression: NodeId,
	fall_through_flag_id: NodeId,
	can_fall_through_to: bool,
	should_update_fall_through_flag: bool,
) -> CaseClauseResult {
	let (condition, prereq_statements) = if let Some(expression) = node.expression() {
		transform_case_clause_expression(
			state,
			expression,
			switch_expression,
			Some(fall_through_flag_id),
			can_fall_through_to,
		)
	} else {
		state.ice("transformer: case clause has no expression");
		(state.ast.none(), state.ast.new_list())
	};

	let statements = transform_switch_clause_body(state, node);

	let can_fall_through_from = state
		.ast
		.list_tail(statements)
		.is_none_or(|tail| !state.ast.kind(state.ast.link_value(tail)).is_final_statement());
	let fall_through_assignment = if can_fall_through_from && should_update_fall_through_flag {
		let value = state.ast.bool(true);
		let assignment = state
			.ast
			.assignment(fall_through_flag_id, AssignmentOperator::Assign, value);
		state.ast.list_push(statements, assignment);
		Some(assignment)
	} else {
		None
	};

	let clause_statements = state.ast.new_list();

	if let Some(hoist_declaration) = create_hoist_declaration(state, node) {
		state.ast.list_push(clause_statements, hoist_declaration);
	}

	let clause_if = state.ast.if_statement(condition, statements, None);
	state.ast.list_push(clause_statements, clause_if);
	state.set_statement_list_source_positions(node, &[prereq_statements, clause_statements]);
	if let Some(fall_through_assignment) = fall_through_assignment
		&& let Some(origin) = state.ast.origin(clause_if)
	{
		state.ast.set_origin(fall_through_assignment, origin);
	}

	CaseClauseResult {
		can_fall_through_from,
		prereqs: prereq_statements,
		clause_statements,
	}
}

/// `transformSwitchClauseBody`: a clause whose only non-empty statement is a block dissolves the
/// braces (the block's statements, with the block as parent), as the trailing-default lowering
/// does.
fn transform_switch_clause_body(state: &mut State<'_>, node: P<Node>) -> Statements {
	let clause_statements = node.statements();
	let non_empty_statements: Vec<P<Node>> = clause_statements
		.iter()
		.copied()
		.filter(|&statement| !tsast::is_empty_statement(statement))
		.collect();
	if let [first_statement] = non_empty_statements[..]
		&& tsast::is_block(first_statement)
	{
		return transform_statement_list(state, Some(first_statement), first_statement.statements(), None);
	}
	transform_statement_list(state, Some(node), clause_statements, None)
}

/// `transformSwitchWithMiddleDefault`: selects the first matching case before running any
/// bodies. The default is selected only after all labels miss; executing bodies in source order
/// then preserves fall-through across it.
fn transform_switch_with_middle_default(
	state: &mut State<'_>,
	clauses: &[P<Node>],
	expression: NodeId,
	default_index: usize,
) -> Statements {
	let entry = state.ast.temp_id("caseIndex");
	let minus_one = state.ast.num(-1.0);
	let entry_declaration = state.ast.variable_declaration(entry, Some(minus_one.into()));
	let statements = state.ast.list_from([entry_declaration]);
	for (index, &clause) in clauses.iter().enumerate() {
		if tsast::is_default_clause(clause) {
			continue;
		}
		let Some(clause_expression) = clause.expression() else {
			state.ice("transformer: case clause has no expression");
			continue;
		};
		let (condition, prereqs) = transform_case_clause_expression(state, clause_expression, expression, None, false);
		let index_value = index_number(&mut state.ast, index);
		let assignment = state.ast.assignment(entry, AssignmentOperator::Assign, index_value);
		let body = state.ast.list_from([assignment]);
		let select = state.ast.if_statement(condition, body, None);
		state.ast.list_push(prereqs, select);
		let unselected = state.ast.num(-1.0);
		let selection_condition = state.ast.binary(entry, BinaryOperator::Equal, unselected);
		let selection = state.ast.if_statement(selection_condition, prereqs, None);
		state.set_node_source_position(selection, clause);
		state.ast.list_push(statements, selection);
	}
	let unselected = state.ast.num(-1.0);
	let default_condition = state.ast.binary(entry, BinaryOperator::Equal, unselected);
	let default_value = index_number(&mut state.ast, default_index);
	let default_assignment = state.ast.assignment(entry, AssignmentOperator::Assign, default_value);
	let default_body = state.ast.list_from([default_assignment]);
	let default_selection = state.ast.if_statement(default_condition, default_body, None);
	state.ast.list_push(statements, default_selection);
	let hoists = state.ast.new_list();
	for (index, &clause) in clauses.iter().enumerate() {
		let body = transform_switch_clause_body(state, clause);
		if let Some(declaration) = create_hoist_declaration(state, clause) {
			state.ast.list_push(hoists, declaration);
		}
		let index_value = index_number(&mut state.ast, index);
		let condition = state.ast.binary(entry, BinaryOperator::LessEqual, index_value);
		let clause_if = state.ast.if_statement(condition, body, None);
		state.set_node_source_position(clause_if, clause);
		state.ast.list_push(statements, clause_if);
	}
	state.ast.list_push_list(hoists, statements);
	hoists
}

/// `canCompareSwitchIdentifierDirectly`: keeps literal-only switches compact when the case block
/// still resolves the operand to the same binding.
fn can_compare_switch_identifier_directly(state: &mut State<'_>, node: P<Node>, expression: NodeId) -> bool {
	let switch_statement = node.as_switch_statement();
	let identifier = skip_downwards(switch_statement.expression);
	if identifier.kind() != Kind::Identifier || state.ast.kind(expression) != LuauKind::Identifier {
		return false;
	}
	let case_block = switch_statement.case_block;
	let resolved = state
		.ck()
		.resolve_name(identifier.text(), Some(case_block), SymbolFlags::Value, false);
	if resolved != state.ck().get_symbol_at_location(identifier) {
		return false;
	}
	for &clause in case_block.as_case_block().clauses.nodes() {
		if tsast::is_default_clause(clause) {
			continue;
		}
		let Some(label) = clause.expression().map(skip_downwards) else {
			return false;
		};
		match label.kind() {
			Kind::StringLiteral
			| Kind::NoSubstitutionTemplateLiteral
			| Kind::NumericLiteral
			| Kind::TrueKeyword
			| Kind::FalseKeyword => {}
			_ => return false,
		}
	}
	true
}

/// `transformSwitchStatement`: the whole switch lowers to `repeat ... until true`, with each case
/// clause an `if` block inside (TS `break` is a plain Luau `break` out of the repeat).
///
/// - Mutable subjects are captured before case evaluation unless every label is a literal and
///   the case block resolves the same identifier. Subject prereqs flow to the outer statement
///   list, before the repeat.
/// - `local _fallthrough = false` is unshifted only when some case clause can fall through (set
///   even when the falling clause is the last case clause, which declares a never-read flag;
///   verbatim).
/// - A trailing default clause is emitted inline. A default before other clauses uses a separate
///   selection pass to preserve case matching and source-order fall-through.
///
/// PORT NOTE: a TS `continue` inside a switch inside a loop emits a Luau `continue` inside the
/// repeat, which jumps to the `until` of the repeat (acting like a switch-break, not a
/// loop-continue); upstream emits this as-is, replicated byte for byte.
pub fn transform_switch_statement(state: &mut State<'_>, node: P<Node>) -> Statements {
	let switch_statement = node.as_switch_statement();

	// The `repeat ... until true` below is a break boundary, so labels must count it (rotor
	// extension).
	state.enter_break_scope();

	let mut expression = transform_expression(state, switch_statement.expression);
	if expression_might_mutate(state, expression, Some(switch_statement.expression))
		&& !can_compare_switch_identifier_directly(state, node, expression)
	{
		expression = state.push_to_var(Some(expression), "exp");
	} else {
		expression = state.push_to_var_if_complex(expression, "exp");
	}
	let clauses = switch_statement.case_block.as_case_block().clauses.nodes();
	for (index, &clause) in clauses.iter().enumerate() {
		if tsast::is_default_clause(clause) && index < clauses.len() - 1 {
			let statements = transform_switch_with_middle_default(state, clauses, expression, index);
			let condition = state.ast.bool(true);
			let repeat = state.ast.repeat_statement(condition, statements);
			let result = state.ast.list_from([repeat]);
			let checks = state.label_checks();
			state.ast.list_push_list(result, checks);
			state.exit_break_scope();
			return result;
		}
	}
	let fall_through_flag_id = state.ast.temp_id("fallthrough");

	let mut is_fall_through_flag_needed = false;

	let statements = state.ast.new_list();
	let mut can_fall_through_to = false;
	for (index, &case_clause_node) in clauses.iter().enumerate() {
		if tsast::is_case_clause(case_clause_node) {
			let should_update_fall_through_flag =
				index < clauses.len() - 1 && tsast::is_case_clause(clauses[index + 1]);
			let result = transform_case_clause(
				state,
				case_clause_node,
				expression,
				fall_through_flag_id,
				can_fall_through_to,
				should_update_fall_through_flag,
			);

			state.ast.list_push_list(statements, result.prereqs);
			state.ast.list_push_list(statements, result.clause_statements);

			can_fall_through_to = result.can_fall_through_from;

			if result.can_fall_through_from {
				is_fall_through_flag_needed = true;
			}
		} else {
			let default_statements =
				transform_statement_list(state, Some(case_clause_node), case_clause_node.statements(), None);
			state.ast.list_push_list(statements, default_statements);
			break;
		}
	}

	if is_fall_through_flag_needed {
		let value = state.ast.bool(false);
		let declaration = state.ast.variable_declaration(fall_through_flag_id, Some(value.into()));
		state.ast.list_unshift(statements, declaration);
	}

	let condition = state.ast.bool(true);
	let repeat = state.ast.repeat_statement(condition, statements);
	let result = state.ast.list_from([repeat]);
	let checks = state.label_checks();
	state.ast.list_push_list(result, checks);
	state.exit_break_scope();
	result
}
