//! `census_test.go`: census mode classifies every file and writes nothing.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;

use rotor_transformer::{MultiState, State, StateOptions};
use rotor_tsrs::Context;

use super::{Temp, large_stack, output, write_project};
use crate::census::{CensusCollector, FileDiagnostics, FileOutcome, ProjectDiagnostics, compile_project_diagnostics};
use crate::compile::{DiagnosticInfo, Error, transform_and_render_source_map_detailed, type_script_diagnostic_code};
use crate::flamework_pipeline::prepare_compile_pipeline;
use crate::overlayfs::match_program_overlays;
use crate::package_layout::hex_sha256;
use crate::project::{
	Outputs, ProjectOptions, compile_project_source_files, compile_project_with_options, new_project_context,
	new_project_program_with_options, project_source_files,
};

/// `writeCensusProject`: a package-type project (no Rojo file needed) whose src/ holds one file
/// per entry of `files`, plus the noLib global stubs.
pub fn write_census_project(files: &[(&str, &str)]) -> Temp {
	let temp = write_project("@scope/census-fixture", "");
	for (name, text) in files {
		temp.write(&format!("src/{name}"), text);
	}
	temp
}

/// `censusFiles`: one clean file, one only TypeScript objects to, one only the transformer
/// objects to, and one that makes the transformer panic (an identifier with no symbol).
pub const CLEAN: &str = "export const clean = 1;\n";
pub const TYPEBAD: &str = "export const s: string = 5;\n";
pub const NOANY: &str = "declare const loose: any;\nexport const taken = loose.field;\n";
pub const PANICKING: &str = "export const x = neverDeclared;\n";

const CENSUS_FILES: [(&str, &str); 4] = [
	("clean.ts", CLEAN),
	("typebad.ts", TYPEBAD),
	("noany.ts", NOANY),
	("panicking.ts", PANICKING),
];

/// `censusByFile`
pub fn census_by_file(census: &ProjectDiagnostics) -> HashMap<String, FileDiagnostics> {
	census
		.files
		.iter()
		.map(|file| (rotor_support::gopath::base(&file.file_name), file.clone()))
		.collect()
}

/// `censusFileNames`
pub fn census_file_names(by_name: &HashMap<String, FileDiagnostics>) -> Vec<String> {
	let mut names: Vec<String> = by_name.keys().cloned().collect();
	names.sort();
	names
}

fn outcome(by_name: &HashMap<String, FileDiagnostics>, name: &str) -> Option<FileOutcome> {
	by_name.get(name).map(|file| file.outcome)
}

fn census(dir: &str, opts: &ProjectOptions) -> ProjectDiagnostics {
	let (census, error) = large_stack(|| compile_project_diagnostics(dir, opts));
	if let Some(error) = error {
		panic!("CompileProjectDiagnostics: {error}");
	}
	census
}

fn census_error(dir: &str, opts: &ProjectOptions) -> Option<String> {
	large_stack(|| compile_project_diagnostics(dir, opts))
		.1
		.map(|error| error.to_string())
}

fn overlays(entries: &[(String, &str)]) -> BTreeMap<String, String> {
	entries
		.iter()
		.map(|(key, text)| (key.clone(), (*text).to_owned()))
		.collect()
}

fn compile(dir: &str, opts: &ProjectOptions) -> Result<Outputs, crate::compile::Failure> {
	large_stack(|| compile_project_with_options(dir, opts))
}

fn output_keys(outputs: &Outputs) -> Vec<&String> {
	outputs.keys().collect()
}

#[test]
fn compile_project_overlays_override_file_text() {
	// Given a project whose on-disk main.ts declares nothing
	let temp = write_census_project(&[("main.ts", "export const fromDisk = 1;\n")]);
	let dir = temp.dir();
	let main_path = temp.join("src/main.ts").to_string_lossy().into_owned();
	// When the file text is overridden in memory
	let outputs = compile(
		&dir,
		&ProjectOptions {
			overlays: overlays(&[(main_path, "export const fromOverlay = 2;\n")]),
			..ProjectOptions::default()
		},
	)
	.unwrap_or_else(|failure| {
		panic!(
			"CompileProjectWithOptions: {} (diags: {:?})",
			failure.error,
			failure.messages()
		)
	});
	// Then the compiled Luau reflects the overlay, not the disk
	assert!(
		outputs.contains_key("out/main.luau"),
		"out/main.luau missing; outputs: {:?}",
		output_keys(&outputs)
	);
	let text = output(&outputs, "out/main.luau");
	assert!(
		text.contains("fromOverlay"),
		"compiled output did not use the overlay text:\n{text}"
	);
	assert!(
		!text.contains("fromDisk"),
		"compiled output still used the on-disk text:\n{text}"
	);
}

