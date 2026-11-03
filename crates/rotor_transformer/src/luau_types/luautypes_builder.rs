//! `luautypes_builder.go`: the TS-to-Luau mapping, at full fidelity.
//!
//! Nothing is widened for the sake of a bytecode tag, `unknown` and `never` stay themselves,
//! `readonly` becomes `read`, overloads stay intersections. What Luau cannot say becomes the
//! closest spelling plus a fidelity loss. The `//!native` annotations widen this tree afterwards
//! (`nativetypes_policy.rs`).

use rotor_luau::{
	NodeData, TypeAccess, TypeAlias, TypeArray, TypeFunctionParameter, TypeGeneric, TypeTable, TypeTableIndexer,
	is_valid_identifier,
};
use rotor_tsrs::ast::{self as tsast, CheckFlags, ModifierFlags, SymbolFlags};
use rotor_tsrs::checker::{
	ElementFlags, IndexInfo, LiteralValue, Signature, TypeFlags, get_declaration_modifier_flags_from_symbol,
};
use rotor_tsrs::types::TypeCheck;

use super::luautypes::{
	AliasPlacement, Builder, BuiltType, EntryId, FidelityLossKind, Spelling, SymKey, TypeAliasEntry, TypeContext,
	TypePosition, any_function, any_table, any_type, is_any_type,
};
use super::luautypes_alias::{alias_declaration_of_symbol, alias_placement_for, is_luau_exported_type};
use super::nativetypes::{
	NATIVE_TYPE_BOOLEAN, NATIVE_TYPE_NIL, NATIVE_TYPE_NUMBER, NATIVE_TYPE_STRING, NativeTypeKind,
	get_native_type_parameter_name,
};
use super::nativetypes_alias::{get_native_roblox_type_name, is_native_vector_type_name};
use super::{pop_type_generics, push_type_generics};
use crate::identifier::get_declaration_from_import;
use crate::prelude::*;
use crate::types::{get_type_arguments, is_undefined_type_check, symbol_is_ambient};

/// `luauTypeMaxDepth`: the hard guard on nested build calls.
pub const LUAU_TYPE_MAX_DEPTH: usize = 64;

/// `luauRobloxExtraTypeNames`: Roblox types outside the native allowlist that Luau's
/// definitions still name directly.
pub const LUAU_ROBLOX_EXTRA_TYPE_NAMES: &[&str] = &["Enum", "EnumItem", "Enums"];

impl Builder<'_, '_> {
	/// `Build`: spells `t` for the given context. The result's node is never `none`.
	pub fn build(&mut self, t: P<Type>, ctx: TypeContext) -> BuiltType {
		self.build_opt(Some(t), ctx)
	}

	/// `Build` for a type that may be missing.
	pub fn build_opt(&mut self, t: Option<P<Type>>, ctx: TypeContext) -> BuiltType {
		let losses = self.b().losses.len();
		let mut result = match t {
			None => {
				let any = any_type(self.ast());
				let node = self.lose_text(
					FidelityLossKind::Unspellable,
					"",
					any,
					ctx,
					"the checker has no type here",
				);
				self.finish(node, false)
			}
			Some(t) if self.b().depth >= LUAU_TYPE_MAX_DEPTH => {
				let any = any_type(self.ast());
				let node = self.lose(
					FidelityLossKind::DepthExceeded,
					Some(t),
					any,
					ctx,
					"type nesting exceeds the Luau annotation depth guard",
				);
				self.finish(node, false)
			}
			Some(t) => {
				self.b().depth += 1;
				let result = self.build_dispatch(t, ctx);
				self.b().depth -= 1;
				result
			}
		};
		result.lossy = self.b().losses.len() > losses;
		result
	}

	/// `build`: the dispatch. Order matters: a `LuaTuple` is a nominal intersection, a Roblox
	/// enum is an aliased union, arrays, maps and Roblox datatypes are library interfaces with a
	/// shape of their own, and only then is a named type spelled by its alias.
	fn build_dispatch(&mut self, t: P<Type>, ctx: TypeContext) -> BuiltType {
		if ctx.position == TypePosition::Return && is_native_lua_tuple_return(self.s, t) {
			let pack = self.build_return_pack(Some(t), ctx);
			return self.finish(pack, false);
		}
		if TypeCheck::LuaTuple.check(self.s, t) {
			let any = any_type(self.ast());
			let node = self.lose(
				FidelityLossKind::LuaTupleOutsideReturn,
				Some(t),
				any,
				ctx,
				"a type pack is only spellable as a function return",
			);
			return self.finish(node, false);
		}
		if let Some(reference) = self.roblox_enum_reference(t) {
			return self.finish(reference, false);
		}
		let flags = t.flags();
		if flags.intersects(TypeFlags::Object)
			&& let Some(special) = self.build_special_object(t, ctx)
		{
			return special;
		}
		if let Some(alias) = self.build_alias_reference(t, ctx) {
			return alias;
		}

		if t.is_union() {
			return self.build_union(t, ctx);
		}
		if t.is_intersection() {
			return self.build_intersection(t, ctx);
		}
		if flags.intersects(TypeFlags::TypeParameter) {
			return self.build_type_parameter(t, ctx);
		}
		if flags.intersects(TypeFlags::IndexedAccess) {
			return self.build_indexed_access(t, ctx);
		}
		if flags.intersects(TypeFlags::Index) {
			return self.build_keyof(t, ctx);
		}
		if flags.intersects(TypeFlags::Substitution) {
			return self.build_opt(t.as_substitution_type().base_type(), ctx);
		}
		if flags.intersects(TypeFlags::Conditional) {
			return self.build_conditional(t, ctx);
		}
		if flags.intersects(TypeFlags::TemplateLiteral | TypeFlags::StringMapping) {
			let string = self.ast().type_name(NATIVE_TYPE_STRING);
			let node = self.lose(
				FidelityLossKind::TemplateLiteral,
				Some(t),
				string,
				ctx,
				"Luau has no template literal types",
			);
			return self.finish(node, false);
		}
		if flags.intersects(TypeFlags::Literal) {
			let node = self.build_literal(t, ctx);
			return self.finish(node, false);
		}
		if flags.intersects(TypeFlags::Object) {
			return self.build_object(t, ctx);
		}
		self.build_intrinsic(t, ctx)
	}

	/// `finish`: classifies a built node.
	pub fn finish(&mut self, node: NodeId, outer_generic: bool) -> BuiltType {
		let kind = self.kind_of(node);
		BuiltType {
			node,
			kind,
			outer_generic,
			lossy: false,
		}
	}

	/// `kindOf`: the native classification of a built node.
	pub fn kind_of(&mut self, node: NodeId) -> NativeTypeKind {
		match node_shape(&self.s.ast, node) {
			NodeShape::Reference { name, module_empty } => {
				if let Some(&symbol) = self.b().origins.get(&node) {
					let flags = symbol.flags();
					if flags.intersects(SymbolFlags::TypeParameter) {
						return NativeTypeKind::Generic;
					}
					if flags.intersects(SymbolFlags::Class) {
						return NativeTypeKind::Table;
					}
					if let Some(entry) = self.b().alias_entry(symbol) {
						return self.b().entry(entry).kind;
					}
					return NativeTypeKind::Vector;
				}
				match name.as_str() {
					NATIVE_TYPE_NUMBER => NativeTypeKind::Number,
					NATIVE_TYPE_STRING => NativeTypeKind::String,
					NATIVE_TYPE_BOOLEAN => NativeTypeKind::Boolean,
					NATIVE_TYPE_NIL => NativeTypeKind::Nil,
					name if module_empty && is_native_vector_type_name(name) => NativeTypeKind::Vector,
					_ => NativeTypeKind::Unspellable,
				}
			}
			NodeShape::StringSingleton => NativeTypeKind::String,
			NodeShape::BooleanSingleton => NativeTypeKind::Boolean,
			NodeShape::First(Some(inner)) => self.kind_of(inner),
			NodeShape::Table => NativeTypeKind::Table,
			NodeShape::Function => NativeTypeKind::Function,
			NodeShape::EmptyPack => NativeTypeKind::Nil,
			NodeShape::First(None) | NodeShape::Other => NativeTypeKind::Unspellable,
		}
	}

	// -------------------------------------------------------------------------------------
	// Unions, intersections, type variables, literals, intrinsics
	// -------------------------------------------------------------------------------------

