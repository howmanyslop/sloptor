//! `newexpression.go`: `expressions/transformNewExpression.ts`. Construct-signature symbol, then
//! the constructor macro, else `X.new(args)` (which also covers `new Instance("Part")`).

use crate::access::validate_not_any_type;
use crate::call::convert_to_indexable_expression;
use crate::dispatch::transform_expression;
use crate::ensureorder::ensure_transform_order;
use crate::prelude::*;
use crate::types::get_first_construct_symbol;
use crate::util::nodes;

/// `transformNewExpression`
pub fn transform_new_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let new_expression = node.as_new_expression();
	let callee = new_expression.expression;
	validate_not_any_type(state, callee);

	if let Some(symbol) = get_first_construct_symbol(state, callee)
		&& let Some(macro_fn) = state.macros().get_constructor_macro(symbol)
	{
		return macro_fn(state, node);
	}

	let transformed = transform_expression(state, callee);
	let expression = convert_to_indexable_expression(&mut state.ast, transformed);
	let args = ensure_transform_order(state, nodes(new_expression.arguments()));
	let access = state.ast.property_access(expression, "new");
	let args = state.ast.list_from(args);
	state.ast.call(access, args)
}
