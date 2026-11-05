//! `delete_test.go`

use super::{render_list, render_node, transformer_testdata, with_state};
use crate::dispatch::transform_expression;
use crate::prelude::*;
use crate::statementlist::transform_statement_list;

fn first_initializer(statement: P<Node>) -> P<Node> {
	statement
		.as_variable_statement()
		.declaration_list
		.as_variable_declaration_list()
		.declarations
		.nodes()[0]
		.as_variable_declaration()
		.initializer()
		.expect("an initializer")
}

#[test]
fn delete_expression_emits_nil_assignment_and_statement_value_semantics() {
	with_state(&transformer_testdata("operators"), "src/delete.ts", |s| {
		let statements = s.source_file.statements().nodes();
		let statement_delete = statements[2].as_expression_statement().expression;
		let (statement_result, statement_prereqs) = s.capture(|s| transform_expression(s, statement_delete));
		assert_eq!(s.ast.kind(statement_result), LuauKind::None, "statement delete result");
		assert_eq!(
			render_list(s, statement_prereqs),
			"obj[key] = nil\n",
			"statement delete prereqs"
		);

		let value_delete = first_initializer(statements[3]);
		let (value_result, value_prereqs) = s.capture(|s| transform_expression(s, value_delete));
		assert_eq!(render_node(s, value_result), "true", "value delete result");
		assert_eq!(render_list(s, value_prereqs), "obj.foo = nil\n", "value delete prereqs");
		let diagnostics = s.diags.flush();
		assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
	});
}

#[test]
fn delete_statement_list_against_rbxtsc() {
	with_state(&transformer_testdata("operators"), "src/delete.ts", |s| {
		let file = s.source_file;
		let statements = transform_statement_list(s, Some(file.as_node()), file.statements().nodes(), None);
		assert_eq!(
			render_list(s, statements),
			"obj[key] = nil\nobj.foo = nil\nlocal deleted = true\n"
		);
		let diagnostics = s.diags.flush();
		assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
	});
}