	/// `BuildUnion`: `undefined`, `void` and `null` become the optional marker, `true | false`
	/// collapses to `boolean`, members are deduped structurally, and an `any` member makes the
	/// whole union `any`.
	pub fn build_union(&mut self, t: P<Type>, ctx: TypeContext) -> BuiltType {
		let mut members: Vec<(NodeId, String)> = Vec::new();
		let mut optional = false;
		let mut outer_generic = false;
		let boolean_pair = has_native_boolean_literal_pair(t);
		for &union_member in t.types() {
			if union_member.flags().intersects(TypeFlags::VoidLike | TypeFlags::Null) {
				optional = true;
				continue;
			}
			if boolean_pair && is_native_boolean_literal_type(union_member) {
				continue;
			}
			let built = self.build(union_member, ctx);
			outer_generic = outer_generic || built.outer_generic;
			if is_any_type(&self.s.ast, built.node) {
				return self.finish(built.node, outer_generic);
			}
			let text = self.luau_type_text(built.node);
			if !members.iter().any(|(_, existing)| *existing == text) {
				members.push((built.node, text));
			}
		}
		if boolean_pair {
			let boolean = self.ast().type_name(NATIVE_TYPE_BOOLEAN);
			members.insert(0, (boolean, NATIVE_TYPE_BOOLEAN.to_owned()));
		}
		if members.is_empty() {
			let nil = self.build_nil(ctx);
			return self.finish(nil, outer_generic);
		}
		let mut node = if members.len() == 1 {
			members[0].0
		} else {
			let list = self
				.ast()
				.list_from::<element::TypeNode>(members.iter().map(|&(node, _)| node));
			self.ast().type_union(list)
		};
		if optional {
			node = self.ast().optional_type(node);
		}
		self.finish(node, outer_generic)
	}

	/// `BuildIntersection`: an intersection of tables, or of callables, as a Luau intersection;
	/// `{}` members contribute nothing. Anything else widens to `any`.
	pub fn build_intersection(&mut self, t: P<Type>, ctx: TypeContext) -> BuiltType {
		let mut members: Vec<BuiltType> = Vec::new();
		let mut outer_generic = false;
		for &intersection_member in t.types() {
			let built = self.build(intersection_member, ctx);
			outer_generic = outer_generic || built.outer_generic;
			if let Some(table) = self.s.ast.get::<TypeTable>(built.node)
				&& self.s.ast.list_is_empty(table.fields)
			{
				continue;
			}
			members.push(built);
		}
		match members.len() {
			0 => {
				let any = any_type(self.ast());
				return self.finish(any, outer_generic);
			}
			1 => return members[0],
			_ => {}
		}
		let tables = members.iter().all(|member| member.kind == NativeTypeKind::Table);
		let functions = members.iter().all(|member| member.kind == NativeTypeKind::Function);
		if !tables && !functions {
			let any = any_type(self.ast());
			let node = self.lose(
				FidelityLossKind::Unspellable,
				Some(t),
				any,
				ctx,
				"only intersections of tables or of functions have a Luau spelling",
			);
			return self.finish(node, outer_generic);
		}
		let list = self
			.ast()
			.list_from::<element::TypeNode>(members.iter().map(|member| member.node));
		let node = self.ast().type_intersection(list);
		self.finish(node, outer_generic)
	}

	/// `BuildTypeParameter`: a type parameter by name when it is in scope.
	pub fn build_type_parameter(&mut self, t: P<Type>, ctx: TypeContext) -> BuiltType {
		if t.as_type_parameter().is_this_type() {
			if let Some(this) = ctx.this {
				let clone = self.clone_node(this);
				return self.finish(clone, false);
			}
			let any = any_type(self.ast());
			let node = self.lose(
				FidelityLossKind::ThisType,
				Some(t),
				any,
				ctx,
				"the polymorphic this type has no name here",
			);
			return self.finish(node, false);
		}
		let Some((name, own)) = get_native_type_parameter_name(self.s, t) else {
			let any = any_type(self.ast());
			let node = self.lose(
				FidelityLossKind::Unspellable,
				Some(t),
				any,
				ctx,
				"the type parameter is not declarable at this site",
			);
			return self.finish(node, false);
		};
		if let Some(node) = self.spell_type_parameter(t, &name, ctx) {
			// The spelling stands in for the parameter (its constraint, say), so the annotation
			// no longer names the generic.
			return self.finish(node, false);
		}
		let outer_generic = ctx.position == TypePosition::Parameter && !own;
		let reference = self.ast().type_name(name);
		if let Some(symbol) = t.symbol() {
			self.set_origin(reference, SymKey::Real(symbol));
		}
		self.finish(reference, outer_generic)
	}

	/// `buildIndexedAccess`: an unresolved `T[K]` as the `index<T, K>` type function.
	fn build_indexed_access(&mut self, t: P<Type>, ctx: TypeContext) -> BuiltType {
		let access = t.as_indexed_access_type();
		let nested = ctx.nested();
		let object = self.build_opt(access.object_type(), nested).node;
		let index = self.build_opt(access.index_type(), nested).node;
		let arguments = self.ast().list_from::<element::TypeNode>([object, index]);
		let node = self.ast().type_reference("index", Some(arguments));
		self.finish(node, false)
	}

	/// `buildKeyof`: an unresolved `keyof T` as the `keyof<T>` type function.
	fn build_keyof(&mut self, t: P<Type>, ctx: TypeContext) -> BuiltType {
		let target = self.build_opt(t.as_index_type().target(), ctx.nested()).node;
		let arguments = self.ast().list_from::<element::TypeNode>([target]);
		let node = self.ast().type_reference("keyof", Some(arguments));
		self.finish(node, false)
	}

	/// `buildConditional`: a deferred conditional type through its constraint or apparent type.
	fn build_conditional(&mut self, t: P<Type>, ctx: TypeContext) -> BuiltType {
		if let Some(constraint) = self.s.ck().get_base_constraint_of_type(t)
			&& constraint != t
		{
			return self.build(constraint, ctx);
		}
		let apparent = self.s.ck().get_apparent_type(t);
		if apparent != t && !apparent.flags().intersects(TypeFlags::Conditional) {
			return self.build(apparent, ctx);
		}
		let any = any_type(self.ast());
		let node = self.lose(
			FidelityLossKind::ConditionalType,
			Some(t),
			any,
			ctx,
			"Luau has no conditional types",
		);
		self.finish(node, false)
	}

	/// `BuildLiteral`: string and boolean singletons exist, number and bigint literals widen.
	/// An enum member asks the spelling first.
	pub fn build_literal(&mut self, t: P<Type>, ctx: TypeContext) -> NodeId {
		let flags = t.flags();
		if flags.intersects(TypeFlags::EnumLiteral)
			&& let Some(node) = self.spell_enum_member(t)
		{
			return node;
		}
		if flags.intersects(TypeFlags::StringLiteral) {
			return match t.as_literal_type().value() {
				Some(LiteralValue::String(value)) => self.ast().type_string_singleton(value),
				_ => self.ast().type_name(NATIVE_TYPE_STRING),
			};
		}
		if flags.intersects(TypeFlags::BooleanLiteral) {
			let value = get_native_boolean_literal_value(t);
			return self.ast().type_boolean_singleton(value);
		}
		if flags.intersects(TypeFlags::NumberLiteral) {
			let number = self.ast().type_name(NATIVE_TYPE_NUMBER);
			return self.lose(
				FidelityLossKind::NumberLiteral,
				Some(t),
				number,
				ctx,
				"Luau has no number singletons",
			);
		}
		if flags.intersects(TypeFlags::BigIntLiteral) {
			let number = self.ast().type_name(NATIVE_TYPE_NUMBER);
			return self.lose(FidelityLossKind::BigInt, Some(t), number, ctx, "Luau has no bigint");
		}
		let any = any_type(self.ast());
		self.lose(FidelityLossKind::Unspellable, Some(t), any, ctx, "unknown literal type")
	}

