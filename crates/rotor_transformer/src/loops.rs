//! `loops.go`: while, do-while and C-style for statements.

use std::collections::HashSet;

use rotor_luau::{DoStatement, IfStatement};
use rotor_tsrs::ast::{self as tsast, NodeFlags};

use crate::access::offset_expr;
use crate::diagnostics;
use crate::dispatch::transform_expression;
use crate::identifier::transform_identifier_defined;
use crate::loops_numeric::{
	get_constant_loop_integer, get_optimized_incrementor_step_value, is_loop_variable, is_mutated_in_body,
};
use crate::prelude::*;
use crate::refwalker::{for_each_symbol_reference, is_symbol_referenced};
use crate::statementlist::transform_statement_list;
use crate::statements::{
	get_statements, is_var_declaration, transform_expression_statement_inner, transform_variable_declaration,
};
use crate::truthiness::create_truthiness_checks;
use crate::util::is_ancestor_of;

fn break_list(ast: &mut Ast) -> Statements {
	let statement = ast.break_statement();
	ast.list_from([statement])
}

/// The truthiness-checked transform of a loop condition.
fn transform_condition(state: &mut State<'_>, condition: P<Node>) -> NodeId {
	let transformed = transform_expression(state, condition);
	let t = state.get_type(condition);
	create_truthiness_checks(state, transformed, condition, t)
}

/// `transformWhileStatement` (`transformWhileStatement.ts` L9-37). A condition with prereqs is
/// re-evaluated every iteration: the prereqs move into the body, then
/// `if not cond then break end`, and the loop condition becomes `true`.
pub fn transform_while_statement(state: &mut State<'_>, node: P<Node>) -> Statements {
	let statement = node.as_while_statement();
	state.enter_break_scope();
	let while_statements = state.ast.new_list();

	let (mut condition, condition_prereqs) = state.capture(|state| transform_condition(state, statement.expression));

	if state.ast.list_is_non_empty(condition_prereqs) {
		state.ast.list_push_list(while_statements, condition_prereqs);
		let negated = state.ast.unary(UnaryOperator::Not, condition);
		let body = break_list(&mut state.ast);
		let check = state.ast.if_statement(negated, body, None);
		state.ast.list_push(while_statements, check);
		condition = state.ast.bool(true);
	}

	let body = statement.statement();
	let body_statements = get_statements(body);
	let transformed = transform_statement_list(state, Some(body), &body_statements, None);
	state.ast.list_push_list(while_statements, transformed);

	// Label bookkeeping (rotor extension): resets go above the hoisted condition prereqs.
	let resets = state.label_resets();
	state.ast.list_unshift_list(while_statements, resets);

	let loop_statement = state.ast.while_statement(condition, while_statements);
	let result = state.ast.list_from([loop_statement]);
	let checks = state.label_checks();
	state.ast.list_push_list(result, checks);
	state.exit_break_scope();
	result
}

/// `transformDoStatement` (`transformDoStatement.ts` L9-37). The body sits in an inner
/// `do ... end` so its locals cannot collide with the condition. `do {} while (!x)` strips the
/// `!` instead of wrapping in `not` (the two inversions cancel).
pub fn transform_do_statement(state: &mut State<'_>, node: P<Node>) -> Statements {
	let do_statement = node.as_do_statement();
	let mut expression = do_statement.expression;
	state.enter_break_scope();
	let body = do_statement.statement();
	let body_statements = get_statements(body);
	let statements = transform_statement_list(state, Some(body), &body_statements, None);

	let condition_is_inverted_in_luau = !(tsast::is_prefix_unary_expression(expression)
		&& expression.as_prefix_unary_expression().operator == Kind::ExclamationToken);
	if !condition_is_inverted_in_luau {
		expression = expression.as_prefix_unary_expression().operand;
	}

	let (mut condition, condition_prereqs) = state.capture(|state| transform_condition(state, expression));

	let condition_needs_prereqs = state.ast.list_is_non_empty(condition_prereqs);

	let repeat_statements = state.ast.new_list();
	let inner = state.ast.do_statement(statements);
	state.ast.list_push(repeat_statements, inner);
	state.ast.list_push_list(repeat_statements, condition_prereqs);
	// A non-empty reset list means an inner boundary emitted a `continue` inside this repeat,
	// which Luau rejects when the condition declared locals after it.
	let resets = state.label_resets();
	if state.ast.list_is_non_empty(resets) && condition_needs_prereqs {
		state.diags.add(diagnostics::rotor_labeled_continue_in_do_while(node));
	}
	state.ast.list_unshift_list(repeat_statements, resets);

	if condition_is_inverted_in_luau {
		condition = state.ast.unary(UnaryOperator::Not, condition);
	}

	let repeat = state.ast.repeat_statement(condition, repeat_statements);
	let result = state.ast.list_from([repeat]);
	let checks = state.label_checks();
	state.ast.list_push_list(result, checks);
	state.exit_break_scope();
	result
}

