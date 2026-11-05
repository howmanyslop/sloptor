//! `binary.go`: `expressions/transformBinaryExpression.ts` and `util/createBinaryFromOperator.ts`.

use rotor_luau::{Array, Map};
use rotor_tsrs::ast as tsast;
use rotor_tsrs::types::TypeCheck;

use crate::access::validate_not_any_type;
use crate::assignment::{
	create_assignment_expression, create_compound_assignment_expression, get_assignable_value,
	get_simple_assignment_operator, transform_writable_assignment,
};
use crate::bitwise::{create_bitwise_call, create_bitwise_from_operator, is_bitwise_operator};
use crate::call::convert_to_indexable_expression;
use crate::conditional::is_used_as_statement;
use crate::dispatch::transform_expression;
use crate::ensureorder::ensure_transform_order;
use crate::logical::transform_logical;
use crate::prelude::*;
use crate::spreadhelpers::array_like_expression_contains_spread;
use crate::statements::wrap_expression_statement;
use crate::types::{check, is_definitely_type, is_possibly_type};
use crate::{diagnostics, expressions};

/// `operatorMap` (`createBinaryFromOperator.ts` L9-24): TS operators with a 1:1 Luau operator.
const fn simple_operator(kind: Kind) -> Option<BinaryOperator> {
	Some(match kind {
		// comparison
		Kind::LessThanToken => BinaryOperator::Less,
		Kind::GreaterThanToken => BinaryOperator::Greater,
		Kind::LessThanEqualsToken => BinaryOperator::LessEqual,
		Kind::GreaterThanEqualsToken => BinaryOperator::GreaterEqual,
		Kind::EqualsEqualsEqualsToken => BinaryOperator::Equal,
		Kind::ExclamationEqualsEqualsToken => BinaryOperator::NotEqual,
		// math
		Kind::MinusToken => BinaryOperator::Subtract,
		Kind::AsteriskToken => BinaryOperator::Multiply,
		Kind::SlashToken => BinaryOperator::Divide,
		Kind::AsteriskAsteriskToken => BinaryOperator::Power,
		Kind::PercentToken => BinaryOperator::Modulo,
		_ => return None,
	})
}

/// `bitwiseOperatorMap` (L26-42): TS bitwise tokens, compound forms included, to `bit32` names.
pub const fn bitwise_operator_name(kind: Kind) -> Option<&'static str> {
	Some(match kind {
		Kind::AmpersandToken | Kind::AmpersandEqualsToken => "band",
		Kind::BarToken | Kind::BarEqualsToken => "bor",
		Kind::CaretToken | Kind::CaretEqualsToken => "bxor",
		Kind::LessThanLessThanToken | Kind::LessThanLessThanEqualsToken => "lshift",
		Kind::GreaterThanGreaterThanGreaterThanToken | Kind::GreaterThanGreaterThanGreaterThanEqualsToken => "rshift",
		Kind::GreaterThanGreaterThanToken | Kind::GreaterThanGreaterThanEqualsToken => "arshift",
		_ => return None,
	})
}

/// `createBinaryAdd` (L44-56): `..` with `tostring()` around any side that is not definitely a
/// string when either side is one, else `+`.
fn create_binary_add(
	state: &mut State<'_>,
	mut left: NodeId,
	left_type: P<Type>,
	mut right: NodeId,
	right_type: P<Type>,
) -> NodeId {
	let left_is_string = is_definitely_type(state, left_type, &[TypeCheck::String]);
	let right_is_string = is_definitely_type(state, right_type, &[TypeCheck::String]);
	if left_is_string || right_is_string {
		if !left_is_string {
			left = tostring_call(&mut state.ast, left);
		}
		if !right_is_string {
			right = tostring_call(&mut state.ast, right);
		}
		return state.ast.binary(left, BinaryOperator::Concat, right);
	}
	state.ast.binary(left, BinaryOperator::Add, right)
}

