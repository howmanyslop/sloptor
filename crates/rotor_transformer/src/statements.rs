//! `statements.go`: variable, expression, if, block, return, break, continue and throw
//! statements.

use rotor_luau::Array;
use rotor_tsrs::ast::{self as tsast, NodeFlags};
use rotor_tsrs::types::TypeCheck;

use crate::assignment::{
	create_compound_assignment_statement, get_assignable_value, get_simple_assignment_operator,
	transform_writable_assignment, transform_writable_expression,
};
use crate::call::array_binding_pattern_contains_hoists;
use crate::dispatch::transform_expression;
use crate::ensureorder::ensure_transform_order;
use crate::exports::is_symbol_mutable;
use crate::extras::{is_break_blocked_by_try_statement, is_return_blocked_by_try_statement};
use crate::identifier::{transform_identifier_defined, validate_identifier};
use crate::prelude::*;
use crate::refwalker::for_each_symbol_reference;
use crate::spreadhelpers::{array_like_expression_contains_spread, get_target_id_for_binding_pattern};
use crate::state::{LABEL_BREAK, LABEL_CONTINUE};
use crate::statementlist::transform_statement_list;
use crate::truthiness::create_truthiness_checks;
use crate::types::{check, get_first_defined_symbol};
use crate::util::is_ancestor_of;
use crate::{control, diagnostics, expressions, expressions as destructuring, luau_types};

// -------------------------------------------------------------------------------------------------
// Variable statements (statements/transformVariableStatement.ts)
// -------------------------------------------------------------------------------------------------

/// `transformVariable` (L19-55): the identifier-binding core, shared by for-loop initializers and
/// catch clauses. With luauTypes on, a fresh `local` is annotated where Luau cannot infer it.
pub fn transform_variable(state: &mut State<'_>, identifier: P<Node>, right: Option<NodeId>) -> NodeId {
	validate_identifier(state, identifier);

	let Some(symbol) = state.ck().get_symbol_at_location(identifier) else {
		state.ice("transformer: transformVariable: no symbol");
		return state.ast.none();
	};

	// export let: declarations of mutable exported symbols write through the exports table.
	if is_symbol_mutable(state, symbol)
		&& let Some(export_access) = state.get_module_id_property_access(symbol)
	{
		if let Some(right) = right {
			let assignment = state.ast.assignment(export_access, AssignmentOperator::Assign, right);
			state.prereq(assignment);
		}
		return export_access;
	}

	let mut left = transform_identifier_defined(state, identifier);

	check_variable_hoist(state, identifier, symbol);
	if state.is_hoisted.get(&symbol).copied().unwrap_or(false) {
		// no need for `x = nil` when the variable already exists
		if let Some(right) = right {
			let assignment = state.ast.assignment(left, AssignmentOperator::Assign, right);
			state.prereq(assignment);
		}
	} else {
		if state.is_luau_typed {
			let local_type = luau_types::get_luau_local_type(state, identifier, right);
			left = luau_types::luau_typed_binding(state, symbol, left, local_type);
		}
		let declaration = state.ast.variable_declaration(left, right.map(NodeOrList::from));
		state.prereq(declaration);
	}

	left
}

/// `checkVariableHoist` (`util/checkVariableHoist.ts`): a `let`/`const` directly inside a switch
/// clause is scoped to the whole case block, so a reference from a sibling clause needs a
/// `local x` hoisted above the declaring clause and the declaration demoted to an assignment.
pub fn check_variable_hoist(state: &mut State<'_>, identifier: P<Node>, symbol: P<Symbol>) {
	if state.is_hoisted.contains_key(&symbol) {
		return;
	}

	let Some(statement) = tsast::find_ancestor(identifier, tsast::is_statement) else {
		return;
	};

	let Some(case_clause) = statement
		.parent()
		.filter(|&parent| tsast::is_case_clause(parent) || tsast::is_default_clause(parent))
	else {
		return;
	};
	let Some(case_block) = case_clause.parent() else {
		return;
	};
	// A trailing default stays inline; earlier defaults use separate body guards, so their
	// locals need the same sibling visibility as case locals.
	let clauses = case_block.as_case_block().clauses.nodes();
	if tsast::is_default_clause(case_clause) && clauses.last() == Some(&case_clause) {
		return;
	}

	let is_used_outside_of_case_clause =
		for_each_symbol_reference(state.checker, identifier, case_block, &mut |token| {
			!is_ancestor_of(case_clause, token)
		});

	if is_used_outside_of_case_clause {
		state
			.hoists_by_statement
			.entry(case_clause)
			.or_default()
			.push(identifier);
		state.is_hoisted.insert(symbol, true);
	}
}

