//! `task7_differential_test.go`: the executable two-compiler differential harness, and the
//! helpers the other `task7_*` tests share.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use regex::Regex;
use rotor_config::config::FlameworkConfig;
use rotor_tsrs::P;
use rotor_tsrs::ast::SourceFile;
use rotor_tsrs::compiler::Program;
use sha2::{Digest, Sha256};

use super::oracle_corpus::{flamework_testdata, hex, repo_root};
use super::{new_transform_program, parse_ts, temp_dir, write_transform_fixture};
use crate::ErrorDetail;
use crate::project::{Project, ProjectOptions, open_project};
use crate::state::FLAMEWORK_VERSION;
use crate::transform_macro_types::MacroRuntime;
use crate::transform_state::TransformInput;

/// `task7ANSI`
pub(super) static TASK7_ANSI: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*m").expect("regex"));
/// `task7OracleDiagnostic`
static TASK7_ORACLE_DIAGNOSTIC: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"TS @flamework/core: ([^\n]+)").expect("regex"));
/// `task7SemanticDiagnostic`
static TASK7_SEMANTIC_DIAGNOSTIC: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"(?m)error TS([0-9]+): (.+)$").expect("regex"));

const NPM_INSTALL: [&str; 6] = [
	"install",
	"--ignore-scripts",
	"--package-lock=false",
	"--no-audit",
	"--no-fund",
	"--prefer-offline",
];

/// The `task7-differential` fixture project.
pub(super) fn task7_fixture() -> PathBuf {
	flamework_testdata("task7-differential/project")
}

/// A command's exit success, its combined stdout and stderr (Go's `CombinedOutput`), and
/// whether it timed out.
pub(super) fn run_combined(mut command: Command, timeout: Duration) -> (bool, String, bool) {
	let (mut reader, writer) = std::io::pipe().expect("pipe");
	command
		.stdin(Stdio::null())
		.stdout(writer.try_clone().expect("pipe clone"))
		.stderr(writer);
	let mut child = command.spawn().expect("spawn command");
	drop(command);
	let reading = std::thread::spawn(move || {
		let mut output = Vec::new();
		let _ = reader.read_to_end(&mut output);
		output
	});
	let deadline = Instant::now() + timeout;
	let mut timed_out = false;
	let status = loop {
		if let Some(status) = child.try_wait().expect("wait") {
			break status;
		}
		if Instant::now() >= deadline {
			let _ = child.kill();
			timed_out = true;
			break child.wait().expect("wait");
		}
		std::thread::sleep(Duration::from_millis(20));
	};
	let output = reading.join().expect("output reader");
	(
		status.success(),
		String::from_utf8_lossy(&output).into_owned(),
		timed_out,
	)
}

/// `runTask7Command`: the trimmed combined output of a command that must succeed.
pub(super) fn run_task7_command(directory: &Path, timeout: Duration, name: &str, args: &[&str]) -> String {
	let mut command = Command::new(name);
	command.args(args).current_dir(directory);
	let (success, output, timed_out) = run_combined(command, timeout);
	assert!(
		success && !timed_out,
		"{name} {args:?} success={success} timeout={timed_out}:\n{output}"
	);
	output.trim().to_owned()
}

/// A fresh `npm install` of the fixture, checked against the pinned transformer version.
pub(super) fn task7_install(fixture: &Path) -> (tempfile::TempDir, PathBuf) {
	let (dir, install) = temp_dir();
	let install = PathBuf::from(install);
	copy_task7_tree(fixture, &install);
	run_task7_command(&install, Duration::from_secs(300), "npm", &NPM_INSTALL);
	let version = run_task7_command(
		&install,
		Duration::from_secs(10),
		"node",
		&[
			"-e",
			r#"process.stdout.write(require("rbxts-transformer-flamework/package.json").version)"#,
		],
	);
	assert_eq!(version, FLAMEWORK_VERSION, "oracle transformer version");
	(dir, install)
}

/// `copyTask7Tree`
pub(super) fn copy_task7_tree(source: &Path, destination: &Path) {
	for entry in std::fs::read_dir(source).expect("copy fixture tree") {
		let entry = entry.expect("directory entry");
		let target = destination.join(entry.file_name());
		if entry.file_type().expect("file type").is_dir() {
			std::fs::create_dir_all(&target).expect("create directory");
			copy_task7_tree(&entry.path(), &target);
		} else {
			std::fs::create_dir_all(destination).expect("create directory");
			std::fs::write(&target, std::fs::read(entry.path()).expect("read fixture")).expect("write fixture");
		}
	}
}

/// `linkTask7Modules`: the installation's `node_modules`, directories recreated, symlinks copied
/// and files hard-linked.
pub(super) fn link_task7_modules(install: &Path, root: &Path) {
	fn link(source: &Path, destination: &Path) {
		std::fs::create_dir_all(destination).expect("create directory");
		for entry in std::fs::read_dir(source).expect("clone oracle node_modules") {
			let entry = entry.expect("directory entry");
			let target = destination.join(entry.file_name());
			let file_type = entry.file_type().expect("file type");
			if file_type.is_dir() {
				link(&entry.path(), &target);
			} else if file_type.is_symlink() {
				let link = std::fs::read_link(entry.path()).expect("read link");
				super::symlink(&link, &target).expect("symlink");
			} else {
				std::fs::hard_link(entry.path(), &target).expect("hard link");
			}
		}
	}
	link(&install.join("node_modules"), &root.join("node_modules"));
}

