//! `access.go`: `expressions/transformPropertyAccessExpression.ts`,
//! `expressions/transformElementAccessExpression.ts`, and their utils
//! (`getConstantValueLiteral`, `offset`, `addOneIfArrayType`, `addIndexDiagnostics`,
//! `isValidMethodIndexWithoutCall`, `validateNotAny`).

use rotor_luau::{BinaryExpression, NumberLiteral, UnaryExpression, js_number_parse};
use rotor_tsrs::ast as tsast;
use rotor_tsrs::checker::LiteralValue;
use rotor_tsrs::types::TypeCheck;

use crate::call::convert_to_indexable_expression;
use crate::diagnostics;
use crate::dispatch::transform_expression;
use crate::exports::get_original_symbol_of_node;
use crate::ismethod::is_method;
use crate::prelude::*;
use crate::types::{check, get_first_defined_symbol, is_definitely_type};

/// `validateNotAnyType` (`util/validateNotAny.ts` L9-37): `any`-typed expressions are a `noAny`
/// error, deduplicated per symbol pass-wide. Array types unwrap to their element type first.
pub fn validate_not_any_type(state: &mut State<'_>, mut node: P<Node>) {
	if tsast::is_spread_element(node)
		&& let Some(expression) = node.expression()
	{
		node = tsast::skip_downwards(expression);
	}

	let mut t = state.get_type(node);

	if is_definitely_type(state, t, &[TypeCheck::Array]) {
		// Array<T> -> T
		if let Some(index_type) = state.ck().get_number_index_type(t) {
			t = index_type;
		}
	}

	if is_definitely_type(state, t, &[TypeCheck::Any]) {
		// `a["b"]` on `{ [index: string]: any }` has no symbol
		if let Some(symbol) = get_original_symbol_of_node(state, node) {
			let mut cache = std::mem::take(&mut state.multi.is_reported_by_no_any_cache);
			state
				.diags
				.add_with_cache(symbol, diagnostics::no_any(node), &mut cache);
			state.multi.is_reported_by_no_any_cache = cache;
		} else {
			state.diags.add(diagnostics::no_any(node));
		}
	}
}

/// `getLiteralNumberValue` (`offset.ts`): number literals, and unary minus over one, fold to
/// their JS value.
pub fn get_literal_number_value(ast: &Ast, expression: NodeId) -> Option<f64> {
	if let Some(literal) = ast.get::<NumberLiteral>(expression) {
		return js_number_parse(&literal.value).ok();
	}
	if let Some(unary) = ast.get::<UnaryExpression>(expression)
		&& unary.operator == UnaryOperator::Negate
	{
		return get_literal_number_value(ast, unary.expression).map(|inner| -inner);
	}
	None
}

/// `offset` (`offset.ts`): folds the offset into number literals and into the right operand of
/// an existing `+`/`-` (so `arr[i - 1]` plus one is `arr[i]`), else appends `+ n`/`- n`.
pub fn offset_expr(ast: &mut Ast, expression: NodeId, value: f64) -> NodeId {
	if value == 0.0 {
		return expression;
	}

	// array[offset - 1 + 1] -> array[offset]
	if let Some(binary) = ast.get::<BinaryExpression>(expression).cloned()
		&& matches!(binary.operator, BinaryOperator::Add | BinaryOperator::Subtract)
		&& let Some(right_value) = get_literal_number_value(ast, binary.right)
	{
		let sign = if binary.operator == BinaryOperator::Subtract {
			-1.0
		} else {
			1.0
		};
		#[expect(
			clippy::suboptimal_flops,
			reason = "Go's `rightValue + value*sign`, spelled the same"
		)]
		let new_right_value = right_value + value * sign;
		if new_right_value == 0.0 {
			return binary.left;
		}
		let right = ast.num(new_right_value);
		let adjusted = ast.binary(binary.left, binary.operator, right);
		if let Some(origin) = ast.origin(expression) {
			ast.set_origin(adjusted, origin);
		}
		return adjusted;
	}

	if let Some(literal_value) = get_literal_number_value(ast, expression) {
		return ast.num(literal_value + value);
	}
	let operator = if value < 0.0 {
		BinaryOperator::Subtract
	} else {
		BinaryOperator::Add
	};
	let magnitude = ast.num(value.abs());
	ast.binary(expression, operator, magnitude)
}