/// `transformVariableDeclaration` (L101-167). The initializer is transformed before the name, so
/// references inside it can be hoisted. Array patterns take the `LuaTuple` direct unpack, the
/// literal-array inline, or the `_binding` fallback; object patterns always take the temp.
pub fn transform_variable_declaration(state: &mut State<'_>, declaration: P<Node>) -> Statements {
	let statements = state.ast.new_list();
	let initializer = declaration.initializer();

	let mut value = None;
	if let Some(initializer) = initializer {
		let prereqs = state.capture_statements(|state| value = Some(transform_expression(state, initializer)));
		state.ast.list_push_list(statements, prereqs);
	}

	let name = declaration.as_variable_declaration().name;
	if name.kind() == Kind::Identifier {
		let prereqs = state.capture_statements(|state| {
			transform_variable(state, name, value);
		});
		state.ast.list_push_list(statements, prereqs);
		return statements;
	}

	// in destructuring, the right side must run first
	let (Some(initializer), Some(value)) = (initializer, value) else {
		state.ice("transformer: transformVariableDeclaration: binding pattern without initializer");
		return statements;
	};

	// an empty destructure keeps only the right side's effects
	if name.as_binding_pattern().elements.nodes().is_empty() {
		let keep = state
			.ast
			.get::<Array>(value)
			.is_none_or(|array| state.ast.list_is_non_empty(array.members));
		if keep {
			let wrapped = wrap_expression_statement(&mut state.ast, value);
			state.ast.list_push_list(statements, wrapped);
		}
		return statements;
	}

	if tsast::is_array_binding_pattern(name) {
		let array_members = state.ast.get::<Array>(value).map(|array| array.members);
		if state.ast.kind(value).is_call()
			&& {
				let t = state.get_type(initializer);
				check(state, TypeCheck::LuaTuple, t)
			} && !array_binding_pattern_contains_hoists(state, name)
			&& !array_like_expression_contains_spread(name)
		{
			let optimized = destructuring::transform_optimized_array_binding_pattern(state, name, value.into());
			state.ast.list_push_list(statements, optimized);
		} else if let Some(members) = array_members
			&& state.ast.list_is_non_empty(members)
			// we can't localize multiple variables at the same time if any of them are hoisted
			&& !array_binding_pattern_contains_hoists(state, name)
			&& !array_like_expression_contains_spread(name)
		{
			let optimized = destructuring::transform_optimized_array_binding_pattern(state, name, members.into());
			state.ast.list_push_list(statements, optimized);
		} else {
			let prereqs = state.capture_statements(|state| {
				let target = get_target_id_for_binding_pattern(state, name, value);
				expressions::transform_array_binding_pattern(state, name, target);
			});
			state.ast.list_push_list(statements, prereqs);
		}
	} else {
		let prereqs = state.capture_statements(|state| {
			let target = get_target_id_for_binding_pattern(state, name, value);
			expressions::transform_object_binding_pattern(state, name, target);
		});
		state.ast.list_push_list(statements, prereqs);
	}

	statements
}

/// `isVarDeclaration` (L169-171): neither `const` nor `let`.
#[must_use]
pub fn is_var_declaration(declaration_list: P<Node>) -> bool {
	!declaration_list.flags().intersects(NodeFlags::Const | NodeFlags::Let)
}

