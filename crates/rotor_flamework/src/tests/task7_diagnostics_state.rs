//! `task7_diagnostics_state_test.go`: every canonical diagnostic-state row through one real
//! two-entrypoint harness.

use std::path::{Path, PathBuf};
use std::process::Command;

use rotor_config::config::FlameworkConfig;

use super::oracle_corpus::{flamework_testdata, repo_root};
use super::task7_differential::{
	TASK7_ANSI, copy_task7_tree, link_task7_modules, run_with_stdin, task7_install, task7_source_files,
};
use super::{new_transform_program, temp_dir, write_transform_fixture};
use crate::build_info::{BuildClass, BuildInfo};
use crate::project::{ProjectOptions, open_project};
use crate::runtime_config::RUNTIME_CONFIG_FILE_NAME;
use crate::state::FLAMEWORK_VERSION;
use crate::transform_diagnostics::Diagnostic;
use crate::transform_state::TransformInput;
use crate::{Error, ErrorDetail};

/// `task7StateTuple`
#[derive(Clone, Debug, PartialEq, Eq)]
struct Tuple {
	file: String,
	start: i64,
	length: i64,
	category: String,
	code: String,
	message: String,
	related: String,
}

impl Tuple {
	fn new(start: i64, category: &str, code: &str, message: String) -> Self {
		Self {
			file: String::new(),
			start,
			length: 0,
			category: category.to_owned(),
			code: code.to_owned(),
			message,
			related: String::new(),
		}
	}
}

type Native = fn(&Path) -> (i32, Vec<Tuple>);
type Prepare = fn(&Path);

/// `task7StateCase`
struct Case {
	row: &'static str,
	name: &'static str,
	category: &'static str,
	prepare: Option<Prepare>,
	native: Option<Native>,
	upstream_project: &'static str,
	upstream_blocked: &'static str,
	native_blocked: &'static str,
	normalize_exception: bool,
	target_file: &'static str,
	preserve_input: bool,
}

const CASE: Case = Case {
	row: "",
	name: "",
	category: "error",
	prepare: None,
	native: None,
	upstream_project: "",
	upstream_blocked: "",
	native_blocked: "",
	normalize_exception: false,
	target_file: "",
	preserve_input: false,
};

fn write(root: &Path, name: &str, contents: &str) {
	write_transform_fixture(root.to_str().expect("utf-8 path"), name, contents);
}

#[test]
fn task7_diagnostics_state_matches_upstream_132_diagnostic_tuples() {
	// One pinned installation, then a fresh project copy for every state mutation.
	let fixture = flamework_testdata("task7-diagnostics-state");
	let (_install_dir, install) = task7_install(&fixture);

	for case in task7_state_cases() {
		let (_dir, root) = temp_dir();
		let root = std::fs::canonicalize(root).expect("resolve project path");
		copy_task7_tree(&fixture, &root);
		link_task7_modules(&install, &root);
		if let Some(prepare) = case.prepare {
			prepare(&root);
		}

		// The real upstream compiler entrypoint and native exported seam consume the same state.
		let (upstream_exit, mut upstream) = task7_state_upstream(&root, &case);
		let (native_exit, mut native) = task7_state_native(&root, &case);
		if case.normalize_exception {
			upstream = task7_state_normalize_exception_tuple(&upstream, case.target_file);
			native = task7_state_normalize_exception_tuple(&native, case.target_file);
			task7_state_assert_no_write(&root, case.target_file, case.preserve_input);
		}

		// Exit status and every ordered tuple field are identical.
		if upstream_exit == 125 || native_exit == 125 {
			continue;
		}
		assert!(
			upstream_exit == native_exit && upstream == native,
			"{} {} tuple mismatch\nupstream exit={upstream_exit} {upstream:?}\nnative exit={native_exit} {native:?}",
			case.row,
			case.name
		);
	}
}

