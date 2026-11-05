//! `loops_numeric.go`: the constant analysis behind the numeric `for` optimization.

use std::collections::HashMap;

use rotor_luau::js_number_parse;
use rotor_tsrs::ast::{self as tsast, ModifierFlags};

use crate::literals::get_text;
use crate::prelude::*;
use crate::refwalker::for_each_symbol_reference;
use crate::statements::unary_operand_and_operator;
use crate::util::{is_ancestor_of, node_end, node_pos};

/// `isLoopVariable`
pub fn is_loop_variable(state: &mut State<'_>, expression: P<Node>, symbol: P<Symbol>) -> bool {
	let expression = tsast::skip_downwards(expression);
	expression.kind() == Kind::Identifier && state.ck().get_symbol_at_location(expression) == Some(symbol)
}

/// `getOptimizedIncrementorStepValue`: the constant step of `i += n`, `i -= n`, `i++` or `i--`.
pub fn get_optimized_incrementor_step_value(
	state: &mut State<'_>,
	incrementor: P<Node>,
	id_symbol: P<Symbol>,
) -> Option<f64> {
	let incrementor = tsast::skip_downwards(incrementor);
	if tsast::is_binary_expression(incrementor) {
		let binary = incrementor.as_binary_expression();
		if !is_loop_variable(state, binary.left, id_symbol) {
			return None;
		}
		match binary.operator_token.kind() {
			Kind::PlusEqualsToken => return get_constant_loop_integer(state, binary.right.get(), true),
			Kind::MinusEqualsToken => {
				return get_constant_loop_integer(state, binary.right.get(), true).map(|value| -value);
			}
			_ => {}
		}
	}
	if tsast::is_postfix_unary_expression(incrementor) || tsast::is_prefix_unary_expression(incrementor) {
		let (operand, operator) = unary_operand_and_operator(incrementor);
		if is_loop_variable(state, operand, id_symbol) {
			match operator {
				Kind::PlusPlusToken => return Some(1.0),
				Kind::MinusMinusToken => return Some(-1.0),
				_ => {}
			}
		}
	}
	None
}

/// `isMutatedInBody`
pub fn is_mutated_in_body(state: &mut State<'_>, identifier: P<Node>, body: P<Node>) -> bool {
	for_each_symbol_reference(state.checker, identifier, body, &mut |token| {
		tsast::is_write_access(tsast::skip_upwards(token))
	})
}

/// `getConstantLoopInteger`: numeric for hoists its bounds and step, and literal types alone do
/// not prove a value stable (properties and calls change between reads).
pub fn get_constant_loop_integer(state: &mut State<'_>, expression: P<Node>, require_initialized: bool) -> Option<f64> {
	let mut cache: HashMap<P<Node>, Option<f64>> = HashMap::new();
	get_integer(state, expression, require_initialized, &mut cache)
}

fn get_integer(
	state: &mut State<'_>,
	expression: P<Node>,
	require_initialized: bool,
	cache: &mut HashMap<P<Node>, Option<f64>>,
) -> Option<f64> {
	let expression = tsast::skip_downwards(expression);
	if let Some(&cached) = cache.get(&expression) {
		return cached;
	}
	cache.insert(expression, None);
	let value = get_constant_loop_number(state, expression, require_initialized, cache)
		.filter(|value| value.abs() <= 9_007_199_254_740_991.0 && value.fract() == 0.0);
	cache.insert(expression, value);
	value
}

fn get_constant_loop_number(
	state: &mut State<'_>,
	expression: P<Node>,
	require_initialized: bool,
	cache: &mut HashMap<P<Node>, Option<f64>>,
) -> Option<f64> {
	match expression.kind() {
		Kind::NumericLiteral => js_number_parse(&get_text(state, expression)).ok(),
		Kind::PrefixUnaryExpression => {
			let unary = expression.as_prefix_unary_expression();
			match unary.operator {
				Kind::PlusToken => get_integer(state, unary.operand, require_initialized, cache),
				Kind::MinusToken => get_integer(state, unary.operand, require_initialized, cache).map(|value| -value),
				_ => None,
			}
		}
		Kind::BinaryExpression => {
			let binary = expression.as_binary_expression();
			let left = get_integer(state, binary.left, require_initialized, cache);
			let right = get_integer(state, binary.right.get(), require_initialized, cache);
			let (left, right) = (left?, right?);
			match binary.operator_token.kind() {
				Kind::PlusToken => Some(left + right),
				Kind::MinusToken => Some(left - right),
				Kind::AsteriskToken => Some(left * right),
				Kind::SlashToken => Some(left / right),
				Kind::AsteriskAsteriskToken => Some(left.powf(right)),
				_ => None,
			}
		}
		Kind::Identifier => {
			let declaration = get_constant_loop_declaration(state, expression, require_initialized)?;
			let initializer = declaration.initializer()?;
			get_integer(state, initializer, require_initialized, cache)
		}
		_ => None,
	}
}

fn get_constant_loop_declaration(
	state: &mut State<'_>,
	expression: P<Node>,
	require_initialized: bool,
) -> Option<P<Node>> {
	let symbol = state.ck().get_symbol_at_location(expression)?;
	let declaration = symbol.value_declaration()?;
	if !tsast::is_variable_declaration(declaration)
		|| !tsast::is_var_const(declaration)
		|| declaration.initializer().is_none()
	{
		return None;
	}
	if tsast::get_source_file_of_node(declaration) != tsast::get_source_file_of_node(expression)
		|| tsast::get_combined_modifier_flags(declaration).intersects(ModifierFlags::Ambient)
	{
		return None;
	}
	// A source step may never run; its const binding can still be uninitialized when a closure
	// containing a loop is called.
	if require_initialized {
		let declaration_statement = declaration.parent().and_then(|parent| parent.parent())?;
		let statement_parent = declaration_statement.parent()?;
		if node_end(declaration) > node_pos(expression)
			|| !is_ancestor_of(statement_parent, expression)
			|| tsast::find_ancestor(declaration, tsast::is_function_like)
				!= tsast::find_ancestor(expression, tsast::is_function_like)
		{
			return None;
		}
	}
	Some(declaration)
}