#[test]
fn compile_project_overlays_leave_disk_untouched() {
	// Given a project with a known on-disk file
	const ON_DISK: &str = "export const fromDisk = 1;\n";
	let temp = write_census_project(&[("main.ts", ON_DISK)]);
	let main_path = temp.join("src/main.ts").to_string_lossy().into_owned();
	// When it is compiled through an overlay
	if let Err(failure) = compile(
		&temp.dir(),
		&ProjectOptions {
			overlays: overlays(&[(main_path.clone(), "export const fromOverlay = 2;\n")]),
			..ProjectOptions::default()
		},
	) {
		panic!(
			"CompileProjectWithOptions: {} (diags: {:?})",
			failure.error,
			failure.messages()
		);
	}
	// Then the file on disk is unchanged
	assert_eq!(
		std::fs::read_to_string(&main_path).expect("read"),
		ON_DISK,
		"overlay wrote through to disk"
	);
}

/// Catches an editor overlay spelled through a symlinked project parent being ignored because
/// the compiler records the source through its physical path.
#[test]
fn compile_project_overlays_follow_symlinked_parent() {
	let temp = write_census_project(&[("main.ts", "export const fromDisk = 1;\n")]);
	let dir = temp.dir();
	let alias_temp = Temp::new();
	let alias = alias_temp.join("project").to_string_lossy().into_owned();
	#[cfg(unix)]
	std::os::unix::fs::symlink(rotor_support::gopath::dir(&dir), &alias).expect("symlink");
	#[cfg(not(unix))]
	return;
	let overlay_path = rotor_support::gopath::join(&[&alias, &rotor_support::gopath::base(&dir), "src", "main.ts"]);
	let outputs = compile(
		&dir,
		&ProjectOptions {
			overlays: overlays(&[(overlay_path, "export const fromOverlay = 2;\n")]),
			..ProjectOptions::default()
		},
	)
	.unwrap_or_else(|failure| {
		panic!(
			"CompileProjectWithOptions: {} (diags: {:?})",
			failure.error,
			failure.messages()
		)
	});
	let text = output(&outputs, "out/main.luau");
	assert!(
		text.contains("fromOverlay") && !text.contains("fromDisk"),
		"symlinked-parent overlay did not replace the emitted source:\n{text}"
	);
}

/// `transformStateForFile` then `transformAndRenderDetailed`: transforms one file without the
/// pre-emit gates, the way census mode pushes a type-broken file into the transformer.
#[expect(
	clippy::significant_drop_tightening,
	reason = "the state borrows the checker until the file is rendered"
)]
fn transform_file_detailed(dir: &str, rel_path: &str) -> Result<(), Error> {
	let opts = ProjectOptions::default();
	let (abs_dir, program) = new_project_program_with_options(dir, "", &opts)
		.unwrap_or_else(|failure| panic!("newProjectProgram: {}", failure.error));
	let file_path = format!("{abs_dir}/{rel_path}");
	let source_file = program
		.get_source_file(&file_path)
		.unwrap_or_else(|| panic!("source file not in program: {file_path}"));
	let pipeline = prepare_compile_pipeline(&abs_dir, program, &[source_file], &opts.overlays, &opts)
		.unwrap_or_else(|failure| panic!("prepareProjectProgramForCompile: {}", failure.error));
	let program = pipeline.prepared.program;
	let source_file = pipeline.prepared.source_files[0];
	let mut pctx = new_project_context(&abs_dir, program, &opts)
		.unwrap_or_else(|failure| panic!("newProjectContext: {}", failure.error));
	pctx.source_traces = pipeline.prepared.source_traces;
	let context = Context::default();
	let mut checker = program.get_type_checker_for_file(&context, source_file);
	let mut multi = MultiState::new();
	let options = StateOptions {
		rojo: Some(&pctx.rojo_context),
		project_type: Some(pctx.project_type),
		env: Some(&pctx.env),
		files: Some(&*pctx.files),
		..StateOptions::default()
	};
	let mut state = State::new(program, &mut checker, source_file, &mut multi, options);
	transform_and_render_source_map_detailed(&mut state, None)
		.map(|_| ())
		.map_err(|failure| failure.error)
}

