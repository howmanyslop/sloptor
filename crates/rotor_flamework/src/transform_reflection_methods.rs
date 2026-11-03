//! `transform_reflection_methods.go`: reflection metadata for constructors, methods and fields.

use rotor_tsrs::P;
use rotor_tsrs::ast::{
	Node, TokenFlags, get_property_name_for_property_name_node, is_constructor_declaration, is_identifier,
	is_method_declaration, is_property_declaration,
};
use rotor_tsrs::checker::Type;

use crate::Error;
use crate::guard::build_flamework_guard;
use crate::metadata::Metadata;
use crate::plan::FilePlan;
use crate::transform_macro_types::alloc;
use crate::transform_reflection::{expression_array, new_reflect_call, string_array};
use crate::transform_reflection_metadata::{metadata_for_class, metadata_for_member};
use crate::transform_state::TransformState;
use crate::transform_symbols::{node_uid, type_uid};

/// `memberReflectionInput`
pub(crate) struct MemberReflectionInput<'p> {
	#[expect(dead_code, reason = "Go's memberReflectionInput keeps it; nothing reads it")]
	pub(crate) plan: &'p FilePlan,
	pub(crate) class_name: String,
	pub(crate) member: P<Node>,
}

struct ReflectionField {
	name: &'static str,
	value: P<Node>,
}

struct ReflectionTarget {
	class_name: String,
	property_name: String,
	member: P<Node>,
	metadata: Metadata,
}

/// `constructorReflectionStatements`
pub(crate) fn constructor_reflection_statements(
	state: &mut TransformState<'_>,
	class_name: &str,
	constructor: P<Node>,
) -> Result<Vec<P<Node>>, Error> {
	if !is_constructor_declaration(constructor) {
		return Ok(Vec::new());
	}
	let Some(parent) = constructor.parent() else {
		return Ok(Vec::new());
	};
	let metadata = metadata_for_class(state, parent);
	method_reflection_statements(
		state,
		&ReflectionTarget {
			class_name: class_name.to_owned(),
			property_name: String::new(),
			member: constructor,
			metadata,
		},
	)
}

/// `memberReflectionStatements`
pub(crate) fn member_reflection_statements(
	state: &mut TransformState<'_>,
	input: &MemberReflectionInput<'_>,
) -> Result<Vec<P<Node>>, Error> {
	let member = input.member;
	let Some(name) = member.name() else {
		return Ok(Vec::new());
	};
	if !is_method_declaration(member) && !is_property_declaration(member) {
		return Ok(Vec::new());
	}
	let target = ReflectionTarget {
		class_name: input.class_name.clone(),
		property_name: get_property_name_for_property_name_node(name).into_owned(),
		member,
		metadata: metadata_for_member(state, member),
	};
	if is_property_declaration(member) {
		return field_reflection_statements(state, &target);
	}
	method_reflection_statements(state, &target)
}

fn field_reflection_statements(
	state: &mut TransformState<'_>,
	target: &ReflectionTarget,
) -> Result<Vec<P<Node>>, Error> {
	let mut fields = Vec::with_capacity(2);
	let type_node = target.member.type_node();
	if target.metadata.requested("flamework:type") {
		let identifier = reflected_node_type_id(state, type_node, target.member)?;
		fields.push(ReflectionField {
			name: "flamework:type",
			value: state.factory.new_string_literal(alloc(&identifier), TokenFlags::None),
		});
	}
	if target.metadata.requested("flamework:guard") {
		let member_type = state.ck().get_type_at_location(target.member);
		let guard = build_flamework_guard(state, member_type, type_node.unwrap_or(target.member))?;
		fields.push(ReflectionField {
			name: "flamework:guard",
			value: guard,
		});
	}
	Ok(reflection_statements(state, target, fields))
}

