//! Stage 4b slice (v), Luau type annotations: `luautypes*.go` and `nativetypes*.go`.
//!
//! Both features are off by default; with them off these hooks behave exactly as Go's do (no
//! statements, no annotations). See `SLICES.md`.

mod luautypes;
mod luautypes_alias;
mod luautypes_builder;
mod luautypes_class;
mod luautypes_emit;
mod luautypes_typefunction;
mod luautypes_typefunction_prelude;
mod nativetypes;
mod nativetypes_alias;
mod nativetypes_policy;
mod nativetypes_spelling;

#[cfg(test)]
mod tests;

use rotor_luau::TypeGeneric;

use self::luautypes::TypeBuilder;
use self::nativetypes::TypeGenericScope;
use crate::prelude::*;

/// Per-file state of this module (Go's `typeScopes`, `luauTypeBuilder`,
/// `nativeTypeBuilder`).
#[derive(Debug, Default)]
pub struct SliceState {
	type_scopes: Vec<TypeGenericScope>,
	/// `luauTypeBuilder`: the file's shared builder, created on first use.
	shared: Option<TypeBuilder>,
	/// `nativeTypeBuilder`: the native annotations' own builder when `luauTypes` owns the
	/// shared one.
	native: Option<TypeBuilder>,
}

/// `hasNativeDirective` (`nativetypes.go`): the file's leading comments contain `//!native`.
pub fn has_native_directive(state: &mut State<'_>) -> bool {
	nativetypes::has_native_directive(state)
}

/// `PushTypeGenerics` (`nativetypes.go`): enters a function's generic scope. Every function
/// pushes one, even with no generics, so the innermost entry is always the current function's.
pub fn push_type_generics(state: &mut State<'_>, names: Vec<String>, barrier: bool) {
	state
		.slices
		.luau_types
		.type_scopes
		.push(TypeGenericScope { names, barrier });
}

/// `PopTypeGenerics` (`nativetypes.go`): leaves the innermost generic scope.
pub fn pop_type_generics(state: &mut State<'_>) {
	state.slices.luau_types.type_scopes.pop();
}

/// `signatureGenerics` (`luautypes_emit.go`): a function's own generics. Without either feature
/// there are none.
pub fn signature_generics(state: &mut State<'_>, node: P<Node>) -> Vec<TypeGeneric> {
	luautypes_emit::signature_generics(state, node)
}

/// `signatureReturnType` (`luautypes_emit.go`): `None` leaves the return unannotated.
pub fn signature_return_type(state: &mut State<'_>, node: P<Node>) -> Option<NodeId> {
	luautypes_emit::signature_return_type(state, node)
}

/// `signatureVarArgsType` (`luautypes_emit.go`): `None` leaves `...` unannotated.
pub fn signature_var_args_type(state: &mut State<'_>, node: P<Node>) -> Option<NodeId> {
	luautypes_emit::signature_var_args_type(state, node)
}

/// `luauDeclarationStatements` (`luautypes_alias.go`): the `type` statement at a top-level
/// interface, type alias or enum declaration of this file.
pub fn luau_declaration_statements(state: &mut State<'_>, node: P<Node>) -> Statements {
	luautypes_alias::luau_declaration_statements(state, node)
}

/// `nativeDeclarationAlias` (`nativetypes_alias.go`): nothing outside a `//!native` file.
pub fn native_declaration_alias(state: &mut State<'_>, node: P<Node>) -> Statements {
	nativetypes_alias::native_declaration_alias(state, node)
}

/// `luauHoistedTypeStatements` (`luautypes_alias.go`): the hoisted alias statements.
pub fn luau_hoisted_type_statements(state: &mut State<'_>) -> Statements {
	luautypes_alias::luau_hoisted_type_statements(state)
}

/// `nativeHoistedTypeStatements` (`nativetypes_alias.go`): the hoisted `//!native` aliases.
pub fn native_hoisted_type_statements(state: &mut State<'_>) -> Statements {
	nativetypes_alias::native_hoisted_type_statements(state)
}