/// `transformVariableDeclarationList` (L173-189).
pub fn transform_variable_declaration_list(state: &mut State<'_>, declaration_list: P<Node>) -> Statements {
	if is_var_declaration(declaration_list) {
		state.diags.add(diagnostics::no_var(declaration_list));
	}

	let statements = state.ast.new_list();
	for &declaration in declaration_list.as_variable_declaration_list().declarations.nodes() {
		let mut variable_statements = None;
		let prereqs = state.capture_statements(|state| {
			variable_statements = Some(transform_variable_declaration(state, declaration));
		});
		state.ast.list_push_list(statements, prereqs);
		if let Some(variable_statements) = variable_statements {
			state.ast.list_push_list(statements, variable_statements);
		}
	}

	statements
}

/// `transformVariableStatement` (L191-196).
pub fn transform_variable_statement(state: &mut State<'_>, node: P<Node>) -> Statements {
	if let Some(statements) = control::transform_matching_named_function_const(state, node) {
		return statements;
	}
	transform_variable_declaration_list(state, node.as_variable_statement().declaration_list)
}

// -------------------------------------------------------------------------------------------------
// Expression statements (statements/transformExpressionStatement.ts)
// -------------------------------------------------------------------------------------------------

/// `transformExpressionStatement` (L86-89).
pub fn transform_expression_statement(state: &mut State<'_>, node: P<Node>) -> Statements {
	let expression = tsast::skip_downwards(node.as_expression_statement().expression);
	transform_expression_statement_inner(state, expression)
}

/// `transformUnaryExpressionStatement` (L13-24): `++`/`--` as a statement needs no temp.
fn transform_unary_expression_statement(state: &mut State<'_>, expression: P<Node>) -> NodeId {
	let (operand, operator_kind) = unary_operand_and_operator(expression);
	let writable = transform_writable_expression(state, operand, false);
	let operator = if operator_kind == Kind::MinusMinusToken {
		AssignmentOperator::Subtract
	} else {
		AssignmentOperator::Add
	};
	let one = state.ast.num(1.0);
	let assignment = state.ast.assignment(writable, operator, one);
	state.set_update_operator_source_position(assignment, expression);
	assignment
}

/// The operand and operator of a prefix or postfix unary expression.
pub fn unary_operand_and_operator(node: P<Node>) -> (P<Node>, Kind) {
	if tsast::is_prefix_unary_expression(node) {
		let unary = node.as_prefix_unary_expression();
		return (unary.operand, unary.operator);
	}
	let unary = node.as_postfix_unary_expression();
	(unary.operand, unary.operator)
}

/// `isUnaryAssignmentOperator` (`typeGuards.ts`).
const fn is_unary_assignment_operator(operator: Kind) -> bool {
	matches!(operator, Kind::PlusPlusToken | Kind::MinusMinusToken)
}

/// `transformExpressionStatementInner` (L26-84): the statement-position forms of assignments and
/// `++`/`--` (no value temps), and the discard semantics of `wrapExpressionStatement` for the
/// rest. A destructuring target falls through to the binary transform.
pub fn transform_expression_statement_inner(state: &mut State<'_>, expression: P<Node>) -> Statements {
	if tsast::is_binary_expression(expression) {
		let binary = expression.as_binary_expression();
		let operator_token = binary.operator_token;
		let operator_kind = operator_token.kind();
		let (left, right) = (binary.left, binary.right.get());
		if tsast::is_logical_or_coalescing_assignment_expression(expression) {
			return expressions::transform_logical_or_coalescing_assignment_expression_statement(state, expression);
		} else if tsast::is_assignment_operator(operator_kind)
			&& !tsast::is_array_literal_expression(left)
			&& !tsast::is_object_literal_expression(left)
		{
			let writable_type = state.get_type(left);
			let value_type = state.get_type(right);
			let simple = get_simple_assignment_operator(state, writable_type, operator_kind, value_type);
			// Both flags are false for simple operators: statement position never re-reads.
			let is_simple = simple.is_some();
			let assignment = transform_writable_assignment(state, left, right, !is_simple, !is_simple);
			if let Some(operator) = simple {
				let value = get_assignable_value(state, operator, assignment.value, value_type);
				let statement = state.ast.assignment(assignment.writable, operator, value);
				if operator_kind != Kind::EqualsToken {
					state.set_operator_source_position(statement, operator_token);
				}
				return state.ast.list_from([statement]);
			}
			let statement = create_compound_assignment_statement(
				state,
				assignment.writable,
				writable_type,
				assignment.readable,
				operator_kind,
				assignment.value,
				value_type,
			);
			return state.ast.list_from([statement]);
		}
	} else if (tsast::is_prefix_unary_expression(expression)
		&& is_unary_assignment_operator(expression.as_prefix_unary_expression().operator))
		|| (tsast::is_postfix_unary_expression(expression)
			&& is_unary_assignment_operator(expression.as_postfix_unary_expression().operator))
	{
		let statement = transform_unary_expression_statement(state, expression);
		return state.ast.list_from([statement]);
	}

	let transformed = transform_expression(state, expression);
	wrap_expression_statement(&mut state.ast, transformed)
}

