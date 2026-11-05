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
mod guard;
mod guard_boolean;
mod guard_conditional;
mod guard_parity_gaps;
mod hashids;
mod id;
mod metadata;
mod oracle_corpus;
mod path;
mod plan_provenance;
mod project;
mod project_artifacts;
mod runtime_config;
mod state;
mod task7_diagnostics_class;
mod task7_diagnostics_state;
mod task7_diagnostics_transform;
mod task7_differential;
mod transform_addpaths;
mod transform_addpaths_index;
mod transform_analysis;
mod transform_binary;
mod transform_build_lookup;
mod transform_call;
mod transform_class;
mod transform_class_accessors;
mod transform_class_comments;
mod transform_class_constraints;
mod transform_class_diagnostics;
mod transform_component;
mod transform_decorators;
mod transform_decorators_component;
mod transform_decorators_uid;
mod transform_driver;
mod transform_emit_context;
mod transform_expressions_call_new;
mod transform_expressions_visitor;
mod transform_imports;
mod transform_integration;
mod transform_intrinsics;
mod transform_macro_adversarial;
mod transform_macro_types;
mod transform_macros;
mod transform_network;
mod transform_oracle;
mod transform_parallel;
mod transform_parameters;
mod transform_performance;
mod transform_reflection;
mod transform_reflection_multifile;
mod transform_symbols;
mod transform_update;

/// A fresh temporary directory, Go's `t.TempDir()`, and its path as a string.
fn temp_dir() -> (tempfile::TempDir, String) {
	let dir = tempfile::tempdir().expect("temp dir");
	let path = dir.path().to_string_lossy().into_owned();
	(dir, path)
}

/// `os.Symlink`: on Windows the link's kind comes from what it points at.
fn symlink(original: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
	#[cfg(unix)]
	{
		std::os::unix::fs::symlink(original, link)
	}
	#[cfg(windows)]
	{
		let target = link
			.parent()
			.map_or_else(|| original.to_path_buf(), |parent| parent.join(original));
		if target.is_dir() {
			std::os::windows::fs::symlink_dir(original, link)
		} else {
			std::os::windows::fs::symlink_file(original, link)
		}
	}
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
	/// Prints without the emit context, as Go's `printer.NewPrinter(..., nil)`.
	plain_print: bool,
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
							(!spec.plain_print).then(|| source.emit_context()),
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
	with_class_analysis_input_in(files, "", file_name, run)
}