/// `tostring(<value>)`
pub fn tostring_call(ast: &mut Ast, value: NodeId) -> NodeId {
	let callee = ast.global_id("tostring");
	let args = ast.list_from([value]);
	ast.call(callee, args)
}

/// `createBinaryFromOperator` (L58-90): the simple map, then plus, then a bitwise call, then
/// comma.
pub fn create_binary_from_operator(
	state: &mut State<'_>,
	left: NodeId,
	left_type: P<Type>,
	operator_kind: Kind,
	right: NodeId,
	right_type: P<Type>,
) -> NodeId {
	// simple
	if let Some(operator) = simple_operator(operator_kind) {
		return state.ast.binary(left, operator, right);
	}

	// plus
	if matches!(operator_kind, Kind::PlusToken | Kind::PlusEqualsToken) {
		return create_binary_add(state, left, left_type, right, right_type);
	}

	// bitwise
	if bitwise_operator_name(operator_kind).is_some() {
		return create_bitwise_call(state, operator_kind, vec![left, right]);
	}

	if operator_kind == Kind::CommaToken {
		let statements = wrap_expression_statement(&mut state.ast, left);
		state.prereq_list(statements);
		return right;
	}

	state.ice(format!(
		"transformer: createBinaryFromOperator unknown operator: {}",
		diagnostics::kind_name(operator_kind)
	));
	state.ast.none()
}

