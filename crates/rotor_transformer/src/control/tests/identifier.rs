//! The hoist cases of `identifier_test.go`, over `testdata/hoist`.

use crate::identifier::transform_identifier;
use crate::prelude::*;
use crate::statementlist::transform_statement_list;
use crate::tests::{collect_identifiers, render_list, transformer_testdata, with_state};

fn must_one_identifier(root: P<Node>, text: &str) -> P<Node> {
	let identifiers = collect_identifiers(root, text);
	assert_eq!(
		identifiers.len(),
		1,
		"found {} identifiers named {text:?}, want 1",
		identifiers.len()
	);
	identifiers[0]
}

/// `function useBeforeDeclare(): number { return a + b; }` precedes `let a = 1; let b = 2;`, so
/// both symbols hoist onto the function statement, and the declarations become assignments.
#[test]
fn check_identifier_hoist_use_before_declaration() {
	with_state(&transformer_testdata("hoist"), "src/hoist.ts", |s| {
		let statements = s.source_file.statements().nodes();
		let function_statement = statements[0];
		let reference_a = must_one_identifier(function_statement, "a");
		let reference_b = must_one_identifier(function_statement, "b");

		for reference in [reference_a, reference_b] {
			let got = transform_identifier(s, reference);
			assert_eq!(
				s.ast.kind(got),
				LuauKind::Identifier,
				"TransformIdentifier({})",
				reference.text()
			);
			let symbol = s
				.ck()
				.get_symbol_at_location(reference)
				.expect("the reference has a symbol");
			assert!(
				s.is_hoisted.get(&symbol).copied().unwrap_or(false),
				"IsHoisted[{}] = false, want true",
				reference.text()
			);
		}
		let hoists = s.hoists_by_statement.get(&function_statement).map_or(0, Vec::len);
		assert_eq!(hoists, 2, "HoistsByStatement[funcStmt] entries");

		// The statement-list driver merges both hoists into one `local a, b` line before the
		// premature-reference statement, and the declarations become assignments.
		let file = s.source_file.as_node();
		let list = transform_statement_list(s, Some(file), statements, None);
		let want = "local a, b\nlocal function useBeforeDeclare()\n\treturn a + b\nend\na = 1\nb = 2\n";
		assert_eq!(render_list(s, list), want);

		let diagnostics = s.diags.flush();
		assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
	});
}

#[test]
fn check_identifier_hoist_self_reference_exemptions() {
	with_state(&transformer_testdata("hoist"), "src/selfref.ts", |s| {
		let root = s.source_file.as_node();
		let statements = s.source_file.statements().nodes();

		let assert_not_hoisted = |s: &mut State<'_>, reference: P<Node>| {
			transform_identifier(s, reference);
			let symbol = s
				.ck()
				.get_symbol_at_location(reference)
				.expect("the reference has a symbol");
			assert!(
				!s.is_hoisted.contains_key(&symbol),
				"IsHoisted[{}] recorded, want no hoist decision",
				reference.text()
			);
		};

		// non-async function declaration self-reference: fact() in its own body
		assert_not_hoisted(s, collect_identifiers(root, "fact")[1]);

		// class declaration self-reference: `new Singleton()` inside the class; [0] is the
		// declaration name, [1] the return-type position, [2] the construct reference.
		assert_not_hoisted(s, collect_identifiers(root, "Singleton")[2]);

		// variable statement direct self-reference: `let first = 1, second = first;`, whose
		// nearest statement ancestor is the declaring variable statement.
		assert_not_hoisted(s, collect_identifiers(root, "first")[1]);

		// function-wrapped self-reference still hoists: `const arrow: () => number = () =>
		// arrow();` is nested inside a function-like, so the exemption does not apply.
		let reference = collect_identifiers(root, "arrow")[1];
		transform_identifier(s, reference);
		let symbol = s
			.ck()
			.get_symbol_at_location(reference)
			.expect("the reference has a symbol");
		assert!(
			s.is_hoisted.get(&symbol).copied().unwrap_or(false),
			"IsHoisted[arrow] = false, want true"
		);
		let arrow_statement = statements[statements.len() - 1];
		let hoists = s.hoists_by_statement.get(&arrow_statement).map_or(0, Vec::len);
		assert_eq!(hoists, 1, "HoistsByStatement[arrowStmt] entries");

		let diagnostics = s.diags.flush();
		assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
	});
}
