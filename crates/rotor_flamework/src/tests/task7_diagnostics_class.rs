//! `task7_diagnostics_class_test.go`: the executable sidecar/native differential in one harness.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;
use rotor_config::config::FlameworkConfig;
use rotor_tsrs::diagnostics::Category;
use sha2::{Digest, Sha256};

use super::oracle_corpus::{flamework_testdata, repo_root};
use super::task7_differential::{
	TASK7_ANSI, copy_task7_tree, link_task7_modules, lower_task7_files, luau_files, normalize_task7_json,
	run_task7_native, run_task7_oracle, run_with_stdin, task7_fixture, task7_install, task7_runtime,
	task7_source_files, task7_ts_config,
};
use super::{new_transform_program, temp_dir, write_transform_fixture};
use crate::project::{ProjectOptions, open_project};
use crate::transform_diagnostics::Diagnostic;
use crate::transform_state::TransformInput;
use crate::{Error, ErrorDetail, ErrorKind};

/// `task7ClassDiagnosticTuple`
#[derive(Clone, Debug, PartialEq, Eq)]
struct Tuple {
	category: String,
	code: String,
	file: String,
	start: i64,
	length: i64,
	message: String,
	related: Vec<Self>,
}

fn uid_tuple(file: &str, start: i64, length: i64, message: &str) -> Tuple {
	Tuple {
		category: "error".to_owned(),
		code: " @flamework/core".to_owned(),
		file: file.to_owned(),
		start,
		length,
		message: message.to_owned(),
		related: Vec::new(),
	}
}

fn write(root: &Path, name: &str, contents: &str) {
	write_transform_fixture(root.to_str().expect("utf-8 path"), name, contents);
}

#[test]
fn task7_diagnostics_class_canonical_diagnostic_branches_match_oracle() {
	let symbol = uid_tuple(
		"src/server/uid-symbol.server.ts",
		95,
		10,
		"Could not find UID for symbol \"globalThis\"",
	);
	let type_tuple = uid_tuple(
		"src/server/uid-type.server.ts",
		102,
		24,
		"Could not find UID for type \"T extends string ? 1 : 2\"",
	);
	let want: [(&str, Vec<Tuple>); 4] = [
		("constraint", Vec::new()),
		("circular", Vec::new()),
		("uid-symbol", vec![symbol]),
		("uid-type", vec![type_tuple]),
	];

	for (name, want) in want {
		let (oracle, native) = task7_class_run_diagnostic_case(name);

		assert!(
			oracle == want && native == want,
			"{name} exact tuples differ\nactual oracle: {oracle:?}\nactual native: {native:?}"
		);
		assert_eq!(oracle, native, "{name} canonical tuple parity RED");
	}
}

#[test]
fn task7_diagnostics_class_ordered_diagnostic_artifact_matches_oracle() {
	let want_oracle = vec![
		uid_tuple(
			"src/server/ordered-symbol.server.ts",
			95,
			10,
			"Could not find UID for symbol \"globalThis\"",
		),
		uid_tuple(
			"src/server/ordered-type.server.ts",
			102,
			24,
			"Could not find UID for type \"T extends string ? 1 : 2\"",
		),
	];
	let want_native = want_oracle.clone();

	let (oracle, native) = task7_class_run_diagnostic_case("ordered");

	assert!(
		oracle == want_oracle && native == want_native,
		"ordered diagnostic artifact differs\nactual oracle: {oracle:?}\nactual native: {native:?}"
	);
	assert_eq!(oracle, native, "ordered diagnostic artifact parity RED");
}

