//! `nativetypes_policy.go`: a post-pass over a full-fidelity type tree that widens or drops
//! what would hurt Luau's native code generator.
//!
//! - A nested position is never type-tagged: kept in full, widened to `any` when it cannot be
//!   written.
//! - A return type is not tagged either, but a shape that cannot be mirrored stays unannotated.
//! - A top-level parameter annotation becomes a runtime tag, so only shapes Luau understands are
//!   kept.
//!
//! Depth counts every child slot except union, intersection and optional members; a table deeper
//! than [`NATIVE_TYPE_MAX_DEPTH`] is `{ [any]: any }`. The policy is idempotent.

use rotor_luau::{NodeData, TypeAccess, TypeFunctionParameter, TypeIntersection};
use rotor_tsrs::ast::SymbolFlags;

use super::SliceState;
use super::luautypes::{Builder, BuilderId, SymKey, any_function, any_table, any_type, is_any_type, render_type_text};
use super::luautypes_builder::LUAU_ROBLOX_EXTRA_TYPE_NAMES;
use super::nativetypes::{
	NATIVE_TYPE_BOOLEAN, NATIVE_TYPE_MAX_DEPTH, NATIVE_TYPE_NIL, NATIVE_TYPE_NUMBER, NATIVE_TYPE_STRING,
	NativePosition, NativeTypeKind, type_generic_in_scope,
};
use super::nativetypes_alias::is_native_vector_type_name;
use crate::prelude::*;

/// `nativeOrigins`: what the policy knows about a tree beyond its shape.
pub trait NativeOrigins {
	/// The TS symbol `node` was spelled from.
	fn origin_of(&self, node: NodeId) -> Option<SymKey>;
	/// What a named alias stands for at runtime, if the symbol is a registered alias.
	fn alias_kind_of(&self, symbol: SymKey) -> Option<NativeTypeKind>;
	/// Whether a bare generic belongs to the function being annotated (rule R1).
	fn own_generic(&self, name: &str) -> bool;
	/// Records that `to` was rebuilt from `from`.
	fn transfer_origin(&mut self, from: NodeId, to: NodeId);
	/// Records an internal error (Go panics).
	fn ice(&mut self, message: String);
}

/// The file's builder as [`NativeOrigins`]: its origins and registry, and the generic scope.
pub struct BuilderOrigins<'x> {
	slice: &'x mut SliceState,
	id: BuilderId,
	internal_error: &'x mut Option<String>,
}

impl<'x> BuilderOrigins<'x> {
	/// The origins of builder `id` of `slice`, reporting internal errors into `internal_error`.
	pub const fn new(slice: &'x mut SliceState, id: BuilderId, internal_error: &'x mut Option<String>) -> Self {
		Self {
			slice,
			id,
			internal_error,
		}
	}
}

impl NativeOrigins for BuilderOrigins<'_> {
	fn ice(&mut self, message: String) {
		if self.internal_error.is_none() {
			*self.internal_error = Some(message);
		}
	}

	fn origin_of(&self, node: NodeId) -> Option<SymKey> {
		self.slice
			.builder(self.id)
			.and_then(|builder| builder.origins.get(&node).copied())
	}

	fn alias_kind_of(&self, symbol: SymKey) -> Option<NativeTypeKind> {
		let builder = self.slice.builder(self.id)?;
		let entry = builder.alias_entry(symbol)?;
		Some(builder.entry(entry).kind)
	}

	fn own_generic(&self, name: &str) -> bool {
		type_generic_in_scope(&self.slice.type_scopes, name).1
	}

	fn transfer_origin(&mut self, from: NodeId, to: NodeId) {
		let builder = self.slice.builder_mut(self.id);
		if let Some(&symbol) = builder.origins.get(&from) {
			builder.origins.insert(to, symbol);
		}
	}
}

impl Builder<'_, '_> {
	/// [`apply_native_policy`] with this builder as the origins.
	pub fn native_policy(
		&mut self,
		node: NodeId,
		position: NativePosition,
		depth: usize,
	) -> (Option<NodeId>, NativeTypeKind, bool) {
		let mut origins = BuilderOrigins::new(&mut self.s.slices.luau_types, self.id, &mut self.s.internal_error);
		apply_native_policy(&mut self.s.ast, &mut origins, node, position, depth)
	}
}

