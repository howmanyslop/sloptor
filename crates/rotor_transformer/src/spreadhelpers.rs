//! `spreadhelpers.go`

use rotor_tsrs::ast as tsast;

use crate::prelude::*;

/// `arrayLikeExpressionContainsSpread`: a rest element or spread anywhere in an array pattern or
/// literal, nested arrays included.
#[must_use]
pub fn array_like_expression_contains_spread(node: P<Node>) -> bool {
	let elements: &[P<Node>] = if tsast::is_array_binding_pattern(node) {
		node.as_binding_pattern().elements.nodes()
	} else if tsast::is_array_literal_expression(node) {
		node.as_array_literal_expression().elements.nodes()
	} else {
		return false;
	};
	elements.iter().any(|&element| {
		if tsast::is_binding_element(element) {
			let binding_element = element.as_binding_element();
			binding_element.dot_dot_dot_token().is_some()
				|| element.name().is_some_and(array_like_expression_contains_spread)
		} else if tsast::is_spread_element(element) {
			true
		} else if tsast::is_array_binding_pattern(element) || tsast::is_array_literal_expression(element) {
			array_like_expression_contains_spread(element)
		} else {
			false
		}
	})
}

/// `getTargetIDForBindingPattern`: an identifier value is reused unless the pattern has
/// initializers; anything else is pinned to a `binding` temp.
pub fn get_target_id_for_binding_pattern(state: &mut State<'_>, pattern: P<Node>, value: NodeId) -> NodeId {
	if state.ast.kind(value).is_any_identifier() {
		if has_binding_pattern_initializers(pattern) {
			return state.push_to_var(Some(value), "binding");
		}
		return value;
	}
	state.push_to_var(Some(value), "binding")
}

/// `hasBindingPatternInitializers`
#[must_use]
pub fn has_binding_pattern_initializers(pattern: P<Node>) -> bool {
	pattern
		.as_binding_pattern()
		.elements
		.nodes()
		.iter()
		.any(|&element| tsast::is_binding_element(element) && element.initializer().is_some())
}