#[expect(clippy::too_many_lines, reason = "one Go test, ported whole")]
#[test]
fn task7_diagnostics_class_same_compiler_five_family_tree_matches_oracle() {
	let fixture = task7_fixture();
	let (_install_dir, install) = task7_install(&fixture);
	let (_oracle_dir, oracle_root) = temp_dir();
	let (_native_dir, native_root) = temp_dir();
	let (oracle_root, native_root) = (PathBuf::from(oracle_root), PathBuf::from(native_root));
	let config = FlameworkConfig {
		salt: "task7-controlled-salt".to_owned(),
		hash_prefix: "task7".to_owned(),
		id_generation_mode: "full".to_owned(),
		optimizations: rotor_config::config::FlameworkOptimizations {
			guard_generation_dedup_limit: Some(2),
		},
		..FlameworkConfig::default()
	};
	for root in [&oracle_root, &native_root] {
		copy_task7_tree(&fixture, root);
		link_task7_modules(&install, root);
		let macros_path = root.join("src/shared/macros.ts");
		let macros = std::fs::read_to_string(&macros_path).expect("read five-family macros fixture");
		let five_family_macros = macros.replace(
			"\nFlamework.addPaths(\"src/server\");\nFlamework.addPathsGlob(\"src/**/*.ts\");",
			"",
		);
		assert_ne!(
			five_family_macros, macros,
			"five-family fixture did not remove the separately covered macro-path family"
		);
		std::fs::write(&macros_path, five_family_macros).expect("write five-family macros fixture");
		write(
			root,
			"flamework.build",
			r#"{"version":1,"flameworkVersion":"1.3.2","identifiers":{},"stringHashes":{"task4:payload":"00000000-0000-4000-8000-000000000004"}}"#,
		);
	}
	// Both compilers receive the controlled salt as an explicit option. It is intentionally absent
	// from persisted build info because explicit salt configuration takes precedence over, and
	// clears, prior persisted salt.
	let plugin = r#""salt":"task7-controlled-salt","hashPrefix":"task7","idGenerationMode":"full","optimizations":{"guardGenerationDedupLimit":2}"#;
	// The seeded string hash represents an incremental project. Upstream Flamework deliberately
	// replaces build info on its first clean compile, so the marker makes this oracle exercise the
	// existing-build-info path whose artifacts the native project is compared against.
	let oracle_config = task7_ts_config(plugin, "src").replacen(
		r#""compilerOptions":{"#,
		r#""compilerOptions":{"incremental":true,"tsBuildInfoFile":"out/task7.tsbuildinfo","#,
		1,
	);
	write(&oracle_root, "tsconfig.json", &oracle_config);
	write(&oracle_root, "out/task7.tsbuildinfo", "{}\n");
	write(&native_root, "tsconfig.json", &task7_ts_config("", "src"));
	let oracle_files = task7_class_compile_files(&oracle_root);
	assert_eq!(oracle_files.len(), 5, "representative family source count");

	let response = task7_class_run_sidecar(&oracle_root, &oracle_files);
	let diagnostics = response
		.get("diagnostics")
		.and_then(serde_json::Value::as_array)
		.cloned()
		.unwrap_or_default();
	let transformed = response
		.get("transformed")
		.and_then(serde_json::Value::as_array)
		.cloned()
		.unwrap_or_default();
	assert!(
		diagnostics.is_empty() && transformed.len() == oracle_files.len(),
		"sidecar diagnostics={diagnostics:?} transformed={}, want zero/{}",
		transformed.len(),
		oracle_files.len()
	);
	for file in &transformed {
		let text = |key: &str| file.get(key).and_then(serde_json::Value::as_str).unwrap_or_default();
		std::fs::write(
			rotor_support::gopath::from_slash(text("fileName")).as_ref(),
			text("text"),
		)
		.expect("write sidecar transform");
	}
	let oracle_project = open_project(&ProjectOptions {
		project_dir: oracle_root.to_string_lossy().into_owned(),
		root_dir: "src".to_owned(),
		out_dir: "out".to_owned(),
		config: config.clone(),
		..ProjectOptions::default()
	})
	.expect("OpenProject() sidecar oracle");
	rotor_tsrs::with_large_stack(|| lower_task7_files(&oracle_root, &oracle_project));
	let (native_diagnostics, native_error) = run_task7_native(&native_root, &config);
	assert!(
		native_error.is_none() && native_diagnostics.is_empty(),
		"native error={native_error:?} diagnostics={native_diagnostics:?}"
	);
	let oracle_tree = task7_class_transformer_tree(&oracle_root);
	let native_tree = task7_class_transformer_tree(&native_root);

	assert!(
		oracle_tree.len() == 7 && native_tree.len() == 7,
		"transformer-owned tree sizes oracle={} native={}, want 7",
		oracle_tree.len(),
		native_tree.len()
	);
	assert!(
		oracle_tree == native_tree,
		"same-compiler transformer-owned tree differs\noracle hash={}\nnative hash={}\n{}",
		task7_class_tree_hash(&oracle_tree),
		task7_class_tree_hash(&native_tree),
		task7_class_tree_diff(&oracle_tree, &native_tree)
	);
	assert_eq!(
		task7_class_tree_hash(&oracle_tree),
		task7_class_tree_hash(&native_tree),
		"independent transformer-owned tree hashes differ after byte equality"
	);
}