/// `classifyNativeType`: what a node would tag.
pub fn classify_native_type(ast: &Ast, node: NodeId, origins: &dyn NativeOrigins) -> NativeTypeKind {
	match ast.data(node) {
		NodeData::TypeReference(_) => classify_native_reference(ast, node, origins),
		NodeData::TypeStringSingleton(_) => NativeTypeKind::String,
		NodeData::TypeBooleanSingleton(_) => NativeTypeKind::Boolean,
		NodeData::TypeOptional(optional) => optional.inner.map_or(NativeTypeKind::Unspellable, |inner| {
			classify_native_type(ast, inner, origins)
		}),
		NodeData::TypeUnion(union) => ast
			.list_head(union.members)
			.map_or(NativeTypeKind::Unspellable, |head| {
				classify_native_type(ast, ast.link_value(head), origins)
			}),
		NodeData::TypeIntersection(_) => {
			if is_overload_set(ast, node) {
				NativeTypeKind::Function
			} else {
				NativeTypeKind::Unspellable
			}
		}
		NodeData::TypeTable(table) => {
			// An enum object and an empty table admit values that are not tables.
			if origins
				.origin_of(node)
				.is_some_and(|symbol| symbol.flags().intersects(SymbolFlags::Enum))
			{
				return NativeTypeKind::Unspellable;
			}
			if ast.list_is_non_empty(table.fields) {
				NativeTypeKind::Table
			} else {
				NativeTypeKind::Unspellable
			}
		}
		NodeData::TypeArray(_) => NativeTypeKind::Table,
		NodeData::TypeFunction(_) => NativeTypeKind::Function,
		NodeData::TypePack(pack) if ast.list_is_empty(pack.elements) => NativeTypeKind::Nil,
		_ => NativeTypeKind::Unspellable,
	}
}

fn classify_native_reference(ast: &Ast, node: NodeId, origins: &dyn NativeOrigins) -> NativeTypeKind {
	let Some(reference) = ast.get::<rotor_luau::TypeReference>(node) else {
		return NativeTypeKind::Unspellable;
	};
	if !reference.module.is_empty() {
		// `Enum.Material`: Luau names the enum, native codegen has no tag for it.
		return NativeTypeKind::Unspellable;
	}
	if let Some(symbol) = origins.origin_of(node) {
		let flags = symbol.flags();
		if flags.intersects(SymbolFlags::TypeParameter) {
			return NativeTypeKind::Generic;
		}
		if flags.intersects(SymbolFlags::Class | SymbolFlags::Enum) {
			// A class instance carries a metatable and an enum is a union of members: neither
			// is a table type.
			return NativeTypeKind::Unspellable;
		}
		if let Some(kind) = origins.alias_kind_of(symbol) {
			return kind;
		}
		if LUAU_ROBLOX_EXTRA_TYPE_NAMES.contains(&reference.name.as_str()) {
			return NativeTypeKind::Unspellable;
		}
		// A Roblox datatype or Instance subclass by name.
		return NativeTypeKind::Vector;
	}
	match reference.name.as_str() {
		NATIVE_TYPE_NUMBER => NativeTypeKind::Number,
		NATIVE_TYPE_STRING => NativeTypeKind::String,
		NATIVE_TYPE_BOOLEAN => NativeTypeKind::Boolean,
		NATIVE_TYPE_NIL => NativeTypeKind::Nil,
		name if is_native_vector_type_name(name) => NativeTypeKind::Vector,
		// `any`, `unknown`, `never`, and a type-function call (`keyof<T>`).
		_ => NativeTypeKind::Unspellable,
	}
}

/// `isOverloadSet`: an intersection of function types.
fn is_overload_set(ast: &Ast, node: NodeId) -> bool {
	let Some(intersection) = ast.get::<TypeIntersection>(node) else {
		return false;
	};
	ast.list_every(intersection.members, |member| {
		ast.kind(member) == LuauKind::TypeFunction
	})
}

/// `unspellableNative`: what a position emits for a type it cannot mirror.
fn unspellable_native(ast: &mut Ast, position: NativePosition) -> Option<NodeId> {
	if position == NativePosition::Nested {
		return Some(any_type(ast));
	}
	None
}

/// The policy's answer: the node to emit (`None` when a parameter or return must stay
/// unannotated), its kind, and whether a bare outer generic was met in a tagged position.
pub type PolicyResult = (Option<NodeId>, NativeTypeKind, bool);

