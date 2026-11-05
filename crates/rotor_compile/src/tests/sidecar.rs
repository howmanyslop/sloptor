//! `sidecar_test.go`: the transformer-plugin sidecar end to end. Node must be on PATH.

use std::io::Write as _;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{Temp, large_stack, repo_root, write_project};
use crate::compile::Error;
use crate::output::{BuildResult, build_project_with_options, emit_declaration_texts};
use crate::project::{ProjectOptions, new_project_program_with_options, project_source_files};
use crate::sidecar::{SIDECAR_RESPONSE_TIMEOUT_ENV, apply_transformer_sidecar_with_plugins_and_host};
use crate::sidecar::{
	SIDECAR_STDERR_LINE_LIMIT, SIDECAR_STDERR_TRUNCATED, SidecarSession, SidecarStderrTail, close_sidecar_sessions,
	set_test_env, sidecar_env, sidecar_response_timeout, sidecar_slot_count, spawn_sidecar_session,
};
use crate::transformer_host::{TransformerRequest, TransformerResponse};
use crate::transformer_host_sidecar::SidecarTransformerHost;

/// `repoSidecarDir`: tools/sidecar in this checkout. Synthetic plugin fixtures have no
/// typescript of their own, so the worker falls back to the sidecar's pinned install.
pub fn repo_sidecar_dir() -> String {
	let dir = repo_root().join("tools/sidecar");
	assert!(dir.join("main.js").exists(), "repo sidecar missing");
	dir.to_string_lossy().into_owned()
}

/// `setRepoSidecarPath`
pub fn set_repo_sidecar_path() {
	set_test_env("ROTOR_SIDECAR_PATH", &repo_sidecar_dir());
}

/// Closes the sidecar sessions when dropped, as Go's `t.Cleanup(closeSidecarSessions)`.
pub struct SessionsGuard;

impl SessionsGuard {
	pub fn new() -> Self {
		close_sidecar_sessions();
		Self
	}
}

impl Drop for SessionsGuard {
	fn drop(&mut self) {
		close_sidecar_sessions();
	}
}

pub const PREFIX_STRING_PLUGIN: &str = r#"const ts = require("typescript");

module.exports = function programTransformer(program, config, helpers) {
	if (!program.getTypeChecker()) {
		throw new Error("missing program checker");
	}
	if (!helpers || helpers.ts !== ts) {
		throw new Error("missing ts helper");
	}

	return (context) => {
		const visit = (node) => {
			if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) {
				return node;
			}
			if (ts.isStringLiteral(node)) {
				return ts.factory.createStringLiteral(config.prefix + ":" + node.text);
			}
			return ts.visitEachChild(node, visit, context);
		};
		return (sourceFile) => ts.visitNode(sourceFile, visit);
	};
};
"#;

const COUNTING_PLUGIN: &str = r#"let buildCount = 0;

module.exports = function (program, config, helpers) {
	const ts = helpers.ts;
	buildCount += 1;
	return (context) => (sourceFile) => {
		const visit = (node) => {
			if (ts.isStringLiteral(node) && node.text === "BUILD_COUNT") {
				return ts.factory.createStringLiteral("build:" + buildCount);
			}
			return ts.visitEachChild(node, visit, context);
		};
		return ts.visitNode(sourceFile, visit);
	};
};
"#;

/// The compiler options every plugin fixture shares, through `outDir`.
pub const PLUGIN_COMPILER_OPTIONS: &str = r#""allowSyntheticDefaultImports": true,
		"module": "CommonJS",
		"moduleResolution": "Node",
		"noLib": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"types": [],
		"typeRoots": ["node_modules/@rbxts"],
		"rootDir": "src",
		"outDir": "out""#;

/// A plugin config with `plugins` as its plugin list.
pub fn plugin_config(plugins: &str) -> String {
	format!(
		"{{\n\t\"compilerOptions\": {{\n\t\t{PLUGIN_COMPILER_OPTIONS},\n\t\t\"plugins\": {plugins}\n\t}},\n\t\"include\": [\"src\"]\n}}"
	)
}

/// `sidecarSubsetConfig`
pub fn sidecar_subset_config() -> String {
	format!(
		"{{\n\t\"compilerOptions\": {{\n\t\t\"declaration\": true,\n\t\t{PLUGIN_COMPILER_OPTIONS},\n\t\t\"plugins\": [\n\t\t\t{{ \"transform\": \"./plugins/prefix-string.js\", \"prefix\": \"prefix\" }},\n\t\t\t{{ \"transform\": \"./plugins/prefix-string.js\", \"prefix\": \"suffix\", \"after\": true }}\n\t\t]\n\t}},\n\t\"include\": [\"src\"]\n}}"
	)
}