	/// `buildIntrinsic`: the primitives and the top and bottom types.
	fn build_intrinsic(&mut self, t: P<Type>, ctx: TypeContext) -> BuiltType {
		let flags = t.flags();
		let node = if flags.intersects(TypeFlags::Any) {
			any_type(self.ast())
		} else if flags.intersects(TypeFlags::Unknown) {
			self.ast().type_name("unknown")
		} else if flags.intersects(TypeFlags::Never) {
			self.ast().type_name("never")
		} else if flags.intersects(TypeFlags::String) {
			self.ast().type_name(NATIVE_TYPE_STRING)
		} else if flags.intersects(TypeFlags::Number | TypeFlags::Enum) {
			self.ast().type_name(NATIVE_TYPE_NUMBER)
		} else if flags.intersects(TypeFlags::Boolean) {
			self.ast().type_name(NATIVE_TYPE_BOOLEAN)
		} else if flags.intersects(TypeFlags::BigInt) {
			let number = self.ast().type_name(NATIVE_TYPE_NUMBER);
			self.lose(FidelityLossKind::BigInt, Some(t), number, ctx, "Luau has no bigint")
		} else if flags.intersects(TypeFlags::ESSymbolLike) {
			let any = any_type(self.ast());
			self.lose(
				FidelityLossKind::SymbolKey,
				Some(t),
				any,
				ctx,
				"Luau has no symbol type",
			)
		} else if flags.intersects(TypeFlags::VoidLike | TypeFlags::Null) {
			self.build_nil(ctx)
		} else if flags.intersects(TypeFlags::NonPrimitive) {
			any_table(self.ast())
		} else {
			let any = any_type(self.ast());
			self.lose(FidelityLossKind::Unspellable, Some(t), any, ctx, "no Luau spelling")
		};
		self.finish(node, false)
	}

	/// `buildNil`: `nil`, or the empty pack `()` where a return is declared.
	fn build_nil(&mut self, ctx: TypeContext) -> NodeId {
		if ctx.position == TypePosition::Return {
			let elements = self.ast().new_list::<element::TypeNode>();
			return self.ast().type_pack(elements);
		}
		self.ast().type_name(NATIVE_TYPE_NIL)
	}

	// -------------------------------------------------------------------------------------
	// Named types: aliases, Roblox types, enums
	// -------------------------------------------------------------------------------------

	/// `BuildAliasReference`: a named interface, type alias or enum as a reference to its
	/// `type` statement, registering the statement on first use. `None` when the spelling does
	/// not alias the symbol, when the body was rejected, and at the top of the alias's own body.
	pub fn build_alias_reference(&mut self, t: P<Type>, ctx: TypeContext) -> Option<BuiltType> {
		let (symbol, declaration) = self.alias_symbol_of(t)?;
		if self.is_type_level_symbol(symbol, declaration) {
			return self.type_level_reference(t, symbol, declaration, ctx);
		}
		let entry = self.alias_for(Some(SymKey::Real(symbol)))?;
		let building = self.b().entry(entry).building;
		if building && ctx.position == TypePosition::AliasBody {
			return None;
		}
		let arguments = self.alias_type_arguments(t, entry);
		if building && !recursive_args_match(self.b().entry(entry), &arguments) {
			let any = any_type(self.ast());
			let node = self.lose(
				FidelityLossKind::RecursiveAliasArgs,
				Some(t),
				any,
				ctx,
				"a recursive alias must be referenced with its own type arguments",
			);
			return Some(self.finish(node, false));
		}
		let declared = declared_arguments(self.b().entry(entry), &arguments);
		let type_arguments = self.build_type_args(&declared, ctx.nested());
		let name = self.b().entry(entry).name.clone();
		let reference = self.ast().type_reference(name, type_arguments);
		self.set_origin(reference, SymKey::Real(symbol));
		Some(self.finish(reference, false))
	}

	/// `aliasSymbolOf`: the symbol a type can be aliased by, with its declaration.
	fn alias_symbol_of(&mut self, t: P<Type>) -> Option<(P<Symbol>, Option<P<Node>>)> {
		if let Some(alias) = t.alias()
			&& let Some(symbol) = alias.symbol()
			&& symbol.flags().intersects(SymbolFlags::TypeAlias)
		{
			return Some((symbol, alias_declaration_of_symbol(symbol)));
		}
		let symbol = t.symbol()?;
		let flags = t.flags();
		let symbol_flags = symbol.flags();
		if flags.intersects(TypeFlags::Object)
			&& symbol_flags.intersects(SymbolFlags::Interface)
			&& !symbol_flags.intersects(SymbolFlags::Class)
		{
			return Some((symbol, alias_declaration_of_symbol(symbol)));
		}
		if flags.intersects(TypeFlags::EnumLiteral) && symbol_flags.intersects(SymbolFlags::Enum) {
			return Some((symbol, alias_declaration_of_symbol(symbol)));
		}
		if flags.intersects(TypeFlags::EnumLiteral)
			&& symbol_flags.intersects(SymbolFlags::EnumMember)
			&& let Some(parent) = symbol.parent()
			&& parent.flags().intersects(SymbolFlags::Enum)
			&& self.s.ck().get_declared_type_of_symbol(parent) == t
		{
			return Some((parent, alias_declaration_of_symbol(parent)));
		}
		None
	}

	/// `aliasFor`: registers `symbol`'s `type` statement on first use and builds its generic
	/// defaults and body with the alias's own generics in a barrier scope, then lets the
	/// spelling finish (or reject) the body. `None` when the spelling declines the symbol or
	/// rejects the body.
	pub fn alias_for(&mut self, symbol: Option<SymKey>) -> Option<EntryId> {
		let symbol = symbol?;
		if let Some(&entry) = self.b().aliases.get(&symbol) {
			return entry;
		}
		let SymKey::Real(real) = symbol else {
			// Go's synthesized symbols are always registered when they are made.
			return None;
		};
		let declaration =
			super::luautypes_alias::luau_first_declaration(self.s, real).or_else(|| alias_declaration_of_symbol(real));
		let Some(declaration) = declaration.filter(|&declaration| self.should_alias(real, declaration)) else {
			self.b().aliases.insert(symbol, None);
			return None;
		};
		if let Some(entry) = self.type_level_alias(real, declaration) {
			return entry;
		}
		let generics = self.generics(declaration);
		let base = self.alias_name(real);
		let name = self.unique_name(&base);
		let declared = self.s.ck().get_declared_type_of_symbol(real);
		let mut entry = TypeAliasEntry::new(symbol, name.clone());
		entry.generics.clone_from(&generics);
		entry.kind = NativeTypeKind::Table;
		entry.exported = is_luau_exported_type(self.s, real);
		entry.placement = alias_placement_for(self.s, declaration);
		entry.building = true;
		if !tsast::is_interface_declaration(declaration) && !self.s.ck().get_call_signatures(declared).is_empty() {
			entry.kind = NativeTypeKind::Function;
			entry.callable = true;
		}
		self.declared_params(&mut entry, declaration);
		let entry = self.b().push_entry(entry);
		self.b().aliases.insert(symbol, Some(entry));
		self.b().alias_order.push(entry);

		// The body names the alias itself for `self` and `this`, with its own generics as
		// arguments.
		let receiver_arguments = if generics.is_empty() {
			None
		} else {
			let names: Vec<NodeId> = generics
				.iter()
				.map(|generic| self.s.ast.type_name(generic.name.clone()))
				.collect();
			Some(self.ast().list_from::<element::TypeNode>(names))
		};
		let receiver = self.ast().type_reference(name.clone(), receiver_arguments);
		self.set_origin(receiver, symbol);

		let names: Vec<String> = generics.iter().map(|generic| generic.name.clone()).collect();
		push_type_generics(self.s, names, true);
		let saved_depth = core::mem::take(&mut self.b().depth);
		let saved_stack = core::mem::take(&mut self.b().stack);
		let losses_before = self.b().losses.len();
		let alias_generics = self.alias_generics(entry, declaration);
		self.b().entry_mut(entry).generics = alias_generics;
		let body_context = TypeContext {
			position: TypePosition::AliasBody,
			receiver: Some(receiver),
			this: Some(receiver),
			owner: Some(declaration),
			class_instance: false,
		};
		let built = self.build(declared, body_context);
		let attributed: Vec<_> = self.b().losses[losses_before..]
			.iter()
			.filter(|loss| loss.node == Some(declaration))
			.cloned()
			.collect();
		self.b().entry_mut(entry).losses.extend(attributed);
		self.b().depth = saved_depth;
		self.b().stack = saved_stack;
		pop_type_generics(self.s);

		let generics = {
			let data = self.b().entry_mut(entry);
			data.building = false;
			data.kind = built.kind;
			data.callable = built.kind == NativeTypeKind::Function;
			data.generics.clone()
		};
		let statement = self.ast().type_alias(name.clone(), generics.clone(), None);
		// Go's NewTypeAlias adopts the defaults in place, so the entry's generics are the
		// statement's.
		let adopted = self
			.s
			.ast
			.get::<TypeAlias>(statement)
			.map_or(generics, |alias| alias.generics.clone());
		{
			let data = self.b().entry_mut(entry);
			data.statement = Some(statement);
			data.generics = adopted;
		}
		let Some(body) = self.finish_alias_body(entry, built.node) else {
			self.b().aliases.insert(symbol, None);
			self.b().alias_order.retain(|&existing| existing != entry);
			self.b().alias_names.remove(&name);
			return None;
		};
		self.s.ast.set_type_alias_body(statement, body);
		Some(entry)
	}