/// `task7ClassRunDiagnosticCase`: the oracle's and native tuples for one fixture case.
fn task7_class_run_diagnostic_case(name: &str) -> (Vec<Tuple>, Vec<Tuple>) {
	let fixture = task7_fixture();
	let (_oracle_dir, oracle_root) = temp_dir();
	let (_native_dir, native_root) = temp_dir();
	let (oracle_root, native_root) = (PathBuf::from(oracle_root), PathBuf::from(native_root));
	let fixture_names: Vec<&str> = if name == "ordered" {
		vec!["uid-symbol", "uid-type"]
	} else {
		vec![name]
	};
	let source_paths: Vec<String> = fixture_names
		.iter()
		.map(|fixture_name| {
			let output_name = if name == "ordered" {
				format!("ordered-{}", fixture_name.trim_start_matches("uid-"))
			} else {
				name.to_owned()
			};
			format!("src/server/{output_name}.server.ts")
		})
		.collect();
	for root in [&oracle_root, &native_root] {
		copy_task7_tree(&fixture, root);
		task7_class_link_pinned_modules(root);
		for (fixture_name, source_path) in fixture_names.iter().zip(&source_paths) {
			let source = std::fs::read_to_string(flamework_testdata(&format!(
				"task7-diagnostics-class/{fixture_name}.ts"
			)))
			.expect("read fixture");
			write(root, source_path, &source);
		}
	}
	let plugin = r#""salt":"task7-controlled-salt","hashPrefix":"task7","idGenerationMode":"full""#;
	write(
		&oracle_root,
		"tsconfig.json",
		&task7_class_ts_files_config(plugin, &source_paths),
	);
	write(
		&native_root,
		"tsconfig.json",
		&task7_class_ts_files_config("", &source_paths),
	);
	let oracle_output = run_task7_oracle(
		&oracle_root,
		"tsconfig.json",
		name == "constraint" || name == "circular",
	);
	let oracle_diagnostics = task7_class_oracle_cli_tuples(&oracle_root, &oracle_output);
	(
		task7_class_relativize_tuples(&oracle_root, oracle_diagnostics),
		task7_class_run_native_tuples(&native_root),
	)
}

/// `task7ClassRunNativeTuples`
fn task7_class_run_native_tuples(root: &Path) -> Vec<Tuple> {
	rotor_tsrs::with_large_stack(|| {
		let root_text = root.to_string_lossy().into_owned();
		let program = new_transform_program(&root_text);
		let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
		let files = task7_source_files(program, root);
		let mut project = open_project(&ProjectOptions {
			project_dir: root_text,
			root_dir: "src".to_owned(),
			out_dir: "out".to_owned(),
			config: FlameworkConfig {
				salt: "task7-controlled-salt".to_owned(),
				hash_prefix: "task7".to_owned(),
				id_generation_mode: "full".to_owned(),
				..FlameworkConfig::default()
			},
			..ProjectOptions::default()
		})
		.expect("OpenProject() native diagnostics");
		let result = crate::transform::transform(TransformInput {
			program,
			checker: &mut checker,
			files,
			project: &mut project,
			analyses: Vec::new(),
			macro_runtime: Some(task7_runtime()),
			print: None,
		});
		match result {
			Err(error) => vec![task7_class_error_tuple(root, &error)],
			Ok(result) => result
				.diagnostics
				.iter()
				.map(|diagnostic| task7_class_ast_tuple(root, diagnostic))
				.collect(),
		}
	})
}

/// `task7ClassASTTuple`
fn task7_class_ast_tuple(root: &Path, diagnostic: &Diagnostic) -> Tuple {
	let related = diagnostic
		.related_information
		.iter()
		.map(|info| task7_class_ast_tuple(root, info))
		.collect();
	let category = if diagnostic.category == Category::Error {
		"error"
	} else {
		"warning"
	};
	let code = diagnostic
		.string_code
		.clone()
		.unwrap_or_else(|| diagnostic.code().to_string());
	let file = diagnostic
		.file
		.map(|file| file.file_name().to_owned())
		.unwrap_or_default();
	Tuple {
		category: category.to_owned(),
		code,
		file: task7_class_relative_path(root, &file),
		start: i64::from(diagnostic.pos()),
		length: i64::from(diagnostic.end()) - i64::from(diagnostic.pos()),
		message: diagnostic.string().to_owned(),
		related,
	}
}