/// `task7TSConfig`
pub(super) fn task7_ts_config(plugin: &str, root_dir: &str) -> String {
	let plugins = match plugin {
		"default" => r#"[{"transform":"rbxts-transformer-flamework"}]"#.to_owned(),
		"" => "[]".to_owned(),
		_ => format!(r#"[{{"transform":"rbxts-transformer-flamework",{plugin}}}]"#),
	};
	format!(
		r#"{{"compilerOptions":{{"allowSyntheticDefaultImports":true,"downlevelIteration":true,"experimentalDecorators":true,"forceConsistentCasingInFileNames":true,"module":"commonjs","moduleDetection":"force","moduleResolution":"Node","noLib":true,"outDir":"out","plugins":{plugins},"rootDir":"{root_dir}","strict":true,"target":"ESNext","typeRoots":["node_modules/@rbxts","node_modules/@flamework"],"types":["types"]}},"include":["{root_dir}"]}}"#
	)
}

/// `task7TSFileConfig`
pub(super) fn task7_ts_file_config(plugin: &str, source: &str) -> String {
	task7_ts_config(plugin, "invalid").replacen(r#""include":["invalid"]"#, &format!(r#""files":["{source}"]"#), 1)
}

fn root_str(root: &Path) -> &str {
	root.to_str().expect("utf-8 path")
}

/// `task7SourceFiles`: the program's non-declaration `.ts` sources under `root`, by name.
pub(super) fn task7_source_files(program: &'static Program, root: &Path) -> Vec<P<SourceFile>> {
	let prefix = format!("{}/", rotor_support::gopath::to_slash(root_str(root)));
	let mut files: Vec<P<SourceFile>> = program
		.source_files()
		.iter()
		.copied()
		.filter(|source| {
			let name = rotor_support::gopath::to_slash(source.file_name());
			!source.is_declaration_file() && name.starts_with(&prefix) && name.ends_with(".ts")
		})
		.collect();
	files.sort_by(|left, right| left.file_name().cmp(right.file_name()));
	files
}

/// `task7RootDir`
pub(super) fn task7_root_dir(files: &[P<SourceFile>]) -> &'static str {
	if files
		.iter()
		.any(|source| rotor_support::gopath::to_slash(source.file_name()).contains("/invalid/"))
	{
		"invalid"
	} else {
		"src"
	}
}

/// `runTask7Oracle`: the real `rbxtsc`, which must succeed or fail as `success` says.
pub(super) fn run_task7_oracle(root: &Path, project: &str, success: bool) -> String {
	let mut command = Command::new(root.join("node_modules/.bin/rbxtsc"));
	command
		.args(["--project", project])
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env("NODE_OPTIONS", "--require=./deterministic-random.cjs");
	let (succeeded, output, timed_out) = run_combined(command, Duration::from_secs(60));
	assert!(
		!timed_out && succeeded == success,
		"oracle rbxtsc success={success} succeeded={succeeded} timeout={timed_out}:\n{output}"
	);
	output
}

/// The fixed UUID and random index both task 7 native runs use.
pub(super) fn task7_runtime() -> MacroRuntime {
	MacroRuntime {
		uuid: Some(Rc::new(|| Ok("00000000-0000-4000-8000-000000000004".to_owned()))),
		random_index: Some(Rc::new(|_| Ok(0))),
		build_guard: None,
	}
}

/// `runTask7Native`
pub(super) fn run_task7_native(root: &Path, config: &FlameworkConfig) -> (Vec<String>, Option<crate::Error>) {
	run_task7_native_mode(root, config, true)
}

/// `runTask7NativeMode`: the native transform of the fixture, written back over its sources, the
/// project persisted and, with `lower`, the result lowered to Luau. The diagnostics' messages
/// and the transform error.
pub(super) fn run_task7_native_mode(
	root: &Path,
	config: &FlameworkConfig,
	lower: bool,
) -> (Vec<String>, Option<crate::Error>) {
	rotor_tsrs::with_large_stack(|| {
		let program = new_transform_program(root_str(root));
		let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
		let files = task7_source_files(program, root);
		let mut project = open_project(&ProjectOptions {
			project_dir: root_str(root).to_owned(),
			root_dir: task7_root_dir(&files).to_owned(),
			out_dir: "out".to_owned(),
			config: config.clone(),
			..ProjectOptions::default()
		})
		.expect("OpenProject");
		let result = crate::transform::transform(TransformInput {
			program,
			checker: &mut checker,
			files,
			project: &mut project,
			analyses: Vec::new(),
			macro_runtime: Some(task7_runtime()),
			print: None,
		});
		drop(checker);
		let result = match result {
			Ok(result) => result,
			Err(error) => return (Vec::new(), Some(error)),
		};
		let diagnostics: Vec<String> = result
			.diagnostics
			.iter()
			.map(|diagnostic| diagnostic.string().to_owned())
			.collect();
		if !diagnostics.is_empty() {
			return (diagnostics, None);
		}
		for (source, metadata) in result.files.iter().zip(&result.sources) {
			let printed = rotor_tsrs::printer::new_printer(
				rotor_tsrs::printer::PrinterOptions::default(),
				rotor_tsrs::printer::PrintHandlers::default(),
				Some(metadata.emit_context()),
			)
			.emit_source_file(*source);
			assert!(
				parse_ts(source.file_name(), &printed).diagnostics().is_empty(),
				"reparse {} diagnostics",
				source.file_name()
			);
			std::fs::write(rotor_support::gopath::from_slash(source.file_name()).as_ref(), &printed)
				.expect("write transformed source");
		}
		project.persist().expect("Persist");
		if lower {
			lower_task7_files(root, &project);
		}
		(diagnostics, None)
	})
}

/// `lowerTask7Files`: every fixture source compiled to Luau under `out`.
pub(super) fn lower_task7_files(root: &Path, project: &Project) {
	let program = new_transform_program(root_str(root));
	let context = rotor_tsrs::Context::default();
	let root_text = rotor_support::gopath::to_slash(root_str(root)).into_owned();
	for source in task7_source_files(program, root) {
		let include_runtime = rotor_support::gopath::join(&[&project.include_directory, "RuntimeLib.lua"]);
		let rojo = project
			.resolver
			.get_rbx_path_from_file_path(&include_runtime)
			.map(|rbx| rotor_transformer::RojoContext {
				resolver: (*project.resolver).clone(),
				path_translator: (*project.translator).clone(),
				runtime_lib_rbx_path: Some(rbx),
				project_path: root_text.clone(),
				pkg_rojo_resolvers: Vec::new(),
				node_modules_path_mapping: rotor_transformer::NodeModulesPathMapping::default(),
				project_reference_paths: std::collections::HashMap::new(),
				node_modules_path: format!("{root_text}/node_modules"),
				type_roots: vec![
					format!("{root_text}/node_modules/@rbxts"),
					format!("{root_text}/node_modules/@flamework"),
				],
				use_case_sensitive_file_names: program.use_case_sensitive_file_names(),
			});
		let mut checker = program.get_type_checker_for_file(&context, source);
		let mut multi = rotor_transformer::MultiState::new();
		let mut state = rotor_transformer::State::new(
			program,
			&mut checker,
			source,
			&mut multi,
			rotor_transformer::StateOptions::default(),
		);
		if let Some(rojo) = &rojo {
			let project_type = if project.is_game {
				rotor_transformer::ProjectType::Game
			} else {
				rotor_transformer::ProjectType::Package
			};
			state.set_rojo_context(rojo, Some(project_type));
		}
		let statements = rotor_transformer::transform_source_file(&mut state);
		let luau = rotor_luau::render::render_ast(&state.ast, statements).expect("render");
		assert!(
			!state.diags.has_errors(),
			"lower {} diagnostics = {:?}",
			source.file_name(),
			state.diags.flush()
		);
		drop(state);
		let root_dir = root.join(task7_root_dir(&[source]));
		let source_path = PathBuf::from(rotor_support::gopath::from_slash(source.file_name()).as_ref());
		let relative = source_path.strip_prefix(&root_dir).expect("relative output path");
		let output = root.join("out").join(relative).with_extension("luau");
		std::fs::create_dir_all(output.parent().expect("output directory")).expect("create output directory");
		std::fs::write(&output, luau).expect("write Luau");
	}
}

/// Go's `fmt.Sprint` of a decoded JSON value.
fn go_sprint(value: &serde_json::Value) -> String {
	match value {
		serde_json::Value::Null => "<nil>".to_owned(),
		serde_json::Value::Bool(value) => value.to_string(),
		serde_json::Value::Number(number) => number.to_string(),
		serde_json::Value::String(text) => text.clone(),
		serde_json::Value::Array(items) => format!("[{}]", items.iter().map(go_sprint).collect::<Vec<_>>().join(" ")),
		serde_json::Value::Object(members) => format!(
			"map[{}]",
			members
				.iter()
				.map(|(key, value)| format!("{key}:{}", go_sprint(value)))
				.collect::<Vec<_>>()
				.join(" ")
		),
	}
}

/// `normalizeTask7JSON`
pub(super) fn normalize_task7_json(value: &mut serde_json::Value, sort_string_arrays: bool, sort_nested_arrays: bool) {
	match value {
		serde_json::Value::Object(members) => {
			for (key, child) in members.iter_mut() {
				if key == "filePath"
					&& let serde_json::Value::String(path) = child
				{
					*path = path.replace('\\', "/");
				}
				normalize_task7_json(
					child,
					sort_string_arrays || key == "paths" || key == "origins",
					sort_nested_arrays,
				);
			}
		}
		serde_json::Value::Array(items) => {
			let nested = items.first().is_some_and(serde_json::Value::is_array);
			if sort_string_arrays || sort_nested_arrays && nested {
				items.sort_by_key(go_sprint);
			}
			for child in items {
				normalize_task7_json(child, sort_string_arrays, sort_nested_arrays);
			}
		}
		_ => {}
	}
}

/// `normalizedTask7JSON`
pub(super) fn normalized_task7_json(path: &Path) -> Vec<u8> {
	let data = std::fs::read(path).unwrap_or_else(|error| panic!("read artifact {}: {error}", path.display()));
	let mut value: serde_json::Value =
		serde_json::from_slice(&data).unwrap_or_else(|error| panic!("decode artifact {}: {error}", path.display()));
	let globs = path.file_name().is_some_and(|name| name == "globs.json");
	normalize_task7_json(&mut value, false, globs);
	serde_json::to_vec(&value).expect("canonicalize artifact")
}

/// `compareTask7JSON`
pub(super) fn compare_task7_json(oracle: &Path, native: &Path) {
	let (want, got) = (normalized_task7_json(oracle), normalized_task7_json(native));
	assert!(
		want == got,
		"artifact structures differ\noracle {}\nnative {}",
		String::from_utf8_lossy(&want),
		String::from_utf8_lossy(&got)
	);
}

/// Intended fork divergence: top-level addPaths/addPathsGlob calls expand to static imports so
/// bundlers can follow them. `addPaths("src/server")` matches only a Script, so the glob's
/// modules are the only imports.
static TASK7_ADD_PATHS_RUNTIME: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"(?m)^Flamework\._addPaths\(.*\)\nFlamework\._addPathsGlob\(.*\)\n").expect("regex"));

