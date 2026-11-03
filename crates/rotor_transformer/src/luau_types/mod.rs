//! Stage 4b slice (v), Luau type annotations: `luautypes*.go` and `nativetypes*.go`.
//!
//! Both features are off by default; with them off these hooks behave exactly as Go's do (no
//! statements, no annotations). With either on, the stubs report `rotorNotYetPorted`. The slice
//! port replaces the bodies and may add submodules below this file. See `SLICES.md`.

use rotor_luau::TypeGeneric;

use crate::prelude::*;

/// Per-file state owned by this slice (Go's `typeScopes`, `luauTypeBuilder`,
/// `nativeTypeBuilder`).
#[derive(Debug, Default)]
pub struct SliceState {
	reported: bool,
	type_scopes: Vec<TypeGenericScope>,
}

/// `typeGenericScope` (`nativetypes.go`): one function's generic names.
#[derive(Debug, Default)]
struct TypeGenericScope {
	names: Vec<String>,
	barrier: bool,
}

/// Pass-wide state owned by this slice.
#[derive(Debug, Default)]
pub struct MultiSliceState {}

fn report_once(state: &mut State<'_>, node: P<Node>, what: &str) {
	if !state.slices.luau_types.reported {
		state.slices.luau_types.reported = true;
		state.not_yet_ported(node, what);
	}
}

/// `hasNativeDirective` (`nativetypes.go`): only asked with `nativeTypes` on.
pub fn has_native_directive(state: &mut State<'_>) -> bool {
	let node = state.source_file.as_node();
	report_once(state, node, "nativeTypes");
	false
}

/// `luauDeclarationStatements` (`luautypes_alias.go`): only called with `luauTypes` on.
pub fn luau_declaration_statements(state: &mut State<'_>, node: P<Node>) -> Statements {
	report_once(state, node, "luauTypes");
	state.ast.new_list()
}

/// `nativeDeclarationAlias` (`nativetypes_alias.go`): nothing outside a `//!native` file.
pub fn native_declaration_alias(state: &mut State<'_>, node: P<Node>) -> Statements {
	if state.is_native {
		report_once(state, node, "nativeTypes");
	}
	state.ast.new_list()
}

/// `luauHoistedTypeStatements` (`luautypes_alias.go`): nothing before any alias was registered.
pub fn luau_hoisted_type_statements(state: &mut State<'_>) -> Statements {
	state.ast.new_list()
}

/// `nativeHoistedTypeStatements` (`nativetypes_alias.go`): nothing before any alias was
/// registered.
pub fn native_hoisted_type_statements(state: &mut State<'_>) -> Statements {
	state.ast.new_list()
}

/// `luauRefinedCondition` (`luautypes_emit.go`): the condition temp itself without `luauTypes`.
pub fn luau_refined_condition(state: &mut State<'_>, node: P<Node>, condition_id: NodeId) -> NodeId {
	if state.is_luau_typed {
		report_once(state, node, "luauTypes");
	}
	condition_id
}

/// `getLuauLocalType` (`luautypes_emit.go`): only called with `luauTypes` on; `None` leaves the
/// local unannotated.
pub fn get_luau_local_type(state: &mut State<'_>, identifier: P<Node>, _right: Option<NodeId>) -> Option<NodeId> {
	report_once(state, identifier, "luauTypes");
	None
}

/// `luauTypedBinding` (`luautypes_emit.go`): `id` annotated with `t`.
pub const fn luau_typed_binding(_state: &mut State<'_>, _symbol: P<Symbol>, id: NodeId, _t: Option<NodeId>) -> NodeId {
	id
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
	if state.luau_typed_signatures() || state.is_native {
		report_once(state, node, "signature generics");
	}
	Vec::new()
}

/// `signatureReturnType` (`luautypes_emit.go`): `None` leaves the return unannotated.
pub fn signature_return_type(state: &mut State<'_>, node: P<Node>) -> Option<NodeId> {
	if state.luau_typed_signatures() || state.is_native {
		report_once(state, node, "signature return types");
	}
	None
}

/// `signatureVarArgsType` (`luautypes_emit.go`): `None` leaves `...` unannotated.
pub fn signature_var_args_type(state: &mut State<'_>, node: P<Node>) -> Option<NodeId> {
	if state.luau_typed_signatures() {
		report_once(state, node, "luauTypes");
	}
	None
}

