//! Stage 4b slice (iii), classes, enums and namespaces: `classes.go`, `enums.go` and
//! `namespaces.go`.
//!
//! The functions here keep the signatures that core code and the other slices call and delegate
//! to one submodule per Go file. `getExtendsNode`, `findConstructor`, `createClassNameFunction`,
//! `isClassHoisted` and `needsInverseEntry` are ported in [`crate::shared`];
//! `getAllSuperTypeNodes` and `validateHeritageClause` in [`crate::methodassignment`]. See
//! `SLICES.md`.

#[expect(
	clippy::module_inception,
	reason = "SLICES.md names the module classes::classes: one Rust file per Go file"
)]
mod classes;
mod enums;
mod namespaces;

#[cfg(test)]
mod tests;

use crate::prelude::*;

/// `transformClassDeclaration` (`classes.go`).
pub fn transform_class_declaration(state: &mut State<'_>, node: P<Node>) -> Statements {
	classes::transform_class_declaration(state, node)
}

/// `transformClassExpression` (`classes.go`).
pub fn transform_class_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	classes::transform_class_expression(state, node)
}

/// `transformThisExpression` (`classes.go`).
pub fn transform_this_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	classes::transform_this_expression(state, node)
}

/// `transformSuperKeyword` (`classes.go`): the `super` local declared in the class's block.
pub fn transform_super_keyword(state: &mut State<'_>) -> NodeId {
	state.ast.global_id("super")
}

/// `transformEnumDeclaration` (`enums.go`).
pub fn transform_enum_declaration(state: &mut State<'_>, node: P<Node>) -> Statements {
	enums::transform_enum_declaration(state, node)
}

/// `transformModuleDeclaration` (`namespaces.go`).
pub fn transform_module_declaration(state: &mut State<'_>, node: P<Node>) -> Statements {
	namespaces::transform_module_declaration(state, node)
}

/// `transformPropertyInitializers` (`classes.go`), called by the Luau class port (slice v).
pub fn transform_property_initializers(state: &mut State<'_>, node: P<Node>) -> Statements {
	classes::transform_property_initializers(state, node)
}