/// `luauRefinedCondition` (`luautypes_emit.go`): the condition temp itself without `luauTypes`.
pub fn luau_refined_condition(state: &mut State<'_>, node: P<Node>, condition_id: NodeId) -> NodeId {
	luautypes_emit::luau_refined_condition(state, node, condition_id)
}

/// `getLuauLocalType` (`luautypes_emit.go`): `None` leaves the local unannotated.
pub fn get_luau_local_type(state: &mut State<'_>, identifier: P<Node>, right: Option<NodeId>) -> Option<NodeId> {
	luautypes_emit::get_luau_local_type(state, identifier, right)
}

/// `luauTypedBinding` (`luautypes_emit.go`): `id` annotated with `t`.
pub fn luau_typed_binding(state: &mut State<'_>, symbol: P<Symbol>, id: NodeId, t: Option<NodeId>) -> NodeId {
	luautypes_emit::luau_typed_binding(state, symbol, id, t)
}

/// `getLuauVarargType` (`luautypes_emit.go`): only called with `luauTypes` on.
pub fn get_luau_vararg_type(state: &mut State<'_>, node: P<Node>) -> Option<NodeId> {
	luautypes_emit::get_luau_vararg_type(state, node)
}

/// `getLuauParameterType` (`luautypes_emit.go`): only called with `luauTypes` on.
#[expect(
	clippy::unnecessary_wraps,
	reason = "the signature the control and classes modules call"
)]
pub fn get_luau_parameter_type(state: &mut State<'_>, parameter: P<Node>) -> Option<NodeId> {
	Some(luautypes_emit::get_luau_parameter_type(state, parameter))
}

/// `getLuauBindingElementType` (`luautypes_emit.go`): only called with `luauTypes` on.
#[expect(
	clippy::unnecessary_wraps,
	reason = "the signature the control and classes modules call"
)]
pub fn get_luau_binding_element_type(state: &mut State<'_>, element: P<Node>) -> Option<NodeId> {
	Some(luautypes_emit::get_luau_binding_element_type(state, element))
}

/// `getLuauSelfType` (`luautypes_emit.go`): only called with `luauTypes` on.
pub fn get_luau_self_type(state: &mut State<'_>, node: P<Node>) -> Option<NodeId> {
	luautypes_emit::get_luau_self_type(state, node)
}

/// `getNativeParameterType` (`nativetypes.go`): only called in a `//!native` file.
pub fn get_native_parameter_type(state: &mut State<'_>, parameter: P<Node>) -> Option<NodeId> {
	nativetypes::get_native_parameter_type(state, parameter)
}

/// `luauDefaultRefinement` (`luautypes_emit.go`): the shadowing local that narrows a defaulted
/// parameter, or `None`. Only called with `luauTypes` on.
pub fn luau_default_refinement(state: &mut State<'_>, name: P<Node>, owner: P<Node>, id: NodeId) -> Option<NodeId> {
	luautypes_emit::luau_default_refinement(state, name, owner, id)
}

/// `luauEnumBinding` (`luautypes_class.go`): `id` itself without `luauTypes`.
pub fn luau_enum_binding(state: &mut State<'_>, node: P<Node>, id: NodeId) -> NodeId {
	luautypes_class::luau_enum_binding(state, node, id)
}

/// `luauEnumTableCast` (`luautypes_class.go`): `table` itself without `luauTypes`.
pub fn luau_enum_table_cast(state: &mut State<'_>, node: P<Node>, table: NodeId) -> NodeId {
	luautypes_class::luau_enum_table_cast(state, node, table)
}

/// `luauClass` (`luautypes_class.go`): a class's Luau type scaffolding. The class port (slice
/// iii) builds one only with `luauTypes` on.
#[derive(Debug)]
pub struct LuauClass {
	node: P<Node>,
	name: NodeId,
	instance_type: Option<P<Type>>,
	generics: Vec<TypeGeneric>,
}

/// `newLuauClass` (`luautypes_class.go`).
pub fn new_luau_class(state: &mut State<'_>, node: P<Node>, name: NodeId) -> LuauClass {
	luautypes_class::new_luau_class(state, node, name)
}