/// `getLuauVarargType` (`luautypes_emit.go`): only called with `luauTypes` on.
pub fn get_luau_vararg_type(state: &mut State<'_>, node: P<Node>) -> Option<NodeId> {
	report_once(state, node, "luauTypes");
	None
}

/// `getLuauParameterType` (`luautypes_emit.go`): only called with `luauTypes` on.
pub fn get_luau_parameter_type(state: &mut State<'_>, parameter: P<Node>) -> Option<NodeId> {
	report_once(state, parameter, "luauTypes");
	None
}

/// `getLuauBindingElementType` (`luautypes_emit.go`): only called with `luauTypes` on.
pub fn get_luau_binding_element_type(state: &mut State<'_>, element: P<Node>) -> Option<NodeId> {
	report_once(state, element, "luauTypes");
	None
}

/// `getLuauSelfType` (`luautypes_emit.go`): only called with `luauTypes` on.
pub fn get_luau_self_type(state: &mut State<'_>, node: P<Node>) -> Option<NodeId> {
	report_once(state, node, "luauTypes");
	None
}

/// `getNativeParameterType` (`nativetypes.go`): only called in a `//!native` file.
pub fn get_native_parameter_type(state: &mut State<'_>, parameter: P<Node>) -> Option<NodeId> {
	report_once(state, parameter, "nativeTypes");
	None
}

/// `luauDefaultRefinement` (`luautypes_emit.go`): the shadowing local that narrows a defaulted
/// parameter, or `None`. Only called with `luauTypes` on.
pub fn luau_default_refinement(state: &mut State<'_>, name: P<Node>, _owner: P<Node>, _id: NodeId) -> Option<NodeId> {
	report_once(state, name, "luauTypes");
	None
}

/// `luauEnumBinding` (`luautypes_class.go`): `id` itself without `luauTypes`.
pub fn luau_enum_binding(state: &mut State<'_>, node: P<Node>, id: NodeId) -> NodeId {
	if state.is_luau_typed {
		report_once(state, node, "luauTypes");
	}
	id
}

/// `luauEnumTableCast` (`luautypes_class.go`): `table` itself without `luauTypes`.
pub fn luau_enum_table_cast(state: &mut State<'_>, node: P<Node>, table: NodeId) -> NodeId {
	if state.is_luau_typed {
		report_once(state, node, "luauTypes");
	}
	table
}

/// `luauClass` (`luautypes_class.go`): a class's Luau type scaffolding. The class port (slice
/// iii) builds one only with `luauTypes` on.
#[derive(Debug)]
pub struct LuauClass {
	node: P<Node>,
	generic_names: Vec<String>,
}

/// `newLuauClass` (`luautypes_class.go`).
pub fn new_luau_class(state: &mut State<'_>, node: P<Node>, _name: NodeId) -> LuauClass {
	report_once(state, node, "luauTypes");
	LuauClass {
		node,
		generic_names: Vec::new(),
	}
}

impl LuauClass {
	/// `genericNames`
	#[must_use]
	pub fn generic_names(&self) -> Vec<String> {
		self.generic_names.clone()
	}

	/// `boilerplate`: the statements before and inside the class body.
	pub fn boilerplate(&self, state: &mut State<'_>, _is_class_expression: bool) -> (Statements, Statements) {
		report_once(state, self.node, "luauTypes");
		(state.ast.new_list(), state.ast.new_list())
	}

	/// `receiverFor`: the `self` type of `member`.
	pub fn receiver_for(&self, state: &mut State<'_>, _member: P<Node>) -> Option<NodeId> {
		report_once(state, self.node, "luauTypes");
		None
	}

	/// `constructorFunction`: the typed constructor declaration.
	pub fn constructor_function(
		&self,
		state: &mut State<'_>,
		_parameters: List<element::AnyIdentifier>,
		_has_dot_dot_dot: bool,
		_statements: Statements,
		_var_args_type: Option<NodeId>,
	) -> NodeId {
		report_once(state, self.node, "luauTypes");
		state.ast.none()
	}

	/// `implicitConstructor`
	pub fn implicit_constructor(&self, state: &mut State<'_>) -> Statements {
		report_once(state, self.node, "luauTypes");
		state.ast.new_list()
	}

	/// `toStringFunction`
	pub fn to_string_function(&self, state: &mut State<'_>) -> NodeId {
		report_once(state, self.node, "luauTypes");
		state.ast.none()
	}
}
