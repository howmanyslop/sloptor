//! `transform_intrinsics.go`: hash and tuple-label macros, many-macros and intrinsics.

use rotor_tsrs::P;
use rotor_tsrs::ast::{Kind, Node, TokenFlags, is_as_expression, is_call_expression};
use rotor_tsrs::checker::{ElementFlags, Type};

use crate::id::DEFAULT_HASH_CONTEXT;
use crate::transform_macro_resolution::literal_macro;
use crate::transform_macro_types::{
	LiteralMacroValue, MacroKind, MacroRuntime, UserMacro, alloc, invalid_macro, is_undefined_expression,
	string_literal_value,
};
use crate::transform_macros::build_user_macro;
use crate::transform_network::{build_obfuscated_object_intrinsic, build_shuffle_array_intrinsic};
use crate::transform_paths::{build_path_glob_intrinsic, build_path_intrinsic};
use crate::transform_state::TransformState;
use crate::transform_symbols::{build_declaration_uid_intrinsic, build_symbol_id_intrinsic};
use crate::{Error, ErrorKind};

/// `hashUserMacro`: a `_flamework_macro_hash` text, hashed when obfuscating.
pub(crate) fn hash_user_macro(
	state: &mut TransformState<'_>,
	trace: P<Node>,
	hash_type: P<Type>,
) -> Result<UserMacro, Error> {
	let text_type = state.ck().get_type_of_property_of_type(hash_type, "0");
	let context_type = state.ck().get_type_of_property_of_type(hash_type, "1");
	let obfuscation_type = state.ck().get_type_of_property_of_type(hash_type, "2");
	let (Some(text_type), Some(context_type)) = (text_type.filter(|text| text.is_string_literal()), context_type)
	else {
		return Err(invalid_macro(trace, "Flamework encountered invalid hash metadata"));
	};
	let context = if context_type.is_string_literal() {
		string_literal_value(context_type)
	} else {
		DEFAULT_HASH_CONTEXT.to_owned()
	};
	let text = string_literal_value(text_type);
	let hash = if obfuscation_type.is_some() {
		hash_text(state, &text, &context, false)?
	} else {
		state.hash_string(&text, &context)?
	};
	Ok(literal_macro(LiteralMacroValue {
		kind: Some(Kind::StringLiteral),
		text: hash,
		..LiteralMacroValue::default()
	}))
}

/// `tupleLabelsMacro`: the labels of a tuple's elements.
pub(crate) fn tuple_labels_macro(target: P<Type>) -> UserMacro {
	if !target.is_tuple_type() {
		return literal_macro(LiteralMacroValue {
			kind: Some(Kind::Identifier),
			..LiteralMacroValue::default()
		});
	}
	let items = target
		.target_tuple_type()
		.element_infos()
		.iter()
		.map(|info| {
			let name = info
				.labeled_declaration()
				.and_then(|declaration| declaration.name())
				.map_or_else(String::new, |name| name.text().to_owned());
			literal_macro(LiteralMacroValue {
				kind: Some(Kind::StringLiteral),
				text: name,
				..LiteralMacroValue::default()
			})
		})
		.collect();
	UserMacro {
		kind: MacroKind::Many,
		items: Some(items),
		..UserMacro::default()
	}
}

/// `buildManyMacro`: an array of the items, or an object of the properties.
pub(crate) fn build_many_macro(
	state: &mut TransformState<'_>,
	trace: P<Node>,
	user_macro: &UserMacro,
	runtime: &MacroRuntime,
) -> Result<(P<Node>, Vec<P<Node>>), Error> {
	let mut prerequisites = Vec::new();
	if let Some(items) = &user_macro.items {
		let mut elements = Vec::with_capacity(items.len());
		for item in items {
			let (expression, statements) = build_user_macro(state, trace, item, runtime)?;
			elements.push(expression);
			prerequisites.extend(statements);
		}
		let array = state
			.factory
			.new_array_literal_expression(state.factory.new_node_list(elements), true);
		return Ok((array, prerequisites));
	}
	let properties_in = user_macro.properties.as_deref().unwrap_or_default();
	let mut properties = Vec::with_capacity(properties_in.len());
	for property in properties_in {
		let (expression, statements) = build_user_macro(state, trace, &property.macro_, runtime)?;
		if is_undefined_as_never(expression) {
			continue;
		}
		properties.push(
			state.factory.new_property_assignment(
				None,
				state
					.factory
					.new_string_literal(alloc(&property.name), TokenFlags::None),
				None,
				None,
				expression,
			),
		);
		prerequisites.extend(statements);
	}
	let object = state
		.factory
		.new_object_literal_expression(state.factory.new_node_list(properties), false);
	Ok((object, prerequisites))
}

