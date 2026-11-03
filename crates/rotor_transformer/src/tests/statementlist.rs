//! `statementlist_test.go`

use rotor_tsrs::ast as tsast;

use super::{render_list, transformer_testdata, with_state};
use crate::statementlist::transform_statement_list;

#[test]
fn block_trailing_comments_are_preserved() {
	with_state(&transformer_testdata("control"), "src/trailingcomment.ts", |s| {
		let declaration = s.source_file.statements().nodes()[0];
		assert!(
			tsast::is_function_declaration(declaration),
			"expected FunctionDeclaration first"
		);
		let body = declaration.body().expect("the function has a body");
		let statements = transform_statement_list(s, Some(body), body.statements(), None);
		let diagnostics = s.diags.flush();
		assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
		assert_eq!(
			render_list(s, statements),
			"print(\"body\")\n-- trailing block comment\n"
		);
	});
}
