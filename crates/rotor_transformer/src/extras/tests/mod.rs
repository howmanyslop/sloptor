//! Slice vi's Go tests, with the expected strings verbatim.

mod async_;
mod decorators;
mod importexpr;
mod imports_elision;
mod jsx;
mod jsxtext;
mod labels;
mod origins;
mod sourcemap;
mod try_;
mod try_exit;

use std::path::Path;

use crate::prelude::*;
use crate::statementlist::transform_statement_list;
use crate::tests::{render_list, transformer_testdata, with_state};

/// Go's `render<Dir>Fixture`: the file's statement list, rendered, with the file's module symbol
/// mapped to `exports` first, as `TransformSourceFile` does.
pub(super) fn render_module_fixture(dir: &str, rel_path: &str) -> (String, Vec<Diagnostic>) {
	with_state(&transformer_testdata(dir), rel_path, |state| {
		let file = state.source_file;
		if let Some(symbol) = state.ck().get_symbol_at_location(file.as_node()) {
			let exports = state.ast.global_id("exports");
			state.set_module_id_by_symbol(symbol, exports);
		}
		let statements = transform_statement_list(state, Some(file.as_node()), file.statements().nodes(), None);
		assert!(
			state.internal_error().is_none(),
			"internal error: {:?}",
			state.internal_error()
		);
		let diagnostics = state.diags.flush();
		(render_list(state, statements), diagnostics)
	})
}

/// The file's statement list, rendered, without the module symbol mapping.
pub(super) fn render_plain_fixture(dir: &str, rel_path: &str) -> (String, Vec<Diagnostic>) {
	with_state(&transformer_testdata(dir), rel_path, |state| {
		let file = state.source_file;
		let statements = transform_statement_list(state, Some(file.as_node()), file.statements().nodes(), None);
		assert!(
			state.internal_error().is_none(),
			"internal error: {:?}",
			state.internal_error()
		);
		let diagnostics = state.diags.flush();
		(render_list(state, statements), diagnostics)
	})
}

/// Go's `writeFixture`: writes `contents` at `name` under `directory`, creating parents.
pub(super) fn write_fixture(directory: &Path, name: &str, contents: &str) {
	let path = directory.join(name);
	if let Some(parent) = path.parent() {
		std::fs::create_dir_all(parent).expect("the fixture directory");
	}
	std::fs::write(path, contents).expect("the fixture writes");
}

/// Go's `transformOriginSource`: a one-file project holding `source` (`input.ts`, with the
/// default lib), its statement list transformed with no diagnostics allowed.
pub(super) fn with_origin_source<R: Send>(
	source: &str,
	test: impl FnOnce(&mut State<'_>, Statements) -> R + Send,
) -> R {
	let directory = tempfile::tempdir().expect("a temporary directory");
	write_fixture(
		directory.path(),
		"tsconfig.json",
		r#"{"compilerOptions":{"target":"ESNext","strict":true,"types":[],"moduleDetection":"force"}}"#,
	);
	write_fixture(directory.path(), "input.ts", source);
	let root = rotor_support::fsutil::real_path(directory.path()).expect("the directory resolves");
	with_state(&root, "input.ts", |state| {
		let file = state.source_file;
		let statements = transform_statement_list(state, Some(file.as_node()), file.statements().nodes(), None);
		let diagnostics = state.diags.flush();
		assert!(diagnostics.is_empty(), "transform diagnostics: {diagnostics:?}");
		test(state, statements)
	})
}

/// [`with_origin_source`], rendered.
pub(super) fn render_origin_source(source: &str) -> String {
	with_origin_source(source, |state, statements| render_list(state, statements))
}

/// Go's `hasDiagnostic`: some diagnostic has `code` and a message containing `substring`.
pub(super) fn has_diagnostic(diagnostics: &[Diagnostic], code: &str, substring: &str) -> bool {
	diagnostics
		.iter()
		.any(|diagnostic| diagnostic.code == code && diagnostic.message.contains(substring))
}

/// Asserts the rendered output and that no diagnostics were reported.
pub(super) fn assert_renders(got: &(String, Vec<Diagnostic>), want: &str) {
	assert_eq!(got.0, want, "rendered output differs from rbxtsc");
	assert!(got.1.is_empty(), "unexpected diagnostics: {:?}", got.1);
}
