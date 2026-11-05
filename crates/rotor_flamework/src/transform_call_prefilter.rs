//! `transform_call_prefilter.go`: a sound filter before resolving a call's signature.

use rotor_tsrs::P;
use rotor_tsrs::ast::{Node, is_new_expression};
use rotor_tsrs::checker::{ObjectFlags, Signature, SignatureKind, Type, TypeFlags};

use crate::transform::FLAMEWORK_MARKER_PREFIX;
use crate::transform_macro_types::{MacroTransformResult, jsdoc_tags};
use crate::transform_parameters::macro_parameter_count;
use crate::transform_state::TransformState;

/// `calleeMayResolveToFlameworkMacro`: whether any of the callee type's signatures has a
/// `_flamework` marker parameter (or one that could gain it) or a `@metadata` tag.
pub(crate) fn callee_may_resolve_to_flamework_macro(state: &mut TransformState<'_>, node: P<Node>) -> bool {
	let kind = if is_new_expression(node) {
		SignatureKind::Construct
	} else {
		SignatureKind::Call
	};
	let Some(callee) = node.expression() else {
		return true;
	};
	let callee_type = state.ck().get_type_at_location(callee);
	let signatures = state.ck().get_signatures_of_type(callee_type, kind);
	if signatures.is_empty() {
		return true;
	}
	signatures
		.iter()
		.any(|signature| signature_may_be_flamework_macro(state, *signature))
}

fn signature_may_be_flamework_macro(state: &mut TransformState<'_>, signature: P<Signature>) -> bool {
	if signature.declaration().is_some_and(declaration_has_metadata_tag) {
		return true;
	}
	for index in 0..macro_parameter_count(state, signature) {
		let position_type = state.ck().get_type_at_position(signature, index);
		if type_may_carry_flamework_marker(state, Some(position_type)) {
			return true;
		}
	}
	false
}

fn declaration_has_metadata_tag(declaration: P<Node>) -> bool {
	declaration
		.jsdoc(None)
		.iter()
		.any(|jsdoc| jsdoc_tags(*jsdoc).iter().any(|tag| tag.tag_name().text() == "metadata"))
}

/// `typeMayCarryFlameworkMarker`: whether a parameter type has, or could gain through
/// instantiation, a `_flamework` property.
fn type_may_carry_flamework_marker(state: &mut TransformState<'_>, target: Option<P<Type>>) -> bool {
	let Some(target) = target else {
		return true;
	};
	if target.flags().intersects(TypeFlags::Instantiable) {
		return true;
	}
	if target.flags().intersects(TypeFlags::UnionOrIntersection)
		&& target
			.types()
			.iter()
			.any(|member| type_may_carry_flamework_marker(state, Some(*member)))
	{
		return true;
	}
	if target.flags().intersects(TypeFlags::Object)
		&& target
			.object_flags()
			.intersects(ObjectFlags::Mapped | ObjectFlags::ReverseMapped)
	{
		return true;
	}
	state
		.ck()
		.get_properties_of_type(target)
		.iter()
		.any(|property| property.name().starts_with(FLAMEWORK_MARKER_PREFIX))
}

/// `passThroughFlameworkCall`: the call rebuilt with a fresh argument list, as the macro route
/// would build it.
pub(crate) fn pass_through_flamework_call(state: &TransformState<'_>, node: P<Node>) -> MacroTransformResult {
	let argument_list = state.factory.new_node_list(node.arguments().to_vec());
	let expression = node.expression().unwrap_or(node);
	let updated = if rotor_tsrs::ast::is_call_expression(node) {
		let call = node.as_call_expression();
		state.factory.update_call_expression(
			node,
			expression,
			call.question_dot_token(),
			call.type_arguments(),
			argument_list,
			node.flags(),
		)
	} else {
		state.factory.update_new_expression(
			node,
			expression,
			node.as_new_expression().type_arguments(),
			Some(argument_list),
		)
	};
	MacroTransformResult {
		expression: Some(updated),
		..MacroTransformResult::default()
	}
}
