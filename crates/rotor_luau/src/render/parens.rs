//! Operator precedence and parenthesization.

use crate::{Ast, BinaryOperator, NodeData, NodeId, UnaryOperator};

// Luau operator precedence: https://www.lua.org/manual/5.1/manual.html#2.5.6
const IF_EXPRESSION_PRECEDENCE: u8 = 1;

const fn unary_operator_precedence(operator: UnaryOperator) -> u8 {
	match operator {
		UnaryOperator::Not | UnaryOperator::Length | UnaryOperator::Negate => 7,
	}
}

const fn binary_operator_precedence(operator: BinaryOperator) -> u8 {
	match operator {
		BinaryOperator::Or => 1,
		BinaryOperator::And => 2,
		BinaryOperator::Less
		| BinaryOperator::Greater
		| BinaryOperator::LessEqual
		| BinaryOperator::GreaterEqual
		| BinaryOperator::NotEqual
		| BinaryOperator::Equal => 3,
		BinaryOperator::Concat => 4,
		BinaryOperator::Add | BinaryOperator::Subtract => 5,
		BinaryOperator::Multiply | BinaryOperator::Divide | BinaryOperator::FloorDivide | BinaryOperator::Modulo => 6,
		BinaryOperator::Power => 8,
	}
}

/// Only called for expressions with precedence; anything else is 0.
fn precedence(ast: &Ast, node: NodeId) -> u8 {
	match ast.data(node) {
		NodeData::IfExpression(_) => IF_EXPRESSION_PRECEDENCE,
		NodeData::BinaryExpression(n) => binary_operator_precedence(n.operator),
		NodeData::UnaryExpression(n) => unary_operator_precedence(n.operator),
		_ => 0,
	}
}

pub fn needs_parentheses(ast: &Ast, node: NodeId) -> bool {
	let Some(parent) = ast.parent(node) else {
		return false;
	};
	if !ast.kind(parent).is_expression_with_precedence() {
		return false;
	}
	let (node_precedence, parent_precedence) = (precedence(ast, node), precedence(ast, parent));
	if node_precedence < parent_precedence {
		return true;
	}
	if node_precedence == parent_precedence
		&& let NodeData::BinaryExpression(binary) = ast.data(parent)
	{
		return node == binary.right;
	}
	false
}

/// `::` is the highest-precedence postfix (Luau `asexp ::= simpleexp ['::'
/// Type]`), so a cast needs parentheses only as the left operand of `<`, where
/// the type parser would read `T < y` as the start of a type argument list. A
/// cast under another cast is parenthesized by the outer one
/// ([`cast_operand_needs_parentheses`]): `x :: any :: Foo` is a syntax error.
pub fn cast_needs_parentheses(ast: &Ast, node: NodeId) -> bool {
	let Some(parent) = ast.parent(node) else {
		return false;
	};
	matches!(ast.data(parent), NodeData::BinaryExpression(binary) if binary.operator == BinaryOperator::Less && binary.left == node)
}

/// The operand of `::` is a simpleexp, so a cast, binary, unary, or
/// if-expression operand must be parenthesized (`(a + b) :: number`; bare
/// `a + b :: number` casts only `b`).
pub fn cast_operand_needs_parentheses(ast: &Ast, operand: NodeId) -> bool {
	matches!(ast.data(operand), NodeData::CastExpression(_)) || ast.kind(operand).is_expression_with_precedence()
}
