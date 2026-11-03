//! `voidexpr_test.go`

use super::{render_list, render_node, transformer_testdata, with_state};
use crate::dispatch::transform_expression;
use crate::statementlist::transform_statement_list;

#[test]
fn void_expression_prereqs_operand_and_yields_nil() {
	with_state(&transformer_testdata("operators"), "src/void.ts", |s| {
		let statements = s.source_file.statements().nodes();
		let value_void = statements[3]
			.as_variable_statement()
			.declaration_list
			.as_variable_declaration_list()
			.declarations
			.nodes()[0]
			.as_variable_declaration()
			.initializer()
			.expect("an initializer");
		let (result, prereqs) = s.capture(|s| transform_expression(s, value_void));
		assert_eq!(render_node(s, result), "nil", "value void result");
		assert_eq!(render_list(s, prereqs), "sideEffect()\n", "value void prereqs");
		let diagnostics = s.diags.flush();
		assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
	});
}

#[test]
fn void_statement_list_against_rbxtsc() {
	with_state(&transformer_testdata("operators"), "src/void.ts", |s| {
		let file = s.source_file;
		let statements = transform_statement_list(s, Some(file.as_node()), file.statements().nodes(), None);
		assert_eq!(
			render_list(s, statements),
			"sideEffect()\nlocal _ = nil\nsideEffect()\nlocal value = nil\nlocal _1 = flag\nlocal dropped = nil\n"
		);
		let diagnostics = s.diags.flush();
		assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
	});
}
