//! `transform_parameters.go`: `intrinsic-const` validation and macro parameter positions.

use rotor_tsrs::P;
use rotor_tsrs::ast::{Kind, Node, Symbol, is_literal_expression, is_spread_assignment, is_spread_element};
use rotor_tsrs::checker::{ElementFlags, Signature};

use crate::Error;
use crate::transform_macro_types::{MacroRelatedInformation, invalid_macro_with_related};
use crate::transform_state::TransformState;

/// `validateParameterConstIntrinsic`: arguments for `intrinsic-const` parameters must be
/// literals, or object and array literals without spreads.
pub(crate) fn validate_parameter_const_intrinsic(
	node: P<Node>,
	signature: P<Signature>,
	parameters: &[P<Symbol>],
) -> Result<(), Error> {
	let arguments = node.arguments();
	for parameter in parameters {
		let Some(index) = parameter_index(signature, *parameter).filter(|index| *index < arguments.len()) else {
			continue;
		};
		let argument = arguments[index];
		if is_literal_expression(argument)
			|| matches!(
				argument.kind(),
				Kind::TrueKeyword | Kind::FalseKeyword | Kind::NullKeyword
			) {
			continue;
		}
		let elements = match argument.kind() {
			Kind::ObjectLiteralExpression => argument.properties(),
			Kind::ArrayLiteralExpression => argument.elements(),
			_ => {
				return Err(invalid_const_parameter_macro(
					argument,
					*parameter,
					"Flamework expected this argument to be a literal expression.",
				));
			}
		};
		for element in elements {
			if is_spread_element(*element) || is_spread_assignment(*element) {
				return Err(invalid_const_parameter_macro(
					*element,
					*parameter,
					"Flamework does not support spread expressions in this location.",
				));
			}
		}
	}
	Ok(())
}

fn invalid_const_parameter_macro(node: P<Node>, parameter: P<Symbol>, message: &str) -> Error {
	let related_node = parameter.value_declaration().unwrap_or(node);
	invalid_macro_with_related(
		node,
		message,
		vec![MacroRelatedInformation {
			node: Some(related_node),
			message: "Required because this parameter must be known at compile-time.".to_owned(),
		}],
	)
}

/// `parameterIndex`: the signature parameter for `target`, by symbol or declaration.
pub(crate) fn parameter_index(signature: P<Signature>, target: P<Symbol>) -> Option<usize> {
	signature.parameters().iter().position(|parameter| {
		*parameter == target
			|| matches!(
				(parameter.value_declaration(), target.value_declaration()),
				(Some(left), Some(right)) if left.symbol() == right.symbol()
			)
	})
}

/// `macroParameterCount`: the parameters, with a tuple rest parameter spread into its fixed
/// elements (plus one when it has a rest element).
pub(crate) fn macro_parameter_count(state: &mut TransformState<'_>, signature: P<Signature>) -> usize {
	let parameters = signature.parameters();
	let mut count = parameters.len();
	if !signature.has_rest_parameter() || count == 0 {
		return count;
	}
	let rest_type = state.ck().get_type_of_symbol(parameters[count - 1]);
	if !rest_type.is_tuple_type() {
		return count;
	}
	let tuple = rest_type.target_tuple_type();
	count = (count + tuple.fixed_length()).saturating_sub(1);
	if tuple
		.element_flags()
		.iter()
		.any(|flags| flags.intersects(ElementFlags::Rest))
	{
		count += 1;
	}
	count
}