#[test]
fn transform_panic_yields_typed_internal_compiler_error() {
	// Given a file whose identifier resolves to no symbol
	let temp = write_census_project(&[("main.ts", PANICKING)]);
	// When it is transformed
	let error = large_stack(|| transform_file_detailed(&temp.dir(), "src/main.ts"));
	// Then the error is typed and carries the file. The Rust transformer reports the assert as
	// an internal error instead of panicking, so there is no panic stack to name a frame of.
	let Err(error) = error else {
		panic!("transform of a symbol-less identifier did not fail");
	};
	let Error::InternalCompiler(ice) = &error else {
		panic!("error {error:?} is not an InternalCompilerError");
	};
	assert!(
		rotor_support::gopath::to_slash(&ice.file_name).ends_with("src/main.ts"),
		"FileName = {:?}, want it to name src/main.ts",
		ice.file_name
	);
	assert!(!ice.value.is_empty(), "Value is empty, want the recovered panic value");
	// The rendered message must stay byte-identical to the untyped error it replaces.
	assert!(
		error
			.to_string()
			.contains(r#"transformer: identifier "neverDeclared" has no symbol"#),
		"Error() = {error:?}, want it to name neverDeclared"
	);
}

#[test]
fn compile_project_stops_before_transform_on_type_errors() {
	// Given a project with one type-broken file and one file the transformer rejects
	let temp = write_census_project(&[("typebad.ts", TYPEBAD), ("noany.ts", NOANY)]);
	// When it is compiled without census mode
	let Err(failure) = compile(&temp.dir(), &ProjectOptions::default()) else {
		panic!("compile of a type-broken project succeeded");
	};
	// Then the type error alone aborts it: the transformer's own diagnostic is never observed
	for diagnostic in failure.messages() {
		assert!(
			!diagnostic.contains("not supported"),
			"transformer diagnostic leaked out of a non-census compile: {diagnostic:?}"
		);
	}
}

#[test]
fn compile_project_reports_type_errors_from_every_file() {
	// Given a project with type errors in two files
	let temp = write_census_project(&[
		("a.ts", "export const probeA: number = \"a\";\n"),
		("b.ts", "export const probeB: number = \"b\";\n"),
	]);
	// When it is compiled without census mode
	let Err(failure) = compile(&temp.dir(), &ProjectOptions::default()) else {
		panic!("compile of a type-broken project succeeded");
	};
	// Then both files' errors are reported, not only the first file's
	let diagnostics = failure.messages();
	assert_eq!(
		diagnostics.len(),
		2,
		"got diagnostics, want one per broken file: {diagnostics:?}"
	);
}

#[test]
fn compile_project_diagnostics_transforms_past_type_errors() {
	// Given the same project
	let temp = write_census_project(&[("typebad.ts", TYPEBAD), ("noany.ts", NOANY)]);
	// When it is compiled through the census entry point
	let census = census(&temp.dir(), &ProjectOptions::default());
	// Then the type-broken file is classified and still reaches the transform, and the
	// transformer's diagnostic on the other file is reported too
	let by_name = census_by_file(&census);
	let typebad = by_name.get("typebad.ts").unwrap_or_else(|| {
		panic!(
			"typebad.ts missing from the census; got {:?}",
			census_file_names(&by_name)
		)
	});
	assert_eq!(typebad.outcome, FileOutcome::TypeError, "typebad.ts outcome");
	assert!(
		typebad.transformed,
		"typebad.ts did not reach the transform stage in census mode"
	);
	let noany = by_name.get("noany.ts").unwrap_or_else(|| {
		panic!(
			"noany.ts missing from the census; got {:?}",
			census_file_names(&by_name)
		)
	});
	assert_eq!(
		noany.outcome,
		FileOutcome::TransformerDiagnostic,
		"noany.ts outcome (diags {:?})",
		noany.diagnostics
	);
}

#[test]
fn compile_project_diagnostics_codes_name_the_diagnostic_class() {
	// Given one file TypeScript rejects and one the transformer rejects
	let temp = write_census_project(&[("typebad.ts", TYPEBAD), ("noany.ts", NOANY)]);
	// When they are censused
	let census = census(&temp.dir(), &ProjectOptions::default());
	// Then every diagnostic carries a code naming its class.
	let by_name = census_by_file(&census);
	for (name, want_code) in [("typebad.ts", "TS2322"), ("noany.ts", "noAny")] {
		let file = by_name.get(name);
		let Some(file) = file.filter(|file| !file.diagnostics.is_empty()) else {
			panic!("{name} carries no diagnostics: {file:?}");
		};
		assert_eq!(
			file.diagnostics[0].code, want_code,
			"{name} code (message {:?})",
			file.diagnostics[0].message
		);
	}
}

#[test]
fn type_script_diagnostic_code_renders_ts_form() {
	// The diagnostic numbers a checker reports read as the upstream "TS####" form.
	for (code, want) in [(2322, "TS2322"), (0, "TS0")] {
		assert_eq!(
			type_script_diagnostic_code(code),
			want,
			"TypeScriptDiagnosticCode({code})"
		);
	}
}

#[test]
fn compile_project_diagnostics_reports_every_failing_file() {
	// Given a project where files fail in every way the census classifies
	let temp = write_census_project(&CENSUS_FILES);
	// When the census runs
	let census = census(&temp.dir(), &ProjectOptions::default());
	// Then every file has an entry, with the outcome it earned
	let by_name = census_by_file(&census);
	let want = [
		("clean.ts", FileOutcome::Ok),
		("main.ts", FileOutcome::Ok),
		("typebad.ts", FileOutcome::TypeError),
		("noany.ts", FileOutcome::TransformerDiagnostic),
		("panicking.ts", FileOutcome::InternalCompilerError),
	];
	for (name, want_outcome) in want {
		let file = by_name
			.get(name)
			.unwrap_or_else(|| panic!("{name} missing from the census; got {:?}", census_file_names(&by_name)));
		assert_eq!(
			file.outcome, want_outcome,
			"{name} outcome (diags {:?})",
			file.diagnostics
		);
	}
	assert_eq!(
		census.files.len(),
		want.len(),
		"census covered files: {:?}",
		census_file_names(&by_name)
	);
}

#[test]
fn compile_project_diagnostics_panicking_file_does_not_stop_the_rest() {
	// Given a project whose first file by name makes the transformer panic
	let temp = write_census_project(&[
		("a_panicking.ts", PANICKING),
		("b_clean.ts", "export const afterThePanic = 1;\n"),
		("c_clean.ts", "export const alsoAfter = 2;\n"),
	]);
	// When the census runs
	let census = census(&temp.dir(), &ProjectOptions::default());
	// Then the files after it were still transformed
	let by_name = census_by_file(&census);
	let panicking = &by_name["a_panicking.ts"];
	assert_eq!(
		panicking.outcome,
		FileOutcome::InternalCompilerError,
		"a_panicking.ts outcome"
	);
	// The Rust transformer reports its asserts without a panic stack (see
	// `transform_panic_yields_typed_internal_compiler_error`).
	assert!(
		panicking.internal_error.is_some(),
		"a_panicking.ts carries no InternalError"
	);
	for name in ["b_clean.ts", "c_clean.ts"] {
		let file = by_name
			.get(name)
			.unwrap_or_else(|| panic!("{name} missing from the census; got {:?}", census_file_names(&by_name)));
		assert!(
			file.outcome == FileOutcome::Ok && file.transformed,
			"{name} outcome = {:?} transformed = {}, want ok/true",
			file.outcome,
			file.transformed
		);
	}
}

#[test]
fn compile_project_diagnostics_counts_transformed_files() {
	// Given a project with one file the transformer cannot get through
	let temp = write_census_project(&[("clean.ts", CLEAN), ("panicking.ts", PANICKING)]);
	// When the census runs
	let census = census(&temp.dir(), &ProjectOptions::default());
	// Then the transformed count excludes it
	assert_eq!(
		census.transformed,
		census.files.len() - 1,
		"Transformed over {} files",
		census.files.len()
	);
}

#[test]
fn compile_project_diagnostics_overlay_introduces_the_failure() {
	// Given a project that is clean on disk
	let temp = write_census_project(&[("clean.ts", CLEAN)]);
	let clean_path = temp.join("src/clean.ts").to_string_lossy().into_owned();
	// When the census runs over an overlay that breaks one file
	let census = census(
		&temp.dir(),
		&ProjectOptions {
			overlays: overlays(&[(clean_path, NOANY)]),
			..ProjectOptions::default()
		},
	);
	// Then the census reports the overlay's failure, not the disk's success
	let by_name = census_by_file(&census);
	assert_eq!(
		outcome(&by_name, "clean.ts"),
		Some(FileOutcome::TransformerDiagnostic),
		"clean.ts outcome (diags {:?})",
		by_name.get("clean.ts").map(|file| &file.diagnostics)
	);
}

#[test]
fn compile_project_overlay_naming_no_file_is_an_error() {
	// Given a project and an overlay for a file that is not in it
	let temp = write_census_project(&[("clean.ts", CLEAN)]);
	let missing = temp.join("src/typo.ts").to_string_lossy().into_owned();
	// When a census is asked for
	let error = census_error(
		&temp.dir(),
		&ProjectOptions {
			overlays: overlays(&[(missing, NOANY)]),
			..ProjectOptions::default()
		},
	);
	// Then it refuses rather than censusing the unmodified tree and calling it green
	let error = error.expect("an overlay matching no file in the program was accepted");
	assert!(
		error.contains("typo.ts"),
		"error = {error}, want it to name the unmatched overlay"
	);
}

#[test]
fn compile_project_ignore_unmatched_overlays_drops_only_the_unmatched() {
	// Given an editor-shaped overlay set: one buffer the program holds and one path it does not
	let temp = write_census_project(&[("clean.ts", CLEAN)]);
	let editor_overlays = overlays(&[
		(temp.join("src/clean.ts").to_string_lossy().into_owned(), NOANY),
		(temp.join("elsewhere/new.ts").to_string_lossy().into_owned(), NOANY),
	]);
	// flag set: it succeeds, the matching overlay still applied, and only that one is counted
	let census = census(
		&temp.dir(),
		&ProjectOptions {
			overlays: editor_overlays.clone(),
			ignore_unmatched_overlays: true,
			..ProjectOptions::default()
		},
	);
	assert_eq!(census.overlay_matches, 1, "OverlayMatches");
	assert_eq!(
		outcome(&census_by_file(&census), "clean.ts"),
		Some(FileOutcome::TransformerDiagnostic),
		"clean.ts outcome — the matching overlay did not apply"
	);
	// flag unset: the strict rule still refuses and names the stray path
	let error = census_error(
		&temp.dir(),
		&ProjectOptions {
			overlays: editor_overlays,
			..ProjectOptions::default()
		},
	)
	.expect("an overlay matching no file in the program was accepted");
	assert!(
		error.contains("new.ts"),
		"error = {error}, want it to name the unmatched overlay"
	);
}

#[test]
fn compile_project_relative_overlay_key_is_an_error() {
	// Given an overlay keyed relatively — the program holds absolute paths
	let temp = write_census_project(&[("clean.ts", CLEAN)]);
	// When a census is asked for, then it refuses
	let error = census_error(
		&temp.dir(),
		&ProjectOptions {
			overlays: overlays(&[("src/clean.ts".to_owned(), NOANY)]),
			..ProjectOptions::default()
		},
	);
	assert!(error.is_some(), "a relative overlay key was accepted");
}

#[test]
fn compile_project_overlay_keys_match_across_separator_and_case() {
	// Given overlay keys written the ways a consumer will actually write them
	let temp = write_census_project(&[("clean.ts", CLEAN)]);
	let slashed = rotor_support::gopath::to_slash(&temp.join("src/clean.ts").to_string_lossy()).into_owned();
	let mut keys = vec![
		("slash-separated", slashed.clone()),
		(
			"backslash-separated",
			rotor_support::gopath::from_slash(&slashed).into_owned(),
		),
	];
	if !rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names() {
		keys.push(("upper-cased", slashed.to_uppercase()));
	}
	for (name, key) in keys {
		// When the census runs over it, then it matched, and is counted
		let census = census(
			&temp.dir(),
			&ProjectOptions {
				overlays: overlays(&[(key, NOANY)]),
				..ProjectOptions::default()
			},
		);
		assert_eq!(census.overlay_matches, 1, "{name}: OverlayMatches");
		assert_eq!(
			outcome(&census_by_file(&census), "clean.ts"),
			Some(FileOutcome::TransformerDiagnostic),
			"{name}: clean.ts outcome — the overlay did not apply"
		);
	}
}

/// `addTransformerPlugin`: rewrites the fixture tsconfig to declare a transformer plugin, which
/// is all projectUsesTransformerPlugins reads.
pub fn add_transformer_plugin(dir: &str) {
	let config_path = rotor_support::gopath::join(&[dir, "tsconfig.json"]);
	let data = std::fs::read_to_string(&config_path).expect("read");
	let patched = data.replacen(
		r#""compilerOptions": {"#,
		"\"compilerOptions\": {\n\t\t\"plugins\": [{ \"transform\": \"rbxts-transformer-fixture\" }],",
		1,
	);
	assert_ne!(
		patched, data,
		"tsconfig fixture shape changed; plugin injection did not apply"
	);
	std::fs::write(&config_path, patched).expect("write");
}

#[test]
fn compile_project_overlays_with_transformer_plugins_are_accepted() {
	// Given a project with a transformer plugin, and an overlay for a file it holds
	let temp = write_census_project(&[("clean.ts", CLEAN)]);
	add_transformer_plugin(&temp.dir());
	let clean_path = temp.join("src/clean.ts").to_string_lossy().into_owned();
	// When the overlays are matched against the program
	large_stack(|| {
		let (_, program) = new_project_program_with_options(&temp.dir(), "", &ProjectOptions::default())
			.unwrap_or_else(|failure| panic!("newProjectProgramWithOptions: {}", failure.error));
		let matched = match_program_overlays(
			program,
			&ProjectOptions {
				overlays: overlays(&[(clean_path, NOANY)]),
				..ProjectOptions::default()
			},
		)
		// Then the plugin does not disqualify them.
		.unwrap_or_else(|error| panic!("overlays on a transformer-plugin project were refused: {error}"));
		assert_eq!(matched, 1, "matched");
	});
}

#[test]
fn compile_project_transformer_plugins_without_overlays_are_untouched() {
	// Given the same project with no overlays
	let temp = write_census_project(&[("clean.ts", CLEAN)]);
	add_transformer_plugin(&temp.dir());
	// When the program is built without overlays (stopping short of the sidecar itself, which
	// needs Node), then the overlay refusal did not fire
	if let Err(failure) =
		large_stack(|| new_project_program_with_options(&temp.dir(), "", &ProjectOptions::default()).map(|_| ()))
	{
		panic!(
			"overlay-free run failed: {} (diags: {:?})",
			failure.error,
			failure.messages()
		);
	}
}

/// `treeSnapshot`: every path under `dir` to a digest of its contents.
pub fn tree_snapshot(dir: &str) -> BTreeMap<String, String> {
	fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
		for entry in std::fs::read_dir(dir).expect("read dir").flatten() {
			let path = entry.path();
			let rel =
				rotor_support::gopath::to_slash(&path.strip_prefix(root).expect("rel").to_string_lossy()).into_owned();
			if entry.file_type().expect("file type").is_dir() {
				out.insert(format!("{rel}/"), "dir".to_owned());
				walk(root, &path, out);
			} else {
				out.insert(rel, hex_sha256(&std::fs::read(&path).expect("read")));
			}
		}
	}
	let mut out = BTreeMap::new();
	walk(Path::new(dir), Path::new(dir), &mut out);
	out
}