/// `getDeclaredVariablesFromBindingName` (`util/getDeclaredVariables.ts`): every declared
/// identifier of a binding name.
fn get_declared_variables_from_binding_name(node: P<Node>, list: &mut Vec<P<Node>>) {
	if node.kind() == Kind::Identifier {
		list.push(node);
	} else if tsast::is_object_binding_pattern(node) {
		for &element in node.as_binding_pattern().elements.nodes() {
			if let Some(name) = element.name() {
				get_declared_variables_from_binding_name(name, list);
			}
		}
	} else if tsast::is_array_binding_pattern(node) {
		for &element in node.as_binding_pattern().elements.nodes() {
			if !tsast::is_omitted_expression(element)
				&& let Some(name) = element.name()
			{
				get_declared_variables_from_binding_name(name, list);
			}
		}
	}
}

/// `getDeclaredVariables` for a variable declaration list.
fn get_declared_variables(declaration_list: P<Node>) -> Vec<P<Node>> {
	let mut list = Vec::new();
	for &declaration in declaration_list.as_variable_declaration_list().declarations.nodes() {
		get_declared_variables_from_binding_name(declaration.as_variable_declaration().name, &mut list);
	}
	list
}

/// `canSkipClone` (L73-76): a loop variable not referenced in its own declaration list can use
/// the declared temp as the loop-carried slot.
fn can_skip_clone(state: &mut State<'_>, initializer: P<Node>, id: P<Node>) -> bool {
	!is_symbol_referenced(state.checker, id, initializer)
}

/// `isIdWriteOrAsyncRead` (L78-100): a write outside the incrementor, or a read inside a nested
/// function-like (a closure capture).
fn is_id_write_or_async_read(state: &mut State<'_>, for_statement_node: P<Node>, id: P<Node>) -> bool {
	let incrementor = for_statement_node.as_for_statement().incrementor;
	for_each_symbol_reference(state.checker, id, for_statement_node, &mut |token| {
		// write
		if tsast::is_write_access(token) && incrementor.is_none_or(|incrementor| !is_ancestor_of(incrementor, token)) {
			return true;
		}
		// async read
		let ancestor = tsast::find_ancestor(token, |candidate| {
			candidate == for_statement_node || tsast::is_function_like(candidate)
		});
		ancestor.is_some_and(|ancestor| ancestor != for_statement_node)
	})
}

/// `addFinalizersToIfStatement` (L22-33): both branches, through else-if chains.
fn add_finalizers_to_if_statement(state: &mut State<'_>, node: NodeId, finalizers: Statements) {
	let Some(if_statement) = state.ast.get::<IfStatement>(node).cloned() else {
		return;
	};
	if let Some(head) = state.ast.list_head(if_statement.statements) {
		add_finalizers(state, if_statement.statements, head, finalizers);
	}
	match if_statement.else_body {
		NodeOrList::List(list) => {
			if let Some(else_list) = list.typed::<element::Statement>()
				&& let Some(head) = state.ast.list_head(else_list)
			{
				add_finalizers(state, else_list, head, finalizers);
			}
		}
		NodeOrList::Node(else_if) => {
			if state.ast.kind(else_if) == LuauKind::IfStatement {
				add_finalizers_to_if_statement(state, else_if, finalizers);
			}
		}
	}
}

