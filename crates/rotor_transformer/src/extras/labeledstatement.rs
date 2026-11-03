//! `labeledstatement.go`, a rotor extension (rbxtsc 3.0.0 rejects labels with
//! `noLabeledStatement`; roblox-ts PR #2928 is the design this follows).
//!
//! Luau has neither labels nor goto, so a TS label lowers to a string flag variable holding
//! `"none" | "break" | "continue"`:
//!
//! - `break <name>` / `continue <name>` that targets the innermost break boundary is a plain
//!   Luau `break` / `continue`, with no flag;
//! - otherwise the flag is assigned and a plain `break` leaves the innermost boundary, after
//!   which the checks from [`State::label_checks`] re-break (unwinding outwards) or continue at
//!   the labeled loop.
//!
//! A label on something that is not a loop needs a break boundary of its own, so it is wrapped
//! in `repeat ... until true`, the same shape the switch transform uses.

use rotor_tsrs::ast::{self as tsast};

use crate::diagnostics;
use crate::dispatch::transform_statement;
use crate::prelude::*;
use crate::statementlist::transform_statement_list;
use crate::statements::get_statements;

/// `transformLabeledStatement`: lowers `name: <statement>`.
pub fn transform_labeled_statement(state: &mut State<'_>, node: P<Node>) -> Statements {
	let labeled = node.as_labeled_statement();
	let inner = labeled.statement;

	// `a: b: for (...)` labels the same loop twice, so look through nested labels: both entries
	// may be targeted by `continue`.
	let entry = state.push_label(labeled.label.text(), tsast::is_iteration_statement(inner, true));

	let result = state.ast.new_list();

	if tsast::is_iteration_statement(inner, false) || tsast::is_labeled_statement(inner) {
		// The loop (or the next label down) opens its own break boundary and emits its own
		// checks; nothing to wrap.
		let statements = transform_statement(state, inner);
		state.ast.list_push_list(result, statements);
	} else {
		if wrapper_captures_flow_control(inner) {
			state.diags.add(diagnostics::rotor_labeled_statement_flow_control(node));
		}

		state.enter_break_scope();
		// Through TransformStatementList rather than TransformStatement, so a labeled block
		// dissolves into the repeat body instead of nesting a redundant `do ... end`, and any
		// prereqs the inner statement produces stay inside the boundary.
		let statements = transform_statement_list(state, Some(inner), &get_statements(inner), None);
		let checks = state.label_checks();
		state.exit_break_scope();

		let until = state.ast.bool(true);
		let repeat = state.ast.repeat_statement(until, statements);
		state.ast.list_push(result, repeat);
		state.ast.list_push_list(result, checks);
	}

	// Both the declaration and (in the loop transforms) the per-iteration reset are decided
	// here, after the body is transformed: an unused label costs nothing.
	let label = state.label(entry);
	if label.used() {
		let id = label.id;
		let declaration = state.ast.variable_declaration(id, None);
		state.ast.list_unshift(result, declaration);
	}

	state.pop_label();
	result
}

/// `wrapperCapturesFlowControl`: whether the `repeat ... until true` wrapping a labeled
/// non-loop statement would swallow an unlabeled `break` or `continue` that TypeScript binds to
/// an enclosing loop or switch. Nested loops are skipped whole and nested switches are searched
/// for `continue` only; function and class bodies are skipped.
fn wrapper_captures_flow_control(node: P<Node>) -> bool {
	fn visit(node: P<Node>, continue_only: bool) -> bool {
		if tsast::is_function_like(node) || tsast::is_class_like(node) || tsast::is_iteration_statement(node, false) {
			return false;
		}
		if tsast::is_switch_statement(node) {
			return node.for_each_child(&mut |child| visit(child, true));
		}
		match node.kind() {
			Kind::BreakStatement => !continue_only && node.as_break_statement().label().is_none(),
			Kind::ContinueStatement => node.as_continue_statement().label().is_none(),
			_ => node.for_each_child(&mut |child| visit(child, continue_only)),
		}
	}
	visit(node, false)
}