/// [`with_class_analysis_input`] for a project in `project` under the fixture root.
fn with_class_analysis_input_in<R: Send>(
	files: &[(&str, &str)],
	project: &str,
	file_name: &str,
	run: impl FnOnce(&mut crate::transform_state::TransformInput<'_>) -> R + Send,
) -> R {
	rotor_tsrs::with_large_stack(|| {
		let (_dir, root) = temp_dir();
		for (name, contents) in files {
			write_transform_fixture(&root, name, contents);
		}
		let directory = if project.is_empty() {
			root
		} else {
			join(&[&root, project])
		};
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

/// A program fixture run with a transform state over `target`: Go's `newExpressionTransformFixture`,
/// `newDecoratorTransformFixture`, `newMacroTestState` and `newGuardTestState`.
struct StateSpec<'s> {
	/// Every fixture file, `package.json` and `tsconfig.json` among them.
	files: Vec<(&'s str, String)>,
	target: &'s str,
	root_dir: &'s str,
	config: rotor_config::config::FlameworkConfig,
	/// Analyzed into the state's plans when set.
	analyses: Option<Vec<crate::plan::FileAnalysis>>,
}

impl<'s> StateSpec<'s> {
	fn new(files: Vec<(&'s str, String)>, target: &'s str) -> Self {
		Self {
			files,
			target,
			root_dir: "src",
			config: rotor_config::config::FlameworkConfig::default(),
			analyses: None,
		}
	}
}

/// Runs `run` with the spec's transform state and target file.
fn with_state<R: Send>(
	spec: &StateSpec<'_>,
	run: impl FnOnce(&mut crate::transform_state::TransformState<'_>, rotor_tsrs::P<rotor_tsrs::ast::SourceFile>) -> R
	+ Send,
) -> R {
	rotor_tsrs::with_large_stack(|| {
		let (_dir, directory) = temp_dir();
		for (name, contents) in &spec.files {
			write_transform_fixture(&directory, name, contents);
		}
		let program = new_transform_program(&directory);
		let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
		let file = program_file(program, &directory, spec.target);
		let mut project = crate::project::open_project(&crate::project::ProjectOptions {
			project_dir: directory.clone(),
			root_dir: spec.root_dir.to_owned(),
			out_dir: "out".to_owned(),
			config: spec.config.clone(),
			..crate::project::ProjectOptions::default()
		})
		.expect("open project");
		let plans = spec
			.analyses
			.as_ref()
			.map_or_else(Vec::new, |analyses| project.analyze(analyses).expect("analyze"));
		let mut state = crate::transform_state::new_transform_state(program, &mut checker, &mut project, None, &plans);
		run(&mut state, file)
	})
}

/// `newExpressionTransformFixture`'s files: `src/expression.ts` beside a `BaseComponent`.
fn expression_fixture_files(source: &str) -> Vec<(&'static str, String)> {
	vec![
		("package.json", r#"{"name":"fixture-game","version":"1.0.0"}"#.to_owned()),
		(
			"tsconfig.json",
			r#"{"compilerOptions":{"strict":true,"rootDir":"src","outDir":"out"},"include":["src/**/*.ts"]}"#.to_owned(),
		),
		(
			"src/base.ts",
			"\nexport class BaseComponent<A> {\n\t/** @metadata intrinsic-component-attributes */\n\treadonly attributes!: A;\n}\n"
				.to_owned(),
		),
		("src/expression.ts", source.to_owned()),
	]
}

/// `newExpressionTransformFixture`
fn with_expression_fixture<R: Send>(
	source: &str,
	run: impl FnOnce(&mut crate::transform_state::TransformState<'_>, rotor_tsrs::P<rotor_tsrs::ast::SourceFile>) -> R
	+ Send,
) -> R {
	with_state(
		&StateSpec::new(expression_fixture_files(source), "src/expression.ts"),
		run,
	)
}

/// `newDecoratorTransformFixture`: the class plan analyzed for `src/service.ts`; the file plan is
/// `state.plans[0]`.
fn with_decorator_fixture<R: Send>(
	source: &str,
	class_plan: crate::plan::ClassPlan,
	run: impl FnOnce(&mut crate::transform_state::TransformState<'_>, rotor_tsrs::P<rotor_tsrs::ast::SourceFile>) -> R
	+ Send,
) -> R {
	let mut spec = StateSpec::new(
		vec![
			("package.json", r#"{"name":"fixture","version":"1.0.0"}"#.to_owned()),
			(
				"tsconfig.json",
				r#"{"compilerOptions":{"experimentalDecorators":true,"rootDir":"src","outDir":"out"},"include":["src/**/*.ts"]}"#
					.to_owned(),
			),
			("src/service.ts", source.to_owned()),
		],
		"src/service.ts",
	);
	spec.config.hash_prefix = "fixture".to_owned();
	spec.analyses = Some(vec![crate::plan::FileAnalysis {
		file_id: "src/service.ts".to_owned(),
		classes: vec![class_plan],
		..crate::plan::FileAnalysis::default()
	}]);
	with_state(&spec, run)
}

/// `newMacroTestState`: `src/main.ts` of a strict fixture under `config`.
fn with_macro_state<R: Send>(
	config: rotor_config::config::FlameworkConfig,
	source: &str,
	run: impl FnOnce(&mut crate::transform_state::TransformState<'_>, rotor_tsrs::P<rotor_tsrs::ast::SourceFile>) -> R
	+ Send,
) -> R {
	let mut spec = StateSpec::new(
		vec![
			(
				"package.json",
				r#"{"name":"fixture-game","version":"1.0.0"}"#.to_owned(),
			),
			(
				"tsconfig.json",
				r#"{"compilerOptions":{"strict":true},"include":["src/**/*.ts"]}"#.to_owned(),
			),
			("src/main.ts", source.to_owned()),
		],
		"src/main.ts",
	);
	spec.config = config;
	with_state(&spec, run)
}

/// `newGuardTestState`: `guards.ts`. Go's state has an empty `Project`; a Rust state needs an
/// opened one, so the fixture also has a `package.json`, which the program never reads.
fn with_guard_state<R: Send>(
	source: &str,
	run: impl FnOnce(&mut crate::transform_state::TransformState<'_>, rotor_tsrs::P<rotor_tsrs::ast::SourceFile>) -> R
	+ Send,
) -> R {
	let mut spec = StateSpec::new(
		vec![
			(
				"package.json",
				r#"{"name":"fixture-game","version":"1.0.0"}"#.to_owned(),
			),
			(
				"tsconfig.json",
				r#"{"compilerOptions":{"strict":true,"target":"esnext"},"files":["guards.ts"]}"#.to_owned(),
			),
			("guards.ts", source.to_owned()),
		],
		"guards.ts",
	);
	spec.root_dir = ".";
	with_state(&spec, run)
}

/// `guardTestTypeNode`: the type of the type alias `name`.
fn guard_test_type_node(
	source_file: rotor_tsrs::P<rotor_tsrs::ast::SourceFile>,
	name: &str,
) -> rotor_tsrs::P<rotor_tsrs::ast::Node> {
	source_file
		.statements()
		.nodes()
		.iter()
		.find(|statement| {
			rotor_tsrs::ast::is_type_alias_declaration(**statement)
				&& statement.name().is_some_and(|alias| alias.text() == name)
		})
		.and_then(|statement| statement.type_node())
		.unwrap_or_else(|| panic!("type alias {name:?} was not found"))
}

/// Go's `node.ForEachChild` walk for the first node `matches` accepts, depth first.
fn find_node(
	root: rotor_tsrs::P<rotor_tsrs::ast::Node>,
	matches: &dyn Fn(rotor_tsrs::P<rotor_tsrs::ast::Node>) -> bool,
) -> Option<rotor_tsrs::P<rotor_tsrs::ast::Node>> {
	if matches(root) {
		return Some(root);
	}
	root.iter_children()
		.into_iter()
		.find_map(|child| find_node(child, matches))
}

/// `findExpressionKind`
fn find_expression_kind(
	root: rotor_tsrs::P<rotor_tsrs::ast::Node>,
	kind: rotor_tsrs::ast::Kind,
) -> rotor_tsrs::P<rotor_tsrs::ast::Node> {
	root.iter_children()
		.into_iter()
		.find_map(|child| find_node(child, &|node| node.kind() == kind))
		.unwrap_or_else(|| panic!("expression kind {kind:?} not found"))
}

/// `findNamedClass`
fn find_named_class(root: rotor_tsrs::P<rotor_tsrs::ast::Node>, name: &str) -> rotor_tsrs::P<rotor_tsrs::ast::Node> {
	find_node(root, &|node| {
		rotor_tsrs::ast::is_class_declaration(node) && node.name().is_some_and(|class_name| class_name.text() == name)
	})
	.unwrap_or_else(|| panic!("class {name:?} not found"))
}

/// `replaceClassSyntaxList`
fn replace_class_syntax_list(
	file: rotor_tsrs::P<rotor_tsrs::ast::SourceFile>,
	class_node: rotor_tsrs::P<rotor_tsrs::ast::Node>,
	transformed: rotor_tsrs::P<rotor_tsrs::ast::Node>,
) -> Vec<rotor_tsrs::P<rotor_tsrs::ast::Node>> {
	let mut statements = Vec::new();
	for &statement in file.statements().nodes() {
		if statement == class_node {
			statements.extend_from_slice(transformed.as_syntax_list().children);
		} else {
			statements.push(statement);
		}
	}
	statements
}

/// The file with `statements`, printed without an emit context.
fn print_with_statements(
	factory: &rotor_tsrs::ast::NodeFactory,
	file: rotor_tsrs::P<rotor_tsrs::ast::SourceFile>,
	statements: Vec<rotor_tsrs::P<rotor_tsrs::ast::Node>>,
) -> String {
	let updated = factory.update_source_file(
		file.as_node(),
		factory.new_node_list(statements),
		file.end_of_file_token,
	);
	print_source_file(updated.as_source_file_p())
}

/// `printer.NewPrinter(PrinterOptions{}, PrintHandlers{}, nil).Emit(node, file)`
fn emit_node(node: rotor_tsrs::P<rotor_tsrs::ast::Node>, file: rotor_tsrs::P<rotor_tsrs::ast::SourceFile>) -> String {
	rotor_tsrs::printer::new_printer(
		rotor_tsrs::printer::PrinterOptions::default(),
		rotor_tsrs::printer::PrintHandlers::default(),
		None,
	)
	.emit(node, Some(file))
}

/// `parser.ParseSourceFile` of a TypeScript file named `name`.
fn parse_ts(name: &str, text: &str) -> rotor_tsrs::P<rotor_tsrs::ast::SourceFile> {
	rotor_tsrs::parser::parse_source_file(
		rotor_tsrs::ast::SourceFileParseOptions {
			file_name: name.to_owned(),
			path: rotor_tsrs::tspath::Path::from(name),
			..rotor_tsrs::ast::SourceFileParseOptions::default()
		},
		text,
		rotor_tsrs::tscore::ScriptKind::TS,
	)
}

/// `collectCallExpressions`: every call expression, in walk order.
fn collect_call_expressions(
	file: rotor_tsrs::P<rotor_tsrs::ast::SourceFile>,
) -> Vec<rotor_tsrs::P<rotor_tsrs::ast::Node>> {
	fn visit(node: rotor_tsrs::P<rotor_tsrs::ast::Node>, calls: &mut Vec<rotor_tsrs::P<rotor_tsrs::ast::Node>>) {
		if rotor_tsrs::ast::is_call_expression(node) {
			calls.push(node);
		}
		for child in node.iter_children() {
			visit(child, calls);
		}
	}
	let mut calls = Vec::new();
	visit(file.as_node(), &mut calls);
	calls
}

/// Go's `Transform(input)` on a [`with_class_analysis_input`] input, with `analyses`.
fn transform_input(
	input: &mut crate::transform_state::TransformInput<'_>,
	analyses: Vec<crate::plan::FileAnalysis>,
) -> Result<crate::transform_state::TransformResult, crate::Error> {
	crate::transform::transform(crate::transform_state::TransformInput {
		program: input.program,
		checker: input.checker,
		files: input.files.clone(),
		project: input.project,
		analyses,
		macro_runtime: None,
		print: None,
	})
}

/// Each named type alias's guard (`buildFlameworkGuard`) in one guard fixture, printed.
fn guard_outputs(source: &str, names: &[&str]) -> Vec<Result<String, crate::Error>> {
	with_guard_state(source, |state, source_file| {
		names
			.iter()
			.map(|name| {
				let type_node = guard_test_type_node(source_file, name);
				let type_value = state.ck().get_type_from_type_node(type_node);
				crate::guard::build_flamework_guard(state, type_value, type_node)
					.map(|guard| emit_node(guard, source_file))
			})
			.collect()
	})
}