/// `applyNativePolicy`: widens or drops `node` for the given position and depth. Nodes that need
/// no change are returned as they are, so origins stay attached.
pub fn apply_native_policy(
	ast: &mut Ast,
	origins: &mut dyn NativeOrigins,
	node: NodeId,
	position: NativePosition,
	depth: usize,
) -> PolicyResult {
	// Packs have no tag but are the shape of a return.
	match ast.data(node) {
		NodeData::TypePack(pack) => {
			let elements = pack.elements;
			let (new_elements, changed) = apply_native_list_policy(ast, origins, elements, depth + 1);
			let kind = classify_native_type(ast, node, origins);
			if !changed {
				return (Some(node), kind, false);
			}
			return (Some(ast.type_pack(new_elements)), kind, false);
		}
		NodeData::TypeVariadic(variadic) => {
			let inner = variadic.inner;
			if position == NativePosition::Return {
				// `...any` as a declared return: the analyzer infers a better pack from the body.
				return (None, NativeTypeKind::Unspellable, false);
			}
			let (new_inner, _, _) = apply_native_policy(ast, origins, inner, NativePosition::Nested, depth);
			return match new_inner {
				Some(new_inner) if new_inner != inner => {
					(Some(ast.type_variadic(new_inner)), NativeTypeKind::Unspellable, false)
				}
				_ => (Some(node), NativeTypeKind::Unspellable, false),
			};
		}
		NodeData::TypeGenericPack(_) => {
			// Only legal where a pack is, and a pack is never widened.
			return (Some(node), NativeTypeKind::Unspellable, false);
		}
		_ => {}
	}

	let kind = classify_native_type(ast, node, origins);
	if kind == NativeTypeKind::Unspellable {
		return (unspellable_native(ast, position), kind, false);
	}
	match ast.data(node) {
		NodeData::TypeReference(_) => apply_native_reference_policy(ast, origins, node, kind, position, depth),
		NodeData::TypeBooleanSingleton(_) => {
			if position == NativePosition::Parameter {
				// Booleans are tagged correctly already; the annotation would only add a guard.
				return (None, kind, false);
			}
			(Some(node), kind, false)
		}
		NodeData::TypeOptional(optional) => {
			let Some(inner) = optional.inner else {
				return (Some(node), kind, false);
			};
			let (new_inner, inner_kind, outer) = apply_native_policy(ast, origins, inner, position, depth);
			match new_inner {
				None => (None, inner_kind, outer),
				Some(new_inner) if new_inner == inner => (Some(node), inner_kind, outer),
				Some(new_inner) => (Some(ast.optional_type(new_inner)), inner_kind, outer),
			}
		}
		NodeData::TypeUnion(_) => apply_native_union_policy(ast, origins, node, position, depth),
		NodeData::TypeIntersection(_) => apply_native_overload_policy(ast, origins, node, position, depth),
		NodeData::TypeTable(_) => apply_native_table_policy(ast, origins, node, depth),
		NodeData::TypeArray(array) => {
			let (element, access) = (array.element, array.access);
			if depth > NATIVE_TYPE_MAX_DEPTH {
				return (Some(any_table(ast)), kind, false);
			}
			let (new_element, _, _) = apply_native_policy(ast, origins, element, NativePosition::Nested, depth + 1);
			let new_element = new_element.unwrap_or(element);
			if new_element == element && access == TypeAccess::None {
				return (Some(node), kind, false);
			}
			(Some(ast.type_array(new_element)), kind, false)
		}
		NodeData::TypeFunction(_) => apply_native_function_policy(ast, origins, node, depth),
		_ => (Some(node), kind, false),
	}
}

/// `applyNativeReferencePolicy`: keeps the references Luau tags usefully.
fn apply_native_reference_policy(
	ast: &mut Ast,
	origins: &mut dyn NativeOrigins,
	node: NodeId,
	kind: NativeTypeKind,
	position: NativePosition,
	depth: usize,
) -> PolicyResult {
	let Some(reference) = ast.get::<rotor_luau::TypeReference>(node) else {
		return (Some(node), kind, false);
	};
	let name = reference.name.clone();
	let arguments = reference.arguments;
	match kind {
		NativeTypeKind::Nil => {
			return match position {
				NativePosition::Return => {
					let elements = ast.new_list::<element::TypeNode>();
					(Some(ast.type_pack(elements)), kind, false)
				}
				NativePosition::Parameter => (None, NativeTypeKind::Unspellable, false),
				NativePosition::Nested => (Some(node), kind, false),
			};
		}
		NativeTypeKind::Boolean => {
			if position == NativePosition::Parameter {
				return (None, kind, false);
			}
			return (Some(node), kind, false);
		}
		NativeTypeKind::Generic => {
			if position == NativePosition::Parameter && !origins.own_generic(&name) {
				return (None, kind, true);
			}
			return (Some(node), kind, false);
		}
		NativeTypeKind::Vector => {
			if position == NativePosition::Parameter && !is_native_vector_type_name(&name) {
				// An Instance tag is userdata, not a speedup.
				return (None, NativeTypeKind::Unspellable, false);
			}
			return (Some(node), kind, false);
		}
		_ => {}
	}
	let Some(arguments) = arguments.filter(|&arguments| ast.list_is_non_empty(arguments)) else {
		return (Some(node), kind, false);
	};
	let (new_arguments, changed) = apply_native_list_policy(ast, origins, arguments, depth + 1);
	if !changed {
		return (Some(node), kind, false);
	}
	let reference = ast.type_reference(name, Some(new_arguments));
	origins.transfer_origin(node, reference);
	(Some(reference), kind, false)
}

