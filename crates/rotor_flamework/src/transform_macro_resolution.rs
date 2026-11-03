//! `transform_macro_resolution.go`: the user macro a parameter type describes.

use rotor_tsrs::P;
use rotor_tsrs::ast::{Kind, Node, SymbolFlags};
use rotor_tsrs::checker::{LiteralValue, ObjectFlags, Type, TypeExt as _, TypeFlags};

use crate::Error;
use crate::transform_intrinsics::{hash_user_macro, tuple_labels_macro};
use crate::transform_macro_types::{
	LiteralMacroValue, MacroKind, MacroProperty, UserMacro, invalid_macro, string_literal_value,
};
use crate::transform_state::TransformState;
use crate::transform_symbols::literal_type_string;

/// `userMacroOfUnion`: the first macro among the union's constituents.
pub(crate) fn user_macro_of_union(
	state: &mut TransformState<'_>,
	trace: P<Node>,
	target: P<Type>,
) -> Result<Option<UserMacro>, Error> {
	for constituent in target.distributed() {
		if let Some(found) = user_macro_of_type(state, trace, constituent)? {
			return Ok(Some(found));
		}
	}
	Ok(None)
}

fn user_macro_of_type(
	state: &mut TransformState<'_>,
	trace: P<Node>,
	target: P<Type>,
) -> Result<Option<UserMacro>, Error> {
	if let Some(many) = state.ck().get_type_of_property_of_type(target, "_flamework_macro_many") {
		return user_macro_of_many(state, trace, many);
	}
	basic_user_macro(state, trace, target)
}

fn basic_user_macro(
	state: &mut TransformState<'_>,
	trace: P<Node>,
	target: P<Type>,
) -> Result<Option<UserMacro>, Error> {
	if let Some(generic) = state
		.ck()
		.get_type_of_property_of_type(target, "_flamework_macro_generic")
	{
		let macro_target = state.ck().get_type_of_property_of_type(generic, "0");
		let metadata = state.ck().get_type_of_property_of_type(generic, "1");
		let (Some(macro_target), Some(metadata)) = (macro_target, metadata) else {
			return Ok(None);
		};
		if !metadata.is_string_literal() {
			let text = state.ck().type_to_string(metadata);
			return Err(invalid_macro(
				trace,
				format!(
					"Flamework encountered invalid generic metadata {}",
					rotor_support::gostr::quote(&text)
				),
			));
		}
		return Ok(Some(UserMacro {
			kind: MacroKind::Generic,
			target: Some(macro_target),
			metadata: string_literal_value(metadata),
			order: Some(metadata.id()),
			..UserMacro::default()
		}));
	}
	if let Some(caller) = state
		.ck()
		.get_type_of_property_of_type(target, "_flamework_macro_caller")
	{
		if !caller.is_string_literal() {
			let text = state.ck().type_to_string(caller);
			return Err(invalid_macro(
				trace,
				format!(
					"Flamework encountered invalid caller metadata {}",
					rotor_support::gostr::quote(&text)
				),
			));
		}
		return Ok(Some(UserMacro {
			kind: MacroKind::Caller,
			metadata: string_literal_value(caller),
			order: Some(caller.id()),
			..UserMacro::default()
		}));
	}
	if let Some(hash) = state.ck().get_type_of_property_of_type(target, "_flamework_macro_hash") {
		return hash_user_macro(state, trace, hash).map(Some);
	}
	let non_nullable = state.ck().get_non_nullable_type(target);
	if let Some(labels) = state
		.ck()
		.get_type_of_property_of_type(non_nullable, "_flamework_macro_tuple_labels")
	{
		return Ok(Some(tuple_labels_macro(labels)));
	}
	if let Some(intrinsic) = state
		.ck()
		.get_type_of_property_of_type(non_nullable, "_flamework_intrinsic")
		&& intrinsic.is_tuple_type()
	{
		let inputs = state.ck().get_type_arguments(intrinsic);
		let Some(first) = inputs.first().filter(|first| first.is_string_literal()) else {
			return Err(invalid_macro(trace, "Flamework encountered invalid intrinsic metadata"));
		};
		return Ok(Some(UserMacro {
			kind: MacroKind::Intrinsic,
			intrinsic: string_literal_value(*first),
			inputs: inputs[1..].to_vec(),
			..UserMacro::default()
		}));
	}
	Ok(None)
}