/// `symlinkFixtureTypeScript`
pub fn symlink_fixture_typescript(dir: &str) {
	let fixture = repo_root().join("testdata/transformers/project/node_modules/typescript");
	assert!(
		fixture.join("package.json").exists(),
		"fixture TypeScript missing at {}",
		fixture.display()
	);
	let target = Path::new(dir).join("node_modules/typescript");
	if std::fs::symlink_metadata(&target).is_ok() {
		return;
	}
	std::fs::create_dir_all(Path::new(dir).join("node_modules")).expect("mkdir");
	#[cfg(unix)]
	std::os::unix::fs::symlink(&fixture, &target).expect("symlink");
	#[cfg(windows)]
	std::os::windows::fs::symlink_dir(&fixture, &target).expect("symlink");
}

/// `writeSidecarPluginFixture`
pub fn write_sidecar_plugin_fixture(dir: &str, base_ts_config: &str, root_ts_config: &str) {
	symlink_fixture_typescript(dir);
	std::fs::create_dir_all(Path::new(dir).join("plugins")).expect("mkdir");
	std::fs::write(Path::new(dir).join("plugins/prefix-string.js"), PREFIX_STRING_PLUGIN).expect("write");
	if !base_ts_config.is_empty() {
		std::fs::write(Path::new(dir).join("tsconfig.base.json"), base_ts_config).expect("write");
	}
	if !root_ts_config.is_empty() {
		std::fs::write(Path::new(dir).join("tsconfig.json"), root_ts_config).expect("write");
	}
}

/// `BuildProjectWithOptions` as Go's `(result, diags, err)`.
pub fn build(dir: &str, opts: &ProjectOptions) -> (Option<BuildResult>, Vec<String>, Option<Error>) {
	large_stack(|| match build_project_with_options(dir, opts) {
		Ok(result) => (Some(result), Vec::new(), None),
		Err(failure) => (
			failure.result.map(|result| *result),
			failure.failure.messages(),
			Some(failure.failure.error),
		),
	})
}

/// A build that must succeed.
pub fn must_build(dir: &str, opts: &ProjectOptions, what: &str) -> BuildResult {
	let (result, diags, error) = build(dir, opts);
	if let Some(error) = error {
		panic!("{what}: {error} (diags: {diags:?})");
	}
	assert!(diags.is_empty(), "{what}: diagnostics: {diags:?}");
	result.expect("a result")
}

/// An output as text.
pub fn output_text(result: &BuildResult, key: &str) -> String {
	result
		.outputs
		.get(key)
		.map(|bytes| String::from_utf8_lossy(bytes).into_owned())
		.unwrap_or_default()
}

/// `envValue`
pub fn env_value(env: &[(String, String)], name: &str) -> String {
	env.iter()
		.find(|(key, _)| key == name)
		.map(|(_, value)| value.clone())
		.unwrap_or_default()
}

/// `session.roundTrip`: one request and its decoded response.
pub fn round_trip(
	session: &mut SidecarSession,
	request: &TransformerRequest,
	timeout: Duration,
) -> Result<TransformerResponse, Error> {
	let payload = request.to_json();
	let line = session
		.write_and_read(None, Instant::now() + timeout, payload.as_bytes())
		.map_err(|(error, _)| error)?;
	TransformerResponse::from_json(&String::from_utf8_lossy(&line)).map_err(Error::message)
}

fn write_main(temp: &Temp, text: &str) {
	temp.write("src/main.ts", text);
}