// -------------------------------------------------------------------------------------------------
// If statements (statements/transformIfStatement.ts)
// -------------------------------------------------------------------------------------------------

/// `getStatements` (`util/getStatements.ts`): a block's statements, or the statement itself.
#[must_use]
pub fn get_statements(statement: P<Node>) -> Vec<P<Node>> {
	if tsast::is_block(statement) {
		return statement.statements().to_vec();
	}
	vec![statement]
}

/// `transformIfStatementInner` (L9-38). An `else if` whose transform produced prereqs cannot be an
/// `elseif`, so its else body becomes `[...prereqs, if]`.
fn transform_if_statement_inner(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let statement = node.as_if_statement();
	let condition_node = statement.expression;
	let transformed = transform_expression(state, condition_node);
	let t = state.get_type(condition_node);
	let condition = create_truthiness_checks(state, transformed, condition_node, t);

	let then_statements = get_statements(statement.then_statement);
	let statements = transform_statement_list(state, Some(statement.then_statement), &then_statements, None);

	let else_body: NodeOrList = match statement.else_statement {
		None => state.ast.new_list::<element::Statement>().into(),
		Some(else_statement) if tsast::is_if_statement(else_statement) => {
			let mut else_if = None;
			let else_if_prereqs = state.capture_statements(|state| {
				else_if = Some(transform_if_statement_inner(state, else_statement));
			});
			let else_if = else_if.unwrap_or_else(|| state.ast.none());
			if state.ast.list_is_empty(else_if_prereqs) {
				else_if.into()
			} else {
				state.set_statement_list_source_positions(else_statement, &[else_if_prereqs]);
				let else_if_statements = state.ast.new_list();
				state.ast.list_push_list(else_if_statements, else_if_prereqs);
				state.ast.list_push(else_if_statements, else_if);
				else_if_statements.into()
			}
		}
		Some(else_statement) => {
			let else_statements = get_statements(else_statement);
			transform_statement_list(state, Some(else_statement), &else_statements, None).into()
		}
	};

	let result = state.ast.if_statement(condition, statements, Some(else_body));
	state.set_node_source_position(result, node);
	result
}

/// `transformIfStatement` (L40-42).
pub fn transform_if_statement(state: &mut State<'_>, node: P<Node>) -> Statements {
	let statement = transform_if_statement_inner(state, node);
	state.ast.list_from([statement])
}

/// `wrapExpressionStatement` (`util/wrapExpressionStatement.ts`): a temp or none is dropped, a
/// call stays a call statement, anything else becomes `local _ = <exp>`.
pub fn wrap_expression_statement(ast: &mut Ast, expression: NodeId) -> Statements {
	match ast.kind(expression) {
		LuauKind::TemporaryIdentifier | LuauKind::None => return ast.new_list(),
		kind if kind.is_call() => {
			let statement = ast.call_statement(expression);
			return ast.list_from([statement]);
		}
		_ => {}
	}
	let temp = ast.temp_id("");
	let declaration = ast.variable_declaration(temp, Some(expression.into()));
	ast.list_from([declaration])
}

// -------------------------------------------------------------------------------------------------
// Blocks (statements/transformBlock.ts)
// -------------------------------------------------------------------------------------------------

/// `transformBlock`: a free-standing `{ ... }` becomes `do ... end`.
pub fn transform_block(state: &mut State<'_>, node: P<Node>) -> Statements {
	let statements = transform_statement_list(state, Some(node), node.statements(), None);
	let block = state.ast.do_statement(statements);
	state.ast.list_from([block])
}

