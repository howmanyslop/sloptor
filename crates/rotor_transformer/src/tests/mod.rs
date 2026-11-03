//! Unit tests ported from Go's `internal/transformer/*_test.go`, with the expected strings kept
//! verbatim. Go builds real tsgo programs over `internal/transformer/testdata`; these build the
//! same programs through `rotor_tsrs`.

use std::path::{Path, PathBuf};

use rotor_luau::render::{RenderState, render, render_ast};
use rotor_tsrs::{Context, ProjectOptions, load_project, with_large_stack};

use crate::prelude::*;
use crate::state::{MultiState, StateOptions};

mod bitwise;
mod call;
mod conditional;
mod delete;
mod diagnostics;
mod identifier;
mod literals;
mod loops;
mod nodemodulespath;
mod operators;
mod optionalchain;
mod refwalker;
mod runtimelib;
mod selectsymlinkpath;
mod sharedtable;
mod sourcefile;
mod spreadhelpers;
mod state;
mod statementlist;
mod statements;
mod template;
mod truthiness;
mod types;
mod voidexpr;

/// The repository root.
pub fn repo_root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Go's `internal/transformer/testdata/<name>`.
pub fn transformer_testdata(name: &str) -> PathBuf {
	repo_root().join("internal/transformer/testdata").join(name)
}

/// Go's `buildState`: a program over `project`, its semantic diagnostics required empty, and a
/// fresh state for `rel_path` (no Rojo context, a fresh `MultiState`).
pub fn with_state<R: Send>(project: &Path, rel_path: &str, test: impl FnOnce(&mut State<'_>) -> R + Send) -> R {
	build_state(project, rel_path, true, MultiState::new(), test)
}

/// [`with_state`] over a caller-supplied `MultiState`.
pub fn with_state_on<R: Send>(
	project: &Path,
	rel_path: &str,
	multi: MultiState,
	test: impl FnOnce(&mut State<'_>) -> R + Send,
) -> R {
	build_state(project, rel_path, true, multi, test)
}

fn build_state<R: Send>(
	project: &Path,
	rel_path: &str,
	check_diagnostics: bool,
	mut multi: MultiState,
	test: impl FnOnce(&mut State<'_>) -> R + Send,
) -> R {
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
		if check_diagnostics {
			let diagnostics = program.get_semantic_diagnostics(&context, Some(file));
			assert!(diagnostics.is_empty(), "unexpected semantic diagnostics in {wanted}");
		}
		let mut checker = program.get_type_checker_for_file(&context, file);
		let mut state = State::new(program, &mut checker, file, &mut multi, StateOptions::default());
		let result = test(&mut state);
		drop(state);
		drop(checker);
		result
	})
}

/// Go's `NewTestState`: a state for mechanics tests that never touch the file's types.
pub fn with_test_state<R: Send>(test: impl FnOnce(&mut State<'_>) -> R + Send) -> R {
	with_state(&transformer_testdata("control"), "src/while.ts", test)
}

/// Go's `parseFirstExpression`: a one-file project holding `source`, with the state and the
/// expression of the first statement.
pub fn with_source<R: Send>(source: &str, test: impl FnOnce(&mut State<'_>, P<Node>) -> R + Send) -> R {
	with_source_file(source, true, |state, statement| {
		assert_eq!(
			statement.kind(),
			Kind::ExpressionStatement,
			"the first statement is an expression"
		);
		let expression = statement.as_expression_statement().expression;
		test(state, expression)
	})
}

/// Go's `parseFirstExpression` over a source that does not type check: Go only parses these, so
/// the semantic diagnostics are not required empty.
pub fn with_parsed_expression<R: Send>(source: &str, test: impl FnOnce(&mut State<'_>, P<Node>) -> R + Send) -> R {
	with_source_file(source, false, |state, statement| {
		assert_eq!(
			statement.kind(),
			Kind::ExpressionStatement,
			"the first statement is an expression"
		);
		let expression = statement.as_expression_statement().expression;
		test(state, expression)
	})
}

/// Go's `parseBindingPatternNode`: the name of the first variable declaration, parsed only.
pub fn with_parsed_binding_pattern<R: Send>(source: &str, test: impl FnOnce(&mut State<'_>, P<Node>) -> R + Send) -> R {
	with_source_file(source, false, |state, statement| {
		assert_eq!(
			statement.kind(),
			Kind::VariableStatement,
			"the first statement is a variable statement"
		);
		let declaration = statement
			.as_variable_statement()
			.declaration_list
			.as_variable_declaration_list()
			.declarations
			.nodes()[0];
		let name = declaration.as_variable_declaration().name;
		test(state, name)
	})
}

/// A one-file project holding `source`, with the state and the file's first statement.
fn with_source_file<R: Send>(
	source: &str,
	check_diagnostics: bool,
	test: impl FnOnce(&mut State<'_>, P<Node>) -> R + Send,
) -> R {
	let directory = tempfile::tempdir().expect("a temporary directory");
	let type_roots = repo_root().join("testdata/diff/project/node_modules/@rbxts");
	let type_roots = type_roots.canonicalize().expect("the type roots exist");
	let tsconfig = format!(
		r#"{{
	"compilerOptions": {{
		"module": "commonjs",
		"moduleResolution": "Node",
		"noLib": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"typeRoots": [{}],
		"rootDir": "src"
	}},
	"include": ["src"]
}}"#,
		serde_quote(&type_roots.to_string_lossy())
	);
	std::fs::write(directory.path().join("tsconfig.json"), tsconfig).expect("the tsconfig writes");
	std::fs::create_dir(directory.path().join("src")).expect("the source directory");
	std::fs::write(directory.path().join("src/test.ts"), source).expect("the source writes");
	let root = directory.path().canonicalize().expect("the directory resolves");
	build_state(&root, "src/test.ts", check_diagnostics, MultiState::new(), |state| {
		let statement = state.source_file.statements().nodes()[0];
		test(state, statement)
	})
}

fn serde_quote(text: &str) -> String {
	format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Go's `render.RenderAST`.
pub fn render_list(state: &State<'_>, list: Statements) -> String {
	render_ast(&state.ast, list).expect("the statements render")
}

/// Go's `render.Render(render.NewRenderState(), expression)`.
pub fn render_node(state: &State<'_>, node: NodeId) -> String {
	render(&mut RenderState::new(), &state.ast, node).expect("the node renders")
}

/// The statements of `state`'s file through `TransformStatementList`, rendered, with no
/// diagnostics allowed (Go's `renderFileStatements`).
pub fn render_file_statements(state: &mut State<'_>) -> String {
	let file = state.source_file;
	let statements =
		crate::statementlist::transform_statement_list(state, Some(file.as_node()), file.statements().nodes(), None);
	let diagnostics = state.diags.flush();
	assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
	render_list(state, statements)
}

/// Every identifier spelled `text` under `root`, in source order (Go's `collectIdentifiers`).
pub fn collect_identifiers(root: P<Node>, text: &str) -> Vec<P<Node>> {
	fn visit(node: P<Node>, text: &str, out: &mut Vec<P<Node>>) {
		if node.kind() == Kind::Identifier && node.text() == text {
			out.push(node);
		}
		node.for_each_child(&mut |child| {
			visit(child, text, out);
			false
		});
	}
	let mut out = Vec::new();
	root.for_each_child(&mut |child| {
		visit(child, text, &mut out);
		false
	});
	out
}