	/// The alias's own type parameter types and which of them Luau can name.
	pub fn declared_params(&mut self, entry: &mut TypeAliasEntry, declaration: P<Node>) {
		for (index, &parameter) in declared_type_parameters(declaration).iter().enumerate() {
			let parameter_type = parameter
				.symbol()
				.map(|parameter_symbol| self.s.ck().get_declared_type_of_symbol(parameter_symbol));
			entry.params.push(parameter_type);
			if let Some(parameter_name) = parameter.name()
				&& is_valid_identifier(parameter_name.text())
			{
				entry.generic_indexes.push(index);
			}
		}
	}

	/// `anonymousAlias`: the `Anonymous_N` alias synthesized for a recursive anonymous object
	/// type, registered (still bodyless) the first time the builder meets it inside its own
	/// table.
	pub fn anonymous_alias(&mut self, t: P<Type>) -> EntryId {
		if let Some(&entry) = self.b().anonymous.get(&t) {
			return entry;
		}
		self.b().anonymous_count += 1;
		let count = self.b().anonymous_count;
		let name = self.unique_name(&format!("Anonymous_{count}"));
		let symbol = self.b().synthetic_symbol();
		let statement = self.ast().type_alias(name.clone(), Vec::new(), None);
		let mut entry = TypeAliasEntry::new(symbol, name);
		entry.statement = Some(statement);
		entry.kind = NativeTypeKind::Table;
		entry.placement = AliasPlacement::Hoisted;
		entry.building = true;
		let entry = self.b().push_entry(entry);
		self.b().anonymous.insert(t, entry);
		self.b().aliases.insert(symbol, Some(entry));
		self.b().alias_order.push(entry);
		entry
	}

	/// `anonymousReference`: a reference to a synthesized alias.
	pub fn anonymous_reference(&mut self, entry: EntryId) -> NodeId {
		let (name, symbol) = {
			let data = self.b().entry(entry);
			(data.name.clone(), data.symbol)
		};
		let reference = self.ast().type_name(name);
		self.set_origin(reference, symbol);
		reference
	}

	/// `finishAnonymousAlias`: installs `table` as the body of `t`'s synthesized alias when one
	/// was registered while `table` was being built, and returns the reference that stands for
	/// `t` from now on; otherwise `table`.
	fn finish_anonymous_alias(&mut self, t: P<Type>, table: NodeId) -> NodeId {
		let Some(&entry) = self.b().anonymous.get(&t) else {
			return table;
		};
		if !self.b().entry(entry).building {
			return table;
		}
		self.b().entry_mut(entry).building = false;
		if let Some(statement) = self.b().entry(entry).statement {
			self.s.ast.set_type_alias_body(statement, table);
		}
		self.anonymous_reference(entry)
	}

	/// `aliasTypeArguments`: the arguments a reference passes to the alias.
	fn alias_type_arguments(&mut self, t: P<Type>, entry: EntryId) -> Vec<P<Type>> {
		let symbol = self.b().entry(entry).symbol;
		if let Some(alias) = t.alias()
			&& alias.symbol().map(SymKey::Real) == Some(symbol)
		{
			return alias.type_arguments().to_vec();
		}
		let mut arguments = get_type_arguments(self.s, t).to_vec();
		let params = self.b().entry(entry).params.len();
		arguments.truncate(params);
		arguments
	}

	/// `robloxEnumReference`: a Roblox enum type or one of its items as the qualified
	/// `Enum.Material`: Luau names the enum, never a single item.
	fn roblox_enum_reference(&mut self, t: P<Type>) -> Option<NodeId> {
		let symbol = t.alias().and_then(|alias| alias.symbol()).or_else(|| t.symbol());
		let symbol = symbol.filter(|&symbol| is_roblox_symbol(symbol))?;
		let mut current = Some(symbol);
		while let Some(this) = current
			&& let Some(parent) = this.parent()
		{
			if parent.parent().is_none() && parent.name() == "Enum" && parent.flags().intersects(SymbolFlags::Module) {
				if !is_valid_identifier(this.name()) {
					return None;
				}
				let reference = self.ast().type_qualified_reference("Enum", this.name(), None);
				self.set_origin(reference, SymKey::Real(symbol));
				return Some(reference);
			}
			current = Some(parent);
		}
		None
	}

	// -------------------------------------------------------------------------------------
	// Objects
	// -------------------------------------------------------------------------------------

	/// `buildSpecialObject`: a Roblox type by name, arrays, tuples, maps and sets by shape, and
	/// the `Function` and `Object` globals.
	fn build_special_object(&mut self, t: P<Type>, ctx: TypeContext) -> Option<BuiltType> {
		if TypeCheck::Roblox.check(self.s, t) {
			let name = roblox_type_name(self.s, t);
			if !name.is_empty() {
				let reference = self.ast().type_name(name);
				if let Some(symbol) = t.symbol() {
					self.set_origin(reference, SymKey::Real(symbol));
				}
				return Some(self.finish(reference, false));
			}
			let any = any_type(self.ast());
			let node = self.lose(
				FidelityLossKind::Unspellable,
				Some(t),
				any,
				ctx,
				"Roblox type without a Luau type name",
			);
			return Some(self.finish(node, false));
		}
		if TypeCheck::Array.check(self.s, t) {
			return Some(self.build_array(t, ctx));
		}
		if TypeCheck::Map.check(self.s, t) {
			return Some(self.build_map(t, ctx));
		}
		if TypeCheck::Set.check(self.s, t) {
			return Some(self.build_set(t, ctx));
		}
		if symbol_is_ambient(self.s, t, &["Function"]) {
			let function = any_function(self.ast());
			return Some(self.finish(function, false));
		}
		if symbol_is_ambient(self.s, t, &["Object"]) {
			let table = any_table(self.ast());
			return Some(self.finish(table, false));
		}
		None
	}

	/// `BuildObject`: the special shapes, a class instance by reference, a class value as its
	/// `{ new, statics }` table, a callable as a function type, anything else as a table.
	pub fn build_object(&mut self, t: P<Type>, ctx: TypeContext) -> BuiltType {
		if let Some(special) = self.build_special_object(t, ctx) {
			return special;
		}
		let symbol = t.symbol();
		let construct_signatures = self.s.ck().get_construct_signatures(t);
		if symbol.is_some_and(|symbol| symbol.flags().intersects(SymbolFlags::Class)) && construct_signatures.is_empty()
		{
			return self.build_class_instance(t, ctx);
		}
		if !construct_signatures.is_empty() {
			return self.build_class_value(t, ctx);
		}
		if !self.s.ck().get_call_signatures(t).is_empty() {
			if !self.s.ck().get_properties_of_type(t).is_empty() {
				let function = any_function(self.ast());
				self.lose(
					FidelityLossKind::Unspellable,
					Some(t),
					function,
					ctx,
					"properties of a callable object are dropped",
				);
			}
			let node = self.build_callable(t, None, ctx);
			return self.finish(node, false);
		}
		if self.s.ck().get_properties_of_type(t).is_empty() && self.s.ck().get_index_infos_of_type(t).is_empty() {
			// `{}` admits primitives too, so only a declared empty interface keeps the empty
			// table.
			if ctx.position == TypePosition::AliasBody {
				let fields = self.ast().new_list::<element::TypeTableField>();
				let table = self.ast().type_table(fields);
				return self.finish(table, false);
			}
			let any = any_type(self.ast());
			return self.finish(any, false);
		}
		if let Some(&entry) = self.b().anonymous.get(&t)
			&& !self.b().entry(entry).building
		{
			let reference = self.anonymous_reference(entry);
			return self.finish(reference, false);
		}
		let Some(table) = self.build_table(t, ctx) else {
			// `t` is on the stack: a cycle through an anonymous type, which only a synthesized
			// alias can spell.
			let entry = self.anonymous_alias(t);
			let reference = self.anonymous_reference(entry);
			return self.finish(reference, false);
		};
		if let Some(symbol) = symbol
			&& symbol
				.flags()
				.intersects(SymbolFlags::Interface | SymbolFlags::Enum | SymbolFlags::Class)
		{
			self.set_origin(table, SymKey::Real(symbol));
		}
		let node = self.finish_anonymous_alias(t, table);
		self.finish(node, false)
	}