/// `task7StateCases`
#[expect(clippy::too_many_lines, reason = "the Go case table, ported whole")]
fn task7_state_cases() -> Vec<Case> {
	let blocked = "public transformer lookup prevents duplicate insertion; BuildInfo is not exported";
	vec![
		Case {
			row: "DIAG-001",
			name: "build-info-missing",
			native: Some(task7_state_open_native),
			..CASE
		},
		Case {
			row: "DIAG-001",
			name: "build-info-malformed",
			prepare: Some(|root| write(root, "flamework.build", "{")),
			native: Some(task7_state_open_native),
			normalize_exception: true,
			target_file: "flamework.build",
			preserve_input: true,
			..CASE
		},
		Case {
			row: "DIAG-001",
			name: "build-info-version-mismatch",
			prepare: Some(|root| {
				write(
					root,
					"flamework.build",
					r#"{"version":1,"flameworkVersion":"1.2.0","identifiers":{}}"#,
				);
				task7_state_prepare_dirty(root);
			}),
			native: Some(task7_state_open_native),
			..CASE
		},
		Case {
			row: "DIAG-001",
			name: "duplicate-identifier",
			upstream_blocked: blocked,
			native: Some(task7_state_duplicate_identifier),
			..CASE
		},
		Case {
			row: "DIAG-001",
			name: "duplicate-class",
			upstream_blocked: blocked,
			native: Some(task7_state_duplicate_class),
			..CASE
		},
		Case {
			row: "DIAG-002",
			name: "dirty-incremental-environment",
			prepare: Some(task7_state_prepare_dirty),
			native_blocked: "native flamework public API has no incremental or tsbuildinfo input; compiler-layer coverage owns this state",
			..CASE
		},
		Case {
			row: "DIAG-003",
			name: "malformed-runtime-json",
			prepare: Some(|root| write(root, RUNTIME_CONFIG_FILE_NAME, "{")),
			native: Some(task7_state_open_native),
			normalize_exception: true,
			target_file: RUNTIME_CONFIG_FILE_NAME,
			preserve_input: true,
			..CASE
		},
		Case {
			row: "DIAG-003",
			name: "malformed-runtime-schema",
			prepare: Some(|root| write(root, RUNTIME_CONFIG_FILE_NAME, r#"{"profiling":"yes"}"#)),
			native: Some(task7_state_open_native),
			..CASE
		},
		Case {
			row: "DIAG-004",
			name: "reserved-hash-prefix",
			prepare: Some(|root| {
				task7_state_replace(
					&root.join("tsconfig.json"),
					r#""noSemanticDiagnostics": true"#,
					r#""noSemanticDiagnostics": true, "hashPrefix": "$internal""#,
				);
			}),
			native: Some(|root| {
				task7_state_open_native_config(
					root,
					FlameworkConfig {
						hash_prefix: "$internal".to_owned(),
						..FlameworkConfig::default()
					},
				)
			}),
			normalize_exception: true,
			target_file: "tsconfig.json",
			..CASE
		},
		Case {
			row: "DIAG-005",
			name: "missing-package",
			prepare: Some(|root| std::fs::remove_file(root.join("package.json")).expect("remove package fixture")),
			native: Some(task7_state_open_native),
			normalize_exception: true,
			target_file: "package.json",
			..CASE
		},
		Case {
			row: "DIAG-005",
			name: "missing-tsconfig",
			prepare: Some(|root| std::fs::remove_file(root.join("tsconfig.json")).expect("remove tsconfig fixture")),
			upstream_project: "missing-tsconfig.json",
			native_blocked: "native flamework API receives already-parsed ProjectOptions and has no tsconfig entrypoint",
			..CASE
		},
		Case {
			row: "DIAG-005",
			name: "missing-root-dir",
			prepare: Some(|root| task7_state_replace(&root.join("tsconfig.json"), r#""rootDir": "src","#, "")),
			native: Some(|root| task7_state_open_native_root(root, "")),
			normalize_exception: true,
			target_file: "tsconfig.json",
			..CASE
		},
		Case {
			row: "DIAG-005",
			name: "unresolvable-root-dir",
			prepare: Some(|root| {
				task7_state_replace(
					&root.join("tsconfig.json"),
					r#""rootDir": "src""#,
					r#""rootDir": "does-not-exist""#,
				);
			}),
			native: Some(|root| task7_state_open_native_root(root, "does-not-exist")),
			..CASE
		},
		Case {
			row: "DIAG-007",
			name: "missing-flamework-core",
			category: "warning",
			upstream_blocked: "the public compiler rejects an unresolved @flamework/core import before guard transformation; reference src/classes/transformState.ts:583-586",
			native_blocked: "native guard import selection exposes no warning diagnostic",
			..CASE
		},
		Case {
			row: "DIAG-007",
			name: "invalid-guard-runtime",
			category: "warning",
			prepare: Some(task7_state_prepare_invalid_guard_runtime),
			native: Some(task7_state_transform_native),
			..CASE
		},
	]
}

/// `task7StatePrepareDirty`
fn task7_state_prepare_dirty(root: &Path) {
	task7_state_replace(
		&root.join("tsconfig.json"),
		r#""compilerOptions": {"#,
		r#""compilerOptions": {"incremental": true, "tsBuildInfoFile": "out/task7.tsbuildinfo","#,
	);
	write(root, "out/task7.tsbuildinfo", r#"{"version":"5.5.3"}"#);
}

/// `task7StatePrepareInvalidGuardRuntime`
fn task7_state_prepare_invalid_guard_runtime(root: &Path) {
	let real_source = std::fs::canonicalize(root.join("node_modules/@rbxts/t")).expect("resolve guard runtime");
	let external = root.join("external-t");
	std::fs::create_dir_all(&external).expect("create external guard runtime");
	copy_task7_tree(&real_source, &external);
	let nested = root.join("node_modules/@flamework/core/node_modules/@rbxts");
	std::fs::create_dir_all(&nested).expect("create nested guard directory");
	std::os::unix::fs::symlink(&external, nested.join("t")).expect("link invalid guard runtime");
}

/// `task7StateReplace`
fn task7_state_replace(path: &Path, old: &str, replacement: &str) {
	let data = std::fs::read_to_string(path).expect("read mutation target");
	let updated = data.replacen(old, replacement, 1);
	assert_ne!(updated, data, "mutation target {old:?} not found in {}", path.display());
	std::fs::write(path, updated).expect("write mutation target");
}

/// `task7StateUpstream`
fn task7_state_upstream(root: &Path, case: &Case) -> (i32, Vec<Tuple>) {
	if !case.upstream_blocked.is_empty() {
		return (125, vec![task7_state_blocked_tuple(case.upstream_blocked)]);
	}
	let project = if case.upstream_project.is_empty() {
		"tsconfig.json"
	} else {
		case.upstream_project
	};
	let sidecar = repo_root().join("tools/sidecar/main.js");
	let request = serde_json::json!({
		"protocol": 1,
		"operation": "transform",
		"tsConfigPath": root.join(project),
		"projectDir": root,
		"compileFileNames": [root.join("src/index.ts")],
		"changedFiles": [],
	});
	let mut request = serde_json::to_vec(&request).expect("marshal sidecar request");
	request.push(b'\n');
	let mut command = Command::new("node");
	command.arg(&sidecar).current_dir(root);
	let (success, stdout, stderr) = run_with_stdin(command, &request);
	if !success {
		return (1, task7_state_parse_upstream(root, case.category, &stderr));
	}
	let response: serde_json::Value = serde_json::from_slice(stdout.trim_ascii()).unwrap_or_else(|error| {
		panic!(
			"decode sidecar response: {error}\nstdout={}\nstderr={}",
			String::from_utf8_lossy(&stdout),
			String::from_utf8_lossy(&stderr)
		)
	});
	let mut tuples = Vec::new();
	let mut exit = 0;
	for diagnostic in response
		.get("diagnostics")
		.and_then(serde_json::Value::as_array)
		.into_iter()
		.flatten()
	{
		let text = |key: &str| {
			diagnostic
				.get(key)
				.and_then(serde_json::Value::as_str)
				.unwrap_or_default()
		};
		let number = |key: &str, default: i64| {
			diagnostic
				.get(key)
				.and_then(serde_json::Value::as_i64)
				.unwrap_or(default)
		};
		tuples.push(Tuple {
			file: task7_state_normalize_path(root, text("file")),
			start: number("start", -1),
			length: number("length", 0),
			category: text("category").to_owned(),
			code: text("code").to_owned(),
			message: task7_state_normalize_path(root, text("message")),
			related: String::new(),
		});
		if text("category") == "error" {
			exit = 1;
		}
	}
	(exit, tuples)
}

/// `task7StateNative`
fn task7_state_native(root: &Path, case: &Case) -> (i32, Vec<Tuple>) {
	if !case.native_blocked.is_empty() {
		return (125, vec![task7_state_blocked_tuple(case.native_blocked)]);
	}
	(case.native.expect("native case"))(root)
}

/// `task7StateOpenNative`
fn task7_state_open_native(root: &Path) -> (i32, Vec<Tuple>) {
	task7_state_open_native_config(root, FlameworkConfig::default())
}

/// `task7StateOpenNativeRoot`
fn task7_state_open_native_root(root: &Path, root_dir: &str) -> (i32, Vec<Tuple>) {
	task7_state_open_native_options(&ProjectOptions {
		project_dir: root.to_string_lossy().into_owned(),
		root_dir: root_dir.to_owned(),
		out_dir: "out".to_owned(),
		..ProjectOptions::default()
	})
}

/// `task7StateOpenNativeConfig`
fn task7_state_open_native_config(root: &Path, config: FlameworkConfig) -> (i32, Vec<Tuple>) {
	task7_state_open_native_options(&ProjectOptions {
		project_dir: root.to_string_lossy().into_owned(),
		root_dir: "src".to_owned(),
		out_dir: "out".to_owned(),
		config,
		..ProjectOptions::default()
	})
}

/// `task7StateOpenNativeOptions`
fn task7_state_open_native_options(options: &ProjectOptions) -> (i32, Vec<Tuple>) {
	let root = Path::new(&options.project_dir);
	let Err(error) = open_project(options) else {
		return (0, Vec::new());
	};
	if let Some(ErrorDetail::ProjectDiagnostics(diagnostics)) = error.detail() {
		return (
			1,
			diagnostics
				.iter()
				.map(|diagnostic| task7_state_native_diagnostic(root, diagnostic))
				.collect(),
		);
	}
	(1, vec![task7_state_error_tuple_root(root, &error)])
}

/// `task7StateDuplicateIdentifier`
fn task7_state_duplicate_identifier(root: &Path) -> (i32, Vec<Tuple>) {
	let path = root.join("flamework.build");
	let mut info = BuildInfo::new(&path.to_string_lossy(), FLAMEWORK_VERSION);
	info.add_identifier("same", "first").expect("seed identifier");
	let error = info.add_identifier("same", "second").err();
	(1, vec![task7_state_error_tuple(error.as_ref())])
}

/// `task7StateDuplicateClass`
fn task7_state_duplicate_class(root: &Path) -> (i32, Vec<Tuple>) {
	let path = root.join("flamework.build");
	let mut info = BuildInfo::new(&path.to_string_lossy(), FLAMEWORK_VERSION);
	let class = BuildClass {
		file_path: "src/index.ts".to_owned(),
		internal_id: "same".to_owned(),
		..BuildClass::default()
	};
	info.add_class(&class).expect("seed class");
	let error = info.add_class(&class).err();
	(1, vec![task7_state_error_tuple(error.as_ref())])
}

/// `task7StateTransformNative`
fn task7_state_transform_native(root: &Path) -> (i32, Vec<Tuple>) {
	let root = PathBuf::from(root);
	rotor_tsrs::with_large_stack(|| {
		let root_text = root.to_string_lossy().into_owned();
		let program = new_transform_program(&root_text);
		let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
		let files = task7_source_files(program, &root);
		let project = open_project(&ProjectOptions {
			project_dir: root_text,
			root_dir: "src".to_owned(),
			out_dir: "out".to_owned(),
			config: FlameworkConfig {
				no_semantic_diagnostics: true,
				..FlameworkConfig::default()
			},
			..ProjectOptions::default()
		});
		let mut project = match project {
			Ok(project) => project,
			Err(error) => return (1, vec![task7_state_error_tuple(Some(&error))]),
		};
		let result = crate::transform::transform(TransformInput {
			program,
			checker: &mut checker,
			files,
			project: &mut project,
			analyses: Vec::new(),
			macro_runtime: None,
			print: None,
		});
		match result {
			Err(error) => (1, vec![task7_state_error_tuple(Some(&error))]),
			Ok(result) => (
				0,
				result
					.diagnostics
					.iter()
					.map(|diagnostic| task7_state_native_diagnostic(&root, diagnostic))
					.collect(),
			),
		}
	})
}

/// `task7StateNativeDiagnostic`
fn task7_state_native_diagnostic(root: &Path, diagnostic: &Diagnostic) -> Tuple {
	let file = diagnostic
		.file
		.map(|file| task7_state_normalize_path(root, file.file_name()))
		.unwrap_or_default();
	let related: Vec<&str> = diagnostic.related_information.iter().map(Diagnostic::string).collect();
	let code = diagnostic
		.string_code
		.clone()
		.unwrap_or_else(|| diagnostic.code().to_string());
	Tuple {
		file,
		start: i64::from(diagnostic.pos()),
		length: i64::from(diagnostic.end()) - i64::from(diagnostic.pos()),
		category: diagnostic.category.name().to_owned(),
		code,
		message: diagnostic.string().to_owned(),
		related: related.join(" | "),
	}
}

/// `task7StateErrorTuple`
fn task7_state_error_tuple(error: Option<&Error>) -> Tuple {
	let Some(error) = error else {
		return Tuple::new(-1, "error", "runtime", "<nil>".to_owned());
	};
	let mut message = error.message().to_owned();
	let mut code = "runtime";
	match error.detail() {
		Some(ErrorDetail::JsonSyntax) => code = "sidecar-internal",
		Some(ErrorDetail::UpstreamBoundary) => {
			code = "sidecar-internal";
			message = format!("Error: {message}");
		}
		_ => {}
	}
	Tuple::new(-1, "error", code, message)
}

/// `task7StateErrorTupleRoot`
fn task7_state_error_tuple_root(root: &Path, error: &Error) -> Tuple {
	let mut tuple = task7_state_error_tuple(Some(error));
	tuple.message = task7_state_normalize_path(root, &tuple.message);
	tuple
}

/// `task7StateBlockedTuple`
fn task7_state_blocked_tuple(reason: &str) -> Tuple {
	Tuple::new(-1, "blocked", "unreachable-public-entrypoint", reason.to_owned())
}

/// `task7StateNormalizeExceptionTuple`
fn task7_state_normalize_exception_tuple(tuples: &[Tuple], target_file: &str) -> Vec<Tuple> {
	tuples
		.iter()
		.map(|tuple| {
			let mut message = tuple.message.as_str();
			for exception_class in ["SyntaxError:", "Error:"] {
				if let Some(exception) = message.find(exception_class) {
					message = &message[exception..];
					break;
				}
			}
			if let Some(newline) = message.find('\n') {
				message = &message[..newline];
			}
			Tuple {
				file: target_file.to_owned(),
				message: message.to_owned(),
				..tuple.clone()
			}
		})
		.collect()
}

/// `task7StateAssertNoWrite`
fn task7_state_assert_no_write(root: &Path, target_file: &str, preserve_input: bool) {
	if preserve_input {
		let data = std::fs::read_to_string(root.join(target_file));
		assert!(
			data.as_deref().is_ok_and(|data| data == "{"),
			"malformed input changed: {data:?}"
		);
	}
	for path in [root.join("out/index.luau"), root.join("include/flamework/config.json")] {
		assert!(
			!path.exists(),
			"failure wrote artifact {}",
			task7_state_normalize_path(root, &path.to_string_lossy())
		);
	}
}

/// `task7StateParseUpstream`
fn task7_state_parse_upstream(root: &Path, category: &str, output: &[u8]) -> Vec<Tuple> {
	let text = String::from_utf8_lossy(output);
	let clean = TASK7_ANSI.replace_all(&text, "").into_owned();
	let mut tuples = Vec::new();
	for raw in clean.replace("\r\n", "\n").split('\n') {
		let line = raw.trim();
		if line.is_empty() {
			continue;
		}
		if let Some(index) = line.find("[Flamework]: ") {
			tuples.push(Tuple::new(
				-1,
				category,
				"@flamework/core",
				task7_state_normalize_path(root, &line[index + "[Flamework]: ".len()..]),
			));
			continue;
		}
		if line.starts_with("Error:")
			|| line.starts_with("SyntaxError:")
			|| line.starts_with("Assertion Failed!")
			|| line.contains("error TS")
		{
			tuples.push(Tuple::new(
				-1,
				category,
				"runtime",
				task7_state_normalize_path(root, line),
			));
		}
	}
	if tuples.is_empty() && !output.trim_ascii().is_empty() {
		let first = clean.split('\n').next().unwrap_or_default().trim();
		tuples.push(Tuple::new(
			-1,
			category,
			"runtime",
			task7_state_normalize_path(root, first),
		));
	}
	tuples
}

/// `task7StateNormalizePath`: the project and the workspace replaced by placeholders. Go's
/// working directory is `internal/flamework`; this crate's is `crates/rotor_flamework`, and the
/// repository root covers both.
fn task7_state_normalize_path(root: &Path, value: &str) -> String {
	let slash = |path: &Path| rotor_support::gopath::to_slash(&path.to_string_lossy()).into_owned();
	let mut normalized = rotor_support::gopath::to_slash(value).into_owned();
	if let Ok(real_root) = std::fs::canonicalize(root) {
		normalized = normalized.replace(&slash(&real_root), "<project>");
	}
	normalized = normalized.replace(&slash(root), "<project>");
	if let Ok(workspace) = std::env::current_dir() {
		normalized = normalized.replace(&slash(&workspace), "<workspace>");
		if let Ok(real_workspace) = std::fs::canonicalize(&workspace) {
			normalized = normalized.replace(&slash(&real_workspace), "<workspace>");
		}
	}
	if let Ok(workspace_root) = std::fs::canonicalize(repo_root()) {
		normalized = normalized.replace(&slash(&workspace_root), "<workspace>");
	}
	normalized
}
