//! `identifier_test.go`. `TestCheckIdentifierHoistUseBeforeDeclaration` renders a function
//! declaration and `...SelfReferenceExemptions` transforms classes, so they move with slices (ii)
//! and (iii).

use rotor_luau::{Identifier, PropertyAccessExpression};

use super::{collect_identifiers, repo_root, with_state};
use crate::identifier::transform_identifier;
use crate::prelude::*;

fn diff_project() -> std::path::PathBuf {
	repo_root().join("testdata/diff/project")
}

/// Fixture 02's initializer `undefined` (the only identifier spelled so) becomes `nil`.
#[test]
fn transform_identifier_undefined_to_nil() {
	with_state(&diff_project(), "src/02_variables.ts", |s| {
		let identifiers = collect_identifiers(s.source_file.as_node(), "undefined");
		assert_eq!(identifiers.len(), 1);
		let got = transform_identifier(s, identifiers[0]);
		assert_eq!(s.ast.kind(got), LuauKind::NilLiteral);
		let diagnostics = s.diags.flush();
		assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
	});
}

/// Fixture 08's reads of `export let mutable` route through the exports table.
#[test]
fn transform_identifier_export_let_routing() {
	with_state(&diff_project(), "src/08_exports.ts", |s| {
		// TransformSourceFile registers the file's module symbol as `exports`.
		let file_node = s.source_file.as_node();
		let file_symbol = s
			.ck()
			.get_symbol_at_location(file_node)
			.expect("source file has no module symbol");
		let exports = s.ast.global_id("exports");
		s.set_module_id_by_symbol(file_symbol, exports);

		// occurrences: declaration name, write lvalue, read in print(...)
		let identifiers = collect_identifiers(s.source_file.as_node(), "mutable");
		assert_eq!(
			identifiers.len(),
			3,
			"found {} identifiers named mutable, want 3",
			identifiers.len()
		);
		let read = identifiers[2];

		let got = transform_identifier(s, read);
		let access = s
			.ast
			.get::<PropertyAccessExpression>(got)
			.expect("TransformIdentifier(mutable read) must be a property access")
			.clone();
		assert_eq!(access.name, "mutable");
		assert_eq!(
			s.ast.get::<Identifier>(access.expression).map(|id| id.name.as_str()),
			Some("exports")
		);

		// reads of the immutable export stay plain identifiers
		let internal_read = collect_identifiers(s.source_file.as_node(), "internal")[1];
		let got = transform_identifier(s, internal_read);
		assert_eq!(s.ast.kind(got), LuauKind::Identifier);
		let diagnostics = s.diags.flush();
		assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
	});
}