// -------------------------------------------------------------------------------------------------
// Return statements (statements/transformReturnStatement.ts)
// -------------------------------------------------------------------------------------------------

/// `isTupleReturningCall` (L10-16): a call whose own type (at the call, deliberately not through
/// `GetType`'s skip-upwards) is a `LuaTuple`.
fn is_tuple_returning_call(state: &mut State<'_>, ts_expression: P<Node>, luau_expression: NodeId) -> bool {
	state.ast.kind(luau_expression).is_call() && {
		let t = state.ck().get_type_at_location(tsast::skip_downwards(ts_expression));
		check(state, TypeCheck::LuaTuple, t)
	}
}

/// `isTupleMacro` (L18-26): a direct `$tuple(...)` call.
pub fn is_tuple_macro(state: &mut State<'_>, expression: P<Node>) -> bool {
	if tsast::is_call_expression(expression) {
		let t = state.get_type(expression.as_call_expression().expression);
		let symbol = get_first_defined_symbol(state, t);
		let tuple_symbol = state.ambient_symbol("$tuple");
		if symbol.is_some() && symbol == tuple_symbol {
			return true;
		}
	}
	false
}

/// `transformReturnStatementInner` (L28-69). `return $tuple(...)` spreads its arguments; a
/// `LuaTuple` value spreads too (array members inline, else `unpack`). A return blocked by a try
/// statement becomes `return TS.TRY_RETURN, { <values> }`.
pub fn transform_return_statement_inner(state: &mut State<'_>, return_expression: P<Node>) -> Statements {
	let result = state.ast.new_list();

	let expression: NodeOrList =
		if tsast::is_call_expression(return_expression) && is_tuple_macro(state, return_expression) {
			let mut args = Vec::new();
			let prereqs = state.capture_statements(|state| {
				args = ensure_transform_order(state, crate::call::call_arguments(return_expression));
			});
			state.ast.list_push_list(result, prereqs);
			state.ast.list_from::<element::Expression>(args).into()
		} else {
			let expression = transform_expression(state, tsast::skip_downwards(return_expression));
			let t = state.get_type(return_expression);
			if check(state, TypeCheck::LuaTuple, t) && !is_tuple_returning_call(state, return_expression, expression) {
				if let Some(array) = state.ast.get::<Array>(expression) {
					array.members.into()
				} else {
					let unpack = state.ast.global_id("unpack");
					let args = state.ast.list_from([expression]);
					state.ast.call(unpack, args).into()
				}
			} else {
				expression.into()
			}
		};

	if is_return_blocked_by_try_statement(return_expression) {
		state.mark_try_uses_return();
		let members = match expression {
			NodeOrList::List(list) => list
				.typed::<element::Expression>()
				.unwrap_or_else(|| state.ast.new_list()),
			NodeOrList::Node(node) => state.ast.list_from([node]),
		};
		let try_return = state.runtime_lib(Some(return_expression), "TRY_RETURN");
		let array = state.ast.array(members);
		let values = state.ast.list_from::<element::Expression>([try_return, array]);
		let statement = state.ast.return_statement(values);
		state.ast.list_push(result, statement);
	} else {
		let statement = state.ast.return_statement(expression);
		state.ast.list_push(result, statement);
	}
	result
}

/// `transformReturnStatement` (L71-84): a bare `return` is `return nil` (or
/// `return TS.TRY_RETURN, {}` when blocked by a try statement).
pub fn transform_return_statement(state: &mut State<'_>, node: P<Node>) -> Statements {
	let Some(expression) = node.as_return_statement().expression() else {
		if is_return_blocked_by_try_statement(node) {
			state.mark_try_uses_return();
			let try_return = state.runtime_lib(Some(node), "TRY_RETURN");
			let members = state.ast.new_list();
			let array = state.ast.array(members);
			let values = state.ast.list_from::<element::Expression>([try_return, array]);
			let statement = state.ast.return_statement(values);
			return state.ast.list_from([statement]);
		}
		let nil = state.ast.nil();
		let statement = state.ast.return_statement(nil);
		return state.ast.list_from([statement]);
	};
	transform_return_statement_inner(state, expression)
}