const TASK7_ADD_PATHS_IMPORTS: &str = r#"TS.import(script, game:GetService("ReplicatedStorage"), "TS", "component-attributes")
TS.import(script, game:GetService("ReplicatedStorage"), "TS", "guards")
TS.import(script, game:GetService("ReplicatedStorage"), "TS", "networking")
"#;

/// `stripTask7LuauHeader`: the leading `-- Compiled with ...` line removed, so the comparison is
/// header-independent.
fn strip_task7_luau_header(data: &str) -> &str {
	if !data.starts_with("-- Compiled with ") {
		return data;
	}
	data.find('\n').map_or("", |index| &data[index + 1..])
}

/// `normalizedTask7Luau`: the banner stripped and the oracle's runtime addPaths calls mapped to
/// the imports the fork emits instead.
fn normalized_task7_luau(relative: &str, data: &str) -> String {
	let data = strip_task7_luau_header(data);
	if relative.ends_with("shared/macros.luau") {
		return TASK7_ADD_PATHS_RUNTIME
			.replace_all(data, TASK7_ADD_PATHS_IMPORTS)
			.into_owned();
	}
	data.to_owned()
}

/// Every `.luau` file under `root`, relative and slash-separated.
pub(super) fn luau_files(root: &Path) -> BTreeMap<String, String> {
	fn walk(root: &Path, directory: &Path, out: &mut BTreeMap<String, String>) {
		let Ok(entries) = std::fs::read_dir(directory) else {
			return;
		};
		for entry in entries {
			let entry = entry.expect("directory entry");
			let path = entry.path();
			if entry.file_type().expect("file type").is_dir() {
				walk(root, &path, out);
			} else if path.extension().is_some_and(|extension| extension == "luau") {
				let relative = path.strip_prefix(root).expect("relative path");
				let relative = rotor_support::gopath::to_slash(&relative.to_string_lossy()).into_owned();
				out.insert(relative, std::fs::read_to_string(&path).expect("read Luau"));
			}
		}
	}
	let mut out = BTreeMap::new();
	walk(root, root, &mut out);
	out
}

