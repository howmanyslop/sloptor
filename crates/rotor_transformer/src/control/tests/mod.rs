//! The Go tests of slice (ii), with the expected strings verbatim. Tests whose fixture reaches
//! another slice's stub are `#[ignore]`d with that stub named; they run once it lands.

use std::path::Path;

use rotor_tsrs::{Context, ProjectOptions, load_project, with_large_stack};

use crate::prelude::*;
use crate::state::{MultiState, StateOptions};
use crate::statementlist::transform_statement_list;
use crate::tests::{render_list, transformer_testdata, with_state};

mod arrayappendoptimization;
mod ensureorder_storage;
mod forof;
mod functions;
mod identifier;
mod loops;
mod namedfunction;
mod sharedtable;
mod switch;
mod varargsoptimization;

/// Go's per-directory `render<Dir>File`: the file's statement list, rendered, with no
/// diagnostics allowed.
fn render_file(dir: &str, rel_path: &str) -> String {
	with_state(
		&transformer_testdata(dir),
		rel_path,
		crate::tests::render_file_statements,
	)
}

/// Go's `renderFunctionsSourceFile`: the whole file through `TransformSourceFile`.
fn render_source_file(dir: &str, rel_path: &str) -> String {
	with_state(&transformer_testdata(dir), rel_path, |state| {
		let statements = crate::sourcefile::transform_source_file(state);
		let diagnostics = state.diags.flush();
		assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
		render_list(state, statements)
	})
}

/// Go's `transformExpectingDiagnostics` / `forOfDiagnostics`: the diagnostics of the file's
/// statement list.
fn transform_expecting_diagnostics(dir: &str, rel_path: &str) -> Vec<Diagnostic> {
	with_state(&transformer_testdata(dir), rel_path, |state| {
		let file = state.source_file;
		transform_statement_list(state, Some(file.as_node()), file.statements().nodes(), None);
		state.diags.flush()
	})
}

/// Go's `hasDiagnostic`.
fn has_diagnostic(diagnostics: &[Diagnostic], code: &str, message_substring: &str) -> bool {
	diagnostics
		.iter()
		.any(|diagnostic| diagnostic.code == code && diagnostic.message.contains(message_substring))
}

/// Go's `buildVarArgsOptimizationState`: a one-file project with only
/// `{"compilerOptions":{"target":"ESNext"}}` (the default lib), not type checked.
fn with_var_args_state<R: Send>(source: &str, test: impl FnOnce(&mut State<'_>) -> R + Send) -> R {
	let directory = tempfile::tempdir().expect("a temporary directory");
	std::fs::write(
		directory.path().join("tsconfig.json"),
		r#"{"compilerOptions":{"target":"ESNext"}}"#,
	)
	.expect("the tsconfig writes");
	std::fs::write(directory.path().join("input.ts"), source).expect("the source writes");
	let root = directory.path().canonicalize().expect("the directory resolves");
	with_project_state(&root, "input.ts", test)
}

fn with_project_state<R: Send>(project: &Path, rel_path: &str, test: impl FnOnce(&mut State<'_>) -> R + Send) -> R {
	let project = project.to_path_buf();
	let rel_path = rel_path.to_owned();
	with_large_stack(move || {
		let options = ProjectOptions {
			checkers: Some(1),
			single_threaded: Some(true),
			skip_validation: true,
		};
		let loaded = load_project(&project.join("tsconfig.json"), &options).expect("the project loads");
		let program = loaded.program;
		let wanted = format!("{}/{rel_path}", loaded.directory());
		let file = program
			.source_files()
			.iter()
			.copied()
			.find(|file| file.file_name() == wanted)
			.unwrap_or_else(|| panic!("source file not in program: {wanted}"));
		let context = Context::default();
		let mut checker = program.get_type_checker_for_file(&context, file);
		let mut multi = MultiState::new();
		let mut state = State::new(program, &mut checker, file, &mut multi, StateOptions::default());
		let result = test(&mut state);
		drop(state);
		drop(checker);
		result
	})
}