/// `transformBinaryExpression` (L113-253).
#[expect(
	clippy::too_many_lines,
	reason = "one block per upstream branch, kept in upstream order"
)]
pub fn transform_binary_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let expression = node.as_binary_expression();
	let operator_token = expression.operator_token;
	let operator_kind = operator_token.kind();
	let (left_node, right_node) = (expression.left, expression.right.get());

	validate_not_any_type(state, left_node);
	validate_not_any_type(state, right_node);

	// banned
	match operator_kind {
		Kind::EqualsEqualsToken => {
			state.diags.add(diagnostics::no_equals_equals(node));
			return state.ast.none();
		}
		Kind::ExclamationEqualsToken => {
			state.diags.add(diagnostics::no_exclamation_equals(node));
			return state.ast.none();
		}
		_ => {}
	}

	// logical
	if matches!(
		operator_kind,
		Kind::AmpersandAmpersandToken | Kind::BarBarToken | Kind::QuestionQuestionToken
	) {
		return transform_logical(state, node);
	}

	// logical assignment (&&=, ||=, ??=) in expression position
	if tsast::is_logical_or_coalescing_assignment_expression(node) {
		return expressions::transform_logical_or_coalescing_assignment_expression(state, node);
	}

	if tsast::is_assignment_operator(operator_kind) {
		// Destructuring assignment (upstream L141-184): the right side runs first.
		if tsast::is_array_literal_expression(left_node) {
			let right = transform_expression(state, right_node);

			// optimize empty array destructure
			if left_node.as_array_literal_expression().elements.nodes().is_empty() {
				if let Some(array) = state.ast.get::<Array>(right)
					&& is_used_as_statement(node)
					&& state.ast.list_is_empty(array.members)
				{
					return state.ast.none();
				}
				return right;
			}

			if state.ast.kind(right).is_call()
				&& {
					let right_type = state.get_type(right_node);
					check(state, TypeCheck::LuaTuple, right_type)
				} && !array_like_expression_contains_spread(left_node)
			{
				expressions::transform_optimized_array_assignment_pattern(state, left_node, right.into());
				if !is_used_as_statement(node) {
					state
						.diags
						.add(diagnostics::no_lua_tuple_destructure_assignment_expression(node));
				}
				return state.ast.none();
			}

			if let Some(array) = state.ast.get::<Array>(right)
				&& state.ast.list_is_non_empty(array.members)
				&& is_used_as_statement(node)
				&& !array_like_expression_contains_spread(left_node)
			{
				let members = array.members;
				expressions::transform_optimized_array_assignment_pattern(state, left_node, members.into());
				return state.ast.none();
			}

			let parent_id = state.push_to_var(Some(right), "binding");
			expressions::transform_array_assignment_pattern(state, left_node, parent_id);
			return parent_id;
		} else if tsast::is_object_literal_expression(left_node) {
			let right = transform_expression(state, right_node);

			// optimize empty object destructure
			if left_node.as_object_literal_expression().properties.nodes().is_empty() {
				if let Some(map) = state.ast.get::<Map>(right)
					&& is_used_as_statement(node)
					&& state.ast.list_is_empty(map.fields)
				{
					return state.ast.none();
				}
				return right;
			}

			let parent_id = state.push_to_var(Some(right), "binding");
			expressions::transform_object_assignment_pattern(state, left_node, parent_id);
			return parent_id;
		}

		let writable_type = state.get_type(left_node);
		let value_type = state.get_type(right_node);
		let simple = get_simple_assignment_operator(state, writable_type, operator_kind, value_type);
		let assignment = transform_writable_assignment(state, left_node, right_node, true, simple.is_none());
		if let Some(operator) = simple {
			let value = get_assignable_value(state, operator, assignment.value, value_type);
			return create_assignment_expression(state, assignment.writable, operator, value, Some(operator_token));
		}
		return create_compound_assignment_expression(
			state,
			assignment.writable,
			writable_type,
			assignment.readable,
			operator_kind,
			assignment.value,
			value_type,
		);
	}

	if is_bitwise_operator(operator_kind) {
		return create_bitwise_from_operator(state, operator_kind, node);
	}

	let ordered = ensure_transform_order(state, &[left_node, right_node]);
	let (left, right) = (ordered[0], ordered[1]);

	match operator_kind {
		Kind::InKeyword => {
			let base = convert_to_indexable_expression(&mut state.ast, right);
			let index = state.ast.computed_index(base, left);
			let nil = state.ast.nil();
			let result = state.ast.binary(index, BinaryOperator::NotEqual, nil);
			state.set_operator_source_position(result, operator_token);
			return result;
		}
		Kind::InstanceOfKeyword => {
			let right_type = state.get_type(right_node);
			if is_possibly_type(state, right_type, &[TypeCheck::Roblox]) {
				state.diags.add(diagnostics::no_roblox_symbol_instanceof(right_node));
			}
			let callee = state.runtime_lib(Some(node), "instanceof");
			let args = state.ast.list_from([left, right]);
			return state.ast.call(callee, args);
		}
		_ => {}
	}

	let left_type = state.get_type(left_node);
	let right_type = state.get_type(right_node);

	if matches!(
		operator_kind,
		Kind::LessThanToken | Kind::LessThanEqualsToken | Kind::GreaterThanToken | Kind::GreaterThanEqualsToken
	) {
		// NOTE: a verbatim upstream quirk (transformBinaryExpression.ts L244-247): the second
		// clause re-tests the LEFT type for number where symmetry suggests the right.
		let left_ok = is_definitely_type(state, left_type, &[TypeCheck::String])
			|| is_definitely_type(state, left_type, &[TypeCheck::Number]);
		if !left_ok || !left_ok_for_quirk(state, right_type, left_type) {
			state
				.diags
				.add(diagnostics::no_non_number_string_relation_operator(node));
		}
	}

	let result = create_binary_from_operator(state, left, left_type, operator_kind, right, right_type);
	if state.ast.kind(result) == LuauKind::BinaryExpression {
		state.set_operator_source_position(result, operator_token);
	}
	result
}

/// The second clause of upstream's relation-operator check:
/// `isDefinitelyType(right, string) || isDefinitelyType(LEFT, number)`.
fn left_ok_for_quirk(state: &mut State<'_>, right_type: P<Type>, left_type: P<Type>) -> bool {
	is_definitely_type(state, right_type, &[TypeCheck::String])
		|| is_definitely_type(state, left_type, &[TypeCheck::Number])
}
