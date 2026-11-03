//! Ports of `internal/flamework/*_test.go`, one module per Go test file. Expected strings are the
//! Go tests' verbatim.
#![expect(
	clippy::significant_drop_tightening,
	reason = "a test holds its checker for the whole transform, as Go's deferred release does"
)]

mod artifacts;
mod artifacts_root;
mod artifacts_unchanged;
mod build_info;
mod glob;
mod hashids;
mod id;
mod metadata;
mod path;
mod plan_provenance;
mod project;
mod project_artifacts;
mod runtime_config;
mod state;
mod transform_class_comments;
mod transform_class_constraints;
mod transform_class_diagnostics;
mod transform_driver;

/// A fresh temporary directory, Go's `t.TempDir()`, and its path as a string.
fn temp_dir() -> (tempfile::TempDir, String) {
	let dir = tempfile::tempdir().expect("temp dir");
	let path = dir.path().to_string_lossy().into_owned();
	(dir, path)
}

/// Joins path segments the way Go's `filepath.Join` does.
fn join(parts: &[&str]) -> String {
	rotor_support::gopath::join(parts)
}

/// `writeProjectFixture`
fn write_project_fixture(root: &str, package_json: &str) {
	for dir in ["src", "out"] {
		std::fs::create_dir(join(&[root, dir])).expect("mkdir");
	}
	std::fs::write(join(&[root, "package.json"]), package_json).expect("write package.json");
}

/// `writeTransformFixture`
fn write_transform_fixture(directory: &str, name: &str, contents: &str) {
	let path = join(&[directory, &rotor_support::gopath::from_slash(name)]);
	std::fs::create_dir_all(rotor_support::gopath::dir(&path)).expect("create fixture directory");
	std::fs::write(&path, contents).expect("write fixture");
}

/// `newTransformProgram`: the directory's `tsconfig.json`, loaded without rbxtsc's validation.
fn new_transform_program(directory: &str) -> &'static rotor_tsrs::compiler::Program {
	let tsconfig = join(&[directory, "tsconfig.json"]);
	let options = rotor_tsrs::ProjectOptions {
		skip_validation: true,
		..rotor_tsrs::ProjectOptions::default()
	};
	rotor_tsrs::load_project(std::path::Path::new(&tsconfig), &options)
		.expect("load program")
		.program
}

/// `printer.NewPrinter(PrinterOptions{}, PrintHandlers{}, nil).EmitSourceFile(file)`
fn print_source_file(file: rotor_tsrs::P<rotor_tsrs::ast::SourceFile>) -> String {
	rotor_tsrs::printer::new_printer(
		rotor_tsrs::printer::PrinterOptions::default(),
		rotor_tsrs::printer::PrintHandlers::default(),
		None,
	)
	.emit_source_file(file)
}

/// The program's source file at `directory/name`.
fn program_file(
	program: &'static rotor_tsrs::compiler::Program,
	directory: &str,
	name: &str,
) -> rotor_tsrs::P<rotor_tsrs::ast::SourceFile> {
	let path = rotor_support::gopath::to_slash(&join(&[directory, name])).into_owned();
	program.get_source_file(&path).expect("source file")
}

/// `transformFixtureSource`: `src/main.ts` of a strict fixture transformed with a fixed UUID,
/// printed.
fn transform_fixture_source(lines: &[&str]) -> String {
	rotor_tsrs::with_large_stack(|| {
		let (_dir, directory) = temp_dir();
		write_transform_fixture(
			&directory,
			"package.json",
			r#"{"name":"fixture-game","version":"1.0.0"}"#,
		);
		write_transform_fixture(
			&directory,
			"tsconfig.json",
			r#"{"compilerOptions":{"strict":true},"files":["src/main.ts"]}"#,
		);
		write_transform_fixture(&directory, "src/main.ts", &lines.join("\n"));
		let program = new_transform_program(&directory);
		let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
		let source_file = program_file(program, &directory, "src/main.ts");
		let mut project = crate::project::open_project(&crate::project::ProjectOptions {
			project_dir: directory.clone(),
			root_dir: "src".to_owned(),
			..crate::project::ProjectOptions::default()
		})
		.expect("open project");
		let runtime = crate::transform_macro_types::MacroRuntime {
			uuid: Some(std::rc::Rc::new(|| {
				Ok("11111111-1111-4111-8111-111111111111".to_owned())
			})),
			..crate::transform_macro_types::MacroRuntime::default()
		};
		let result = crate::transform::transform(crate::transform_state::TransformInput {
			program,
			checker: &mut checker,
			files: vec![source_file],
			project: &mut project,
			analyses: Vec::new(),
			macro_runtime: Some(runtime),
			print: None,
		})
		.expect("transform");
		print_source_file(result.files[0])
	})
}

