//! `trystatement.go`: `statements/transformTryStatement.ts`. The two
//! `util/isBlockedByTryStatement.ts` predicates live in `extras/mod.rs`, where core reaches them.
//!
//! Runtime protocol (`TS.try`): `TS.try(tryFn, catchFn|nil, finallyFn?)` returns
//! `exitType, returns`. The callbacks return nothing on normal completion,
//! `TS.TRY_BREAK`/`TS.TRY_CONTINUE` for rerouted break/continue, or `TS.TRY_RETURN, { ... }` for
//! a rerouted return.

use rotor_tsrs::ast::{self as tsast};

use super::{is_break_blocked_by_try_statement, is_return_blocked_by_try_statement};
use crate::expressions::transform_binding_name;
use crate::prelude::*;
use crate::state::TryUses;
use crate::statementlist::transform_statement_list;

/// The statements of a block, through `TransformStatementList` with the block as parent.
fn transform_block(state: &mut State<'_>, block: P<Node>) -> Statements {
	transform_statement_list(state, Some(block), block.statements(), None)
}

/// `transformCatchClause` (L13-30): `catch (e)` is `function(e)`; `catch` is `function()`.
/// Binding pattern destructuring lands before the block's statements.
fn transform_catch_clause(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let clause = node.as_catch_clause();
	let parameters = state.ast.new_list();
	let statements = state.ast.new_list();

	if let Some(variable_declaration) = clause.variable_declaration
		&& let Some(name) = variable_declaration.name()
	{
		let parameter = transform_binding_name(state, name, statements);
		state.ast.list_push(parameters, parameter);
	}

	let block = transform_block(state, clause.block);
	state.ast.list_push_list(statements, block);

	let callback = state.ast.function_expression(parameters, false, statements);
	state.set_node_source_position(callback, node);
	callback
}

/// `transformIntoTryCall` (L32-76). try+catch passes two callbacks; try+finally passes an
/// explicit `nil` catch. The try uses are read after the blocks are transformed: this pops the
/// entry the caller pushed, which Go pops right after this returns.
fn transform_into_try_call(
	state: &mut State<'_>,
	node: P<Node>,
	exit_type_id: NodeId,
	returns_id: NodeId,
) -> (NodeId, TryUses) {
	let try_statement = node.as_try_statement();
	let try_call_args = state.ast.new_list();

	let try_statements = transform_block(state, try_statement.try_block);
	let params = state.ast.new_list();
	let try_callback = state.ast.function_expression(params, false, try_statements);
	state.set_node_source_position(try_callback, try_statement.try_block);
	state.ast.list_push(try_call_args, try_callback);

	if let Some(catch_clause) = try_statement.catch_clause {
		let catch_callback = transform_catch_clause(state, catch_clause);
		state.ast.list_push(try_call_args, catch_callback);
	} else {
		if try_statement.finally_block.is_none() {
			state.ice("transformer: try statement without catch or finally");
		}
		let nil = state.ast.nil();
		state.ast.list_push(try_call_args, nil);
	}

	if let Some(finally_block) = try_statement.finally_block {
		let finally_statements = transform_block(state, finally_block);
		let params = state.ast.new_list();
		let finally_callback = state.ast.function_expression(params, false, finally_statements);
		state.set_node_source_position(finally_callback, finally_block);
		state.ast.list_push(try_call_args, finally_callback);
	}

	let try_uses = state.pop_try_uses_stack();

	if !try_uses.uses_return && !try_uses.uses_break && !try_uses.uses_continue {
		let try_ = state.runtime_lib(Some(node), "try");
		let call = state.ast.call(try_, try_call_args);
		return (state.ast.call_statement(call), try_uses);
	}

	let ids = state
		.ast
		.list_from::<element::AnyIdentifier>([exit_type_id, returns_id]);
	let try_ = state.runtime_lib(Some(node), "try");
	let call = state.ast.call(try_, try_call_args);
	(state.ast.variable_declaration(ids, Some(call.into())), try_uses)
}

/// `createFlowControlCondition` (L78-85): `exitTypeId == TS.TRY_RETURN|TRY_BREAK|TRY_CONTINUE`.
fn create_flow_control_condition(
	state: &mut State<'_>,
	node: P<Node>,
	exit_type_id: NodeId,
	flow_control_constant: &str,
) -> NodeId {
	let constant = state.runtime_lib(Some(node), flow_control_constant);
	state.ast.binary(exit_type_id, BinaryOperator::Equal, constant)
}

/// `flowControlCase`: no condition means an unconditional tail (guarded by `exitTypeId` when
/// normal completion is possible).
struct FlowControlCase {
	condition: Option<NodeId>,
	statements: Statements,
}

/// `collapseFlowControlCases`: an elseif chain. The last case checks only `exitTypeId` unless
/// normal completion has been ruled out, when it is unconditional.
fn collapse_flow_control_cases(
	state: &mut State<'_>,
	exit_type_id: NodeId,
	cases: &[FlowControlCase],
	always_exits: bool,
) -> Statements {
	let Some((final_case, rest)) = cases.split_last() else {
		state.ice("transformer: collapseFlowControlCases: no cases");
		return state.ast.new_list();
	};

	let mut next = if always_exits {
		NodeOrList::from(final_case.statements)
	} else {
		let empty = state.ast.new_list::<element::Statement>();
		NodeOrList::from(
			state
				.ast
				.if_statement(exit_type_id, final_case.statements, Some(empty.into())),
		)
	};

	for case in rest.iter().rev() {
		let condition = case.condition.unwrap_or(exit_type_id);
		next = NodeOrList::from(state.ast.if_statement(condition, case.statements, Some(next)));
	}

	match next {
		NodeOrList::Node(statement) => state.ast.list_from([statement]),
		NodeOrList::List(list) => list.typed().unwrap_or_else(|| state.ast.new_list()),
	}
}

