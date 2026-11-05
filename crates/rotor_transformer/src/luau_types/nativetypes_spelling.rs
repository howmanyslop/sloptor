//! `nativetypes_spelling.go`: the spelling of a `//!native` file, the handful of decisions the
//! native annotations take while the type is being built.
//!
//! - A type parameter is spelled as its base constraint when the constraint is itself
//!   annotatable at that site; the bare name is kept otherwise.
//! - An enum member keeps the primitive spelling (`string`, `number`).
//! - A class value is the table roblox-ts emits at runtime: `new` plus the statics, `prototype`
//!   included, with no receiver on the statics.
//! - Aliases: non-class interfaces and non-generic object aliases only; an interface body the
//!   policy cannot spell is `{ [any]: any }`, a type alias body it cannot spell unregisters the
//!   alias.

use rotor_luau::{TypeAccess, TypeAlias};
use rotor_tsrs::ast::SymbolFlags;
use rotor_tsrs::checker::TypeFlags;

use super::luautypes::{Builder, EntryId, TypeContext, TypePosition, any_table};
use super::luautypes_class::is_symbol_keyed_member;
use super::nativetypes::{NATIVE_TYPE_NUMBER, NATIVE_TYPE_STRING, NativePosition, NativeTypeKind, native_position_of};
use crate::prelude::*;

impl Builder<'_, '_> {
	/// `nativeSpelling.SpellTypeParameter`: a constrained type parameter as its base constraint
	/// when that constraint survives the native policy at this site.
	pub fn native_spell_type_parameter(&mut self, t: P<Type>, ctx: TypeContext) -> Option<NodeId> {
		let constraint = self.s.ck().get_base_constraint_of_type(t)?;
		if constraint == t || constraint.flags().intersects(TypeFlags::AnyOrUnknown) {
			return None;
		}
		let built = self.build(constraint, ctx);
		let (node, _, _) = self.native_policy(built.node, native_position_of(ctx.position), 0);
		node
	}

	/// `nativeSpelling.SpellEnumMember`: `string` for a string member, `number` otherwise.
	pub fn native_spell_enum_member(&mut self, t: P<Type>) -> NodeId {
		if t.flags().intersects(TypeFlags::StringLike) {
			return self.ast().type_name(NATIVE_TYPE_STRING);
		}
		self.ast().type_name(NATIVE_TYPE_NUMBER)
	}

	/// `nativeSpelling.SpellClassValue`: a value with construct signatures as the plain table it
	/// is at runtime.
	pub fn native_spell_class_value(&mut self, t: P<Type>) -> NodeId {
		if self.b().stack.contains(&t) {
			return any_table(self.ast());
		}
		self.b().stack.push(t);
		let node = self.native_class_value_table(t);
		self.b().stack.pop();
		node
	}

	fn native_class_value_table(&mut self, t: P<Type>) -> NodeId {
		let ctx = TypeContext::at(TypePosition::Nested, None);
		let fields = self.ast().new_list::<element::TypeTableField>();
		if let Some(construct) = self.construct_member(t, ctx) {
			self.ast().list_push(fields, construct);
		}
		let properties = self.s.ck().get_properties_of_type(t);
		for &property in properties {
			if is_symbol_keyed_member(property.name()) {
				continue;
			}
			let property_type = self.s.ck().get_type_of_symbol(property);
			let mut value = self.build(property_type, ctx).node;
			if property.flags().intersects(SymbolFlags::Optional) {
				value = self.ast().optional_type(value);
			}
			let prop = self.ast().type_table_prop(TypeAccess::None, property.name(), value);
			self.ast().list_push(fields, prop);
		}
		if let Some(indexer) = self.build_indexer(t, ctx) {
			self.ast().list_push(fields, indexer);
		}
		if self.s.ast.list_is_empty(fields) {
			return any_table(self.ast());
		}
		self.ast().type_table(fields)
	}

	/// `nativeSpelling.FinishAliasBody`: runs the native policy over the body and drops the
	/// generic defaults. An interface is a table however little of it can be written; a type
	/// alias is kept only when its body came out as a table or a function type.
	pub fn native_finish_alias_body(&mut self, entry: EntryId, body: NodeId) -> Option<NodeId> {
		let (node, kind, _) = self.native_policy(body, NativePosition::Nested, 0);
		let statement = self.b().entry(entry).statement;
		if let Some(statement) = statement
			&& let Some(alias) = self.s.ast.get_mut::<TypeAlias>(statement)
		{
			for generic in &mut alias.generics {
				generic.default = None;
			}
		}
		// Go's statement shares its generics slice with the entry.
		for generic in &mut self.b().entry_mut(entry).generics {
			generic.default = None;
		}
		let symbol_flags = self.b().entry(entry).symbol.flags();
		if symbol_flags.intersects(SymbolFlags::Interface) {
			let node = if kind == NativeTypeKind::Table {
				node
			} else {
				Some(any_table(self.ast()))
			};
			self.b().entry_mut(entry).kind = NativeTypeKind::Table;
			return node;
		}
		if kind != NativeTypeKind::Table && kind != NativeTypeKind::Function {
			return None;
		}
		self.b().entry_mut(entry).kind = kind;
		node
	}
}