/// A transform fixture: files written into a temporary project, transformed, printed.
#[derive(Default)]
struct TransformSpec<'s> {
	package_json: &'s str,
	tsconfig: &'s str,
	files: Vec<(&'s str, String)>,
	targets: Vec<&'s str>,
	config: rotor_config::config::FlameworkConfig,
	out_dir: &'s str,
	analyses: Vec<crate::plan::FileAnalysis>,
	runtime: Option<fn() -> crate::transform_macro_types::MacroRuntime>,
}

/// What a [`TransformSpec`] produced.
struct TransformOutput {
	/// Each target printed with its emit context.
	printed: Vec<String>,
	/// The transform error, if it failed.
	error: Option<crate::Error>,
}

fn run_transform(spec: TransformSpec<'_>) -> TransformOutput {
	rotor_tsrs::with_large_stack(|| {
		let (_dir, directory) = temp_dir();
		let package_json = if spec.package_json.is_empty() {
			r#"{"name":"fixture-game","version":"1.0.0"}"#
		} else {
			spec.package_json
		};
		write_transform_fixture(&directory, "package.json", package_json);
		write_transform_fixture(&directory, "tsconfig.json", spec.tsconfig);
		for (name, contents) in &spec.files {
			write_transform_fixture(&directory, name, contents);
		}
		let program = new_transform_program(&directory);
		let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
		let files = spec
			.targets
			.iter()
			.map(|target| program_file(program, &directory, target))
			.collect();
		let mut project = crate::project::open_project(&crate::project::ProjectOptions {
			project_dir: directory.clone(),
			root_dir: "src".to_owned(),
			out_dir: spec.out_dir.to_owned(),
			config: spec.config.clone(),
			..crate::project::ProjectOptions::default()
		})
		.expect("open project");
		let result = crate::transform::transform(crate::transform_state::TransformInput {
			program,
			checker: &mut checker,
			files,
			project: &mut project,
			analyses: spec.analyses,
			macro_runtime: spec.runtime.map(|build| build()),
			print: None,
		});
		match result {
			Ok(result) => TransformOutput {
				printed: result
					.files
					.iter()
					.zip(&result.sources)
					.map(|(file, source)| {
						rotor_tsrs::printer::new_printer(
							rotor_tsrs::printer::PrinterOptions::default(),
							rotor_tsrs::printer::PrintHandlers::default(),
							Some(source.emit_context()),
						)
						.emit_source_file(*file)
					})
					.collect(),
				error: None,
			},
			Err(error) => TransformOutput {
				printed: Vec::new(),
				error: Some(error),
			},
		}
	})
}

/// `newClassAnalysisInput`: runs `run` with the transform input of `file_name` in a fixture of
/// `files` (`package.json` and `tsconfig.json` among them).
fn with_class_analysis_input<R: Send>(
	files: &[(&str, &str)],
	file_name: &str,
	run: impl FnOnce(&mut crate::transform_state::TransformInput<'_>) -> R + Send,
) -> R {
	rotor_tsrs::with_large_stack(|| {
		let (_dir, directory) = temp_dir();
		for (name, contents) in files {
			write_transform_fixture(&directory, name, contents);
		}
		let program = new_transform_program(&directory);
		let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
		let file = program_file(program, &directory, file_name);
		let mut project = crate::project::open_project(&crate::project::ProjectOptions {
			project_dir: directory.clone(),
			root_dir: "src".to_owned(),
			out_dir: "out".to_owned(),
			..crate::project::ProjectOptions::default()
		})
		.expect("open project");
		run(&mut crate::transform_state::TransformInput {
			program,
			checker: &mut checker,
			files: vec![file],
			project: &mut project,
			analyses: Vec::new(),
			macro_runtime: None,
			print: None,
		})
	})
}
