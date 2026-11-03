//! `guard.go`: generating `@rbxts/t` guards from types.

use std::collections::{HashMap, HashSet};

use rotor_tsrs::P;
use rotor_tsrs::ast::{
	Node, SymbolFlags, get_source_file_of_node, is_conditional_type_node, is_type_alias_declaration,
	is_type_parameter_declaration, is_type_reference_node,
};
use rotor_tsrs::checker::{SignatureKind, Type, TypeFlags};

use crate::guard_dedup::guard_types_requiring_dedup;
use crate::guard_factory::{guard_call, guard_field, guard_list_call, guard_variable};
use crate::guard_types::{ROBLOX_GUARD_TYPES, guard_declaration, guard_type_arguments};
use crate::transform_macro_types::GuardBuildResult;
use crate::transform_state::TransformState;
use crate::{Error, ErrorKind};

/// `guardGenerator`
pub(crate) struct GuardGenerator<'s, 'a> {
	pub(crate) state: &'s mut TransformState<'a>,
	pub(crate) source: P<Node>,
	pub(crate) requires: HashSet<P<Type>>,
	pub(crate) dedup_ids: HashMap<P<Type>, String>,
	pub(crate) statements: Vec<P<Node>>,
	pub(crate) active: HashSet<P<Type>>,
	pub(crate) tracking: Vec<GuardTracking>,
}

/// `guardTracking`
pub(crate) struct GuardTracking {
	pub(crate) node: P<Node>,
	pub(crate) type_value: P<Type>,
}

/// `buildFlameworkGuard`
pub(crate) fn build_flamework_guard(
	state: &mut TransformState<'_>,
	type_value: P<Type>,
	source: P<Node>,
) -> Result<P<Node>, Error> {
	let result = build_flamework_guard_with_dedup(state, type_value, source, None)?;
	result
		.expression
		.ok_or_else(|| Error::kinded(ErrorKind::InvalidTransformInput, ": guard generator produced no guard"))
}

/// `buildFlameworkGuardWithDedup`
pub(crate) fn build_flamework_guard_with_dedup(
	state: &mut TransformState<'_>,
	type_value: P<Type>,
	source: P<Node>,
	dedup_limit: Option<i64>,
) -> Result<GuardBuildResult, Error> {
	let requires = dedup_limit.map_or_else(HashSet::new, |limit| {
		guard_types_requiring_dedup(type_value, &mut state.ck(), limit.max(1))
	});
	let mut generator = GuardGenerator {
		state,
		source,
		requires,
		dedup_ids: HashMap::new(),
		statements: Vec::new(),
		active: HashSet::new(),
		tracking: Vec::new(),
	};
	let guard = generator.build(type_value)?;
	Ok(GuardBuildResult {
		expression: Some(guard),
		statements: generator.statements,
	})
}

/// `buildConfiguredFlameworkGuard`: the dedup limit comes from the project's configuration.
pub(crate) fn build_configured_flamework_guard(
	state: &mut TransformState<'_>,
	type_value: P<Type>,
	source: P<Node>,
) -> Result<GuardBuildResult, Error> {
	let limit = state.project.config.optimizations.guard_generation_dedup_limit;
	build_flamework_guard_with_dedup(state, type_value, source, limit)
}

/// `buildFlameworkGuardForMacro`: the default runtime's guard builder.
pub(crate) fn build_flamework_guard_for_macro(
	state: &mut TransformState<'_>,
	source: P<Node>,
	type_value: P<Type>,
) -> Result<GuardBuildResult, Error> {
	build_configured_flamework_guard(state, type_value, source)
}

impl GuardGenerator<'_, '_> {
	pub(crate) fn build(&mut self, type_value: P<Type>) -> Result<P<Node>, Error> {
		if let Some(id) = self.dedup_ids.get(&type_value) {
			return Ok(self
				.state
				.factory
				.new_identifier(crate::transform_macro_types::alloc(id)));
		}
		if self.active.contains(&type_value) {
			return Err(self.fail(
				type_value,
				"recursive types cannot be represented without a previously emitted guard",
			));
		}
		let tracked = guard_declaration(type_value).is_some_and(|declaration| {
			self.tracking.push(GuardTracking {
				node: declaration,
				type_value,
			});
			true
		});
		self.active.insert(type_value);
		let guard = self.build_inner(type_value);
		self.active.remove(&type_value);
		if tracked {
			self.tracking.pop();
		}
		let guard = guard?;
		if self.requires.contains(&type_value) {
			let name = type_value
				.alias()
				.and_then(|alias| alias.symbol())
				.map_or_else(|| "dedup".to_owned(), |symbol| symbol.name().to_owned());
			let name = self.unique_dedup_name(&name);
			self.dedup_ids.insert(type_value, name.clone());
			self.statements.push(guard_variable(&self.state.factory, &name, guard));
			return Ok(self
				.state
				.factory
				.new_identifier(crate::transform_macro_types::alloc(&name)));
		}
		Ok(guard)
	}