#[test]
fn compile_project_diagnostics_writes_nothing_to_disk() {
	// Given a project and a snapshot of its tree
	let temp = write_census_project(&CENSUS_FILES);
	let before = tree_snapshot(&temp.dir());
	// When the census runs
	census(&temp.dir(), &ProjectOptions::default());
	// Then nothing on disk moved — no outDir, no include folder, no rotor.d.ts
	assert_eq!(tree_snapshot(&temp.dir()), before, "tree changed");
}

/// `censusCompileOutputs`: the outputs map the census gates build, which
/// `CompileProjectDiagnostics` discards.
fn census_compile_outputs(dir: &str) -> Outputs {
	let opts = ProjectOptions {
		census: Some(Arc::new(CensusCollector::default())),
		..ProjectOptions::default()
	};
	large_stack(|| {
		let (abs_dir, program) = new_project_program_with_options(dir, "", &opts)
			.unwrap_or_else(|failure| panic!("newProjectProgramWithOptions: {}", failure.error));
		let source_files = project_source_files(program);
		let pipeline = prepare_compile_pipeline(&abs_dir, program, &source_files, &opts.overlays, &opts)
			.unwrap_or_else(|failure| panic!("prepareProjectProgramForCompile: {}", failure.error));
		let program = pipeline.prepared.program;
		let mut pctx = new_project_context(&abs_dir, program, &opts)
			.unwrap_or_else(|failure| panic!("newProjectContext: {}", failure.error));
		pctx.source_traces = pipeline.prepared.source_traces;
		compile_project_source_files(&abs_dir, program, &pctx, &pipeline.prepared.source_files, &opts)
			.unwrap_or_else(|failure| panic!("compileProjectSourceFiles: {}", failure.error))
			.0
	})
}