/// `addOneIfArrayType` (`util/addOneIfArrayType.ts`): 0-based TS indices become 1-based Luau
/// ones when the object is definitely an array (or undefined).
pub fn add_one_if_array_type(state: &mut State<'_>, t: P<Type>, expression: NodeId) -> NodeId {
	if is_definitely_type(state, t, &[TypeCheck::Array, TypeCheck::Undefined]) {
		return offset_expr(&mut state.ast, expression, 1.0);
	}
	expression
}

/// `isValidMethodIndexWithoutCall` (`util/isValidMethodIndexWithoutCall.ts`): a method may be
/// indexed without a call under a binary expression, a prefix unary expression, or as an
/// argument of the `typeIs`/`typeOf` call macros.
pub fn is_valid_method_index_without_call(state: &mut State<'_>, node: P<Node>) -> bool {
	let Some(parent) = node.parent() else {
		return false;
	};
	// a.b !== undefined
	if tsast::is_binary_expression(parent) {
		return true;
	}
	// !a.b
	if tsast::is_prefix_unary_expression(parent) {
		return true;
	}
	// typeIs/typeOf macros
	if tsast::is_call_expression(parent) {
		let callee_type = state.get_type(parent.as_call_expression().expression);
		let expression_type = state.ck().get_non_optional_type(callee_type);
		if let Some(symbol) = get_first_defined_symbol(state, expression_type)
			&& state.macros().is_type_check_call_macro(symbol)
		{
			return true;
		}
	}
	false
}

/// `addIndexDiagnostics` (`util/addIndexDiagnostics.ts`). Property access passes the access's own
/// type; element access passes the object's.
pub fn add_index_diagnostics(state: &mut State<'_>, node: P<Node>, expression_type: P<Type>) {
	let symbol = get_first_defined_symbol(state, expression_type);
	let is_property_call_macro = symbol.is_some_and(|symbol| state.get_property_call_macro(symbol).is_some());
	if is_property_call_macro
		|| (!is_valid_method_index_without_call(state, tsast::skip_upwards(node)) && is_method(state, node))
	{
		state.diags.add(diagnostics::no_index_without_call(node));
	}

	if tsast::is_prototype_access(node) {
		state.diags.add(diagnostics::no_prototype(node));
	}
}

/// `getConstantValueLiteral` (`util/getConstantValueLiteral.ts`): const enum member accesses fold
/// to their literal value.
pub fn get_constant_value_literal(state: &mut State<'_>, node: P<Node>) -> Option<NodeId> {
	match state.ck().get_constant_value(node)? {
		LiteralValue::String(value) => Some(state.ast.str(value)),
		LiteralValue::Number(value) => Some(state.ast.num(value.0)),
		LiteralValue::Boolean(_) | LiteralValue::BigInt(_) => None,
	}
}

fn is_delete_target(node: P<Node>) -> bool {
	tsast::skip_upwards(node)
		.parent()
		.is_some_and(tsast::is_delete_expression)
}

/// `transformPropertyAccessExpressionInner` (L11-34). `expression` is the transformed object.
pub fn transform_property_access_expression_inner(
	state: &mut State<'_>,
	node: P<Node>,
	expression: NodeId,
	name: &str,
) -> NodeId {
	// a in a.b
	validate_not_any_type(state, node.as_property_access_expression().expression);

	let t = state.get_type(node);
	let t = state.ck().get_non_optional_type(t);
	add_index_diagnostics(state, node, t);

	if is_delete_target(node) {
		let base = convert_to_indexable_expression(&mut state.ast, expression);
		let target = state.ast.property_access(base, name);
		let nil = state.ast.nil();
		let assignment = state.ast.assignment(target, AssignmentOperator::Assign, nil);
		state.prereq(assignment);
		return state.ast.none();
	}

	let base = convert_to_indexable_expression(&mut state.ast, expression);
	state.ast.property_access(base, name)
}

