//! `transform_inlining.go`: the `intrinsic-inline` macro.

use rotor_tsrs::P;
use rotor_tsrs::ast::{Node, Symbol};
use rotor_tsrs::checker::Signature;

use crate::Error;
use crate::transform_macro_types::invalid_macro;
use crate::transform_parameters::parameter_index;
use crate::transform_state::TransformState;

/// `inlineMacroIntrinsic`: the inlined argument asserted as the signature's return type.
pub(crate) fn inline_macro_intrinsic(
	state: &TransformState<'_>,
	signature: P<Signature>,
	arguments: &[P<Node>],
	parameter: P<Symbol>,
) -> Result<P<Node>, Error> {
	let declaration = signature.declaration();
	let Some(index) = parameter_index(signature, parameter).filter(|index| *index < arguments.len()) else {
		return Err(invalid_macro(
			declaration,
			"Flamework inline parameter was not supplied",
		));
	};
	let Some(type_node) = declaration.and_then(|declaration| declaration.type_node()) else {
		return Err(invalid_macro(
			declaration,
			"Flamework inline signature has no return type",
		));
	};
	Ok(state.factory.new_as_expression(arguments[index], type_node))
}
