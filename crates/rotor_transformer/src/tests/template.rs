//! `template_test.go`

use super::{render_file_statements, transformer_testdata, with_state};

#[test]
fn tagged_template_against_rbxtsc() {
	with_state(&transformer_testdata("operators"), "src/template.ts", |s| {
		assert_eq!(render_file_statements(s), "tag({ \"a\", \"b\", \"c\" }, 1, 2)\n");
	});
}