/// `compareTask7Trees`
fn compare_task7_trees(oracle: &Path, native: &Path) {
	let normalize = |root: &Path| -> BTreeMap<String, String> {
		luau_files(root)
			.into_iter()
			.map(|(relative, data)| {
				let normalized = normalized_task7_luau(&relative, &data);
				(relative, normalized)
			})
			.collect()
	};
	let (want, got) = (normalize(oracle), normalize(native));
	assert_eq!(
		want.keys().collect::<Vec<_>>(),
		got.keys().collect::<Vec<_>>(),
		"Luau tree paths differ"
	);
	for (name, expected) in &want {
		assert!(
			*expected == got[name],
			"final Luau {name} differs\n--- oracle\n{expected}\n--- native\n{}",
			got[name]
		);
	}
}

/// `task7Manifest`
fn task7_manifest(root: &Path) -> Vec<String> {
	let mut entries = Vec::new();
	for artifact in [
		"flamework.build",
		"include/flamework/config.json",
		"include/flamework/globs.json",
	] {
		let data = normalized_task7_json(&root.join(artifact));
		entries.push(format!("{}  {artifact}", hex(&Sha256::digest(&data))));
	}
	for (relative, data) in luau_files(&root.join("out")) {
		let relative = format!("out/{relative}");
		let normalized = normalized_task7_luau(&relative, &data);
		entries.push(format!("{}  {relative}", hex(&Sha256::digest(normalized.as_bytes()))));
	}
	entries.sort();
	entries
}

/// `readTask7TSV`
fn read_task7_tsv(path: &Path) -> Vec<Vec<String>> {
	let data = std::fs::read_to_string(path).unwrap_or_else(|error| panic!("open {}: {error}", path.display()));
	let records: Vec<Vec<String>> = data
		.lines()
		.map(|line| line.split('\t').map(str::to_owned).collect())
		.collect();
	assert!(!records.is_empty(), "read {}: no records", path.display());
	records
}