#[test]
fn build_project_transformer_plugin_sidecar() {
	set_repo_sidecar_path();
	let _sessions = SessionsGuard::new();
	let temp = write_project("@scope/plugin-fixture", "");
	let dir = temp.dir();
	write_sidecar_plugin_fixture(
		&dir,
		&format!(
			"{{\n\t\"compilerOptions\": {{\n\t\t{PLUGIN_COMPILER_OPTIONS},\n\t\t\"plugins\": [\n\t\t\t{{\n\t\t\t\t\"transform\": \"./plugins/prefix-string.js\",\n\t\t\t\t\"prefix\": \"plugin\"\n\t\t\t}}\n\t\t]\n\t}}\n}}"
		),
		&format!(
			"{{\n\t\"extends\": \"./tsconfig.base.json\",\n\t\"compilerOptions\": {{\n\t\t{PLUGIN_COMPILER_OPTIONS}\n\t}},\n\t\"include\": [\"src\"]\n}}"
		),
	);
	write_main(&temp, "export const phase = \"start\";\n");
	let result = must_build(&dir, &ProjectOptions::default(), "BuildProjectWithOptions");
	let got = output_text(&result, "out/main.luau");
	assert!(
		got.contains(r#"local phase = "plugin:start""#),
		"out/main.luau missing transformed string:\n{got}"
	);
}

#[test]
fn build_project_without_plugins_does_not_require_node() {
	let temp = write_project("@scope/no-plugin-fixture", "");
	set_test_env("ROTOR_NODE_PATH", &temp.join("missing-node").to_string_lossy());
	must_build(&temp.dir(), &ProjectOptions::default(), "BuildProjectWithOptions");
}

#[test]
fn build_project_native_flamework_does_not_require_node_or_sidecar() {
	// Given: an enabled native Flamework project with no usable Node or sidecar.
	let temp = write_project("@scope/native-flamework-no-node", "");
	temp.write("rotor.toml", "[flamework]\n");
	set_test_env("ROTOR_NODE_PATH", &temp.join("missing-node").to_string_lossy());
	set_test_env("ROTOR_SIDECAR_PATH", &temp.join("must-not-be-read").to_string_lossy());
	let _sessions = SessionsGuard::new();

	// When: the real compiler builds the project.
	let (result, diags, error) = build(&temp.dir(), &ProjectOptions::default());
	// Then: Luau is emitted and no Node sidecar session exists.
	assert!(error.is_none(), "BuildProjectWithOptions: {error:?} (diags: {diags:?})");
	let result = result.expect("a result");
	assert!(
		!output_text(&result, "out/main.luau").is_empty(),
		"BuildProjectWithOptions result = {:?}, want emitted Luau",
		result.outputs.keys()
	);
	let sessions = sidecar_slot_count();
	assert_eq!(
		sessions, 0,
		"native Flamework build spawned {sessions} sidecar session(s)"
	);
}

#[test]
fn apply_transformer_sidecar_with_plugins_runs_prefix_and_suffix_once() {
	// Given: a project with two external transforms and its own TypeScript runtime.
	set_repo_sidecar_path();
	let _sessions = SessionsGuard::new();
	let temp = write_project("@scope/filtered-sidecar", "");
	let dir = temp.dir();
	symlink_fixture_typescript(&dir);
	write_sidecar_plugin_fixture(&dir, "", &sidecar_subset_config());
	write_main(&temp, "export const phase = \"start\";\n");
	large_stack(|| {
		let (abs_dir, program) = new_project_program_with_options(&dir, "", &ProjectOptions::default())
			.unwrap_or_else(|failure| panic!("newProjectProgram: {}", failure.error));
		let prefix = vec![r#"{"transform":"./plugins/prefix-string.js","prefix":"prefix"}"#.to_owned()];
		let suffix = vec![r#"{"transform":"./plugins/prefix-string.js","prefix":"suffix","after":true}"#.to_owned()];
		// When: the prefix and suffix run as source-only stages.
		let apply = |program, plugins: &[String]| {
			apply_transformer_sidecar_with_plugins_and_host(
				None,
				&SidecarTransformerHost,
				&abs_dir,
				program,
				&project_source_files(program),
				&std::collections::BTreeMap::default(),
				Some(plugins),
				None,
				"",
				None,
			)
		};
		let first = apply(program, &prefix)
			.unwrap_or_else(|failure| panic!("prefix transform: {} (diags: {:?})", failure.error, failure.messages()));
		let second = apply(first.program, &suffix)
			.unwrap_or_else(|failure| panic!("suffix transform: {} (diags: {:?})", failure.error, failure.messages()));
		// Then: each subset is applied in order.
		assert!(
			!std::ptr::eq(first.program, program),
			"prefix transform did not produce a new program"
		);
		let source = second.program.get_source_file(&format!("{abs_dir}/src/main.ts"));
		let text = source.map(|source| source.text().to_owned()).unwrap_or_default();
		assert!(
			text.contains(r#""suffix:prefix:start""#),
			"filtered transforms produced {text:?}, want suffix:prefix:start"
		);
	});
}

/// Declarations are emitted natively from the ORIGINAL program, so a source transformer must
/// never reach them.
#[test]
fn apply_transformer_sidecar_leaves_declarations_to_native_emit() {
	set_repo_sidecar_path();
	let _sessions = SessionsGuard::new();
	let temp = write_project("@scope/combined-sidecar", "");
	let dir = temp.dir();
	write_sidecar_plugin_fixture(&dir, "", &sidecar_subset_config());
	write_main(&temp, "export const phase = \"start\";\n");
	large_stack(|| {
		let (abs_dir, program) = new_project_program_with_options(&dir, "", &ProjectOptions::default())
			.unwrap_or_else(|failure| panic!("newProjectProgram: {}", failure.error));
		let combined = apply_transformer_sidecar_with_plugins_and_host(
			None,
			&SidecarTransformerHost,
			&abs_dir,
			program,
			&project_source_files(program),
			&std::collections::BTreeMap::default(),
			None,
			None,
			"",
			None,
		)
		.unwrap_or_else(|failure| {
			panic!(
				"applyTransformerSidecar: {} (diags: {:?})",
				failure.error,
				failure.messages()
			)
		});
		let source = combined
			.program
			.get_source_file(&format!("{abs_dir}/src/main.ts"))
			.expect("combined source missing");
		assert!(
			source.text().contains(r#""suffix:prefix:start""#),
			"combined source = {:?}, want suffix:prefix:start",
			source.text()
		);
		let declarations = emit_declaration_texts(program, &project_source_files(program))
			.unwrap_or_else(|error| panic!("emitDeclarationTexts: {error}"));
		assert_eq!(declarations.len(), 1, "declarations");
		assert_eq!(
			declarations[0].text, "export declare const phase = \"start\";\n",
			"declaration"
		);
	});
}

#[test]
fn sidecar_round_trip_times_out() {
	// Given: a worker that never answers.
	set_test_env(SIDECAR_RESPONSE_TIMEOUT_ENV, "10ms");
	let timeout = sidecar_response_timeout().unwrap_or_else(|error| panic!("{error}"));
	let sidecar = Temp::new();
	// It records the end of its input, which Go's close sends before any kill.
	sidecar.write(
		"main.js",
		"const fs = require(\"node:fs\");\nconst path = require(\"node:path\");\nprocess.stdin.resume();\nprocess.stdin.on(\"end\", () => {\n\tfs.writeFileSync(path.join(__dirname, \"stdin-closed\"), \"\");\n\tprocess.exit(0);\n});\nsetInterval(() => {}, 1 << 30);\n",
	);
	let project = Temp::new();
	let mut session = spawn_sidecar_session(&project.dir(), &sidecar.dir()).unwrap_or_else(|error| panic!("{error}"));
	// When
	let error = round_trip(&mut session, &TransformerRequest::default(), timeout)
		.err()
		.map(|error| error.to_string());
	// Then
	assert!(
		error
			.as_deref()
			.is_some_and(|error| error.contains("response timed out")),
		"roundTrip error = {error:?}, want timeout context"
	);
	assert!(session.dead, "timed-out sidecar session remains reusable");
	session.close();
	assert!(
		sidecar.join("stdin-closed").exists(),
		"the timed-out worker's stdin was not closed before the kill"
	);
}

#[test]
fn build_project_transformer_plugin_requires_node() {
	set_repo_sidecar_path();
	let temp = write_project("@scope/plugin-node-fixture", "");
	let dir = temp.dir();
	write_sidecar_plugin_fixture(
		&dir,
		"",
		&plugin_config(
			"[\n\t\t\t{\n\t\t\t\t\"transform\": \"./plugins/prefix-string.js\",\n\t\t\t\t\"prefix\": \"plugin\"\n\t\t\t}\n\t\t]",
		),
	);
	set_test_env("ROTOR_NODE_PATH", &temp.join("missing-node").to_string_lossy());
	let (_, diags, error) = build(&dir, &ProjectOptions::default());
	assert!(error.is_some(), "expected missing-node error");
	assert!(
		diags.len() == 1 && diags[0].contains("node executable not found"),
		"diags = {diags:?}, want missing-node diagnostic"
	);
	assert!(
		diags[0].contains("./plugins/prefix-string.js"),
		"diags = {diags:?}, want the remaining external transformer"
	);
}

#[test]
fn transformer_missing_is_error() {
	set_repo_sidecar_path();
	let _sessions = SessionsGuard::new();
	let temp = write_project("@scope/plugin-missing-fixture", "");
	let dir = temp.dir();
	temp.write(
		"tsconfig.json",
		plugin_config("[\n\t\t\t{\n\t\t\t\t\"transform\": \"./plugins/does-not-exist.js\",\n\t\t\t\t\"prefix\": \"plugin\"\n\t\t\t}\n\t\t]"),
	);
	write_main(&temp, "export const phase = \"start\";\n");
	let (result, diags, error) = build(&dir, &ProjectOptions::default());
	assert!(
		error.is_some(),
		"BuildProjectWithOptions succeeded with a missing transformer"
	);
	assert!(result.is_none(), "result = {result:?}, want nil");
	assert_eq!(
		diags.len(),
		1,
		"diagnostics = {diags:?}, want one missing-transformer error"
	);
	assert!(
		diags[0].contains("Transformer `./plugins/does-not-exist.js` failed to load!"),
		"diagnostic = {:?}, want fork missing-transformer text",
		diags[0]
	);
	assert!(
		diags[0].contains("Suggestion: Did you forget to install the package, or to build it?"),
		"diagnostic = {:?}, want fork suggestion text",
		diags[0]
	);
}

#[test]
fn transformer_warm_session() {
	set_repo_sidecar_path();
	let _sessions = SessionsGuard::new();
	let temp = write_project("@scope/plugin-warm-fixture", "");
	let dir = temp.dir();
	symlink_fixture_typescript(&dir);
	temp.write("plugins/counting.js", COUNTING_PLUGIN);
	temp.write(
		"tsconfig.json",
		plugin_config("[{ \"transform\": \"./plugins/counting.js\" }]"),
	);
	write_main(
		&temp,
		"export const tag = \"BUILD_COUNT\";\nexport const phase = \"first\";\n",
	);
	let result = must_build(&dir, &ProjectOptions::default(), "first build");
	let got = output_text(&result, "out/main.luau");
	assert!(
		got.contains(r#"local tag = "build:1""#) && got.contains(r#"local phase = "first""#),
		"first build output unexpected:\n{got}"
	);
	write_main(
		&temp,
		"export const tag = \"BUILD_COUNT\";\nexport const phase = \"second\";\n",
	);
	let result = must_build(&dir, &ProjectOptions::default(), "second build");
	let got = output_text(&result, "out/main.luau");
	assert!(
		got.contains(r#"local tag = "build:2""#),
		"second build did not reuse a warm sidecar process:\n{got}"
	);
	assert!(
		got.contains(r#"local phase = "second""#),
		"warm sidecar served a stale snapshot (changedFiles overlay broken):\n{got}"
	);
}

#[test]
fn sidecar_env_node_path_reaches_the_workspace_root() {
	// Given
	set_test_env("NODE_PATH", "");
	let temp = Temp::new();
	let root = temp.dir();
	let project_dir = rotor_support::gopath::join(&[&root, "packages", "topbar"]);
	let sidecar_dir = rotor_support::gopath::join(&[&root, "sidecar"]);
	for dir in [
		rotor_support::gopath::join(&[&root, "node_modules"]),
		rotor_support::gopath::join(&[&project_dir, "node_modules"]),
	] {
		std::fs::create_dir_all(dir).expect("mkdir");
	}
	// When
	let env = sidecar_env(&project_dir, &sidecar_dir);
	// Then
	let separator = if cfg!(windows) { ";" } else { ":" };
	let want = [
		rotor_support::gopath::join(&[&project_dir, "node_modules"]),
		rotor_support::gopath::join(&[&root, "node_modules"]),
	]
	.join(separator);
	assert_eq!(env_value(&env, "NODE_PATH"), want, "NODE_PATH");
}

#[test]
fn sidecar_stderr_tail_caps_long_lines_and_keeps_draining() {
	// Given: a worker that writes 4 MiB of stderr with no newline, then ends that line and
	// writes another.
	let (reader, mut writer) = std::io::pipe().expect("a pipe");
	let tail: Arc<SidecarStderrTail> = SidecarStderrTail::spawn(reader);
	let long = "x".repeat(4 * 1024 * 1024);
	// When: the output is written. A pipe write blocks until it is read.
	let (written, written_rx) = std::sync::mpsc::channel();
	std::thread::spawn(move || {
		let result = writer
			.write_all(long.as_bytes())
			.and_then(|()| writer.write_all(b"\nafter\n"));
		let _ = written.send(result);
	});
	// Then: the reader drains everything, stores the long line cut to the cap with a marker,
	// and still records the next line.
	written_rx
		.recv_timeout(Duration::from_secs(5))
		.expect("stderr reader stopped draining a long line")
		.expect("the write");
	let want = [
		format!("{}{SIDECAR_STDERR_TRUNCATED}", "x".repeat(SIDECAR_STDERR_LINE_LIMIT)),
		"after".to_owned(),
	];
	let deadline = Instant::now() + Duration::from_secs(5);
	loop {
		let (got, pending) = tail.snapshot();
		if got == want && pending == want.len() {
			return;
		}
		assert!(
			Instant::now() < deadline,
			"stored line lengths = {:?}, want {} and {}",
			got.iter().map(String::len).collect::<Vec<_>>(),
			want[0].len(),
			want[1].len()
		);
		std::thread::sleep(Duration::from_millis(1));
	}
}
