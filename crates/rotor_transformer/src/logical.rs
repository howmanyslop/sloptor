//! `logical.go`: `nodes/transformLogical.ts`, for `&&`, `||` and `??`.

use rotor_luau::Origin;
use rotor_tsrs::ast as tsast;
use rotor_tsrs::types::TypeCheck;

use crate::diagnostics;
use crate::dispatch::transform_expression;
use crate::luau_types::luau_refined_condition;
use crate::prelude::*;
use crate::truthiness::{binary_expression_chain, create_truthiness_checks, will_create_truthiness_checks};
use crate::types::is_possibly_type;

/// `logicalChainItem` (`LogicalChainItem`, L11-16).
#[derive(Clone, Debug)]
struct LogicalChainItem {
	node: P<Node>,
	/// The token after this item, or `None` for the final item.
	operator: Option<P<Node>>,
	expression: NodeId,
	statements: Statements,
	inline: bool,
}

/// The `buildCondition` callback: given the `_condition` temp and the node just assigned to it,
/// the if-statement condition that decides whether evaluation continues.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConditionBuilder {
	/// `&&`: the truthiness checks.
	And,
	/// `||`: `not` the truthiness checks.
	Or,
	/// `??`: `_condition == nil`.
	Coalesce,
}

impl ConditionBuilder {
	fn build(self, state: &mut State<'_>, condition_id: NodeId, node: P<Node>) -> NodeId {
		match self {
			Self::And => {
				let t = state.get_type(node);
				create_truthiness_checks(state, condition_id, node, t)
			}
			Self::Or => {
				let t = state.get_type(node);
				let checks = create_truthiness_checks(state, condition_id, node, t);
				state.ast.unary(UnaryOperator::Not, checks)
			}
			Self::Coalesce => {
				let nil = state.ast.nil();
				state.ast.binary(condition_id, BinaryOperator::Equal, nil)
			}
		}
	}
}

/// `flattenByOperator` (L23-31): `a && b && c` is `[a, b, c]`. Parenthesized sub-chains do not
/// flatten (no skipDownwards, verbatim upstream).
fn flatten_by_operator(
	mut node: P<Node>,
	operator_kind: Kind,
	collect_operators: bool,
) -> (Vec<P<Node>>, Vec<P<Node>>) {
	let mut result = Vec::new();
	let mut operators = Vec::new();
	while tsast::is_binary_expression(node) && node.as_binary_expression().operator_token.kind() == operator_kind {
		let binary = node.as_binary_expression();
		result.push(binary.right.get());
		if collect_operators {
			operators.push(binary.operator_token);
		}
		node = binary.left;
	}
	result.push(node);
	result.reverse();
	operators.reverse();
	(result, operators)
}

/// `getLogicalChain` (L37-53): flatten, capture each item, mark inlineability. An item inlines
/// only without prereqs and, unless last, when Luau truthiness matches TS truthiness.
fn get_logical_chain(
	state: &mut State<'_>,
	binary_expression: P<Node>,
	operator_kind: Kind,
	enable_inlining: bool,
) -> Vec<LogicalChainItem> {
	let (nodes, operators) = flatten_by_operator(binary_expression, operator_kind, state.source_map_enabled);
	let mut chain = Vec::with_capacity(nodes.len());
	for (index, &node) in nodes.iter().enumerate() {
		let t = state.get_type(node);
		let (expression, statements) = state.capture(|state| transform_expression(state, node));
		let inline = enable_inlining && {
			let will_wrap = index < nodes.len() - 1 && will_create_truthiness_checks(state, t);
			state.ast.list_is_empty(statements) && !will_wrap
		};
		chain.push(LogicalChainItem {
			node,
			operator: operators.get(index).copied(),
			expression,
			statements,
			inline,
		});
	}
	chain
}