#[test]
fn census_never_emits_an_output_for_a_file_that_did_not_transform() {
	// Given a file the transformer panics on, alongside a clean one
	let temp = write_census_project(&[("clean.ts", CLEAN), ("panicking.ts", PANICKING)]);
	// When the census gates run
	let outputs = census_compile_outputs(&temp.dir());
	// Then no entry was keyed on the empty output path
	assert!(
		!outputs.contains_key(""),
		r#"outputs[""] = {:?}; a file that never transformed must not reach the map"#,
		output(&outputs, "")
	);
	assert!(
		outputs.contains_key("out/clean.luau"),
		"out/clean.luau missing; outputs: {:?}",
		output_keys(&outputs)
	);
}

#[test]
fn census_never_emits_an_output_for_a_type_broken_file() {
	// Given a file TypeScript rejects but the transformer gets through
	let temp = write_census_project(&[("clean.ts", CLEAN), ("typebad.ts", TYPEBAD)]);
	// When the census gates run
	let outputs = census_compile_outputs(&temp.dir());
	// Then its output is not in the map
	assert!(
		!outputs.contains_key("out/typebad.luau"),
		"out/typebad.luau = {:?}; type-broken output must not reach the map",
		output(&outputs, "out/typebad.luau")
	);
	assert!(
		outputs.contains_key("out/clean.luau"),
		"out/clean.luau missing; outputs: {:?}",
		output_keys(&outputs)
	);
}

#[test]
fn census_collector_accumulates_project_diagnostics() {
	// Given a collector
	let collector = CensusCollector::default();
	// When project-level diagnostics arrive from more than one gate
	let info = |message: &str| DiagnosticInfo {
		message: message.to_owned(),
		..DiagnosticInfo::default()
	};
	collector.add_project_diagnostics(vec![info("gate 1")]);
	collector.add_project_diagnostics(Vec::new());
	collector.add_project_diagnostics(vec![info("gate 3a"), info("gate 3b")]);
	// Then all of them are kept, in order
	let messages: Vec<String> = collector
		.diagnostics()
		.into_iter()
		.map(|diagnostic| diagnostic.message)
		.collect();
	assert_eq!(messages, ["gate 1", "gate 3a", "gate 3b"], "diagnostics");
}