/// `addFinalizers` (L35-71): splices a clone of the finalizers (`_i = i` write-backs) before
/// every `continue`, recursing into `do` bodies and if branches but not nested loops. Parent
/// fixups mirror upstream L47.
fn add_finalizers(state: &mut State<'_>, list: Statements, mut link: rotor_luau::ListNodeId, finalizers: Statements) {
	loop {
		let statement = state.ast.link_value(link);
		if state.ast.kind(statement) == LuauKind::ContinueStatement {
			let clone = state.ast.list_clone(finalizers);
			// fix node parents
			let parent = state.ast.parent(statement);
			for value in state.ast.list_to_vec(clone) {
				state.ast.set_parent(value, parent);
			}
			state.ast.list_splice_before(list, link, clone);
		}

		if let Some(do_statement) = state.ast.get::<DoStatement>(statement) {
			let statements = do_statement.statements;
			if let Some(head) = state.ast.list_head(statements) {
				add_finalizers(state, statements, head, finalizers);
			}
		} else if state.ast.kind(statement) == LuauKind::IfStatement {
			add_finalizers_to_if_statement(state, statement, finalizers);
		}

		match state.ast.link_next(link) {
			Some(next) => link = next,
			None => return,
		}
	}
}

/// `transformForStatementFallback` (L102-297): the general while-loop lowering with the
/// per-iteration `let` copies. A loop variable written in the body or captured by a closure gets
/// a loop-carried slot (`local _i = 0`), a copy at the body head (`local i = _i`) and `_i = i`
/// finalizers at the body tail and before every `continue`.
#[expect(
	clippy::too_many_lines,
	reason = "one block per upstream section, kept in upstream order"
)]
fn transform_for_statement_fallback(state: &mut State<'_>, node: P<Node>) -> Statements {
	let for_statement = node.as_for_statement();
	let (initializer, condition, incrementor) = (
		for_statement.initializer,
		for_statement.condition,
		for_statement.incrementor,
	);
	let statement = for_statement.statement();

	let result = state.ast.new_list();
	let while_statements = state.ast.new_list();
	let finalizer_statements = state.ast.new_list();

	// Reference analyses (L109-124).
	let mut variables = Vec::new();
	let mut has_write_or_async_read = HashSet::new();
	let mut skip_clone = HashSet::new();

	if let Some(initializer) = initializer
		&& tsast::is_variable_declaration_list(initializer)
	{
		variables = get_declared_variables(initializer);
		for &id in &variables {
			let Some(symbol) = symbol_of(state, id) else {
				return result;
			};
			if is_id_write_or_async_read(state, node, id) {
				has_write_or_async_read.insert(symbol);
			}
			if can_skip_clone(state, initializer, id) {
				skip_clone.insert(symbol);
			}
		}
	}

	if let Some(initializer) = initializer {
		if tsast::is_variable_declaration_list(initializer) {
			if is_var_declaration(initializer) {
				state.diags.add(diagnostics::no_var(node));
			}

			// Pre-bind the symbols that need copies to temps (L132-143), so the declaration
			// emits `local _i = 0` or `local _iCopy = 0`.
			for &id in &variables {
				let Some(symbol) = symbol_of(state, id) else {
					return result;
				};
				if has_write_or_async_read.contains(&symbol) {
					let name = if skip_clone.contains(&symbol) {
						id.text().to_owned()
					} else {
						format!("{}Copy", id.text())
					};
					let temp = state.ast.temp_id(name);
					state.symbol_to_id.insert(symbol, temp);
				}
			}

			// each declaration's prereqs land before its statements (L145-157)
			for &declaration in initializer.as_variable_declaration_list().declarations.nodes() {
				let mut declaration_statements = None;
				let declaration_prereqs = state.capture_statements(|state| {
					declaration_statements = Some(transform_variable_declaration(state, declaration));
				});
				state.ast.list_push_list(result, declaration_prereqs);
				if let Some(declaration_statements) = declaration_statements {
					state.ast.list_push_list(result, declaration_statements);
				}
			}

			// copies and finalizers (L159-203)
			for &id in &variables {
				let Some(symbol) = symbol_of(state, id) else {
					return result;
				};
				if !has_write_or_async_read.contains(&symbol) {
					continue;
				}
				let temp_id = if skip_clone.contains(&symbol) {
					let Some(&temp_id) = state.symbol_to_id.get(&symbol) else {
						state.ice("transformer: transformForStatementFallback: missing temp id");
						return result;
					};
					temp_id
				} else {
					let temp_id = state.ast.temp_id(id.text());
					let Some(&copy_id) = state.symbol_to_id.get(&symbol) else {
						state.ice("transformer: transformForStatementFallback: missing copy id");
						return result;
					};
					// local _i = _iCopy
					let declaration = state.ast.variable_declaration(temp_id, Some(copy_id.into()));
					state.ast.list_push(result, declaration);
					temp_id
				};
				state.symbol_to_id.remove(&symbol);
				let real_id = transform_identifier_defined(state, id);

				// local i = _i
				let copy = state.ast.variable_declaration(real_id, Some(temp_id.into()));
				state.ast.list_push(while_statements, copy);

				// _i = i
				let finalizer = state.ast.assignment(temp_id, AssignmentOperator::Assign, real_id);
				state.ast.list_push(finalizer_statements, finalizer);
			}
		} else {
			// an expression initializer (L204-208)
			let mut statements = None;
			let prereqs = state.capture_statements(|state| {
				statements = Some(transform_expression_statement_inner(state, initializer));
			});
			state.ast.list_push_list(result, prereqs);
			if let Some(statements) = statements {
				state.ast.list_push_list(result, statements);
			}
		}
	}

	// The incrementor (L211-247) runs at the top of every iteration but the first, so
	// `continue` still triggers it.
	if let Some(incrementor) = incrementor {
		let should_increment = state.ast.temp_id("shouldIncrement");

		// local _shouldIncrement = false
		let initial = state.ast.bool(false);
		let declaration = state.ast.variable_declaration(should_increment, Some(initial.into()));
		state.ast.list_push(result, declaration);

		let incrementor_statements = state.ast.new_list();
		let mut statements = None;
		let prereqs = state.capture_statements(|state| {
			statements = Some(transform_expression_statement_inner(state, incrementor));
		});
		state.ast.list_push_list(incrementor_statements, prereqs);
		if let Some(statements) = statements {
			state.ast.list_push_list(incrementor_statements, statements);
		}

		let set_true = state.ast.bool(true);
		let assignment = state
			.ast
			.assignment(should_increment, AssignmentOperator::Assign, set_true);
		let else_body = state.ast.list_from::<element::Statement>([assignment]);
		let guard = state
			.ast
			.if_statement(should_increment, incrementor_statements, Some(else_body.into()));
		state.ast.list_push(while_statements, guard);
	}

	// The condition (L249-274): when anything precedes the body, it is checked after it with
	// `if not cond then break end` and the loop condition becomes `true`.
	let (mut condition_expression, condition_prereqs) = state.capture(|state| match condition {
		Some(condition) => transform_condition(state, condition),
		None => state.ast.bool(true),
	});

	state.ast.list_push_list(while_statements, condition_prereqs);

	if state.ast.list_is_non_empty(while_statements) {
		if condition.is_some() {
			let negated = state.ast.unary(UnaryOperator::Not, condition_expression);
			let body = break_list(&mut state.ast);
			let check = state.ast.if_statement(negated, body, None);
			state.ast.list_push(while_statements, check);
		}
		condition_expression = state.ast.bool(true);
	}

	let body_statements = get_statements(statement);
	let body = transform_statement_list(state, Some(statement), &body_statements, None);
	state.ast.list_push_list(while_statements, body);

	// Finalizers (L278-284): before every `continue`, then at the tail unless the body ends in a
	// final statement.
	if state.ast.list_is_non_empty(finalizer_statements)
		&& let Some(head) = state.ast.list_head(while_statements)
	{
		add_finalizers(state, while_statements, head, finalizer_statements);
	}

	let ends_final = state
		.ast
		.list_tail(while_statements)
		.is_some_and(|tail| state.ast.kind(state.ast.link_value(tail)).is_final_statement());
	if !ends_final {
		state.ast.list_push_list(while_statements, finalizer_statements);
	}

	// Label resets go at the very top, above the incrementor guard.
	let resets = state.label_resets();
	state.ast.list_unshift_list(while_statements, resets);

	let loop_statement = state.ast.while_statement(condition_expression, while_statements);
	state.ast.list_push(result, loop_statement);

	// Several statements wrap in `do ... end` to scope the loop variables (L286-296).
	if state.ast.list_head(result) == state.ast.list_tail(result) {
		return result;
	}
	let block = state.ast.do_statement(result);
	state.ast.list_from([block])
}

