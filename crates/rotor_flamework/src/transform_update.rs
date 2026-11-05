//! `transform_update.go`: the component config's attribute and instance guards.

use std::collections::HashSet;

use rotor_tsrs::P;
use rotor_tsrs::ast::{
	Node, SymbolFlags, TokenFlags, get_class_extends_heritage_element, is_class_declaration, is_identifier,
	is_object_literal_expression, is_property_assignment, is_string_literal,
};
use rotor_tsrs::checker::{Type, TypeFlags};

use crate::Error;
use crate::guard::build_flamework_guard;
use crate::transform_component::{
	COMPONENT_ATTRIBUTES_METADATA, COMPONENT_INSTANCE_METADATA, flamework_symbol_requests_metadata,
};
use crate::transform_macro_types::alloc;
use crate::transform_state::TransformState;

/// `updateFlameworkComponentConfig`
pub(crate) fn update_flamework_component_config(
	state: &mut TransformState<'_>,
	class_declaration: P<Node>,
	properties: Vec<P<Node>>,
) -> Result<Vec<P<Node>>, Error> {
	let updated = update_flamework_attribute_guards(state, class_declaration, properties)?;
	update_flamework_instance_guard(state, class_declaration, updated)
}

fn update_flamework_attribute_guards(
	state: &mut TransformState<'_>,
	class_declaration: P<Node>,
	properties: Vec<P<Node>>,
) -> Result<Vec<P<Node>>, Error> {
	let class_type = state.ck().get_type_at_location(class_declaration);
	let attribute_symbol = state.ck().get_property_of_type(class_type, "attributes");
	let Some(attribute_symbol) = attribute_symbol else {
		return Ok(properties);
	};
	if !flamework_symbol_requests_metadata(state, Some(attribute_symbol), COMPONENT_ATTRIBUTES_METADATA) {
		return Ok(properties);
	}
	let Some(attribute_type) = state
		.ck()
		.get_type_of_symbol_at_location(attribute_symbol, Some(class_declaration))
	else {
		return Ok(properties);
	};
	let custom = named_property(&properties, "attributes");
	let omitted = omitted_flamework_attribute_guards(
		state,
		class_declaration,
		attribute_type,
		custom.map(|(_, property)| property),
	);
	let mut guards = Vec::new();
	for &property in state.ck().get_properties_of_type(attribute_type) {
		if omitted.contains(property.name()) {
			continue;
		}
		let property_type = state.ck().get_type_of_property_of_type(attribute_type, property.name());
		let guard = property_type
			.ok_or_else(|| Error::msg("unknown attribute type"))
			.and_then(|property_type| build_flamework_guard(state, property_type, class_declaration))
			.map_err(|error| {
				error.wrap(&format!(
					"generate component attribute guard {}",
					rotor_support::gostr::quote(property.name())
				))
			})?;
		guards.push(
			state.factory.new_property_assignment(
				None,
				state
					.factory
					.new_string_literal(alloc(property.name()), TokenFlags::None),
				None,
				None,
				guard,
			),
		);
	}
	if let Some((custom_index, custom_attributes)) = custom
		&& is_property_assignment(custom_attributes)
		&& let Some(initializer) = custom_attributes
			.initializer()
			.filter(|initializer| is_object_literal_expression(*initializer))
	{
		let mut merged = initializer.properties().to_vec();
		merged.extend(guards);
		let assignment = custom_attributes.as_property_assignment();
		let updated_property = state.factory.update_property_assignment(
			custom_attributes,
			custom_attributes.modifiers(),
			assignment.name(),
			assignment.postfix_token(),
			assignment.type_(),
			state
				.factory
				.update_object_literal_expression(initializer, state.factory.new_node_list(merged), true),
		);
		let mut updated = properties;
		updated[custom_index] = updated_property;
		return Ok(updated);
	}
	let attribute_guards = state
		.factory
		.new_object_literal_expression(state.factory.new_node_list(guards), true);
	let mut properties = properties;
	properties.push(state.factory.new_property_assignment(
		None,
		state.factory.new_string_literal("attributes", TokenFlags::None),
		None,
		None,
		attribute_guards,
	));
	Ok(properties)
}