	/// `buildArray`: `T[]` as `{ T }`, `ReadonlyArray<T>` as `{ read T }`, a tuple as an array
	/// of the element union.
	fn build_array(&mut self, t: P<Type>, ctx: TypeContext) -> BuiltType {
		if t.is_tuple_type() {
			return self.build_tuple(t, ctx);
		}
		let element = match self.s.ck().get_element_type_of_array_type(t) {
			Some(element_type) => self.build(element_type, ctx.nested()).node,
			None => any_type(self.ast()),
		};
		let array = self.ast().type_array(element);
		if symbol_is_ambient(self.s, t, &["ReadonlyArray"])
			&& let Some(payload) = self.s.ast.get_mut::<TypeArray>(array)
		{
			payload.access = TypeAccess::Read;
		}
		self.finish(array, false)
	}

	/// `buildTuple`: a tuple value type as `{ A | B }`; the positions are lost.
	fn build_tuple(&mut self, t: P<Type>, ctx: TypeContext) -> BuiltType {
		let mut elements: Vec<(NodeId, String)> = Vec::new();
		for &element_type in get_type_arguments(self.s, t) {
			let node = self.build(element_type, ctx.nested()).node;
			let text = self.luau_type_text(node);
			if !elements.iter().any(|(_, existing)| *existing == text) {
				elements.push((node, text));
			}
		}
		let member = match elements.len() {
			0 => any_type(self.ast()),
			1 => elements[0].0,
			_ => {
				let list = self
					.ast()
					.list_from::<element::TypeNode>(elements.iter().map(|&(node, _)| node));
				self.ast().type_union(list)
			}
		};
		let array = self.ast().type_array(member);
		if t.target_tuple_type().is_readonly()
			&& let Some(payload) = self.s.ast.get_mut::<TypeArray>(array)
		{
			payload.access = TypeAccess::Read;
		}
		let node = self.lose(
			FidelityLossKind::Tuple,
			Some(t),
			array,
			ctx,
			"Luau has no tuple types; positions are lost",
		);
		self.finish(node, false)
	}

	/// `buildMap`: `Map`, `ReadonlyMap` and `WeakMap` as `{ [K]: V }`.
	fn build_map(&mut self, t: P<Type>, ctx: TypeContext) -> BuiltType {
		let arguments = get_type_arguments(self.s, t);
		if arguments.len() < 2 {
			let table = any_table(self.ast());
			return self.finish(table, false);
		}
		let nested = ctx.nested();
		let key = self.build(arguments[0], nested).node;
		let value = self.build(arguments[1], nested).node;
		let indexer = self.ast().type_table_indexer(TypeAccess::None, key, value);
		if symbol_is_ambient(self.s, t, &["ReadonlyMap"])
			&& let Some(payload) = self.s.ast.get_mut::<TypeTableIndexer>(indexer)
		{
			payload.access = TypeAccess::Read;
		}
		let fields = self.ast().list_from::<element::TypeTableField>([indexer]);
		let table = self.ast().type_table(fields);
		self.finish(table, false)
	}

	/// `buildSet`: `Set`, `ReadonlySet` and `WeakSet` as `{ [T]: true }`.
	fn build_set(&mut self, t: P<Type>, ctx: TypeContext) -> BuiltType {
		let arguments = get_type_arguments(self.s, t);
		if arguments.is_empty() {
			let table = any_table(self.ast());
			return self.finish(table, false);
		}
		let key = self.build(arguments[0], ctx.nested()).node;
		let value = self.ast().type_boolean_singleton(true);
		let indexer = self.ast().type_table_indexer(TypeAccess::None, key, value);
		if symbol_is_ambient(self.s, t, &["ReadonlySet"])
			&& let Some(payload) = self.s.ast.get_mut::<TypeTableIndexer>(indexer)
		{
			payload.access = TypeAccess::Read;
		}
		let fields = self.ast().list_from::<element::TypeTableField>([indexer]);
		let table = self.ast().type_table(fields);
		self.finish(table, false)
	}

	/// `buildClassValue`: a value with construct signatures as the structural
	/// `{ new: ..., statics }` table the spelling may override.
	fn build_class_value(&mut self, t: P<Type>, ctx: TypeContext) -> BuiltType {
		if let Some(node) = self.spell_class_value(t) {
			return self.finish(node, false);
		}
		if let Some(&entry) = self.b().anonymous.get(&t)
			&& !self.b().entry(entry).building
		{
			let reference = self.anonymous_reference(entry);
			return self.finish(reference, false);
		}
		let Some(table) = self.build_table(t, ctx) else {
			// A static typed as its own class value: a cycle through the anonymous class value
			// table, spelled by a synthesized alias.
			let entry = self.anonymous_alias(t);
			let reference = self.anonymous_reference(entry);
			return self.finish(reference, false);
		};
		if let Some(symbol) = t.symbol()
			&& symbol.flags().intersects(SymbolFlags::Class)
		{
			self.set_origin(table, SymKey::Real(symbol));
		}
		let node = self.finish_anonymous_alias(t, table);
		self.finish(node, false)
	}

	/// `BuildTable`: one object level as a table type, or `None` when the type is already on
	/// the stack (a recursive anonymous type).
	pub fn build_table(&mut self, t: P<Type>, ctx: TypeContext) -> Option<NodeId> {
		if self.b().stack.contains(&t) {
			return None;
		}
		self.b().stack.push(t);
		let table = self.build_table_body(t, ctx);
		self.b().stack.pop();
		Some(table)
	}

	fn build_table_body(&mut self, t: P<Type>, ctx: TypeContext) -> NodeId {
		let fields = self.ast().new_list::<element::TypeTableField>();
		let receivers = self.s.ck().get_construct_signatures(t).is_empty();
		if let Some(construct) = self.construct_member(t, ctx) {
			self.ast().list_push(fields, construct);
		}
		let mut receiver = ctx.receiver;
		let mut symbol_key_lost = false;
		let mut readonly_lost = false;
		let mut private_lost = false;
		for &property in self.s.ck().get_properties_of_type(t) {
			let name = property.name();
			if name.starts_with("__@") || name.starts_with(tsast::INTERNAL_SYMBOL_NAME_PREFIX) {
				// Computed/ES-symbol member: no Luau spelling.
				if !symbol_key_lost {
					self.record(
						FidelityLossKind::SymbolKey,
						name.to_owned(),
						None,
						ctx,
						"symbol-keyed members are skipped",
					);
					symbol_key_lost = true;
				}
				continue;
			}
			if !receivers && name == "prototype" {
				// A class value's `prototype` is a TS artifact: the instance is built by `new`,
				// and the emitted class table has no such key.
				continue;
			}
			let property_type = self.s.ck().get_type_of_symbol(property);
			let callable = self.s.ck().get_non_nullable_type(property_type);
			let mut value = if receivers
				&& property.flags().intersects(SymbolFlags::Method)
				&& !self.s.ck().get_call_signatures(callable).is_empty()
			{
				if receiver.is_none() {
					let any = any_type(self.ast());
					receiver = Some(self.lose(
						FidelityLossKind::ObjectLiteralSelf,
						Some(t),
						any,
						ctx,
						"methods of an unnamed object type take `self: any`",
					));
				}
				self.build_callable(callable, receiver, ctx.nested())
			} else {
				self.build(property_type, ctx.nested()).node
			};
			if property.flags().intersects(SymbolFlags::Optional) {
				value = self.ast().optional_type(value);
			}
			let mut access = TypeAccess::None;
			if is_readonly_property(property) {
				if ctx.class_instance {
					if !readonly_lost {
						self.lose_text(
							FidelityLossKind::ClassReadonlyDropped,
							name,
							value,
							ctx,
							"class constructors write through self, so readonly members stay writable",
						);
						readonly_lost = true;
					}
				} else {
					access = TypeAccess::Read;
				}
			}
			if ctx.class_instance
				&& !private_lost
				&& get_declaration_modifier_flags_from_symbol(property)
					.intersects(ModifierFlags::Private | ModifierFlags::Protected)
			{
				self.lose_text(
					FidelityLossKind::PrivateExposed,
					name,
					value,
					ctx,
					"private and protected members are plain table fields in Luau",
				);
				private_lost = true;
			}
			let prop = self.ast().type_table_prop(access, name, value);
			self.ast().list_push(fields, prop);
		}
		if let Some(indexer) = self.build_indexer(t, ctx) {
			self.ast().list_push(fields, indexer);
		}
		self.ast().type_table(fields)
	}

