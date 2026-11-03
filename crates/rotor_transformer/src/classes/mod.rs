//! Stage 4b slice (iii), classes, enums and namespaces: `classes.go`, `enums.go` and
//! `namespaces.go`.
//!
//! Stubs with the signatures core code calls; the slice port replaces the bodies and may add
//! submodules below this file. `getAllSuperTypeNodes` and `validateHeritageClause` are already
//! ported in [`crate::methodassignment`]. See `SLICES.md`.

use crate::prelude::*;

/// Per-file state owned by this slice.
#[derive(Debug, Default)]
pub struct SliceState {}

/// Pass-wide state owned by this slice.
#[derive(Debug, Default)]
pub struct MultiSliceState {}

/// `transformClassDeclaration` (`classes.go`).
pub fn transform_class_declaration(state: &mut State<'_>, node: P<Node>) -> Statements {
	state.not_yet_ported(node, "class declarations");
	state.ast.new_list()
}

/// `transformClassExpression` (`classes.go`).
pub fn transform_class_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	state.not_yet_ported(node, "class expressions");
	state.ast.none()
}

/// `transformThisExpression` (`classes.go`).
pub fn transform_this_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	state.not_yet_ported(node, "`this`");
	state.ast.none()
}

/// `transformSuperKeyword` (`classes.go`): the `super` local declared in the class's block.
pub fn transform_super_keyword(state: &mut State<'_>) -> NodeId {
	state.ast.global_id("super")
}

/// `transformEnumDeclaration` (`enums.go`).
pub fn transform_enum_declaration(state: &mut State<'_>, node: P<Node>) -> Statements {
	state.not_yet_ported(node, "enums");
	state.ast.new_list()
}

/// `transformModuleDeclaration` (`namespaces.go`).
pub fn transform_module_declaration(state: &mut State<'_>, node: P<Node>) -> Statements {
	state.not_yet_ported(node, "namespaces");
	state.ast.new_list()
}

/// `transformPropertyInitializers` (`classes.go`), called by the Luau class port (slice v).
pub fn transform_property_initializers(state: &mut State<'_>, node: P<Node>) -> Statements {
	state.not_yet_ported(node, "property initializers");
	state.ast.new_list()
}