fn method_reflection_statements(
	state: &mut TransformState<'_>,
	target: &ReflectionTarget,
) -> Result<Vec<P<Node>>, Error> {
	let signature = state.ck().get_signature_from_declaration(target.member);
	let mut fields = Vec::with_capacity(5);
	let method_type = target.member.type_node();
	let return_type = state.ck().get_return_type_of_signature(signature);
	let parameters = target.member.parameters();
	if target.metadata.requested("flamework:return_type") {
		let identifier = reflected_return_type_id(state, target.member, return_type)?;
		fields.push(ReflectionField {
			name: "flamework:return_type",
			value: state.factory.new_string_literal(alloc(&identifier), TokenFlags::None),
		});
	}
	if target.metadata.requested("flamework:return_guard") {
		let guard = build_flamework_guard(state, return_type, method_type.unwrap_or(target.member))?;
		fields.push(ReflectionField {
			name: "flamework:return_guard",
			value: guard,
		});
	}
	if target.metadata.requested("flamework:parameters") {
		let identifiers = reflected_parameter_ids(state, parameters)?;
		if !identifiers.is_empty() {
			fields.push(ReflectionField {
				name: "flamework:parameters",
				value: string_array(&state.factory, &identifiers),
			});
		}
	}
	if target.metadata.requested("flamework:parameter_names") {
		let names: Vec<String> = parameters
			.iter()
			.map(|parameter| {
				parameter
					.name()
					.filter(|name| is_identifier(*name))
					.map_or_else(|| "_binding_".to_owned(), |name| name.text().to_owned())
			})
			.collect();
		if !names.is_empty() {
			fields.push(ReflectionField {
				name: "flamework:parameter_names",
				value: string_array(&state.factory, &names),
			});
		}
	}
	if target.metadata.requested("flamework:parameter_guards") {
		let mut guards = Vec::with_capacity(parameters.len());
		for &parameter in parameters {
			let parameter_type = state.ck().get_type_at_location(parameter);
			guards.push(build_flamework_guard(state, parameter_type, parameter)?);
		}
		if !guards.is_empty() {
			fields.push(ReflectionField {
				name: "flamework:parameter_guards",
				value: expression_array(&state.factory, guards),
			});
		}
	}
	Ok(reflection_statements(state, target, fields))
}

/// `reflectedParameterIDs`
pub(crate) fn reflected_parameter_ids(
	state: &mut TransformState<'_>,
	parameters: &[P<Node>],
) -> Result<Vec<String>, Error> {
	let mut identifiers = Vec::with_capacity(parameters.len());
	for &parameter in parameters {
		identifiers.push(reflected_node_type_id(state, parameter.type_node(), parameter)?);
	}
	Ok(identifiers)
}

fn reflected_node_type_id(
	state: &mut TransformState<'_>,
	type_node: Option<P<Node>>,
	location: P<Node>,
) -> Result<String, Error> {
	if let Some(type_node) = type_node {
		return node_uid(state, type_node);
	}
	let location_type = state.ck().get_type_at_location(location);
	type_uid(state, location_type, location)
}

fn reflected_return_type_id(
	state: &mut TransformState<'_>,
	method: P<Node>,
	inferred_type: P<Type>,
) -> Result<String, Error> {
	if let Some(type_node) = method.type_node() {
		return node_uid(state, type_node);
	}
	type_uid(state, inferred_type, method)
}

fn reflection_statements(
	state: &TransformState<'_>,
	target: &ReflectionTarget,
	fields: Vec<ReflectionField>,
) -> Vec<P<Node>> {
	let factory = &state.factory;
	fields
		.into_iter()
		.map(|field| {
			let mut arguments = vec![
				factory.new_identifier(alloc(&target.class_name)),
				factory.new_string_literal(field.name, TokenFlags::None),
				field.value,
			];
			if !target.property_name.is_empty() {
				arguments.push(factory.new_string_literal(alloc(&target.property_name), TokenFlags::None));
			}
			new_reflect_call(factory, "defineMetadata", arguments)
		})
		.collect()
}