/// `statementAlwaysExits` is deliberately syntactic: a call typed never can still return at
/// runtime. Loops and switches are not exits from their containing block.
fn statement_always_exits(node: Option<P<Node>>) -> bool {
	let Some(node) = node else {
		return false;
	};
	match node.kind() {
		Kind::ReturnStatement | Kind::ThrowStatement | Kind::BreakStatement | Kind::ContinueStatement => true,
		Kind::IfStatement => {
			let statement = node.as_if_statement();
			statement_always_exits(Some(statement.then_statement)) && statement_always_exits(statement.else_statement)
		}
		Kind::Block => node
			.statements()
			.iter()
			.any(|&statement| statement_always_exits(Some(statement))),
		_ => false,
	}
}

fn try_always_exits(node: P<Node>) -> bool {
	let statement = node.as_try_statement();
	if !statement_always_exits(Some(statement.try_block))
		|| statement
			.catch_clause
			.is_some_and(|clause| !statement_always_exits(Some(clause.as_catch_clause().block)))
	{
		return false;
	}
	// Keep the guarded output when more statements follow. In particular, a bare Luau return
	// must be the last statement, including in generated blocks.
	let Some(parent) = node.parent() else {
		return false;
	};
	if tsast::is_if_statement(parent)
		|| tsast::is_iteration_statement(parent, false)
		|| tsast::is_labeled_statement(parent)
	{
		return true;
	}
	let statements = match parent.kind() {
		Kind::Block | Kind::CaseClause | Kind::DefaultClause => parent.statements(),
		_ => return false,
	};
	statements.last() == Some(&node)
}

/// `transformFlowControl` (L109-186): the post-try dispatch on `_exitType`. The blocked checks
/// start at the try's parent. The propagation marks land on the enclosing try's uses, because
/// the caller popped this try's entry first.
fn transform_flow_control(
	state: &mut State<'_>,
	node: P<Node>,
	exit_type_id: NodeId,
	returns_id: NodeId,
	try_uses: TryUses,
) -> Statements {
	let mut flow_control_cases = Vec::new();
	let always_exits = try_always_exits(node);

	if !try_uses.uses_return && !try_uses.uses_break && !try_uses.uses_continue {
		return state.ast.new_list();
	}

	let parent = node.parent();
	let return_blocked = parent.is_some_and(is_return_blocked_by_try_statement);
	let break_blocked = parent.is_some_and(is_break_blocked_by_try_statement);

	if try_uses.uses_return && return_blocked {
		state.mark_try_uses_return();
	}
	if try_uses.uses_break && break_blocked {
		state.mark_try_uses_break();
	}
	if try_uses.uses_continue && break_blocked {
		state.mark_try_uses_continue();
	}

	if try_uses.uses_return {
		let condition = create_flow_control_condition(state, node, exit_type_id, "TRY_RETURN");
		if return_blocked {
			let ids = state.ast.list_from::<element::Expression>([exit_type_id, returns_id]);
			let statement = state.ast.return_statement(ids);
			flow_control_cases.push(FlowControlCase {
				condition: Some(condition),
				statements: state.ast.list_from([statement]),
			});
			if break_blocked {
				return collapse_flow_control_cases(state, exit_type_id, &flow_control_cases, always_exits);
			}
		} else {
			let unpack = state.ast.global_id("unpack");
			let args = state.ast.list_from([returns_id]);
			let call = state.ast.call(unpack, args);
			let statement = state.ast.return_statement(call);
			flow_control_cases.push(FlowControlCase {
				condition: Some(condition),
				statements: state.ast.list_from([statement]),
			});
		}
	}

	if try_uses.uses_break || try_uses.uses_continue {
		if break_blocked {
			let statement = state.ast.return_statement(exit_type_id);
			flow_control_cases.push(FlowControlCase {
				condition: None,
				statements: state.ast.list_from([statement]),
			});
		} else {
			if try_uses.uses_break {
				let condition = create_flow_control_condition(state, node, exit_type_id, "TRY_BREAK");
				let statement = state.ast.break_statement();
				flow_control_cases.push(FlowControlCase {
					condition: Some(condition),
					statements: state.ast.list_from([statement]),
				});
			}
			if try_uses.uses_continue {
				let condition = create_flow_control_condition(state, node, exit_type_id, "TRY_CONTINUE");
				let statement = state.ast.continue_statement();
				flow_control_cases.push(FlowControlCase {
					condition: Some(condition),
					statements: state.ast.list_from([statement]),
				});
			}
		}
	}

	collapse_flow_control_cases(state, exit_type_id, &flow_control_cases, always_exits)
}

/// `transformTryStatement` (L188-200). Ordering is load-bearing twice: the exit type and returns
/// temps are created before the body is transformed (so an outer try gets the unsuffixed names),
/// and this try's uses entry is popped before `transformFlowControl`, so the propagation marks
/// land on the enclosing try.
pub fn transform_try_statement(state: &mut State<'_>, node: P<Node>) -> Statements {
	let statements = state.ast.new_list();
	let exit_type_id = state.ast.temp_id("exitType"); // created first: temp numbering parity
	let returns_id = state.ast.temp_id("returns"); // created second

	state.push_try_uses_stack();
	// ROTOR EXTENSION (loop labels): TS.try's three callbacks are Luau functions, so they are
	// label barriers like any other function body.
	let (try_call, try_uses) =
		state.without_labels(|state| transform_into_try_call(state, node, exit_type_id, returns_id));
	state.ast.list_push(statements, try_call);

	let flow_control = transform_flow_control(state, node, exit_type_id, returns_id, try_uses);
	state.ast.list_push_list(statements, flow_control);

	statements
}