// -------------------------------------------------------------------------------------------------
// Break / continue (statements/transformBreakStatement.ts, transformContinueStatement.ts)
// -------------------------------------------------------------------------------------------------

/// `transformBreakStatement` (L8-25). A break blocked by a try statement becomes
/// `return TS.TRY_BREAK`.
///
/// ROTOR EXTENSION: `break <label>` on the innermost boundary is a plain `break`; further out it
/// sets the label's flag first. A label crossing a try is rejected.
pub fn transform_break_statement(state: &mut State<'_>, node: P<Node>) -> Statements {
	let blocked_by_try = is_break_blocked_by_try_statement(node);

	if let Some(label) = node.as_break_statement().label() {
		if blocked_by_try {
			state
				.diags
				.add(diagnostics::no_labeled_statements_within_try_catch(label));
			return state.ast.new_list();
		}
		let Some(entry) = state.find_label(label.text()) else {
			state.diags.add(diagnostics::rotor_unknown_label(label));
			return state.ast.new_list();
		};
		if state.label(entry).depth != state.break_depth() {
			state.label_mut(entry).ever_broken = true;
			let id = state.label(entry).id;
			let value = state.ast.str(LABEL_BREAK);
			let assignment = state.ast.assignment(id, AssignmentOperator::Assign, value);
			state.prereq(assignment);
		}
		let statement = state.ast.break_statement();
		return state.ast.list_from([statement]);
	}

	if blocked_by_try {
		state.mark_try_uses_break();
		let try_break = state.runtime_lib(Some(node), "TRY_BREAK");
		let statement = state.ast.return_statement(try_break);
		return state.ast.list_from([statement]);
	}
	let statement = state.ast.break_statement();
	state.ast.list_from([statement])
}

/// `transformContinueStatement` (L8-25): the same shape; the try reroute is
/// `return TS.TRY_CONTINUE`.
///
/// ROTOR EXTENSION: `continue <label>` on the innermost loop is a plain `continue`; otherwise the
/// flag is set and a plain `break` leaves the innermost boundary.
pub fn transform_continue_statement(state: &mut State<'_>, node: P<Node>) -> Statements {
	let blocked_by_try = is_break_blocked_by_try_statement(node);

	if let Some(label) = node.as_continue_statement().label() {
		if blocked_by_try {
			state
				.diags
				.add(diagnostics::no_labeled_statements_within_try_catch(label));
			return state.ast.new_list();
		}
		let Some(entry) = state.find_label(label.text()) else {
			state.diags.add(diagnostics::rotor_unknown_label(label));
			return state.ast.new_list();
		};
		if state.label(entry).depth == state.break_depth() && state.label(entry).is_loop {
			let statement = state.ast.continue_statement();
			return state.ast.list_from([statement]);
		}
		state.label_mut(entry).ever_continued = true;
		let id = state.label(entry).id;
		let value = state.ast.str(LABEL_CONTINUE);
		let assignment = state.ast.assignment(id, AssignmentOperator::Assign, value);
		state.prereq(assignment);
		let statement = state.ast.break_statement();
		return state.ast.list_from([statement]);
	}

	if blocked_by_try {
		state.mark_try_uses_continue();
		let try_continue = state.runtime_lib(Some(node), "TRY_CONTINUE");
		let statement = state.ast.return_statement(try_continue);
		return state.ast.list_from([statement]);
	}
	let statement = state.ast.continue_statement();
	state.ast.list_from([statement])
}

// -------------------------------------------------------------------------------------------------
// Throw statements (statements/transformThrowStatement.ts)
// -------------------------------------------------------------------------------------------------

/// `transformThrowStatement`: `throw x` is `error(x)`.
pub fn transform_throw_statement(state: &mut State<'_>, node: P<Node>) -> Statements {
	let args = state.ast.new_list();
	if let Some(expression) = node.expression() {
		let transformed = transform_expression(state, expression);
		state.ast.list_push(args, transformed);
	}
	let callee = state.ast.global_id("error");
	let call = state.ast.call(callee, args);
	let statement = state.ast.call_statement(call);
	state.ast.list_from([statement])
}
