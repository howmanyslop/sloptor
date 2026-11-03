//! `guard_factory.go`: guard node construction, nominal Instance guards and literals.

use rotor_tsrs::P;
use rotor_tsrs::ast::{
	Kind, Node, NodeFactory, NodeFlags, SymbolFlags, TokenFlags, get_name_of_declaration, get_source_file_of_node,
};
use rotor_tsrs::checker::{LiteralValue, Type, TypeFlags};

use crate::Error;
use crate::guard::GuardGenerator;
use crate::guard_diagnostics::{GuardGenerationError, GuardRelatedInformation};
use crate::transform_diagnostics::{node_token_range, text_pos};
use crate::transform_macro_types::alloc;

/// `guardProperty`
pub(crate) struct GuardProperty {
	pub(crate) name: String,
	pub(crate) guard: P<Node>,
}

/// `guardField`: `object["field"]`.
pub(crate) fn guard_field(factory: &NodeFactory, object: &str, field: &str) -> P<Node> {
	factory.new_element_access_expression(
		factory.new_identifier(alloc(object)),
		None,
		factory.new_string_literal(alloc(field), TokenFlags::None),
		NodeFlags::None,
	)
}

/// `guardCall`: `t["field"](arguments...)`.
pub(crate) fn guard_call(factory: &NodeFactory, field: &str, arguments: Vec<P<Node>>) -> P<Node> {
	factory.new_call_expression(
		guard_field(factory, "t", field),
		None,
		None,
		factory.new_node_list(arguments),
		NodeFlags::None,
	)
}

/// `guardListCall`: `t.field(a, b)`, or `t.fieldList([...])` past two arguments.
pub(crate) fn guard_list_call(factory: &NodeFactory, field: &str, arguments: Vec<P<Node>>) -> P<Node> {
	if arguments.len() <= 2 {
		return guard_call(factory, field, arguments);
	}
	let list = factory.new_array_literal_expression(factory.new_node_list(arguments), false);
	guard_call(factory, &format!("{field}List"), vec![list])
}

/// `guardLiteral`: a literal type's or enum member's value as an expression.
pub(crate) fn guard_literal(factory: &NodeFactory, value: Option<LiteralValue>) -> P<Node> {
	match value {
		Some(LiteralValue::String(text)) => factory.new_string_literal(text, TokenFlags::None),
		Some(LiteralValue::Boolean(true)) => factory.new_token(Kind::TrueKeyword),
		Some(LiteralValue::Boolean(false)) => factory.new_token(Kind::FalseKeyword),
		None => factory.new_token(Kind::NullKeyword),
		Some(LiteralValue::Number(number)) => factory.new_numeric_literal(alloc(&number.to_string()), TokenFlags::None),
		// Go's fmt.Sprint of a PseudoBigInt struct.
		Some(LiteralValue::BigInt(big)) => factory.new_numeric_literal(
			alloc(&format!("{{{} {}}}", big.negative, big.base10_value)),
			TokenFlags::None,
		),
	}
}

/// `guardObject`: `{ "name": guard, ... }`.
pub(crate) fn guard_object(factory: &NodeFactory, properties: Vec<GuardProperty>) -> P<Node> {
	let nodes = properties
		.into_iter()
		.map(|property| {
			factory.new_property_assignment(
				None,
				factory.new_string_literal(alloc(&property.name), TokenFlags::None),
				None,
				None,
				property.guard,
			)
		})
		.collect();
	factory.new_object_literal_expression(factory.new_node_list(nodes), false)
}

/// `guardVariable`: `const name = initializer`.
pub(crate) fn guard_variable(factory: &NodeFactory, name: &str, initializer: P<Node>) -> P<Node> {
	let declaration =
		factory.new_variable_declaration(factory.new_identifier(alloc(name)), None, None, Some(initializer));
	let declarations =
		factory.new_variable_declaration_list(factory.new_node_list(vec![declaration]), NodeFlags::Const);
	factory.new_variable_statement(None, declarations)
}

impl GuardGenerator<'_, '_> {
	pub(crate) fn build_instance(&mut self, type_value: P<Type>) -> Result<P<Node>, Error> {
		let properties = self.state.ck().get_properties_of_type(type_value);
		let nominal_names: Vec<&str> = properties
			.iter()
			.filter_map(|property| property.name().strip_prefix("_nominal_"))
			.collect();
		let mut specific = type_value;
		let mut specific_count = 0;
		for name in nominal_names {
			let Some(symbol) = self
				.state
				.ck()
				.resolve_name_exported(name, None, SymbolFlags::Type, false)
			else {
				continue;
			};
			let Some(&declaration) = symbol.declarations().first() else {
				continue;
			};
			let candidate = self.state.ck().get_type_at_location(declaration);
			let candidate_count = self.nominal_properties(candidate).len();
			if candidate_count <= specific_count {
				continue;
			}
			specific = candidate;
			specific_count = candidate_count;
		}
		let Some(specific_symbol) = specific.symbol() else {
			return Err(self.fail(type_value, "could not resolve nominal Instance type"));
		};
		for nominal in self.nominal_properties(type_value) {
			if self.state.ck().get_property_of_type(specific, &nominal).is_none() {
				return Err(self.fail(type_value, "intersection between nominal types is forbidden"));
			}
		}
		let factory = self.state.factory.clone();
		let base = guard_call(
			&factory,
			"instanceIsA",
			vec![factory.new_string_literal(alloc(specific_symbol.name()), TokenFlags::None)],
		);
		let mut additional = Vec::new();
		for property in properties {
			if self
				.state
				.ck()
				.get_property_of_type(specific, property.name())
				.is_some()
			{
				continue;
			}
			let property_type = self
				.state
				.ck()
				.get_type_of_property_of_type(type_value, property.name());
			let guard = self.build_optional(type_value, property_type)?;
			additional.push(GuardProperty {
				name: property.name().to_owned(),
				guard,
			});
		}
		if additional.is_empty() {
			return Ok(base);
		}
		let children = guard_call(&factory, "children", vec![guard_object(&factory, additional)]);
		Ok(guard_call(&factory, "intersection", vec![base, children]))
	}

