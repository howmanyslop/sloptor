//! `transform_network.go`: obfuscated objects, shuffled arrays and networking middleware.

use rotor_tsrs::P;
use rotor_tsrs::ast::{Node, Symbol, TokenFlags, is_object_literal_expression, is_property_assignment};
use rotor_tsrs::checker::{Signature, Type};

use crate::Error;
use crate::transform_intrinsics::{build_many_macro, hash_text};
use crate::transform_macro_resolution::user_macro_of_many;
use crate::transform_macro_types::{MacroRuntime, alloc, invalid_macro, string_literal_value};
use crate::transform_parameters::parameter_index;
use crate::transform_state::TransformState;

/// `buildObfuscatedObjectIntrinsic`
pub(crate) fn build_obfuscated_object_intrinsic(
	state: &mut TransformState<'_>,
	trace: P<Node>,
	inputs: &[P<Type>],
	runtime: &MacroRuntime,
) -> Result<(P<Node>, Vec<P<Node>>), Error> {
	let [inner, context_type] = inputs else {
		return Err(invalid_macro(trace, "Invalid obfuscate-obj intrinsic usage"));
	};
	let Some(mut user_macro) = user_macro_of_many(state, trace, *inner)? else {
		return Err(invalid_macro(trace, "Intrinsic obfuscate-obj received no inner macro"));
	};
	let context = if context_type.is_string_literal() {
		string_literal_value(*context_type)
	} else {
		String::new()
	};
	if let Some(properties) = user_macro.properties.take() {
		let mut properties = shuffled(state, &properties, runtime)?;
		for property in &mut properties {
			property.name = hash_text(state, &property.name, &context, false)?;
		}
		user_macro.properties = Some(properties);
	}
	build_many_macro(state, trace, &user_macro, runtime)
}

/// `buildShuffleArrayIntrinsic`
pub(crate) fn build_shuffle_array_intrinsic(
	state: &mut TransformState<'_>,
	trace: P<Node>,
	inputs: &[P<Type>],
	runtime: &MacroRuntime,
) -> Result<(P<Node>, Vec<P<Node>>), Error> {
	let [inner] = inputs else {
		return Err(invalid_macro(trace, "Invalid shuffle-array intrinsic usage"));
	};
	let Some(mut user_macro) = user_macro_of_many(state, trace, *inner)? else {
		return Err(invalid_macro(trace, "Intrinsic shuffle-array received no inner macro"));
	};
	if let Some(items) = user_macro.items.take() {
		user_macro.items = Some(shuffled(state, &items, runtime)?);
	}
	build_many_macro(state, trace, &user_macro, runtime)
}

/// `transformNetworkingMiddlewareIntrinsic`: obfuscates each `middleware` object passed for
/// the given parameters.
pub(crate) fn transform_networking_middleware_intrinsic(
	state: &mut TransformState<'_>,
	signature: P<Signature>,
	arguments: &mut [P<Node>],
	parameters: &[P<Symbol>],
	runtime: &MacroRuntime,
) -> Result<(), Error> {
	for parameter in parameters {
		let Some(index) = parameter_index(signature, *parameter).filter(|index| *index < arguments.len()) else {
			continue;
		};
		if !is_object_literal_expression(arguments[index]) {
			continue;
		}
		let mut properties: Vec<P<Node>> = arguments[index].properties().to_vec();
		for slot in &mut properties {
			let property = *slot;
			if !is_property_assignment(property) || property.name().is_none_or(|name| name.text() != "middleware") {
				continue;
			}
			let Some(initializer) = property.initializer() else {
				continue;
			};
			if !is_object_literal_expression(initializer) {
				return Err(invalid_macro(initializer, "Networking middleware must be an object."));
			}
			let obfuscated = obfuscate_middleware_object(state, initializer, runtime)?;
			let assignment = property.as_property_assignment();
			*slot = state.factory.update_property_assignment(
				property,
				property.modifiers(),
				assignment.name(),
				assignment.postfix_token(),
				assignment.type_(),
				obfuscated,
			);
		}
		arguments[index] = state
			.factory
			.new_object_literal_expression(state.factory.new_node_list(properties), true);
	}
	Ok(())
}

pub(crate) fn obfuscate_middleware_object(
	state: &mut TransformState<'_>,
	node: P<Node>,
	runtime: &MacroRuntime,
) -> Result<P<Node>, Error> {
	let mut properties = shuffled(state, node.properties(), runtime)?;
	for property in &mut properties {
		let current = *property;
		if !is_property_assignment(current) || current.name().is_none_or(|name| name.text().is_empty()) {
			continue;
		}
		let assignment = current.as_property_assignment();
		let mut initializer = assignment.initializer();
		if is_object_literal_expression(initializer) {
			initializer = obfuscate_middleware_object(state, initializer, runtime)?;
		}
		let original = assignment.name().text();
		let hash = hash_text(state, original, "remotes", false)?;
		let factory = &state.factory;
		let literal = factory.new_string_literal(original, TokenFlags::None);
		let computed = factory.new_computed_property_name(factory.new_as_expression(
			factory.new_string_literal(alloc(&hash), TokenFlags::None),
			factory.new_literal_type_node(literal),
		));
		*property = factory.update_property_assignment(
			current,
			current.modifiers(),
			computed,
			assignment.postfix_token(),
			assignment.type_(),
			initializer,
		);
	}
	let multi_line = node.as_object_literal_expression().multi_line;
	Ok(state
		.factory
		.new_object_literal_expression(state.factory.new_node_list(properties), multi_line))
}

/// `shuffledProperties`, `shuffledMacros` and `shuffledNodes`.
fn shuffled<T: Clone>(state: &TransformState<'_>, values: &[T], runtime: &MacroRuntime) -> Result<Vec<T>, Error> {
	let indexes = shuffled_indexes(state, values.len(), runtime)?;
	Ok(indexes.into_iter().map(|source| values[source].clone()).collect())
}

/// `shuffledIndexes`: a Fisher-Yates shuffle with the runtime's random indexes when
/// obfuscating, else the identity.
pub(crate) fn shuffled_indexes(
	state: &TransformState<'_>,
	length: usize,
	runtime: &MacroRuntime,
) -> Result<Vec<usize>, Error> {
	let mut indexes: Vec<usize> = (0..length).collect();
	if !state.project.config.obfuscation {
		return Ok(indexes);
	}
	let Some(random_index) = &runtime.random_index else {
		return Err(invalid_macro(None, "Flamework random-index runtime is not configured"));
	};
	for index in (1..length).rev() {
		let target = random_index(index + 1).map_err(|error| error.wrap("shuffle Flamework metadata"))?;
		if target > index {
			return Err(invalid_macro(
				None,
				format!("Flamework random index {target} is outside [0, {})", index + 1),
			));
		}
		indexes.swap(index, target);
	}
	Ok(indexes)
}