	/// `constructMember`: the `new` member of a value with construct signatures. An overloaded
	/// constructor keeps the intersection on the member.
	pub fn construct_member(&mut self, t: P<Type>, ctx: TypeContext) -> Option<NodeId> {
		let signatures = self.s.ck().get_construct_signatures(t);
		if signatures.is_empty() {
			return None;
		}
		let nested = ctx.nested();
		let value = if signatures.len() == 1 {
			self.build_signature(signatures[0], None, nested)
		} else {
			let mut built = Vec::with_capacity(signatures.len());
			for &signature in signatures {
				built.push(self.build_signature(signature, None, nested));
			}
			let members = self.ast().list_from::<element::TypeNode>(built);
			self.ast().type_intersection(members)
		};
		Some(self.ast().type_table_prop(TypeAccess::None, "new", value))
	}

	/// `BuildIndexer`: a type's index signatures collapsed into Luau's single indexer.
	pub fn build_indexer(&mut self, t: P<Type>, ctx: TypeContext) -> Option<NodeId> {
		let mut string_info: Option<P<IndexInfo>> = None;
		let mut number_info: Option<P<IndexInfo>> = None;
		for &info in self.s.ck().get_index_infos_of_type(t) {
			let key = info.key_type();
			if key.flags().intersects(TypeFlags::Number) {
				if number_info.is_none() {
					number_info = Some(info);
				}
			} else if key.flags().intersects(TypeFlags::ESSymbolLike) {
				let text = self.type_text(Some(key));
				self.record(
					FidelityLossKind::SymbolKey,
					text,
					None,
					ctx,
					"symbol index signatures are skipped",
				);
			} else if string_info.is_none() {
				string_info = Some(info);
			}
		}
		let nested = ctx.nested();
		match (string_info, number_info) {
			(Some(string_info), Some(number_info)) => {
				let string_value = self.build(string_info.value_type(), nested).node;
				let number_value = self.build(number_info.value_type(), nested).node;
				let value = if self.luau_type_text(string_value) == self.luau_type_text(number_value) {
					string_value
				} else {
					any_type(self.ast())
				};
				self.lose_text(
					FidelityLossKind::IndexerCollapsed,
					"[string] + [number]",
					value,
					ctx,
					"Luau allows one indexer, so both keys collapse to [any]",
				);
				let access = if string_info.is_readonly() && number_info.is_readonly() {
					TypeAccess::Read
				} else {
					TypeAccess::None
				};
				let key = any_type(self.ast());
				Some(self.ast().type_table_indexer(access, key, value))
			}
			(Some(string_info), None) => {
				let key = self.ast().type_name(NATIVE_TYPE_STRING);
				Some(self.indexer_for(key, string_info, nested))
			}
			(None, Some(number_info)) => {
				let key = self.ast().type_name(NATIVE_TYPE_NUMBER);
				Some(self.indexer_for(key, number_info, nested))
			}
			(None, None) => None,
		}
	}

	fn indexer_for(&mut self, key: NodeId, info: P<IndexInfo>, ctx: TypeContext) -> NodeId {
		let access = if info.is_readonly() {
			TypeAccess::Read
		} else {
			TypeAccess::None
		};
		let value = self.build(info.value_type(), ctx).node;
		self.ast().type_table_indexer(access, key, value)
	}

	// -------------------------------------------------------------------------------------
	// Callables
	// -------------------------------------------------------------------------------------

	/// `BuildCallable`: a callable object as a function type, or an overload set as an
	/// intersection of function types. `receiver` is the leading `self` of every signature.
	pub fn build_callable(&mut self, t: P<Type>, receiver: Option<NodeId>, ctx: TypeContext) -> NodeId {
		let signatures = self.s.ck().get_call_signatures(t);
		match signatures.len() {
			0 => {
				let function = any_function(self.ast());
				return self.lose(
					FidelityLossKind::Unspellable,
					Some(t),
					function,
					ctx,
					"callable without a call signature",
				);
			}
			1 => return self.build_signature(signatures[0], receiver, ctx),
			_ => {}
		}
		let mut built = Vec::with_capacity(signatures.len());
		for &signature in signatures {
			built.push(self.build_signature(signature, receiver, ctx));
		}
		let members = self.ast().list_from::<element::TypeNode>(built);
		self.ast().type_intersection(members)
	}

	/// `BuildSignature`: one call or construct signature.
	pub fn build_signature(&mut self, signature: P<Signature>, receiver: Option<NodeId>, ctx: TypeContext) -> NodeId {
		let generics = self.signature_generics(signature);
		let names: Vec<String> = generics.iter().map(|generic| generic.name.clone()).collect();
		push_type_generics(self.s, names, false);
		let function = self.build_signature_body(signature, receiver, ctx, generics);
		pop_type_generics(self.s);
		function
	}

	fn build_signature_body(
		&mut self,
		signature: P<Signature>,
		receiver: Option<NodeId>,
		ctx: TypeContext,
		generics: Vec<TypeGeneric>,
	) -> NodeId {
		let nested = ctx.nested();
		let mut parameters: Vec<TypeFunctionParameter> = Vec::new();
		if let Some(this) = signature.this_parameter() {
			let this_type = self.s.ck().get_type_of_symbol(this);
			let node = self.build(this_type, nested).node;
			parameters.push(TypeFunctionParameter::new("self", node));
		} else if let Some(receiver) = receiver {
			let node = self.clone_node(receiver);
			parameters.push(TypeFunctionParameter::new("self", node));
		}
		let mut var_args = None;
		let signature_parameters = signature.parameters();
		let min_argument_count = signature.min_argument_count();
		let has_rest = signature.has_rest_parameter();
		for (index, &parameter) in signature_parameters.iter().enumerate() {
			let parameter_type = self.s.ck().get_type_of_symbol(parameter);
			if has_rest && index == signature_parameters.len() - 1 {
				let inner = if let Some(element) = self.s.ck().get_element_type_of_array_type(parameter_type) {
					self.build(element, nested).node
				} else {
					let any = any_type(self.ast());
					self.lose(
						FidelityLossKind::Unspellable,
						Some(parameter_type),
						any,
						ctx,
						"a rest parameter that is not an array has no Luau spelling",
					)
				};
				var_args = Some(self.ast().type_variadic(inner));
				continue;
			}
			let mut node = self.build(parameter_type, nested).node;
			if index >= min_argument_count {
				node = self.ast().optional_type(node);
			}
			let name = if is_valid_identifier(parameter.name()) {
				parameter.name()
			} else {
				""
			};
			parameters.push(TypeFunctionParameter::new(name, node));
		}
		let mut return_context = nested;
		return_context.position = TypePosition::Return;
		let return_type = self.s.ck().get_return_type_of_signature(signature);
		let return_type = self.build_return_pack(Some(return_type), return_context);
		self.ast().type_function(generics, parameters, var_args, return_type)
	}

	/// `signatureGenerics` (builder): a signature's spellable type parameters.
	fn signature_generics(&mut self, signature: P<Signature>) -> Vec<TypeGeneric> {
		let mut generics = Vec::new();
		for &parameter in signature.type_parameters() {
			let Some(symbol) = parameter.symbol() else {
				continue;
			};
			if !is_valid_identifier(symbol.name()) {
				continue;
			}
			generics.push(TypeGeneric::new(symbol.name()));
			if self.s.ck().get_constraint_of_type_parameter(parameter).is_some() {
				self.constraint_loss(Some(symbol));
			}
		}
		generics
	}

