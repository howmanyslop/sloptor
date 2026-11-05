//! `bitwise.go`: `bit32` calls for the bitwise operators.

use rotor_tsrs::ast as tsast;

use crate::binary::bitwise_operator_name;
use crate::diagnostics;
use crate::ensureorder::ensure_transform_order;
use crate::prelude::*;

/// `logicalBitwiseOperatorMap`: the associative operators, whose chains flatten into one call.
const fn logical_bitwise_operator_name(kind: Kind) -> Option<&'static str> {
	match kind {
		Kind::AmpersandToken => Some("band"),
		Kind::BarToken => Some("bor"),
		Kind::CaretToken => Some("bxor"),
		_ => None,
	}
}

fn flatten_bitwise_chain_into(expressions: &mut Vec<P<Node>>, operator_kind: Kind, node: P<Node>) {
	if tsast::is_binary_expression(node) {
		let expression = node.as_binary_expression();
		if expression.operator_token.kind() == operator_kind {
			flatten_bitwise_chain_into(expressions, operator_kind, expression.left);
			expressions.push(tsast::skip_downwards(expression.right.get()));
		} else {
			expressions.push(node);
		}
		return;
	}
	expressions.push(tsast::skip_downwards(node));
}

pub fn flatten_bitwise_chain(operator_kind: Kind, node: P<Node>) -> Vec<P<Node>> {
	let mut expressions = Vec::with_capacity(2);
	flatten_bitwise_chain_into(&mut expressions, operator_kind, node);
	expressions
}

const fn is_bitwise_logical_operator(operator_kind: Kind) -> bool {
	logical_bitwise_operator_name(operator_kind).is_some()
}

/// `isBitwiseOperator`
#[must_use]
pub const fn is_bitwise_operator(operator_kind: Kind) -> bool {
	is_bitwise_logical_operator(operator_kind) || bitwise_operator_name(operator_kind).is_some()
}

/// `createBitwiseCall`: `bit32.<name>(expressions...)`.
pub fn create_bitwise_call(state: &mut State<'_>, operator_kind: Kind, expressions: Vec<NodeId>) -> NodeId {
	let Some(name) = logical_bitwise_operator_name(operator_kind).or_else(|| bitwise_operator_name(operator_kind))
	else {
		state.ice(format!(
			"transformer: createBitwiseCall unknown operator: {}",
			diagnostics::kind_name(operator_kind)
		));
		return state.ast.none();
	};
	let callee = state.ast.global_property("bit32", name);
	let args = state.ast.list_from(expressions);
	state.ast.call(callee, args)
}

/// `createBitwiseFromOperator`
pub fn create_bitwise_from_operator(state: &mut State<'_>, operator_kind: Kind, node: P<Node>) -> NodeId {
	let expression = node.as_binary_expression();
	let mut operands = Vec::with_capacity(2);
	if is_bitwise_logical_operator(operator_kind) {
		operands.extend(flatten_bitwise_chain(operator_kind, expression.left));
	} else {
		operands.push(expression.left);
	}
	operands.push(expression.right.get());
	let ordered = ensure_transform_order(state, &operands);
	create_bitwise_call(state, operator_kind, ordered)
}