	/// `g.build` of a type Go would hold as a possibly-nil pointer.
	pub(crate) fn build_optional(&mut self, owner: P<Type>, type_value: Option<P<Type>>) -> Result<P<Node>, Error> {
		match type_value {
			Some(type_value) => self.build(type_value),
			None => Err(self.fail(owner, "unknown type was encountered")),
		}
	}

	pub(crate) fn nominal_properties(&mut self, type_value: P<Type>) -> Vec<String> {
		self.state
			.ck()
			.get_properties_of_type(type_value)
			.iter()
			.filter(|property| property.name().starts_with("_nominal_"))
			.map(|property| property.name().to_owned())
			.collect()
	}

	pub(crate) fn literal(&mut self, type_value: P<Type>) -> Option<P<Node>> {
		let literal = self.literal_value(type_value)?;
		Some(guard_call(&self.state.factory, "literal", vec![literal]))
	}

	pub(crate) fn literal_value(&mut self, type_value: P<Type>) -> Option<P<Node>> {
		if type_value.flags().intersects(TypeFlags::Literal) {
			return Some(guard_literal(&self.state.factory, type_value.as_literal_type().value()));
		}
		if !type_value.flags().intersects(TypeFlags::Enum) {
			return None;
		}
		let symbol = type_value.symbol().filter(|symbol| symbol.declarations().len() == 1)?;
		let declaration = symbol.declarations()[0];
		if !rotor_tsrs::ast::is_enum_declaration(declaration) {
			return None;
		}
		let members = declaration.members();
		let mut values = Vec::with_capacity(members.len());
		for member in members {
			let value = self.state.ck().get_constant_value(*member)?;
			values.push(guard_literal(&self.state.factory, Some(value)));
		}
		Some(guard_list_call(&self.state.factory, "literal", values))
	}

	pub(crate) fn build_all(&mut self, types: &[P<Type>]) -> Result<Vec<P<Node>>, Error> {
		let mut guards = Vec::with_capacity(types.len());
		for type_value in types {
			guards.push(self.build(*type_value)?);
		}
		Ok(guards)
	}

	pub(crate) fn is_named_type(&mut self, type_value: P<Type>, names: &[&str]) -> bool {
		let Some(symbol) = type_value.symbol() else {
			return false;
		};
		for name in names {
			let resolved = self
				.state
				.ck()
				.resolve_name_exported(name, None, SymbolFlags::Type, false);
			if resolved == Some(symbol) || (resolved.is_none() && symbol.name() == *name) {
				return true;
			}
		}
		false
	}

	pub(crate) fn unique_dedup_name(&mut self, preferred: &str) -> String {
		let preferred = preferred.trim();
		let preferred = if preferred.is_empty() { "dedup" } else { preferred };
		let file_name = self.source_file_name();
		self.state.next_generated_name(&file_name, preferred)
	}

	/// `fail`: a [`GuardGenerationError`] with the path of types being generated.
	pub(crate) fn fail(&mut self, type_value: P<Type>, reason: &str) -> Error {
		let mut source = self.source;
		if reason == "intersection between nominal types is forbidden"
			&& let Some(file) = get_source_file_of_node(source)
		{
			source = file.as_node();
		}
		let (file, start, _) = node_token_range(source);
		let mut error = GuardGenerationError {
			type_name: self.state.ck().type_to_string(type_value),
			reason: reason.to_owned(),
			file_name: file.map_or_else(String::new, |file| file.file_name().to_owned()),
			start,
			end: text_pos(source.end()),
			..GuardGenerationError::default()
		};
		let mut previous_type: Option<P<Type>> = None;
		let tracking: Vec<(P<Node>, P<Type>)> = self
			.tracking
			.iter()
			.map(|location| (location.node, location.type_value))
			.collect();
		for (node, location_type) in tracking {
			if previous_type == Some(location_type) {
				continue;
			}
			previous_type = Some(location_type);
			let node = get_name_of_declaration(node).unwrap_or(node);
			let file_name = get_source_file_of_node(node).map_or_else(String::new, |file| file.file_name().to_owned());
			let type_name = self.state.ck().type_to_string(location_type);
			error.path.push(format!("{file_name}:{type_name}"));
			error.related_information.push(GuardRelatedInformation {
				type_name,
				file_name,
				start: text_pos(node.pos()),
				end: text_pos(node.end()),
			});
		}
		error.into_error()
	}
}