	/// `Generics`: the spellable type parameters a declaration declares, ready for `<K, V>`.
	pub fn generics(&mut self, node: P<Node>) -> Vec<TypeGeneric> {
		let mut generics = Vec::new();
		for &parameter in declared_type_parameters(node) {
			let Some(name) = parameter.name() else {
				continue;
			};
			if !is_valid_identifier(name.text()) {
				continue;
			}
			generics.push(TypeGeneric::new(name.text()));
			if parameter.as_type_parameter_declaration().constraint.is_some() {
				self.constraint_loss(parameter.symbol());
			}
		}
		generics
	}

	/// `constraintLoss`: records a dropped `extends` clause once per type parameter.
	fn constraint_loss(&mut self, symbol: Option<P<Symbol>>) {
		let Some(symbol) = symbol else {
			return;
		};
		if !self.b().constraint_losses.insert(symbol) {
			return;
		}
		self.b().losses.push(super::luautypes::FidelityLoss {
			kind: FidelityLossKind::ConstraintDropped,
			ts_text: symbol.name().to_owned(),
			luau: symbol.name().to_owned(),
			reason: "Luau generics have no constraints".to_owned(),
			node: None,
		});
	}

	/// `BuildReturnPack`: what follows `->` or `:` on a function.
	pub fn build_return_pack(&mut self, t: Option<P<Type>>, mut ctx: TypeContext) -> NodeId {
		ctx.position = TypePosition::Return;
		let Some(t) = t else {
			let pack = any_pack(self.ast());
			return self.lose_text(
				FidelityLossKind::Unspellable,
				"",
				pack,
				ctx,
				"the checker has no return type here",
			);
		};
		if is_undefined_type_check(t) {
			let elements = self.ast().new_list::<element::TypeNode>();
			return self.ast().type_pack(elements);
		}
		if TypeCheck::LuaTuple.check(self.s, t) {
			let Some(tuple) = get_native_lua_tuple_tuple(t) else {
				let pack = any_pack(self.ast());
				return self.lose(
					FidelityLossKind::Unspellable,
					Some(t),
					pack,
					ctx,
					"a LuaTuple of an array, a type parameter or a union of tuples has no pack spelling",
				);
			};
			if let Some(pack) = self.build_tuple_pack(tuple, ctx) {
				return pack;
			}
			return any_pack(self.ast());
		}
		if t.is_union()
			&& t.types()
				.iter()
				.any(|&member| TypeCheck::LuaTuple.check(self.s, member))
		{
			let pack = any_pack(self.ast());
			return self.lose(
				FidelityLossKind::OptionalPack,
				Some(t),
				pack,
				ctx,
				"`?` after a return pack would make the function optional",
			);
		}
		self.build(t, ctx).node
	}

	/// `BuildTuplePack`: a fixed tuple as a return pack. `None` (with a loss recorded) when a
	/// rest element is not last.
	pub fn build_tuple_pack(&mut self, tuple: P<Type>, ctx: TypeContext) -> Option<NodeId> {
		let infos = tuple.target_tuple_type().element_infos();
		let arguments = get_type_arguments(self.s, tuple);
		let elements = self.ast().new_list::<element::TypeNode>();
		if infos.is_empty() {
			return Some(self.ast().type_pack(elements));
		}
		if arguments.len() != infos.len() {
			let text = self.type_text(Some(tuple));
			self.record(
				FidelityLossKind::Unspellable,
				text,
				None,
				ctx,
				"tuple element count does not match its type arguments",
			);
			return None;
		}
		let nested = ctx.nested();
		for (index, info) in infos.iter().enumerate() {
			let mut element_type = arguments[index];
			let flags = info.flags;
			if flags.intersects(ElementFlags::Variable) {
				if index != infos.len() - 1 {
					let text = self.type_text(Some(tuple));
					self.record(
						FidelityLossKind::Unspellable,
						text,
						None,
						ctx,
						"a rest element that is not last has no pack spelling",
					);
					return None;
				}
				// A rest element's type argument is already its element type in a resolved
				// tuple; an unresolved array still unwraps.
				if let Some(element) = self.s.ck().get_element_type_of_array_type(element_type) {
					element_type = element;
				}
				let node = self.build(element_type, nested).node;
				let variadic = self.ast().type_variadic(node);
				self.ast().list_push(elements, variadic);
				continue;
			}
			let mut node = self.build(element_type, nested).node;
			if flags.intersects(ElementFlags::Optional) {
				node = self.ast().optional_type(node);
			}
			self.ast().list_push(elements, node);
		}
		Some(self.ast().type_pack(elements))
	}

	/// `BuildTypeArgs`: a type argument list, or `None` for none.
	pub fn build_type_args(&mut self, arguments: &[P<Type>], ctx: TypeContext) -> Option<List<element::TypeNode>> {
		if arguments.is_empty() {
			return None;
		}
		let mut built = Vec::with_capacity(arguments.len());
		for &argument in arguments {
			built.push(self.build(argument, ctx).node);
		}
		Some(self.ast().list_from::<element::TypeNode>(built))
	}

	// -------------------------------------------------------------------------------------
	// Type nodes
	// -------------------------------------------------------------------------------------

	/// `BuildTypeNode`: a written TS type. `typeof x` keeps Luau's own `typeof(x)` when `x` is a
	/// value Luau can see at the annotation site; a written `Name<Args>` naming a type function
	/// of this file keeps that call-site spelling when its resolved type no longer names the
	/// alias. Everything else is spelled from the checker's resolved type.
	pub fn build_type_node(&mut self, node: P<Node>, ctx: TypeContext) -> BuiltType {
		match node.kind() {
			Kind::TypeQuery => {
				let expression = node.as_type_query_node().expr_name;
				if tsast::is_identifier(expression)
					&& let Some(identifier) = self.local_typeof_identifier(expression, ctx)
				{
					let typeof_node = self.ast().type_typeof(identifier);
					return self.finish(typeof_node, false);
				}
			}
			Kind::TypeReference => {
				if let Some(reference) = self.type_level_node_reference(node, ctx) {
					return reference;
				}
			}
			_ => {}
		}
		let t = self.s.ck().get_type_from_type_node(node);
		self.build(t, ctx)
	}

	/// `localTypeofIdentifier`: the Luau identifier `typeof(x)` may name.
	fn local_typeof_identifier(&mut self, expression: P<Node>, ctx: TypeContext) -> Option<NodeId> {
		let symbol = self.s.ck().get_symbol_at_location(expression)?;
		if !symbol
			.flags()
			.intersects(SymbolFlags::Variable | SymbolFlags::Function | SymbolFlags::Class)
		{
			return None;
		}
		let declaration = symbol.value_declaration()?;
		if tsast::get_source_file_of_node(declaration) != Some(self.s.source_file)
			|| get_declaration_from_import(symbol).is_some()
		{
			return None;
		}
		if tsast::is_variable_declaration(declaration) && !tsast::is_var_const(declaration) {
			return None;
		}
		if !is_top_level_declaration(declaration)
			|| !is_valid_identifier(symbol.name())
			|| self.s.symbol_to_id.contains_key(&symbol)
		{
			return None;
		}
		if let Some(owner) = ctx.owner
			&& declaration.pos() > owner.pos()
		{
			return None;
		}
		Some(self.ast().id(symbol.name()))
	}

	/// Whether the spelling aliases `symbol` (`typeSpelling.ShouldAlias`).
	fn should_alias(&mut self, symbol: P<Symbol>, declaration: P<Node>) -> bool {
		match self.spelling() {
			Spelling::Default => default_should_alias(self.s, symbol),
			Spelling::Native => super::nativetypes_alias::is_native_aliasable_symbol(self.s, symbol, Some(declaration)),
		}
	}

	/// `typeSpelling.AliasName`
	pub fn alias_name(&mut self, symbol: P<Symbol>) -> String {
		match self.spelling() {
			Spelling::Default => super::luautypes_alias::luau_alias_base_name(self.s, symbol),
			Spelling::Native => symbol.name().to_owned(),
		}
	}

	/// `typeSpelling.FinishAliasBody`: `None` unregisters the alias.
	fn finish_alias_body(&mut self, entry: EntryId, body: NodeId) -> Option<NodeId> {
		match self.spelling() {
			Spelling::Default => Some(body),
			Spelling::Native => self.native_finish_alias_body(entry, body),
		}
	}