/// `task7ClassErrorTuple`. Go names an unrecognized error by its dynamic type (`%T`), which Rust
/// errors do not have; such errors read `error`.
fn task7_class_error_tuple(root: &Path, error: &Error) -> Tuple {
	if let Some(ErrorDetail::Macro(macro_error)) = error.detail()
		&& let Some(node) = macro_error.node
	{
		let file = rotor_tsrs::ast::get_source_file_of_node(node).expect("macro error file");
		let start = rotor_tsrs::scanner::get_token_pos_of_node(node, file, false);
		let end = crate::transform_diagnostics::text_pos(node.end());
		return Tuple {
			category: "error".to_owned(),
			code: "ErrMacroDiagnostic".to_owned(),
			file: task7_class_relative_path(root, file.file_name()),
			start: i64::from(start),
			length: i64::from(end) - i64::from(start),
			message: macro_error.message.clone(),
			related: Vec::new(),
		};
	}
	let code = if error.is(ErrorKind::CircularDependency) {
		"ErrCircularDependency"
	} else {
		"error"
	};
	Tuple {
		category: "error".to_owned(),
		code: code.to_owned(),
		file: String::new(),
		start: -1,
		length: 0,
		message: error.message().to_owned(),
		related: Vec::new(),
	}
}

/// `task7ClassRunSidecar`
fn task7_class_run_sidecar(root: &Path, compile_files: &[String]) -> serde_json::Value {
	let sidecar = repo_root().join("tools/sidecar/main.js");
	let request = serde_json::json!({
		"protocol": 1,
		"operation": "transform",
		"tsConfigPath": root.join("tsconfig.json"),
		"projectDir": root,
		"compileFileNames": compile_files,
		"changedFiles": [],
	});
	let mut request = serde_json::to_vec(&request).expect("marshal sidecar request");
	request.push(b'\n');
	let mut command = Command::new("node");
	command
		.arg(&sidecar)
		.arg("--project")
		.arg(root.join("tsconfig.json"))
		.arg("--rojo")
		.arg(root.join("default.project.json"))
		.current_dir(root)
		.env("NODE_OPTIONS", "--require=./deterministic-random.cjs");
	let (success, stdout, stderr) = run_with_stdin(command, &request);
	assert!(success, "sidecar failed stderr={}", String::from_utf8_lossy(&stderr));
	serde_json::from_slice(stdout.trim_ascii()).unwrap_or_else(|error| {
		panic!(
			"decode sidecar response: {error}\nstdout={}\nstderr={}",
			String::from_utf8_lossy(&stdout),
			String::from_utf8_lossy(&stderr)
		)
	})
}

/// `task7ClassOracleCLITuples`
fn task7_class_oracle_cli_tuples(root: &Path, output: &str) -> Vec<Tuple> {
	let clean = TASK7_ANSI.replace_all(output, "").into_owned();
	let line_pattern =
		Regex::new(r"(?m)^\s*([^\n:]+):(\d+):(\d+) - (error|message) TS([^:]+): ([^\n]+)$").expect("regex");
	let matches: Vec<regex::Captures<'_>> = line_pattern.captures_iter(&clean).collect();
	let mut tuples = Vec::with_capacity(matches.len());
	for (match_index, fields) in matches.iter().enumerate() {
		let whole = fields.get(0).expect("match");
		let line: usize = fields[2].parse().expect("line");
		let column: usize = fields[3].parse().expect("column");
		let file_name = rotor_support::gopath::to_slash(fields[1].trim()).into_owned();
		let source = std::fs::read_to_string(root.join(&file_name)).expect("read oracle diagnostic source");
		let start = task7_class_line_column_offset(&source, line, column);
		let block_end = matches
			.get(match_index + 1)
			.map_or(clean.len(), |next| next.get(0).expect("match").start());
		let underline_length = clean[whole.end()..block_end]
			.split('\n')
			.map(|output_line| output_line.matches('~').count())
			.find(|count| *count > 0)
			.unwrap_or(0);
		let category = if &fields[4] == "message" { "warning" } else { &fields[4] };
		tuples.push(Tuple {
			category: category.to_owned(),
			code: fields[5].to_owned(),
			file: file_name,
			start: i64::try_from(start).expect("start"),
			length: i64::try_from(underline_length).expect("length"),
			message: fields[6].trim_end_matches('\r').to_owned(),
			related: Vec::new(),
		});
	}
	tuples
}

/// `task7ClassLineColumnOffset`
fn task7_class_line_column_offset(source: &str, line: usize, column: usize) -> usize {
	let lines: Vec<&str> = source.split('\n').collect();
	let offset: usize = lines[..line - 1].iter().map(|text| text.len() + 1).sum();
	offset + column - 1
}