/// `buildLogicalChainPrereqs` (L58-94): the nested prereq statements of a non-inlined chain.
fn build_logical_chain_prereqs(
	state: &mut State<'_>,
	chain: &[LogicalChainItem],
	condition_id: NodeId,
	build_condition: ConditionBuilder,
	index: usize,
) {
	let item = &chain[index];
	state.prereq_list(item.statements);
	if index == 0 {
		let declaration = state
			.ast
			.variable_declaration(condition_id, Some(item.expression.into()));
		state.prereq(declaration);
	} else {
		let assignment = state
			.ast
			.assignment(condition_id, AssignmentOperator::Assign, item.expression);
		state.prereq(assignment);
	}
	if index + 1 < chain.len() {
		let statements = state.capture_statements(|state| {
			build_logical_chain_prereqs(state, chain, condition_id, build_condition, index + 1);
		});
		let condition = build_condition.build(state, condition_id, item.node);
		let condition = tag_condition_operator(state, condition, item.operator);
		let statement = state.ast.if_statement(condition, statements, None);
		state.prereq(statement);
	}
}

/// `tagConditionOperator`
pub fn tag_condition_operator(state: &mut State<'_>, condition: NodeId, token: Option<P<Node>>) -> NodeId {
	if state.source_map_enabled
		&& let Some(position) = state.original_source_position(token, false)
	{
		let origin = state.ast.add_origin(Origin {
			start: position,
			operator: Some(position),
			..Origin::default()
		});
		return state.ast.with_origin(condition, Some(origin));
	}
	condition
}

/// `mergeInlineExpressions` (L108-121): adjacent inline items merge into one chain item.
fn merge_inline_expressions(
	state: &mut State<'_>,
	mut chain: Vec<LogicalChainItem>,
	operator: BinaryOperator,
) -> Vec<LogicalChainItem> {
	let mut index = 0;
	while index < chain.len() {
		if chain[index].inline {
			let mut expressions = vec![chain[index].expression];
			let mut origins = Vec::new();
			let next = index + 1;
			while next < chain.len() && chain[next].inline {
				if state.source_map_enabled {
					let operator_position = state.original_source_position(chain[index].operator, false);
					origins.push(state.ast.add_origin(Origin {
						start_unset: true,
						operator: operator_position,
						..Origin::default()
					}));
				}
				expressions.push(chain[next].expression);
				chain[index].operator = chain[next].operator;
				chain.remove(next);
			}
			chain[index].expression = binary_expression_chain(&mut state.ast, &expressions, operator, &origins);
		}
		index += 1;
	}
	chain
}

/// `buildInlineConditionExpression` (L126-145): a single inline item after merging returns
/// directly; otherwise the `_condition` temp and its if chain.
fn build_inline_condition_expression(
	state: &mut State<'_>,
	node: P<Node>,
	ts_operator: Kind,
	luau_operator: BinaryOperator,
	build_condition: ConditionBuilder,
) -> NodeId {
	let chain = get_logical_chain(state, node, ts_operator, true);
	let chain = merge_inline_expressions(state, chain, luau_operator);

	// a single inline item at the end needs no temp
	if chain.len() == 1 && chain[0].inline {
		return chain[0].expression;
	}

	let condition_id = state.ast.temp_id("condition");
	build_logical_chain_prereqs(state, &chain, condition_id, build_condition, 0);
	luau_refined_condition(state, node, condition_id)
}

/// `transformLogical` (L147-167).
pub fn transform_logical(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let operator_kind = node.as_binary_expression().operator_token.kind();
	match operator_kind {
		Kind::AmpersandAmpersandToken => {
			build_inline_condition_expression(state, node, operator_kind, BinaryOperator::And, ConditionBuilder::And)
		}
		Kind::BarBarToken => {
			build_inline_condition_expression(state, node, operator_kind, BinaryOperator::Or, ConditionBuilder::Or)
		}
		Kind::QuestionQuestionToken => {
			// `a or b` is right only when the result cannot be a legitimate `false`.
			let t = state.get_type(node);
			if !is_possibly_type(state, t, &[TypeCheck::BooleanLiteral(false)]) {
				return build_inline_condition_expression(
					state,
					node,
					operator_kind,
					BinaryOperator::Or,
					ConditionBuilder::Coalesce,
				);
			}
			let chain = get_logical_chain(state, node, Kind::QuestionQuestionToken, false);
			let condition_id = state.ast.temp_id("condition");
			build_logical_chain_prereqs(state, &chain, condition_id, ConditionBuilder::Coalesce, 0);
			luau_refined_condition(state, node, condition_id)
		}
		_ => {
			state.ice(format!(
				"transformer: transformLogical operator not implemented: {}",
				diagnostics::kind_name(operator_kind)
			));
			state.ast.none()
		}
	}
}