fn omitted_flamework_attribute_guards(
	state: &mut TransformState<'_>,
	class_declaration: P<Node>,
	attribute_type: P<Type>,
	custom_attributes: Option<P<Node>>,
) -> HashSet<String> {
	let mut omitted = object_property_names(custom_attributes);
	let Some(super_class) = flamework_direct_superclass(state, class_declaration) else {
		return omitted;
	};
	let super_type = state.ck().get_type_at_location(super_class);
	let Some(super_attributes) = state.ck().get_property_of_type(super_type, "attributes") else {
		return omitted;
	};
	let Some(super_attribute_type) = state
		.ck()
		.get_type_of_symbol_at_location(super_attributes, Some(super_class))
	else {
		return omitted;
	};
	for &property in state.ck().get_properties_of_type(super_attribute_type) {
		let property_type = state.ck().get_type_of_property_of_type(attribute_type, property.name());
		let super_property_type = state
			.ck()
			.get_type_of_property_of_type(super_attribute_type, property.name());
		if property_type.is_some() && property_type == super_property_type {
			omitted.insert(property.name().to_owned());
		}
	}
	omitted
}

fn update_flamework_instance_guard(
	state: &mut TransformState<'_>,
	class_declaration: P<Node>,
	mut properties: Vec<P<Node>>,
) -> Result<Vec<P<Node>>, Error> {
	let class_type = state.ck().get_type_at_location(class_declaration);
	let Some(instance_symbol) = state.ck().get_property_of_type(class_type, "instance") else {
		return Ok(properties);
	};
	if !flamework_symbol_requests_metadata(state, Some(instance_symbol), COMPONENT_INSTANCE_METADATA) {
		return Ok(properties);
	}
	if named_property(&properties, "instanceGuard").is_some() {
		return Ok(properties);
	}
	let Some(super_class) = flamework_direct_superclass(state, class_declaration) else {
		return Ok(properties);
	};
	let super_type = state.ck().get_type_at_location(super_class);
	let Some(super_symbol) = state.ck().get_property_of_type(super_type, "instance") else {
		return Ok(properties);
	};
	let instance_type = state
		.ck()
		.get_type_of_symbol_at_location(instance_symbol, Some(class_declaration));
	let super_instance_type = state
		.ck()
		.get_type_of_symbol_at_location(super_symbol, Some(super_class));
	let (Some(instance_type), Some(super_instance_type)) = (instance_type, super_instance_type) else {
		return Ok(properties);
	};
	if super_instance_type.flags().intersects(TypeFlags::TypeParameter) {
		if state.ck().get_default_from_type_parameter(super_instance_type) == Some(instance_type) {
			return Ok(properties);
		}
	} else if state.ck().is_type_assignable_to(super_instance_type, instance_type) {
		return Ok(properties);
	}
	let guard = build_flamework_guard(state, instance_type, class_declaration)
		.map_err(|error| error.wrap("generate component instance guard"))?;
	properties.push(state.factory.new_property_assignment(
		None,
		state.factory.new_string_literal("instanceGuard", TokenFlags::None),
		None,
		None,
		guard,
	));
	Ok(properties)
}

/// `flameworkDirectSuperclass`: the class declaration the class extends.
fn flamework_direct_superclass(state: &mut TransformState<'_>, class_declaration: P<Node>) -> Option<P<Node>> {
	let extends = get_class_extends_heritage_element(class_declaration)?;
	let mut symbol = state.ck().get_symbol_at_location(extends.expression()?)?;
	if symbol.flags().intersects(SymbolFlags::Alias) {
		symbol = state.ck().get_aliased_symbol(symbol);
	}
	symbol
		.declarations()
		.iter()
		.copied()
		.find(|declaration| is_class_declaration(*declaration))
}

fn named_property(properties: &[P<Node>], name: &str) -> Option<(usize, P<Node>)> {
	properties
		.iter()
		.enumerate()
		.find(|(_, property)| {
			property
				.name()
				.is_some_and(|property_name| property_name.text() == name)
		})
		.map(|(index, property)| (index, *property))
}

fn object_property_names(property: Option<P<Node>>) -> HashSet<String> {
	let mut names = HashSet::new();
	let Some(initializer) = property
		.filter(|property| is_property_assignment(*property))
		.and_then(|property| property.initializer())
		.filter(|initializer| is_object_literal_expression(*initializer))
	else {
		return names;
	};
	for &child in initializer.properties() {
		if let Some(name) = child.name()
			&& (is_identifier(name) || is_string_literal(name))
		{
			names.insert(name.text().to_owned());
		}
	}
	names
}