/// `transformPropertyAccessExpression` (L36-43): a const enum fold, else the chain walker.
pub fn transform_property_access_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	if let Some(constant) = get_constant_value_literal(state, node) {
		return constant;
	}
	crate::optionalchain::transform_optional_chain(state, node)
}

/// `transformElementAccessExpressionInner` (L15-69).
pub fn transform_element_access_expression_inner(
	state: &mut State<'_>,
	node: P<Node>,
	mut expression: NodeId,
	argument_expression: P<Node>,
) -> NodeId {
	let element_access = node.as_element_access_expression();
	// a in a[b]
	validate_not_any_type(state, element_access.expression);
	// b in a[b]
	validate_not_any_type(state, element_access.argument_expression);

	let object_type = state.get_type(element_access.expression);
	let expression_type = state.ck().get_non_optional_type(object_type);
	add_index_diagnostics(state, node, expression_type);

	let (index, prereqs) = state.capture(|state| transform_expression(state, argument_expression));
	if let Some(data) = state.get_optimizable_var_args_data(element_access.expression)
		&& data.is_optimizable
	{
		state.prereq_list(prereqs);
		if get_literal_number_value(&state.ast, index) == Some(0.0) {
			let var_args = state.ast.var_args();
			return state.ast.parenthesized(var_args);
		}
		let select = state.ast.global_id("select");
		let position = offset_expr(&mut state.ast, index, 1.0);
		let var_args = state.ast.var_args();
		let args = state.ast.list_from([position, var_args]);
		let call = state.ast.call(select, args);
		return state.ast.parenthesized(call);
	}

	if state.ast.list_is_non_empty(prereqs) {
		// hack because wrapReturnIfLuaTuple will not wrap this, but now we need to!
		if check(state, TypeCheck::LuaTuple, expression_type) {
			let members = state.ast.list_from([expression]);
			expression = state.ast.array(members);
		}
		expression = state.push_to_var(Some(expression), "exp");
		state.prereq_list(prereqs);
	}

	// LuaTuple<T> checks
	if state.ast.kind(expression).is_call() && check(state, TypeCheck::LuaTuple, expression_type) {
		// wrap in select() if it isn't the first value
		let first = state
			.ast
			.get::<NumberLiteral>(index)
			.map(|literal| js_number_parse(&literal.value).unwrap_or(0.0));
		if first != Some(0.0) {
			let select = state.ast.global_id("select");
			let position = offset_expr(&mut state.ast, index, 1.0);
			let args = state.ast.list_from([position, expression]);
			expression = state.ast.call(select, args);
		}
		// parentheses trim off the rest of the values
		return state.ast.parenthesized(expression);
	}

	if is_delete_target(node) {
		let base = convert_to_indexable_expression(&mut state.ast, expression);
		let index = add_one_if_array_type(state, expression_type, index);
		let target = state.ast.computed_index(base, index);
		let nil = state.ast.nil();
		let assignment = state.ast.assignment(target, AssignmentOperator::Assign, nil);
		state.prereq(assignment);
		return state.ast.none();
	}

	let base = convert_to_indexable_expression(&mut state.ast, expression);
	let index = add_one_if_array_type(state, expression_type, index);
	state.ast.computed_index(base, index)
}

/// `transformElementAccessExpression` (L71-78): a const enum fold, else the chain walker.
pub fn transform_element_access_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	if let Some(constant) = get_constant_value_literal(state, node) {
		return constant;
	}
	crate::optionalchain::transform_optional_chain(state, node)
}