	#[expect(clippy::too_many_lines, reason = "one branch per type kind, in Go's order")]
	fn build_inner(&mut self, type_value: P<Type>) -> Result<P<Node>, Error> {
		if type_value.is_union() {
			return self.build_union(type_value);
		}
		if self
			.state
			.ck()
			.get_property_of_type(type_value, "_nominal_Instance")
			.is_some()
		{
			return self.build_instance(type_value);
		}
		if type_value.is_intersection() {
			return self.build_intersection(type_value);
		}
		let flags = type_value.flags();
		if flags.intersects(TypeFlags::Conditional) {
			let Some(declaration) = type_value
				.alias()
				.and_then(|alias| alias.symbol())
				.and_then(|symbol| (symbol.declarations().len() == 1).then(|| symbol.declarations()[0]))
			else {
				return Err(self.fail(type_value, "could not find conditional type declaration"));
			};
			let conditional = declaration
				.type_node()
				.filter(|node| is_type_alias_declaration(declaration) && is_conditional_type_node(*node));
			let Some(conditional) = conditional else {
				return Err(self.fail(type_value, "could not find conditional type branches"));
			};
			let conditional = conditional.as_conditional_type_node();
			for branch in [conditional.true_type, conditional.false_type] {
				if !is_type_reference_node(branch) {
					continue;
				}
				let Some(symbol) = self
					.state
					.ck()
					.get_symbol_at_location(branch.as_type_reference_node().type_name)
				else {
					continue;
				};
				if !symbol.flags().intersects(SymbolFlags::TypeParameter) || symbol.declarations().len() != 1 {
					continue;
				}
				let parameter = symbol.declarations()[0];
				if is_type_parameter_declaration(parameter)
					&& parameter.as_type_parameter_declaration().constraint.is_none()
				{
					return Err(self.fail(type_value, "could not find constraint of type parameter"));
				}
			}
			let true_type = self.state.ck().get_type_from_type_node(conditional.true_type);
			let true_guard = self.build(true_type)?;
			let false_type = self.state.ck().get_type_from_type_node(conditional.false_type);
			let false_guard = self.build(false_type)?;
			return Ok(guard_list_call(
				&self.state.factory,
				"union",
				vec![true_guard, false_guard],
			));
		}
		if flags.intersects(TypeFlags::TypeVariable) {
			let Some(constraint) = self.state.ck().get_base_constraint_of_type(type_value) else {
				return Err(self.fail(type_value, "could not find constraint of type parameter"));
			};
			return self.build(constraint);
		}
		if let Some(literal) = self.literal(type_value) {
			return Ok(literal);
		}
		let factory = self.state.factory.clone();
		if type_value.is_tuple_type() {
			let arguments = guard_type_arguments(type_value, &mut self.state.ck());
			let guards = self.build_all(&arguments)?;
			return Ok(guard_call(&factory, "strictArray", guards));
		}
		if self.is_named_type(type_value, &["Array", "ReadonlyArray"]) {
			let arguments = guard_type_arguments(type_value, &mut self.state.ck());
			let element = match arguments.first() {
				Some(first) => self.build(*first)?,
				None => guard_field(&factory, "t", "any"),
			};
			return Ok(guard_call(&factory, "array", vec![element]));
		}
		if !self
			.state
			.ck()
			.get_signatures_of_type(type_value, SignatureKind::Call)
			.is_empty()
		{
			return Ok(guard_field(&factory, "t", "callback"));
		}
		let ck = self.state.ck();
		if type_value == ck.get_void_type() || type_value == ck.get_undefined_type() {
			return Ok(guard_field(&factory, "t", "none"));
		}
		if type_value == ck.get_any_type() {
			return Ok(guard_field(&factory, "t", "any"));
		}
		if type_value == ck.get_string_type() {
			return Ok(guard_field(&factory, "t", "string"));
		}
		if type_value == ck.get_number_type() {
			return Ok(guard_field(&factory, "t", "number"));
		}
		if type_value == ck.get_boolean_type() {
			return Ok(guard_field(&factory, "t", "boolean"));
		}
		if flags.intersects(TypeFlags::Unknown) {
			return Ok(guard_call(
				&factory,
				"union",
				vec![guard_field(&factory, "t", "any"), guard_field(&factory, "t", "none")],
			));
		}
		if flags.intersects(TypeFlags::Any) {
			return Ok(guard_field(&factory, "t", "any"));
		}
		if flags.intersects(TypeFlags::TemplateLiteral) {
			return Err(self.fail(type_value, "template literal types are unsupported"));
		}
		let Some(symbol) = type_value.symbol() else {
			return Err(self.fail(type_value, "unknown type has no symbol"));
		};
		if self.is_named_type(type_value, &["Map", "ReadonlyMap", "WeakMap"]) {
			return self.build_map(type_value);
		}
		if self.is_named_type(type_value, &["Set", "ReadonlySet"]) {
			return self.build_set(type_value);
		}
		if self.is_named_type(type_value, &["Promise"]) {
			return Ok(guard_field(&factory, "Promise", "is"));
		}
		for name in ROBLOX_GUARD_TYPES {
			if !self.is_named_type(type_value, &[name]) {
				continue;
			}
			if name == "buffer" {
				return Ok(guard_call(
					&factory,
					"typeof",
					vec![factory.new_string_literal(name, rotor_tsrs::ast::TokenFlags::None)],
				));
			}
			return Ok(guard_field(&factory, "t", name));
		}
		if type_value.is_class() {
			return Err(self.fail(
				type_value,
				&format!("class {} is unsupported", rotor_support::gostr::quote(symbol.name())),
			));
		}
		if flags.intersects(TypeFlags::Object) {
			return self.build_object(type_value);
		}
		Err(self.fail(type_value, "unknown type was encountered"))
	}

	/// The source file of the guard's source node.
	pub(crate) fn source_file_name(&self) -> String {
		get_source_file_of_node(self.source).map_or_else(String::new, |file| file.file_name().to_owned())
	}
}