/// `buildIntrinsicMacro`
pub(crate) fn build_intrinsic_macro(
	state: &mut TransformState<'_>,
	trace: P<Node>,
	user_macro: &UserMacro,
	runtime: &MacroRuntime,
) -> Result<(P<Node>, Vec<P<Node>>), Error> {
	let inputs = &user_macro.inputs;
	match user_macro.intrinsic.as_str() {
		"pathglob" => {
			let [input] = inputs.as_slice() else {
				return Err(invalid_macro(trace, "Invalid pathglob intrinsic usage"));
			};
			Ok((build_path_glob_intrinsic(state, trace, *input)?, Vec::new()))
		}
		"path" => {
			let [input] = inputs.as_slice() else {
				return Err(invalid_macro(trace, "Invalid path intrinsic usage"));
			};
			Ok((build_path_intrinsic(state, trace, *input)?, Vec::new()))
		}
		"obfuscate-obj" => build_obfuscated_object_intrinsic(state, trace, inputs, runtime),
		"shuffle-array" => build_shuffle_array_intrinsic(state, trace, inputs, runtime),
		"tuple-guards" => {
			let ([input], Some(_)) = (inputs.as_slice(), runtime.build_guard) else {
				return Err(Error::sentinel(ErrorKind::GuardBuilderAbsent));
			};
			build_tuple_guards_intrinsic(state, trace, *input, runtime)
		}
		"declaration-uid" => Ok((build_declaration_uid_intrinsic(state, trace)?, Vec::new())),
		"symbol-id" => {
			let ([input], true) = (inputs.as_slice(), is_call_expression(trace)) else {
				return Err(invalid_macro(trace, "Invalid symbol-id intrinsic usage"));
			};
			Ok((build_symbol_id_intrinsic(state, trace, *input)?, Vec::new()))
		}
		other => Err(invalid_macro(
			trace,
			format!(
				"Unexpected intrinsic ID {} with {} inputs",
				rotor_support::gostr::quote(other),
				inputs.len()
			),
		)),
	}
}

pub(crate) fn build_tuple_guards_intrinsic(
	state: &mut TransformState<'_>,
	trace: P<Node>,
	target: P<Type>,
	runtime: &MacroRuntime,
) -> Result<(P<Node>, Vec<P<Node>>), Error> {
	let Some(build_guard) = runtime.build_guard else {
		return Err(Error::sentinel(ErrorKind::GuardBuilderAbsent));
	};
	if state.ck().is_array_like_type(target) && !target.is_tuple_type() {
		let type_arguments = state.ck().get_type_arguments(target).to_vec();
		let [argument] = type_arguments.as_slice() else {
			let text = state.ck().type_to_string(target);
			return Err(invalid_macro(
				trace,
				format!("Intrinsic encountered invalid array type: {text}"),
			));
		};
		let result = build_guard(state, trace, *argument)?;
		let factory = &state.factory;
		let fixed = factory.new_array_literal_expression(factory.new_node_list(Vec::new()), true);
		let guard = result.expression.unwrap_or_else(|| factory.new_identifier("undefined"));
		let pair = factory.new_array_literal_expression(factory.new_node_list(vec![fixed, guard]), true);
		return Ok((pair, result.statements));
	}
	if !target.is_tuple_type() {
		let text = state.ck().type_to_string(target);
		return Err(invalid_macro(
			trace,
			format!("Intrinsic encountered non-tuple type: {text}"),
		));
	}
	let mut fixed_guards = Vec::new();
	let mut statements = Vec::new();
	let mut rest_guard = state.factory.new_identifier("undefined");
	let element_infos = target.target_tuple_type().element_infos();
	let type_arguments = state.ck().get_type_arguments(target).to_vec();
	for (index, item_type) in type_arguments.into_iter().enumerate() {
		let info = element_infos.get(index);
		let guard_trace = info.and_then(|info| info.labeled_declaration()).unwrap_or(trace);
		let result = build_guard(state, guard_trace, item_type)?;
		let guard = result
			.expression
			.unwrap_or_else(|| state.factory.new_identifier("undefined"));
		if info.is_some_and(|info| info.flags.intersects(ElementFlags::Rest)) {
			rest_guard = guard;
		} else {
			fixed_guards.push(guard);
		}
		statements.extend(result.statements);
	}
	let factory = &state.factory;
	let fixed = factory.new_array_literal_expression(factory.new_node_list(fixed_guards), true);
	Ok((
		factory.new_array_literal_expression(factory.new_node_list(vec![fixed, rest_guard]), true),
		statements,
	))
}

/// `hashText`: `text` itself, or its hash when obfuscating (or `force`d).
pub(crate) fn hash_text(
	state: &mut TransformState<'_>,
	text: &str,
	context: &str,
	force: bool,
) -> Result<String, Error> {
	if !force && !state.project.config.obfuscation {
		return Ok(text.to_owned());
	}
	state
		.hash_string(text, context)
		.map_err(|error| error.wrap("hash Flamework text"))
}

/// `isUndefinedAsNever`
pub(crate) fn is_undefined_as_never(expression: P<Node>) -> bool {
	is_as_expression(expression) && expression.expression().is_some_and(is_undefined_expression)
}
