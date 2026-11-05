//! `comment_directives_test.go`

use super::{expect_compiled, expect_failed, keys, large_stack, write_project};
use crate::ProjectOptions;
use crate::project::{compile_project, compile_project_with_options};

const SOURCE: &str = "// @ts-ignore\n// @ts-expect-error\n// @ts-nocheck\nexport const value = 1;\n";

#[test]
fn compile_project_comment_directives_rejected_by_default() {
	let temp = write_project("@scope/comment-directives", "");
	temp.write("src/main.ts", SOURCE);

	let diags = expect_failed(large_stack(|| compile_project(&temp.dir())));
	assert_eq!(diags.len(), 3, "diags: {diags:?}");
	let want =
		rotor_transformer::diagnostics::no_comment_directives(None::<rotor_tsrs::P<rotor_tsrs::ast::Node>>).message;
	for diag in &diags {
		assert_eq!(diag, &want);
	}
}

#[test]
fn compile_project_allow_comment_directives_suppresses_diagnostics() {
	let temp = write_project("@scope/comment-directives-allowed", "");
	temp.write("src/main.ts", SOURCE);

	let options = ProjectOptions {
		allow_comment_directives: true,
		..ProjectOptions::default()
	};
	let files = expect_compiled(large_stack(|| compile_project_with_options(&temp.dir(), &options)));
	assert!(
		files.contains_key("out/main.luau"),
		"out/main.luau missing ({:?})",
		keys(&files)
	);
}
