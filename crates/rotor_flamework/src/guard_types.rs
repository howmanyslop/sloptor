//! `guard_types.go`: guards for unions, intersections, objects, maps and sets.

use rotor_tsrs::P;
use rotor_tsrs::ast::{Node, NodeFlags, Symbol, SymbolFlags, TokenFlags};
use rotor_tsrs::checker::{ObjectFlags, Type, TypeChecker, TypeFlags};

use crate::Error;
use crate::guard::{GuardGenerator, GuardTracking};
use crate::guard_factory::{GuardProperty, guard_call, guard_field, guard_list_call, guard_object};
use crate::transform_macro_types::alloc;

/// `robloxGuardTypes`
pub(crate) const ROBLOX_GUARD_TYPES: [&str; 35] = [
	"UDim",
	"UDim2",
	"BrickColor",
	"Color3",
	"Vector2",
	"Vector3",
	"NumberSequence",
	"NumberSequenceKeypoint",
	"ColorSequence",
	"ColorSequenceKeypoint",
	"NumberRange",
	"Rect",
	"DockWidgetPluginGuiInfo",
	"CFrame",
	"Axes",
	"Faces",
	"Font",
	"Instance",
	"Ray",
	"Random",
	"Region3",
	"Region3int16",
	"Enum",
	"TweenInfo",
	"PhysicalProperties",
	"Vector3int16",
	"Vector2int16",
	"PathWaypoint",
	"EnumItem",
	"RBXScriptSignal",
	"RBXScriptConnection",
	"FloatCurveKey",
	"OverlapParams",
	"thread",
	"buffer",
];

/// `guardTypeArguments`
pub(crate) fn guard_type_arguments(type_value: P<Type>, checker: &mut TypeChecker<'_>) -> Vec<P<Type>> {
	if !type_value.flags().intersects(TypeFlags::Object)
		|| !type_value.object_flags().intersects(ObjectFlags::Reference)
	{
		return Vec::new();
	}
	checker.get_type_arguments(type_value).to_vec()
}

/// `guardDeclaration`: the type symbol's first declaration.
pub(crate) fn guard_declaration(type_value: P<Type>) -> Option<P<Node>> {
	type_value.symbol()?.declarations().first().copied()
}

/// `guardIsDefinedType`
#[expect(dead_code, reason = "only guard_parity_gaps_test calls it, as in Go")]
pub(crate) fn guard_is_defined_type(type_value: P<Type>, checker: &mut TypeChecker<'_>) -> bool {
	type_value.flags() == TypeFlags::Object
		&& checker.get_properties_of_type(type_value).is_empty()
		&& checker.get_call_signatures(type_value).is_empty()
		&& checker.get_construct_signatures(type_value).is_empty()
		&& checker.get_number_index_type(type_value).is_none()
		&& checker.get_string_index_type(type_value).is_none()
}

struct EnumGroup {
	symbol: P<Symbol>,
	members: Vec<P<Type>>,
}

impl GuardGenerator<'_, '_> {
	pub(crate) fn build_union(&mut self, type_value: P<Type>) -> Result<P<Node>, Error> {
		let factory = self.state.factory.clone();
		if type_value == self.state.ck().get_boolean_type() {
			return Ok(guard_field(&factory, "t", "boolean"));
		}
		let mut optional = false;
		let mut guards = Vec::with_capacity(type_value.types().len());
		let mut literals = Vec::new();
		let enum_root = self
			.state
			.ck()
			.resolve_name_exported("Enum", None, SymbolFlags::Type, false);
		let mut enum_groups: Vec<EnumGroup> = Vec::new();
		// A union of both boolean literals is a full `boolean`: one guard, ahead of the rest.
		let collapses_to_boolean = boolean_literal_count(type_value.types()) == 2;
		if collapses_to_boolean {
			guards.push(guard_field(&factory, "t", "boolean"));
		}
		let undefined_type = self.state.ck().get_undefined_type();
		let void_type = self.state.ck().get_void_type();
		for &member in type_value.types() {
			if member == undefined_type || member == void_type {
				optional = true;
				continue;
			}
			if member.flags().intersects(TypeFlags::ESSymbolLike) {
				continue;
			}
			if collapses_to_boolean && member.flags().intersects(TypeFlags::BooleanLiteral) {
				continue;
			}
			if let Some(literal) = self.literal_value(member) {
				literals.push(literal);
				continue;
			}
			if let Some(enum_kind) = guard_roblox_enum_kind(member, enum_root, &mut self.state.ck()) {
				if let Some(group) = enum_groups.iter_mut().find(|group| group.symbol == enum_kind) {
					group.members.push(member);
				} else {
					enum_groups.push(EnumGroup {
						symbol: enum_kind,
						members: vec![member],
					});
				}
				continue;
			}
			guards.push(self.build(member)?);
		}
		for group in enum_groups {
			let export_count = group.symbol.exports().map_or(0, |exports| exports.len());
			if group.members.len() + 1 == export_count {
				let enum_expression = guard_field(&factory, "Enum", group.symbol.name());
				guards.push(guard_call(&factory, "enum", vec![enum_expression]));
				continue;
			}
			for member in group.members {
				let enum_expression = guard_field(&factory, "Enum", group.symbol.name());
				let member_name = member.symbol().map_or("", |symbol| symbol.name());
				literals.push(factory.new_element_access_expression(
					enum_expression,
					None,
					factory.new_string_literal(alloc(member_name), TokenFlags::None),
					NodeFlags::None,
				));
			}
		}
		if !literals.is_empty() {
			guards.push(guard_list_call(&factory, "literal", literals));
		}
		let has_guards = !guards.is_empty();
		let union = match guards.len() {
			0 => guard_field(&factory, "t", "none"),
			1 => guards[0],
			_ => guard_list_call(&factory, "union", guards),
		};
		if optional && has_guards {
			return Ok(guard_call(&factory, "optional", vec![union]));
		}
		Ok(union)
	}