/// `applyNativeUnionPolicy`: mirrors every member at the union's position.
fn apply_native_union_policy(
	ast: &mut Ast,
	origins: &mut dyn NativeOrigins,
	node: NodeId,
	position: NativePosition,
	depth: usize,
) -> PolicyResult {
	let Some(union) = ast.get::<rotor_luau::TypeUnion>(node) else {
		return (Some(node), NativeTypeKind::Unspellable, false);
	};
	let current_members = ast.list_to_vec(union.members);
	let mut members: Vec<(NodeId, NativeTypeKind, String)> = Vec::new();
	let mut outer = false;
	let mut changed = false;
	for current in current_members {
		let (member, kind, member_outer) = apply_native_policy(ast, origins, current, position, depth);
		outer = outer || member_outer;
		let Some(member) = member else {
			return (None, NativeTypeKind::Unspellable, outer);
		};
		if is_any_type(ast, member) {
			return (Some(member), NativeTypeKind::Unspellable, outer);
		}
		changed = changed || member != current;
		let text = render_type_text(ast, member).unwrap_or_else(|error| {
			origins.ice(format!("transformer: renderTypeText: {error}"));
			String::new()
		});
		if members.iter().any(|(_, _, existing)| *existing == text) {
			changed = true;
			continue;
		}
		members.push((member, kind, text));
	}
	if members.is_empty() {
		return (unspellable_native(ast, position), NativeTypeKind::Unspellable, outer);
	}
	if members.len() == 1 {
		return (Some(members[0].0), members[0].1, outer);
	}
	let first_kind = members[0].1;
	if position == NativePosition::Parameter {
		for &(_, kind, _) in &members {
			if kind != first_kind
				|| (kind != NativeTypeKind::Table && kind != NativeTypeKind::Vector && kind != NativeTypeKind::String)
			{
				return (None, NativeTypeKind::Unspellable, outer);
			}
		}
	}
	if !changed {
		return (Some(node), first_kind, outer);
	}
	let list = ast.list_from::<element::TypeNode>(members.iter().map(|&(member, _, _)| member));
	(Some(ast.type_union(list)), first_kind, outer)
}

/// `applyNativeOverloadPolicy`: keeps an overload set everywhere but at a tagged parameter,
/// where `(...any) -> ...any` keeps a real `function` tag.
fn apply_native_overload_policy(
	ast: &mut Ast,
	origins: &mut dyn NativeOrigins,
	node: NodeId,
	position: NativePosition,
	depth: usize,
) -> PolicyResult {
	if position == NativePosition::Parameter {
		return (Some(any_function(ast)), NativeTypeKind::Function, false);
	}
	let Some(intersection) = ast.get::<TypeIntersection>(node) else {
		return (Some(node), NativeTypeKind::Function, false);
	};
	let members = intersection.members;
	let (new_members, changed) = apply_native_list_policy(ast, origins, members, depth);
	if !changed {
		return (Some(node), NativeTypeKind::Function, false);
	}
	(
		Some(ast.type_intersection(new_members)),
		NativeTypeKind::Function,
		false,
	)
}