fn symbol_of(state: &mut State<'_>, id: P<Node>) -> Option<P<Symbol>> {
	let symbol = state.ck().get_symbol_at_location(id);
	if symbol.is_none() {
		state.ice("transformer: transformForStatementFallback: no symbol");
	}
	symbol
}

/// `transformForStatementOptimized`: a numeric for, when every header expression is a stable
/// integer and the body leaves the induction variable alone.
fn transform_for_statement_optimized(state: &mut State<'_>, node: P<Node>) -> Option<Statements> {
	let for_statement = node.as_for_statement();
	let (initializer, condition, incrementor) = (
		for_statement.initializer?,
		for_statement.condition,
		for_statement.incrementor,
	);
	let statement = for_statement.statement();

	// one block-scoped induction variable
	if !tsast::is_variable_declaration_list(initializer) || !initializer.flags().intersects(NodeFlags::Let) {
		return None;
	}
	let declarations = initializer.as_variable_declaration_list().declarations.nodes();
	let [declaration] = declarations else {
		return None;
	};
	let declaration_name = declaration.as_variable_declaration().name;
	let declaration_initializer = declaration.initializer()?;
	if declaration_name.kind() != Kind::Identifier {
		return None;
	}

	let id_symbol = state.ck().get_symbol_at_location(declaration_name)?;

	get_constant_loop_integer(state, declaration_initializer, false)?;

	// a numeric for cannot represent a zero or changing step
	let incrementor = incrementor?;
	let step_value = get_optimized_incrementor_step_value(state, incrementor, id_symbol)?;
	if step_value == 0.0 {
		return None;
	}

	// the condition is a binary expression whose operator matches the incrementor
	let condition = tsast::skip_downwards(condition?);
	if !tsast::is_binary_expression(condition) {
		return None;
	}
	let condition_binary = condition.as_binary_expression();
	if !is_loop_variable(state, condition_binary.left, id_symbol) {
		return None;
	}
	let operator_kind = condition_binary.operator_token.kind();

	match operator_kind {
		// do not optimize loops that never run, like `for (let i = 10; i < 0; i--)`
		Kind::LessThanToken | Kind::LessThanEqualsToken if step_value < 0.0 => return None,
		// or `for (let i = 0; i > 10; i++)`
		Kind::GreaterThanToken | Kind::GreaterThanEqualsToken if step_value > 0.0 => return None,
		Kind::LessThanToken | Kind::LessThanEqualsToken | Kind::GreaterThanToken | Kind::GreaterThanEqualsToken => {}
		// nor other comparisons like !== or ===
		_ => return None,
	}

	let condition_right = condition_binary.right.get();
	get_constant_loop_integer(state, condition_right, false)?;

	if is_mutated_in_body(state, declaration_name, statement) {
		return None;
	}

	// commit to the optimization
	let result = state.ast.new_list();

	let id = transform_identifier_defined(state, declaration_name);

	let (start, start_prereqs) = state.capture(|state| transform_expression(state, declaration_initializer));
	state.ast.list_push_list(result, start_prereqs);

	let (mut end, end_prereqs) = state.capture(|state| transform_expression(state, condition_right));
	state.ast.list_push_list(result, end_prereqs);

	let step = state.ast.num(step_value);
	let body_statements = get_statements(statement);
	let statements = transform_statement_list(state, Some(statement), &body_statements, None);
	let resets = state.label_resets();
	state.ast.list_unshift_list(statements, resets);

	match operator_kind {
		Kind::LessThanToken => end = offset_expr(&mut state.ast, end, -1.0),
		Kind::GreaterThanToken => end = offset_expr(&mut state.ast, end, 1.0),
		_ => {}
	}

	let loop_statement = state.ast.numeric_for(id, start, end, Some(step), statements);
	state.ast.list_push(result, loop_statement);

	Some(result)
}

/// `transformForStatement` (L491-499): the optimized pass is gated on `optimizedLoops`. The break
/// scope wraps both paths so label bookkeeping stays balanced when the optimized pass bails (it
/// bails before transforming anything).
pub fn transform_for_statement(state: &mut State<'_>, node: P<Node>) -> Statements {
	state.enter_break_scope();

	let optimized = if state.optimized_loops {
		transform_for_statement_optimized(state, node)
	} else {
		None
	};
	let result = optimized.unwrap_or_else(|| transform_for_statement_fallback(state, node));
	let checks = state.label_checks();
	state.ast.list_push_list(result, checks);
	state.exit_break_scope();
	result
}
