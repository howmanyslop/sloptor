//! A stand-in for `rotor_compile` (Stage 5): the parts of Go's `internal/compile` (`compile.go`
//! around `NewState`/`TransformSourceFile`, `project.go` `newProjectContext` and
//! `compileProjectSourceFile`) that the differential fixtures need. Keep it small; Stage 5
//! replaces it.

use std::path::{Path, PathBuf};

use rotor_luau::render::render_ast;
use rotor_rojo::{PathTranslator, RojoResolver, find_rojo_config_file_path};
use rotor_support::gopath;
use rotor_transformer::{Diagnostic, MultiState, ProjectType, RojoContext, State, StateOptions, transform_source_file};
use rotor_tsrs::ast::SourceFile;
use rotor_tsrs::compiler::Program;
use rotor_tsrs::{Context, P, ProjectOptions, load_project};

/// The repository root.
pub fn repo_root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A loaded project with its Rojo context, ready to transform files.
pub struct CompiledProject {
	pub program: &'static Program,
	pub rojo: RojoContext,
	pub project_type: ProjectType,
	pub multi: MultiState,
}

/// What transforming one file produced.
pub struct FileOutput {
	pub text: Option<String>,
	pub diagnostics: Vec<Diagnostic>,
	pub internal_error: Option<String>,
}

/// Loads `tsconfig` as `compile.CompileProject` does, with one checker so files share one
/// `MultiState` the way one Go checker group does, and runs the pre-emit diagnostics gate.
///
/// # Panics
///
/// When the project does not load or has TypeScript diagnostics (Go fails the compile).
#[expect(clippy::expect_used, reason = "a test harness fails loudly")]
pub fn load(tsconfig: &Path) -> CompiledProject {
	let options = ProjectOptions {
		checkers: Some(1),
		single_threaded: Some(true),
		skip_validation: false,
	};
	let project = load_project(tsconfig, &options).expect("the project loads");
	let program = project.program;
	let directory = project.directory().to_owned();

	// Gates 1-3 of compileProject: syntactic and semantic diagnostics of every file (which also
	// runs the full check the transform relies on), then the global diagnostics.
	let context = Context::default();
	let mut count = program.get_program_diagnostics().len();
	for file in project.source_files() {
		count += program.get_syntactic_diagnostics(&context, Some(file)).len();
		count += program.get_semantic_diagnostics(&context, Some(file)).len();
	}
	count += program.get_global_diagnostics(&context).len();
	assert_eq!(count, 0, "the project has TypeScript diagnostics");

	// newProjectContext: the Rojo resolver, path translator and runtime library path.
	let (rojo_config, _) = find_rojo_config_file_path(&gopath::from_slash(&directory));
	let resolver = RojoResolver::from_path(&rojo_config.expect("the project has a Rojo config"));
	let compiler_options = program.options();
	let root_dir = gopath::join(&[&directory, &compiler_options.root_dir]);
	let root_dir = if Path::new(&compiler_options.root_dir).is_absolute() {
		compiler_options.root_dir.clone()
	} else {
		root_dir
	};
	let path_translator = PathTranslator::new(
		&gopath::from_slash(&root_dir),
		&gopath::from_slash(&compiler_options.out_dir),
		"",
		compiler_options.declaration.is_true(),
		true,
	);
	let include_path = gopath::join(&[&gopath::from_slash(&directory), "include"]);
	let runtime_lib_rbx_path = resolver.get_rbx_path_from_file_path(&gopath::join(&[&include_path, "RuntimeLib.lua"]));
	let project_type = if resolver.is_game {
		ProjectType::Game
	} else {
		ProjectType::Model
	};
	let rojo = RojoContext {
		resolver,
		path_translator,
		runtime_lib_rbx_path,
		project_path: gopath::from_slash(&directory).into_owned(),
		pkg_rojo_resolvers: Vec::new(),
		node_modules_path_mapping: rotor_transformer::NodeModulesPathMapping::default(),
		project_reference_paths: std::collections::HashMap::new(),
		node_modules_path: gopath::join(&[&gopath::from_slash(&directory), "node_modules"]),
		type_roots: compiler_options.type_roots.clone().unwrap_or_default(),
		use_case_sensitive_file_names: program.use_case_sensitive_file_names(),
	};

	CompiledProject {
		program,
		rojo,
		project_type,
		multi: MultiState::new(),
	}
}

impl CompiledProject {
	/// The project's source files rotor compiles, in program order.
	pub fn source_files(&self) -> Vec<P<SourceFile>> {
		rotor_tsrs::project::project_source_files(self.program)
	}

	/// `compileProjectSourceFile`: transforms and renders one file. Text is `None` when the
	/// transform reported errors (upstream bails before rendering).
	///
	/// # Panics
	///
	/// When macro registration fails or the output does not render (Go fails the compile).
	#[expect(clippy::expect_used, reason = "a test harness fails loudly")]
	pub fn compile_file(&mut self, file: P<SourceFile>) -> FileOutput {
		let context = Context::default();
		let mut checker = self.program.get_type_checker_for_file(&context, file);
		let options = StateOptions {
			rojo: Some(&self.rojo),
			project_type: Some(self.project_type),
			..StateOptions::default()
		};
		let mut state = State::new(self.program, &mut checker, file, &mut self.multi, options);
		let missing = state.multi.macro_registration_failures();
		assert!(missing.is_empty(), "macro registration failure: {missing:?}");

		let statements = transform_source_file(&mut state);
		let internal_error = state.internal_error().map(str::to_owned);
		let has_errors = state.diags.has_errors();
		// Go's `compile.go` recover drops the file's text and diagnostics on an internal error.
		let mut diagnostics = state.diags.flush();
		if internal_error.is_some() {
			diagnostics.clear();
		}
		let text = if has_errors || internal_error.is_some() {
			None
		} else {
			Some(render_ast(&state.ast, statements).expect("the output renders"))
		};
		drop(state);
		drop(checker);
		FileOutput {
			text,
			diagnostics,
			internal_error,
		}
	}
}