/// `task7PinnedDiagnosticLoci`
fn task7_pinned_diagnostic_loci() -> BTreeSet<String> {
	fn walk(root: &Path, directory: &Path, pattern: &Regex, loci: &mut BTreeSet<String>) {
		let mut entries: Vec<_> = std::fs::read_dir(directory)
			.expect("scan pinned diagnostic loci")
			.map(|entry| entry.expect("directory entry"))
			.collect();
		entries.sort_by_key(std::fs::DirEntry::file_name);
		for entry in entries {
			let path = entry.path();
			if entry.file_type().expect("file type").is_dir() {
				walk(root, &path, pattern, loci);
			} else if path.extension().is_some_and(|extension| extension == "ts") {
				let relative = path.strip_prefix(root).expect("relative path");
				let relative = rotor_support::gopath::to_slash(&relative.to_string_lossy()).into_owned();
				let text = std::fs::read_to_string(&path).expect("read source");
				for (index, line) in text.lines().enumerate() {
					if pattern.is_match(line) {
						loci.insert(format!("{relative}:{}", index + 1));
					}
				}
			}
		}
	}

	let root = repo_root().join("reference/rbxts-transformer-flamework");
	let pattern = Regex::new(
		r"Diagnostics\.(error|warning|createDiagnostic|addDiagnostic)|throw new (Error|DiagnosticError)|throw `|process\.exit\(|Logger\.error",
	)
	.expect("regex");
	let mut loci = BTreeSet::new();
	walk(&root, &root.join("src"), &pattern, &mut loci);
	loci
}

/// A fixture copy with the installation's modules linked in.
fn task7_root(fixture: &Path, install: &Path) -> (tempfile::TempDir, PathBuf) {
	let (dir, root) = temp_dir();
	let root = PathBuf::from(root);
	copy_task7_tree(fixture, &root);
	link_task7_modules(install, &root);
	(dir, root)
}

fn write(root: &Path, name: &str, contents: &str) {
	write_transform_fixture(root_str(root), name, contents);
}