/// `applyNativeTablePolicy`: a table's members as nested slots with `read`/`write` dropped.
fn apply_native_table_policy(
	ast: &mut Ast,
	origins: &mut dyn NativeOrigins,
	node: NodeId,
	depth: usize,
) -> PolicyResult {
	if depth > NATIVE_TYPE_MAX_DEPTH {
		return (Some(any_table(ast)), NativeTypeKind::Table, false);
	}
	let Some(table) = ast.get::<rotor_luau::TypeTable>(node) else {
		return (Some(node), NativeTypeKind::Table, false);
	};
	let current_fields = ast.list_to_vec(table.fields);
	let fields = ast.new_list::<element::TypeTableField>();
	let mut changed = false;
	for field in current_fields {
		match ast.data(field) {
			NodeData::TypeTableProp(prop) => {
				let (name, value, access) = (prop.name.clone(), prop.value, prop.access);
				let (new_value, _, _) = apply_native_policy(ast, origins, value, NativePosition::Nested, depth + 1);
				let new_value = new_value.unwrap_or(value);
				if new_value == value && access == TypeAccess::None {
					ast.list_push(fields, field);
					continue;
				}
				changed = true;
				let prop = ast.type_table_prop(TypeAccess::None, name, new_value);
				ast.list_push(fields, prop);
			}
			NodeData::TypeTableIndexer(indexer) => {
				let (key, value, access) = (indexer.key, indexer.value, indexer.access);
				let (new_key, _, _) = apply_native_policy(ast, origins, key, NativePosition::Nested, depth + 1);
				let (new_value, _, _) = apply_native_policy(ast, origins, value, NativePosition::Nested, depth + 1);
				let new_key = new_key.unwrap_or(key);
				let new_value = new_value.unwrap_or(value);
				if new_key == key && new_value == value && access == TypeAccess::None {
					ast.list_push(fields, field);
					continue;
				}
				changed = true;
				let indexer = ast.type_table_indexer(TypeAccess::None, new_key, new_value);
				ast.list_push(fields, indexer);
			}
			_ => {}
		}
	}
	if !changed {
		return (Some(node), NativeTypeKind::Table, false);
	}
	let table = ast.type_table(fields);
	origins.transfer_origin(node, table);
	(Some(table), NativeTypeKind::Table, false)
}

/// `applyNativeFunctionPolicy`: a function type's parameters, varargs and return as nested
/// slots. A method's `self` is `any`.
fn apply_native_function_policy(
	ast: &mut Ast,
	origins: &mut dyn NativeOrigins,
	node: NodeId,
	depth: usize,
) -> PolicyResult {
	let Some(function) = ast.get::<rotor_luau::TypeFunction>(node) else {
		return (Some(node), NativeTypeKind::Function, false);
	};
	let generics = function.generics.clone();
	let current_parameters = function.parameters.clone();
	let current_var_args = function.var_args;
	let current_return = function.ret;
	let mut changed = false;
	let mut parameters = Vec::with_capacity(current_parameters.len());
	for (index, parameter) in current_parameters.into_iter().enumerate() {
		if index == 0 && parameter.name == "self" {
			if is_any_type(ast, parameter.ty) {
				parameters.push(parameter);
			} else {
				changed = true;
				let any = any_type(ast);
				parameters.push(TypeFunctionParameter::new(parameter.name, any));
			}
			continue;
		}
		let (parameter_type, _, _) = apply_native_policy(ast, origins, parameter.ty, NativePosition::Nested, depth + 1);
		let parameter_type = parameter_type.unwrap_or(parameter.ty);
		changed = changed || parameter_type != parameter.ty;
		parameters.push(TypeFunctionParameter::new(parameter.name, parameter_type));
	}
	let mut var_args = current_var_args;
	if let Some(current) = current_var_args {
		let (new_var_args, _, _) = apply_native_policy(ast, origins, current, NativePosition::Nested, depth + 1);
		var_args = new_var_args;
		changed = changed || var_args != current_var_args;
	}
	let mut return_type = current_return;
	if let Some(current) = current_return {
		let (new_return, _, _) = apply_native_policy(ast, origins, current, NativePosition::Nested, depth + 1);
		return_type = new_return;
		changed = changed || return_type != current_return;
	}
	if !changed {
		return (Some(node), NativeTypeKind::Function, false);
	}
	let ret = return_type.unwrap_or_else(|| any_type(ast));
	(
		Some(ast.type_function(generics, parameters, var_args, ret)),
		NativeTypeKind::Function,
		false,
	)
}

/// `applyNativeListPolicy`: every element of a list mirrored at a nested slot of the given
/// depth, and whether any element changed.
fn apply_native_list_policy(
	ast: &mut Ast,
	origins: &mut dyn NativeOrigins,
	list: List<element::TypeNode>,
	depth: usize,
) -> (List<element::TypeNode>, bool) {
	let result = ast.new_list::<element::TypeNode>();
	let mut changed = false;
	for element in ast.list_to_vec(list) {
		let (node, _, _) = apply_native_policy(ast, origins, element, NativePosition::Nested, depth);
		let node = node.unwrap_or(element);
		changed = changed || node != element;
		ast.list_push(result, node);
	}
	(result, changed)
}