/// `task7ClassLinkPinnedModules`
fn task7_class_link_pinned_modules(root: &Path) {
	let modules = rotor_support::fsutil::real_path(repo_root().join("testdata/transformers/project/node_modules"))
		.expect("resolve pinned modules");
	let version = std::fs::read_to_string(modules.join("rbxts-transformer-flamework/package.json"));
	assert!(
		version
			.as_deref()
			.is_ok_and(|version| version.contains(r#""version": "1.3.2""#)),
		"pinned transformer unavailable or wrong version"
	);
	super::symlink(&modules, &root.join("node_modules")).expect("link pinned modules");
}

/// `task7ClassCompileFiles`
fn task7_class_compile_files(root: &Path) -> Vec<String> {
	rotor_tsrs::with_large_stack(|| {
		let program = new_transform_program(&root.to_string_lossy());
		task7_source_files(program, root)
			.iter()
			.map(|file| rotor_support::gopath::from_slash(file.file_name()).into_owned())
			.collect()
	})
}

/// `task7ClassTSFilesConfig`
fn task7_class_ts_files_config(plugin: &str, sources: &[String]) -> String {
	let quoted: Vec<String> = sources
		.iter()
		.map(|source| rotor_support::gostr::quote(source))
		.collect();
	task7_ts_config(plugin, "src").replacen(r#""include":["src"]"#, &format!(r#""files":[{}]"#, quoted.join(",")), 1)
}

/// `task7ClassRelativizeTuples`
fn task7_class_relativize_tuples(root: &Path, tuples: Vec<Tuple>) -> Vec<Tuple> {
	tuples
		.into_iter()
		.map(|tuple| Tuple {
			file: task7_class_relative_path(root, &tuple.file),
			..tuple
		})
		.collect()
}

/// `task7ClassRelativePath`
fn task7_class_relative_path(root: &Path, path: &str) -> String {
	if path.is_empty() {
		return String::new();
	}
	let native = rotor_support::gopath::from_slash(path);
	Path::new(native.as_ref()).strip_prefix(root).map_or_else(
		|_| rotor_support::gopath::to_slash(path).into_owned(),
		|relative| rotor_support::gopath::to_slash(&relative.to_string_lossy()).into_owned(),
	)
}

/// `task7ClassTransformerTree`: the Luau output and the normalized JSON artifacts.
fn task7_class_transformer_tree(root: &Path) -> BTreeMap<String, String> {
	let mut result: BTreeMap<String, String> = luau_files(&root.join("out"))
		.into_iter()
		.map(|(relative, data)| (format!("out/{relative}"), data))
		.collect();
	let mut artifact_names = vec!["flamework.build".to_owned()];
	if let Ok(entries) = std::fs::read_dir(root.join("include/flamework")) {
		for entry in entries {
			let path = entry.expect("directory entry").path();
			if path.extension().is_some_and(|extension| extension == "json") {
				artifact_names.push(task7_class_relative_path(root, &path.to_string_lossy()));
			}
		}
	}
	artifact_names.sort();
	for name in artifact_names {
		let data = std::fs::read(root.join(&name)).expect("read transformer artifact");
		let mut artifact: serde_json::Value = serde_json::from_slice(&data).expect("decode transformer artifact");
		let globs = name.ends_with("globs.json");
		normalize_task7_json(&mut artifact, false, globs);
		result.insert(
			name,
			serde_json::to_string(&artifact).expect("canonicalize transformer artifact"),
		);
	}
	result
}

/// `task7ClassTreeHash`
fn task7_class_tree_hash(tree: &BTreeMap<String, String>) -> String {
	let mut hash = Sha256::new();
	for (name, data) in tree {
		hash.update(name.as_bytes());
		hash.update([0]);
		hash.update(data.as_bytes());
		hash.update([0]);
	}
	super::oracle_corpus::hex(&hash.finalize())
}

/// `task7ClassTreeDiff`
fn task7_class_tree_diff(oracle: &BTreeMap<String, String>, native: &BTreeMap<String, String>) -> String {
	let names: std::collections::BTreeSet<&String> = oracle.keys().chain(native.keys()).collect();
	let empty = String::new();
	let mut differences = Vec::new();
	for name in names {
		let (left, right) = (oracle.get(name).unwrap_or(&empty), native.get(name).unwrap_or(&empty));
		if left != right {
			differences.push(format!("{name}\noracle={left}\nnative={right}"));
		}
	}
	differences.join("\n")
}