	/// `typeSpelling.SpellTypeParameter`: `None` keeps the bare `T` reference.
	fn spell_type_parameter(&mut self, t: P<Type>, _name: &str, ctx: TypeContext) -> Option<NodeId> {
		match self.spelling() {
			Spelling::Default => None,
			Spelling::Native => self.native_spell_type_parameter(t, ctx),
		}
	}

	/// `typeSpelling.SpellEnumMember`: `None` keeps the singleton.
	fn spell_enum_member(&mut self, t: P<Type>) -> Option<NodeId> {
		match self.spelling() {
			Spelling::Default => None,
			Spelling::Native => Some(self.native_spell_enum_member(t)),
		}
	}

	/// `typeSpelling.SpellClassValue`: `None` keeps the structural table.
	fn spell_class_value(&mut self, t: P<Type>) -> Option<NodeId> {
		match self.spelling() {
			Spelling::Default => None,
			Spelling::Native => Some(self.native_spell_class_value(t)),
		}
	}
}

/// What `kindOf` reads off a node.
enum NodeShape {
	Reference {
		name: String,
		module_empty: bool,
	},
	StringSingleton,
	BooleanSingleton,
	/// An optional's inner type, or the first member of a union or intersection.
	First(Option<NodeId>),
	Table,
	Function,
	EmptyPack,
	Other,
}

fn node_shape(ast: &Ast, node: NodeId) -> NodeShape {
	match ast.data(node) {
		NodeData::TypeReference(reference) => NodeShape::Reference {
			name: reference.name.clone(),
			module_empty: reference.module.is_empty(),
		},
		NodeData::TypeStringSingleton(_) => NodeShape::StringSingleton,
		NodeData::TypeBooleanSingleton(_) => NodeShape::BooleanSingleton,
		NodeData::TypeOptional(optional) => NodeShape::First(optional.inner),
		NodeData::TypeUnion(union) => NodeShape::First(ast.list_head(union.members).map(|head| ast.link_value(head))),
		NodeData::TypeIntersection(intersection) => {
			NodeShape::First(ast.list_head(intersection.members).map(|head| ast.link_value(head)))
		}
		NodeData::TypeTable(_) | NodeData::TypeArray(_) => NodeShape::Table,
		NodeData::TypeFunction(_) => NodeShape::Function,
		NodeData::TypePack(pack) if ast.list_is_empty(pack.elements) => NodeShape::EmptyPack,
		_ => NodeShape::Other,
	}
}

/// `defaultSpelling.ShouldAlias`: every interface, type alias and enum that has a declaration,
/// library types included, except a `LuaTuple` alias (a pack is only spellable as a return).
fn default_should_alias(state: &mut State<'_>, symbol: P<Symbol>) -> bool {
	if symbol.flags().intersects(SymbolFlags::TypeAlias) {
		let declared = state.ck().get_declared_type_of_symbol(symbol);
		if TypeCheck::LuaTuple.check(state, declared) {
			return false;
		}
	}
	true
}

/// `anyPack`: `...any`.
fn any_pack(ast: &mut Ast) -> NodeId {
	let any = any_type(ast);
	ast.type_variadic(any)
}

/// `recursiveArgsMatch`: a reference into an alias under construction passes exactly the
/// alias's own type parameters.
pub fn recursive_args_match(entry: &TypeAliasEntry, arguments: &[P<Type>]) -> bool {
	arguments.len() == entry.params.len()
		&& arguments
			.iter()
			.zip(&entry.params)
			.all(|(&argument, &parameter)| Some(argument) == parameter)
}

/// `declaredArguments`: the type arguments that belong to the alias's declared generics.
pub fn declared_arguments(entry: &TypeAliasEntry, arguments: &[P<Type>]) -> Vec<P<Type>> {
	if entry.generic_indexes.len() == entry.params.len() {
		return arguments.to_vec();
	}
	entry
		.generic_indexes
		.iter()
		.filter_map(|&index| arguments.get(index).copied())
		.collect()
}

/// `isRobloxSymbol`: a declaration inside the `@rbxts/types` package.
pub fn is_roblox_symbol(symbol: P<Symbol>) -> bool {
	symbol.declarations().iter().any(|&declaration| {
		tsast::get_source_file_of_node(declaration)
			.is_some_and(|file| file.file_name().contains("node_modules/@rbxts/types/"))
	})
}

/// `robloxTypeName`: the Luau name of a Roblox type, `""` for one Luau cannot name.
pub fn roblox_type_name(state: &mut State<'_>, t: P<Type>) -> String {
	let name = get_native_roblox_type_name(state, t);
	if !name.is_empty() {
		return name;
	}
	if let Some(symbol) = t.symbol()
		&& TypeCheck::Roblox.check(state, t)
		&& LUAU_ROBLOX_EXTRA_TYPE_NAMES.contains(&symbol.name())
	{
		return symbol.name().to_owned();
	}
	String::new()
}

/// `isReadonlyProperty`: the checker's `isReadonlySymbol` for members.
pub fn is_readonly_property(property: P<Symbol>) -> bool {
	let flags = property.flags();
	property.check_flags().intersects(CheckFlags::Readonly)
		|| (flags.intersects(SymbolFlags::Property)
			&& get_declaration_modifier_flags_from_symbol(property).intersects(ModifierFlags::Readonly))
		|| (flags.intersects(SymbolFlags::Accessor) && !flags.intersects(SymbolFlags::SetAccessor))
		|| flags.intersects(SymbolFlags::EnumMember)
}

/// `declaredTypeParameters`: `Node.TypeParameters` for the declarations that can carry them.
pub fn declared_type_parameters(node: P<Node>) -> &'static [P<Node>] {
	if tsast::is_class_like(node)
		|| tsast::is_interface_declaration(node)
		|| tsast::is_type_alias_declaration(node)
		|| tsast::is_function_like(node)
	{
		return node.type_parameters();
	}
	&[]
}

/// `isTopLevelDeclaration`: a declaration whose nearest enclosing scope is the source file.
pub fn is_top_level_declaration(declaration: P<Node>) -> bool {
	let mut parent = declaration.parent();
	while let Some(current) = parent {
		if tsast::is_source_file(current) {
			return true;
		}
		if tsast::is_block(current)
			|| tsast::is_module_declaration(current)
			|| tsast::is_function_like(current)
			|| tsast::is_class_like(current)
		{
			return false;
		}
		parent = current.parent();
	}
	false
}

/// `isNativeBooleanLiteralType`: `true` or `false`, not an enum member.
pub fn is_native_boolean_literal_type(t: P<Type>) -> bool {
	t.flags().intersects(TypeFlags::BooleanLiteral) && !t.flags().intersects(TypeFlags::EnumLiteral)
}

/// `getNativeBooleanLiteralValue`
pub fn get_native_boolean_literal_value(t: P<Type>) -> bool {
	matches!(t.as_literal_type().value(), Some(LiteralValue::Boolean(true)))
}

/// `hasNativeBooleanLiteralPair`: the union contains both `true` and `false`.
pub fn has_native_boolean_literal_pair(t: P<Type>) -> bool {
	let mut has_true = false;
	let mut has_false = false;
	for &member in t.types() {
		if !is_native_boolean_literal_type(member) {
			continue;
		}
		if get_native_boolean_literal_value(member) {
			has_true = true;
		} else {
			has_false = true;
		}
	}
	has_true && has_false
}

/// `isNativeLuaTupleReturn`: a `LuaTuple`, or a union with one as a member.
pub fn is_native_lua_tuple_return(state: &mut State<'_>, t: P<Type>) -> bool {
	if TypeCheck::LuaTuple.check(state, t) {
		return true;
	}
	t.is_union() && t.types().iter().any(|&member| TypeCheck::LuaTuple.check(state, member))
}

/// `getNativeLuaTupleTuple`: the tuple a `LuaTuple<T>` carries, or `None`.
pub fn get_native_lua_tuple_tuple(t: P<Type>) -> Option<P<Type>> {
	if let Some(alias) = t.alias() {
		let arguments = alias.type_arguments();
		if arguments.len() == 1 && arguments[0].is_tuple_type() {
			return Some(arguments[0]);
		}
	}
	if t.is_union() || t.is_intersection() {
		return t.types().iter().copied().find(|member| member.is_tuple_type());
	}
	None
}