	pub(crate) fn build_intersection(&mut self, type_value: P<Type>) -> Result<P<Node>, Error> {
		if self.state.ck().get_index_infos_of_type(type_value).len() > 1 {
			return Err(self.fail(
				type_value,
				"intersections with multiple index signatures are unsupported",
			));
		}
		for &member in type_value.types() {
			if member.flags().intersects(TypeFlags::DisjointDomains) {
				return self.build(member);
			}
		}
		let guards = self.build_all(type_value.types())?;
		Ok(guard_list_call(&self.state.factory, "intersection", guards))
	}

	pub(crate) fn build_object(&mut self, type_value: P<Type>) -> Result<P<Node>, Error> {
		let factory = self.state.factory.clone();
		let properties = self.state.ck().get_apparent_properties(type_value);
		let index_infos = self.state.ck().get_index_infos_of_type(type_value);
		if properties.is_empty() && index_infos.is_empty() {
			return Ok(guard_field(&factory, "t", "any"));
		}
		let mut guards = Vec::with_capacity(2);
		if !properties.is_empty() {
			let property_guards = self.build_properties(type_value, true)?;
			guards.push(guard_call(
				&factory,
				"interface",
				vec![guard_object(&factory, property_guards)],
			));
		}
		if index_infos.len() > 1 {
			return Err(self.fail(type_value, "types with multiple index signatures are unsupported"));
		}
		if let Some(info) = index_infos.first() {
			let key_guard = self.build(info.key_type())?;
			let value_guard = self.build(info.value_type())?;
			guards.push(guard_call(&factory, "map", vec![key_guard, value_guard]));
		}
		if guards.len() == 1 {
			return Ok(guards[0]);
		}
		Ok(guard_list_call(&factory, "intersection", guards))
	}

	pub(crate) fn build_properties(
		&mut self,
		type_value: P<Type>,
		interface_type: bool,
	) -> Result<Vec<GuardProperty>, Error> {
		let mut properties = Vec::new();
		for &property in self.state.ck().get_properties_of_type(type_value) {
			let Some(property_type) = self
				.state
				.ck()
				.get_type_of_property_of_type(type_value, property.name())
			else {
				let reason = format!(
					"could not find type for field {}",
					rotor_support::gostr::quote(property.name())
				);
				return Err(self.fail(type_value, &reason));
			};
			if interface_type
				&& property_type
					.flags()
					.intersects(TypeFlags::Unknown | TypeFlags::Never | TypeFlags::UniqueESSymbol)
			{
				continue;
			}
			let tracked = property.value_declaration().is_some_and(|declaration| {
				self.tracking.push(GuardTracking {
					node: declaration,
					type_value: property_type,
				});
				true
			});
			let guard = self.build(property_type);
			if tracked {
				self.tracking.pop();
			}
			properties.push(GuardProperty {
				name: property.name().to_owned(),
				guard: guard?,
			});
		}
		Ok(properties)
	}

	pub(crate) fn build_map(&mut self, type_value: P<Type>) -> Result<P<Node>, Error> {
		let factory = self.state.factory.clone();
		let arguments = guard_type_arguments(type_value, &mut self.state.ck());
		let mut guards = vec![guard_field(&factory, "t", "any"), guard_field(&factory, "t", "any")];
		for (index, argument) in arguments.iter().take(2).enumerate() {
			guards[index] = self.build(*argument)?;
		}
		Ok(guard_call(&factory, "map", guards))
	}

	pub(crate) fn build_set(&mut self, type_value: P<Type>) -> Result<P<Node>, Error> {
		let factory = self.state.factory.clone();
		let arguments = guard_type_arguments(type_value, &mut self.state.ck());
		let guard = match arguments.first() {
			Some(first) => self.build(*first)?,
			None => guard_field(&factory, "t", "any"),
		};
		Ok(guard_call(&factory, "set", vec![guard]))
	}
}

/// `guardRobloxEnumKind`: the `Enum.X` symbol a Roblox enum item type belongs to.
fn guard_roblox_enum_kind(
	type_value: P<Type>,
	enum_root: Option<P<Symbol>>,
	checker: &mut TypeChecker<'_>,
) -> Option<P<Symbol>> {
	let symbol = type_value.symbol()?;
	let enum_kind = symbol.parent()?;
	let grandparent = enum_kind.parent()?;
	let enum_root = enum_root?;
	if checker.get_merged_symbol(grandparent) != enum_root {
		return None;
	}
	let exported = enum_kind
		.exports()
		.and_then(|exports| rotor_tsrs::ast::SymbolTable::get(&exports, symbol.name()));
	(exported == Some(symbol)).then_some(enum_kind)
}

fn boolean_literal_count(types: &[P<Type>]) -> usize {
	types
		.iter()
		.filter(|member| member.flags().intersects(TypeFlags::BooleanLiteral))
		.count()
}