/// `userMacroOfMany`
pub(crate) fn user_macro_of_many(
	state: &mut TransformState<'_>,
	trace: P<Node>,
	target: P<Type>,
) -> Result<Option<UserMacro>, Error> {
	if let Some(found) = basic_user_macro(state, trace, target)? {
		return Ok(Some(found));
	}
	if let Some(nested) = state.ck().get_type_of_property_of_type(target, "_flamework_macro_many") {
		return user_macro_of_many(state, trace, nested);
	}
	if target.is_tuple_type() {
		let arguments = state.ck().get_type_arguments(target).to_vec();
		return many_items_macro(state, trace, &arguments);
	}
	if is_array_macro_target(state, trace, target) {
		let arguments = state.ck().get_type_arguments(target).to_vec();
		if arguments.len() != 1 {
			return Err(invalid_macro(
				trace,
				format!("Flamework array macro has {} type arguments", arguments.len()),
			));
		}
		return many_items_macro(state, trace, &arguments[0].distributed());
	}
	if target.flags().intersects(TypeFlags::Object) || target.is_intersection() {
		let properties = state.ck().get_properties_of_type(target);
		let mut result_properties = Vec::with_capacity(properties.len());
		let mut ordered = true;
		for property in properties {
			let Some(member_type) = state.ck().get_type_of_property_of_type(target, property.name()) else {
				return Ok(None);
			};
			let Some(member) = user_macro_of_many(state, trace, member_type)? else {
				return Ok(None);
			};
			ordered = ordered && member.order.is_some();
			result_properties.push(MacroProperty {
				name: property.name().to_owned(),
				macro_: member,
			});
		}
		if ordered {
			result_properties.sort_by_key(|property| property.macro_.order);
		}
		return Ok(Some(UserMacro {
			kind: MacroKind::Many,
			properties: Some(result_properties),
			..UserMacro::default()
		}));
	}
	if target.is_string_literal() {
		return Ok(Some(literal_macro(LiteralMacroValue {
			kind: Some(Kind::StringLiteral),
			text: string_literal_value(target),
			..LiteralMacroValue::default()
		})));
	}
	if target.is_number_literal() {
		return Ok(Some(literal_macro(LiteralMacroValue {
			kind: Some(Kind::NumericLiteral),
			number: literal_type_string(target),
			..LiteralMacroValue::default()
		})));
	}
	if target.flags().intersects(TypeFlags::BooleanLiteral) {
		let Some(LiteralValue::Boolean(value)) = target.as_literal_type().value() else {
			return Err(invalid_macro(trace, "Flamework boolean literal has an invalid value"));
		};
		return Ok(Some(literal_macro(LiteralMacroValue {
			kind: Some(Kind::TrueKeyword),
			boolean: value,
			..LiteralMacroValue::default()
		})));
	}
	if target.flags().intersects(TypeFlags::Undefined | TypeFlags::Never) {
		return Ok(Some(literal_macro(LiteralMacroValue {
			kind: Some(Kind::Identifier),
			..LiteralMacroValue::default()
		})));
	}
	let text = state.ck().type_to_string(target);
	Err(invalid_macro(trace, format!("Unknown type '{text}' encountered")))
}

pub(crate) fn literal_macro(literal: LiteralMacroValue) -> UserMacro {
	UserMacro {
		kind: MacroKind::Literal,
		literal,
		..UserMacro::default()
	}
}

fn is_array_macro_target(state: &mut TransformState<'_>, trace: P<Node>, target: P<Type>) -> bool {
	if !target.object_flags().intersects(ObjectFlags::Reference) {
		return false;
	}
	let Some(target_symbol) = target.target().and_then(|reference| reference.symbol()) else {
		return false;
	};
	["Array", "ReadonlyArray"].iter().any(|name| {
		state
			.ck()
			.resolve_name_exported(name, Some(trace), SymbolFlags::Type, false)
			== Some(target_symbol)
	})
}

fn many_items_macro(
	state: &mut TransformState<'_>,
	trace: P<Node>,
	types: &[P<Type>],
) -> Result<Option<UserMacro>, Error> {
	let mut items = Vec::with_capacity(types.len());
	for item_type in types {
		if item_type.flags().intersects(TypeFlags::Never) {
			break;
		}
		let Some(item) = user_macro_of_many(state, trace, *item_type)? else {
			return Ok(None);
		};
		items.push(item);
	}
	Ok(Some(UserMacro {
		kind: MacroKind::Many,
		items: Some(items),
		..UserMacro::default()
	}))
}