#[expect(clippy::too_many_lines, reason = "one Go test, ported whole")]
#[test]
fn task7_differential_matches_real_upstream_final_luau_diagnostics_and_artifacts() {
	// A fresh real v1.3.2 npm installation and a controlled behavior/option matrix.
	let fixture = task7_fixture();
	let (_install_dir, install) = task7_install(&fixture);
	let config = |salt: &str, prefix: &str, mode: &str, limit: Option<i64>| FlameworkConfig {
		salt: salt.to_owned(),
		hash_prefix: prefix.to_owned(),
		id_generation_mode: mode.to_owned(),
		optimizations: rotor_config::config::FlameworkOptimizations {
			guard_generation_dedup_limit: limit,
		},
		..FlameworkConfig::default()
	};
	let salt = "task7-controlled-salt";
	let cases: Vec<(&str, &str, FlameworkConfig, bool)> = vec![
		("defaults", "default", FlameworkConfig::default(), false),
		(
			"no-semantic-diagnostics",
			r#""noSemanticDiagnostics":true"#,
			FlameworkConfig::default(),
			false,
		),
		(
			"full",
			r#""salt":"task7-controlled-salt","hashPrefix":"task7","idGenerationMode":"full","optimizations":{"guardGenerationDedupLimit":2}"#,
			config(salt, "task7", "full", Some(2)),
			false,
		),
		(
			"short",
			r#""salt":"task7-controlled-salt","hashPrefix":"task7","idGenerationMode":"short","preloadIds":false,"optimizations":{"guardGenerationDedupLimit":0}"#,
			config(salt, "task7", "short", Some(0)),
			false,
		),
		(
			"short-preload",
			r#""salt":"task7-controlled-salt","hashPrefix":"task7","idGenerationMode":"short","preloadIds":true,"optimizations":{"guardGenerationDedupLimit":0}"#,
			FlameworkConfig {
				preload_ids: true,
				..config(salt, "task7", "short", Some(0))
			},
			false,
		),
		(
			"tiny",
			r#""salt":"task7-controlled-salt","hashPrefix":"task7","idGenerationMode":"tiny","optimizations":{"guardGenerationDedupLimit":2}"#,
			config(salt, "task7", "tiny", Some(2)),
			false,
		),
		(
			"obfuscated",
			r#""salt":"task7-controlled-salt","hashPrefix":"task7","idGenerationMode":"obfuscated","obfuscation":true,"optimizations":{"guardGenerationDedupLimit":2}"#,
			FlameworkConfig {
				obfuscation: true,
				..config(salt, "task7", "obfuscated", Some(2))
			},
			false,
		),
		(
			"implicit-obfuscated-mode-and-persisted-salt",
			r#""obfuscation":true"#,
			FlameworkConfig {
				obfuscation: true,
				..FlameworkConfig::default()
			},
			true,
		),
	];

	for (name, plugin, config, seed_salt) in cases {
		let (_oracle_dir, oracle_root) = task7_root(&fixture, &install);
		let (_native_dir, native_root) = task7_root(&fixture, &install);
		let seed_salt_field = if seed_salt {
			r#","salt":"task7-controlled-salt""#
		} else {
			""
		};
		let path_hash = if config.obfuscation {
			r#","addPaths:src/**/*.ts":"00000000-0000-4000-8000-000000000004""#
		} else {
			""
		};
		let seed = format!(
			r#"{{"version":1,"flameworkVersion":"1.3.2","identifiers":{{}},"stringHashes":{{"task4:payload":"00000000-0000-4000-8000-000000000004"{path_hash}}}{seed_salt_field}}}"#
		);
		write(&oracle_root, "flamework.build", &seed);
		write(&native_root, "flamework.build", &seed);
		if seed_salt {
			write(&oracle_root, "out/task7.tsbuildinfo", r#"{"version":"5.5.3"}"#);
			write(&native_root, "out/task7.tsbuildinfo", r#"{"version":"5.5.3"}"#);
		}
		let mut oracle_config = task7_ts_config(plugin, "src");
		if seed_salt {
			oracle_config = oracle_config.replacen(
				r#""compilerOptions":{"#,
				r#""compilerOptions":{"incremental":true,"tsBuildInfoFile":"out/task7.tsbuildinfo","#,
				1,
			);
		}
		write(&oracle_root, "tsconfig.json", &oracle_config);
		write(&native_root, "tsconfig.json", &task7_ts_config("", "src"));

		// The real npm transformer and Rotor native path compile the same pristine tree.
		let oracle_output = run_task7_oracle(&oracle_root, "tsconfig.json", true);
		let (diagnostics, native_error) = run_task7_native(&native_root, &config);

		// Final Luau bytes, ordered diagnostics, and normalized artifact structures match.
		compare_task7_trees(&oracle_root.join("out"), &native_root.join("out"));
		assert!(
			native_error.is_none() && diagnostics.is_empty(),
			"{name}: native error={native_error:?} diagnostics={diagnostics:?}, want none; oracle output:\n{oracle_output}"
		);
		for artifact in [
			"flamework.build",
			"include/flamework/config.json",
			"include/flamework/globs.json",
		] {
			compare_task7_json(&oracle_root.join(artifact), &native_root.join(artifact));
		}
		assert_eq!(
			task7_manifest(&oracle_root),
			task7_manifest(&native_root),
			"{name}: SHA-256 manifests differ"
		);
	}
}

#[test]
fn task7_diagnostic006_public_compiler_preconditions_are_executable() {
	// The pinned package has an intentionally mismatched private TypeScript, while roblox-ts and
	// the native compiler each retain their public compiler ABI.
	let fixture = task7_fixture();
	let (_install_dir, install) = temp_dir();
	let install = PathBuf::from(install);
	copy_task7_tree(&fixture, &install);
	write(&install, "tsconfig.json", &task7_ts_config("default", "src"));
	run_task7_command(&install, Duration::from_secs(300), "npm", &NPM_INSTALL);
	let transformer_root = install.join("node_modules/rbxts-transformer-flamework");
	let transformer_root = root_str(&transformer_root).to_owned();
	run_task7_command(
		&install,
		Duration::from_secs(300),
		"npm",
		&[
			"install",
			"--prefix",
			&transformer_root,
			"--no-save",
			"--ignore-scripts",
			"--package-lock=false",
			"--no-audit",
			"--no-fund",
			"--prefer-offline",
			"typescript@5.4.5",
		],
	);
	let versions = run_task7_command(
		&install,
		Duration::from_secs(10),
		"node",
		&[
			"-e",
			r#"process.stdout.write(require("./node_modules/rbxts-transformer-flamework/node_modules/typescript").version + "|" + require(require.resolve("typescript", { paths: [require.resolve("roblox-ts")] })).version)"#,
		],
	);
	assert_eq!(versions, "5.4.5|5.5.3", "mismatched TypeScript fixture versions");

	// The real package entrypoint performs its public hook, and native receives only SourceFiles
	// obtained from its single compiled program.
	let upstream = run_task7_command(
		&install,
		Duration::from_secs(10),
		"node",
		&[
			"-e",
			r#"require("rbxts-transformer-flamework")"#,
			"--",
			"--force-flamework-hook",
		],
	);
	let (_native_dir, native_root) = task7_root(&fixture, &install);
	write(&native_root, "tsconfig.json", &task7_ts_config("", "src"));
	let file_count = rotor_tsrs::with_large_stack(|| {
		let program = new_transform_program(root_str(&native_root));
		let files = task7_source_files(program, &native_root);
		for file in &files {
			let owned = program.get_source_file(&rotor_support::gopath::to_slash(file.file_name()));
			assert_eq!(
				owned,
				Some(*file),
				"native callback file {:?} is not owned by its compiler Program",
				file.file_name()
			);
		}
		files.len()
	});
	let config = FlameworkConfig {
		no_semantic_diagnostics: true,
		..FlameworkConfig::default()
	};
	let (native_diagnostics, native_error) = run_task7_native_mode(&native_root, &config, false);

	// The upstream precondition emits its exact three-line warning and hooks to roblox-ts 5.5.3
	// without failing. Native succeeds with program-owned files.
	let want_upstream = [
		"[Flamework]: TypeScript version differs",
		"[Flamework]: Flamework: v5.4.5, roblox-ts: v5.5.3",
		"[Flamework]: Flamework will switch to v5.5.3, but you can get rid of this warning by running: npm i -D typescript@5.5.3",
	]
	.join("\n");
	assert_eq!(upstream, want_upstream, "upstream TypeScript hook output");
	assert!(
		native_error.is_none() && native_diagnostics.is_empty() && file_count != 0,
		"native single-ABI build = files:{file_count} diagnostics:{native_diagnostics:?} error:{native_error:?}"
	);
}

#[test]
fn task7_differential_matches_real_upstream_ordered_diagnostics() {
	// Each malformed upstream branch in a fresh deterministic compilation.
	let fixture = task7_fixture();
	let (_install_dir, install) = task7_install(&fixture);
	let plugin = r#""salt":"task7-controlled-salt","hashPrefix":"task7","idGenerationMode":"full""#;
	let config = FlameworkConfig {
		salt: "task7-controlled-salt".to_owned(),
		hash_prefix: "task7".to_owned(),
		id_generation_mode: "full".to_owned(),
		..FlameworkConfig::default()
	};
	for source in [
		"invalid/non-constant-path.ts",
		"invalid/missing-rojo-path.ts",
		"invalid/template-literal-guard.ts",
		"invalid/type-parameter-guard.ts",
		"invalid/class-guard.ts",
		"invalid/non-tuple-guard.ts",
		"invalid/declaration-uid-placement.ts",
		"invalid/const-parameter.ts",
		"invalid/const-parameter-spread.ts",
		"invalid/network-middleware.ts",
	] {
		let (_oracle_dir, oracle_root) = task7_root(&fixture, &install);
		let (_native_dir, native_root) = task7_root(&fixture, &install);
		write(&oracle_root, "tsconfig.json", &task7_ts_file_config(plugin, source));
		write(&native_root, "tsconfig.json", &task7_ts_file_config("", source));

		// Both implementations process the same invalid file.
		let oracle_output = run_task7_oracle(&oracle_root, "tsconfig.json", false);
		let (mut native_diagnostics, native_error) = run_task7_native(&native_root, &config);
		if let Some(error) = native_error {
			native_diagnostics.push(match error.detail() {
				Some(ErrorDetail::Macro(macro_error)) => macro_error.message.clone(),
				Some(ErrorDetail::Guard(guard_error)) => guard_error.message(),
				_ => error.message().to_owned(),
			});
		}
		let clean = TASK7_ANSI.replace_all(&oracle_output, "");
		let ordered_oracle: Vec<String> = TASK7_ORACLE_DIAGNOSTIC
			.captures_iter(&clean)
			.map(|captures| captures[1].trim_end_matches('\r').to_owned())
			.collect();

		// The exact ordered diagnostic messages match.
		assert_eq!(
			ordered_oracle, native_diagnostics,
			"{source}: ordered diagnostics differ\nraw oracle:\n{oracle_output}"
		);
	}
}

#[test]
fn task7_differential_no_semantic_diagnostics_controls_the_semantic_gate() {
	// A macro-bearing source with one stable TypeScript semantic error.
	let fixture = task7_fixture();
	let (_install_dir, install) = task7_install(&fixture);
	let source = "invalid/semantic-error.ts";

	// The default false branch validates semantics in both compiler implementations.
	let (_oracle_dir, oracle_root) = task7_root(&fixture, &install);
	let (_native_dir, native_root) = task7_root(&fixture, &install);
	write(&oracle_root, "tsconfig.json", &task7_ts_file_config("default", source));
	let oracle_output = TASK7_ANSI
		.replace_all(&run_task7_oracle(&oracle_root, "tsconfig.json", false), "")
		.into_owned();
	write(&native_root, "tsconfig.json", &task7_ts_file_config("", source));
	let native_diagnostics: Vec<(i32, String)> = rotor_tsrs::with_large_stack(|| {
		let program = new_transform_program(root_str(&native_root));
		let path = rotor_support::gopath::to_slash(root_str(&native_root.join(source))).into_owned();
		let native_source = program
			.get_source_file(&path)
			.expect("native semantic fixture source not found");
		program
			.get_semantic_diagnostics(&rotor_tsrs::Context::default(), Some(native_source))
			.iter()
			.map(|diagnostic| (diagnostic.code(), diagnostic.localize()))
			.collect()
	});

	// The ordered TypeScript diagnostic tuple is identical and neither path emits Luau.
	let matches: Vec<(String, String)> = TASK7_SEMANTIC_DIAGNOSTIC
		.captures_iter(&oracle_output)
		.map(|captures| (captures[1].to_owned(), captures[2].trim_end_matches('\r').to_owned()))
		.collect();
	assert!(
		matches.len() == 1 && native_diagnostics.len() == 1,
		"semantic diagnostics oracle={matches:?} native={native_diagnostics:?}\n{oracle_output}"
	);
	let want_tuple = format!("{}:{}", matches[0].0, matches[0].1);
	let got_tuple = format!("{}:{}", native_diagnostics[0].0, native_diagnostics[0].1);
	assert_eq!(got_tuple, want_tuple, "semantic diagnostic tuple");

	// noSemanticDiagnostics=true skips that gate and both transformers emit the macro result.
	let (_oracle_skip_dir, oracle_skip_root) = task7_root(&fixture, &install);
	let (_native_skip_dir, native_skip_root) = task7_root(&fixture, &install);
	write(
		&oracle_skip_root,
		"tsconfig.json",
		&task7_ts_file_config(r#""noSemanticDiagnostics":true"#, source),
	);
	run_task7_oracle(&oracle_skip_root, "tsconfig.json", false);
	write(&native_skip_root, "tsconfig.json", &task7_ts_file_config("", source));
	let config = FlameworkConfig {
		no_semantic_diagnostics: true,
		..FlameworkConfig::default()
	};
	let (diagnostics, error) = run_task7_native_mode(&native_skip_root, &config, false);
	assert!(
		error.is_none() && diagnostics.is_empty(),
		"native skip gate error={error:?} diagnostics={diagnostics:?}"
	);
	compare_task7_json(
		&oracle_skip_root.join("flamework.build"),
		&native_skip_root.join("flamework.build"),
	);
}

#[test]
fn task7_differential_scoped_package_defaults_hash_prefix_to_package_name() {
	// The representative corpus published as a scoped package with hashPrefix omitted.
	let fixture = task7_fixture();
	let (_install_dir, install) = task7_install(&fixture);
	let (_oracle_dir, oracle_root) = task7_root(&fixture, &install);
	let (_native_dir, native_root) = task7_root(&fixture, &install);
	for root in [&oracle_root, &native_root] {
		let package = std::fs::read_to_string(root.join("package.json")).expect("read package fixture");
		write(
			root,
			"package.json",
			&package.replacen(r#""rotor-flamework-task4-oracle""#, r#""@task7/scoped-package""#, 1),
		);
		write(
			root,
			"flamework.build",
			r#"{"version":1,"flameworkVersion":"1.3.2","identifiers":{},"stringHashes":{"task4:payload":"00000000-0000-4000-8000-000000000004"}}"#,
		);
	}
	write(&oracle_root, "tsconfig.json", &task7_ts_config("default", "src"));
	write(&native_root, "tsconfig.json", &task7_ts_config("", "src"));

	// The real transformer and native transformer compile the package.
	run_task7_oracle(&oracle_root, "tsconfig.json", true);
	let (diagnostics, error) = run_task7_native_mode(&native_root, &FlameworkConfig::default(), false);

	// Package-prefixed IDs and transformer-owned build metadata are exact.
	assert!(
		error.is_none() && diagnostics.is_empty(),
		"native package error={error:?} diagnostics={diagnostics:?}"
	);
	compare_task7_json(
		&oracle_root.join("flamework.build"),
		&native_root.join("flamework.build"),
	);
	let build = std::fs::read_to_string(native_root.join("flamework.build")).expect("read build info");
	assert!(
		build.contains(r#""identifierPrefix": "@task7/scoped-package""#),
		"scoped package build info:\n{build}"
	);
}

#[test]
fn task7_differential_accounts_for_every_pinned_upstream_entry_and_case() {
	// The pinned upstream inventory and machine-consumed differential matrix.
	let testdata = flamework_testdata("task7-differential");
	let inventory = read_task7_tsv(&testdata.join("upstream-inventory.tsv"));
	let coverage = read_task7_tsv(&testdata.join("coverage.tsv"));
	let loci = read_task7_tsv(&testdata.join("diagnostic-loci.tsv"));

	// All source entries and declared behavior families are accounted.
	let mut paths = BTreeSet::new();
	let mut families = BTreeSet::new();
	for record in &inventory[1..] {
		paths.insert(record[1].clone());
		families.insert(record[2].clone());
	}
	let mut covered = BTreeSet::new();
	for record in &coverage[1..] {
		for family in record[1].split(',') {
			covered.insert(family.to_owned());
		}
	}

	// The unported entry and behavior-family counts are exactly zero.
	let unported: Vec<&String> = families
		.iter()
		.filter(|family| {
			!covered.contains(*family) && *family != "entrypoint" && *family != "nonbehavior" && *family != "dispatch"
		})
		.collect();
	let pinned_loci = task7_pinned_diagnostic_loci();
	let mut accounted_loci = BTreeSet::new();
	for record in &loci[1..] {
		assert!(
			record.len() == 3 && !record[2].is_empty(),
			"invalid diagnostic locus record {record:?}"
		);
		accounted_loci.insert(format!("{}:{}", record[0], record[1]));
	}
	let missing: Vec<&String> = pinned_loci.difference(&accounted_loci).collect();
	assert!(
		paths.len() == 67
			&& unported.is_empty()
			&& coverage.len() == 22
			&& pinned_loci.len() == 64
			&& pinned_loci == accounted_loci,
		"accounting paths={} cases={} diagnostic-loci={} unported={unported:?} missing-loci={missing:?}, want 67/21/64/zero",
		paths.len(),
		coverage.len() - 1,
		pinned_loci.len()
	);
}

/// Go's `exec.Command(...).Output()` with `stdin`: exit success, stdout and stderr.
pub(super) fn run_with_stdin(mut command: Command, stdin: &[u8]) -> (bool, Vec<u8>, Vec<u8>) {
	command
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped());
	let mut child = command.spawn().expect("spawn command");
	child
		.stdin
		.take()
		.expect("stdin")
		.write_all(stdin)
		.expect("write stdin");
	let output = child.wait_with_output().expect("wait");
	(output.status.success(), output.stdout, output.stderr)
}
