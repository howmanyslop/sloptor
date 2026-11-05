//! Compiled output run in Lune (the runtime half of Go's `internal/conformance`).
//!
//! Each suite compiles a project with `sloptor` (and, for the pinned suites, with roblox-ts
//! `3.0.0-dev-3106b14` too), builds a place with Rojo, and runs its `TestEZ` specs with
//! `reference/roblox-ts/tests/runTestsWithLune.lua`. Without Rojo or Lune a suite prints why on
//! stderr and passes without running; `ROTOR_ROJO_PATH` and `ROTOR_LUNE_PATH` name the tools when
//! they are not on `PATH`. A ledger row's verification never skips: [`verify_behavioral_test`]
//! fails without the tools.
//!
//! The pinned suites install the upstream compiler and `@rbxts/types` with npm into the user
//! cache once (`<cache>/rotor/compatibility`), or use the `cli.js` `ROTOR_PINNED_RBXTS_CLI` names.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use rotor_compile::ProjectOptions;
use rotor_compile::compile::compile_file_detailed_with_options;
use rotor_compile::output::build_project_with_options;

use super::{
	Output, copy_file, copy_tree, copy_tree_filtered, has_extension, read_tree, repo_root, run, run_sloptor,
	symlink_dir, write_file,
};

/// How the ledger names a test of this module: `<prefix><function name>`.
const BEHAVIORAL_TEST_PREFIX: &str = "rotor_cli::forkparity::runtime::";

const PINNED_ROBLOX_TS_VERSION: &str = "3.0.0-dev-3106b14";
const PINNED_ROBLOX_TS_COMMIT: &str = "3106b1492a2cf5b5b06f73354e94b03e3eb45b4e";
const PINNED_ROBLOX_TYPES: &str = "1.0.954";

/// The runtime tests a ledger row may name, and what each runs.
const BEHAVIORAL_TESTS: [(&str, Suite); 5] = [
	("behavioral_suite", Suite::Behavioral),
	("pinned_compatibility_loops", Suite::Pinned("loops")),
	(
		"pinned_compatibility_calling_conventions",
		Suite::Pinned("calling-conventions"),
	),
	("pinned_compatibility_iterator_rest", Suite::Pinned("iterator-rest")),
	("pinned_compatibility_switch", Suite::Pinned("switch")),
];

#[derive(Clone, Copy)]
enum Suite {
	Behavioral,
	Pinned(&'static str),
}

fn behavioral_suite_named(name: &str) -> Option<Suite> {
	let name = name.strip_prefix(BEHAVIORAL_TEST_PREFIX)?;
	BEHAVIORAL_TESTS
		.iter()
		.find(|(test, _)| *test == name)
		.map(|(_, suite)| *suite)
}

/// Whether `name` is a ledger-nameable runtime test of this module.
pub fn is_behavioral_test(name: &str) -> bool {
	behavioral_suite_named(name).is_some()
}

/// Runs the runtime test a ledger row names (`VerifyBehavioralTest`). Missing tools are an error
/// here: a skipped run does not verify a correction. A failing suite panics.
pub fn verify_behavioral_test(name: &str) -> Result<(), String> {
	let suite = behavioral_suite_named(name).ok_or_else(|| format!("{name:?} is not a runtime test"))?;
	let tools = detect_tools();
	if let Some(reason) = missing_tools_reason(&tools) {
		return Err(format!("{name} cannot verify anything without its tools: {reason}"));
	}
	let tools = tools.require();
	match suite {
		Suite::Behavioral => run_behavioral_suite(&tools),
		Suite::Pinned(fixture) => run_pinned_compatibility_fixture(&tools, &pinned_compiler(), fixture),
	}
	Ok(())
}

/// Rojo and Lune, where found.
struct Tools {
	rojo: Option<PathBuf>,
	lune: Option<PathBuf>,
}

/// Both tools, known present.
struct RuntimeTools {
	rojo: PathBuf,
	lune: PathBuf,
}

impl Tools {
	fn require(self) -> RuntimeTools {
		match (self.rojo, self.lune) {
			(Some(rojo), Some(lune)) => RuntimeTools { rojo, lune },
			_ => panic!("rojo and lune are required"),
		}
	}
}

/// `detectRuntimeTools`: the env override when set (and a file), else the tool on `PATH`.
fn detect_tools() -> Tools {
	Tools {
		rojo: detect_tool("ROTOR_ROJO_PATH", "rojo"),
		lune: detect_tool("ROTOR_LUNE_PATH", "lune"),
	}
}

fn detect_tool(variable: &str, binary: &str) -> Option<PathBuf> {
	if let Some(path) = std::env::var_os(variable).filter(|path| !path.is_empty()) {
		let path = PathBuf::from(path);
		return path.is_file().then_some(path);
	}
	look_path(binary)
}

fn look_path(binary: &str) -> Option<PathBuf> {
	let names = if cfg!(windows) {
		vec![format!("{binary}.exe"), format!("{binary}.cmd"), binary.to_owned()]
	} else {
		vec![binary.to_owned()]
	};
	std::env::split_paths(&std::env::var_os("PATH")?)
		.flat_map(|dir| names.iter().map(move |name| dir.join(name)))
		.find(|path| path.is_file())
}

/// `runtimeSuiteSkipReason`: which tool is missing and how to supply it.
fn missing_tools_reason(tools: &Tools) -> Option<String> {
	let mut missing = Vec::new();
	let mut hints = Vec::new();
	if tools.rojo.is_none() {
		missing.push("rojo");
		hints.push("set ROTOR_ROJO_PATH to a rojo executable or add rojo to PATH");
	}
	if tools.lune.is_none() {
		missing.push("lune");
		hints.push("set ROTOR_LUNE_PATH to a lune executable or add lune to PATH");
	}
	(!missing.is_empty()).then(|| format!("runtime suite requires {}; {}", missing.join(" and "), hints.join("; ")))
}

/// The tools, or `None` after saying on stderr why `test` is skipped.
fn tools_or_skip(test: &str) -> Option<RuntimeTools> {
	let tools = detect_tools();
	if let Some(reason) = missing_tools_reason(&tools) {
		eprintln!("SKIPPED {test}: {reason}");
		return None;
	}
	Some(tools.require())
}

/// Panics with the command's output unless it succeeded.
fn expect_success(output: &Output, what: &str) {
	assert!(
		output.success(),
		"{what} failed (exit {}):\n{}",
		output.code,
		output.combined()
	);
}

fn conformance_project() -> PathBuf {
	let project = repo_root().join("testdata/conformance/project");
	assert!(
		project.join("node_modules/@rbxts").is_dir(),
		"install testdata/conformance/project's dependencies (bun install --no-save) first"
	);
	project
}

fn lune_runner() -> PathBuf {
	repo_root().join("reference/roblox-ts/tests/runTestsWithLune.lua")
}

/// `sourceRelFromGolden`
fn source_rel_from_golden(base_project: &Path, golden_rel: &str) -> String {
	let rel = golden_rel.strip_suffix(".luau").unwrap_or(golden_rel);
	[".ts", ".tsx"]
		.iter()
		.map(|extension| format!("{rel}{extension}"))
		.find(|candidate| base_project.join("src").join(candidate).is_file())
		.unwrap_or_else(|| panic!("no source file for golden {golden_rel}"))
}

/// `runtimeSuiteSourceRels`: the entry point, the services shim and every spec with a golden.
fn runtime_suite_source_rels(base_project: &Path) -> Vec<String> {
	let goldens = read_tree(&repo_root().join("testdata/conformance/golden/tests"), &|path| {
		has_extension(path, "luau")
	});
	let mut rels = vec!["main.server.ts".to_owned(), "services.d.ts".to_owned()];
	rels.extend(
		goldens
			.keys()
			.map(|golden| source_rel_from_golden(base_project, &format!("tests/{golden}"))),
	);
	rels.sort();
	rels
}

/// `runtimeSuiteMainSource`: runs every spec under `ServerScriptService.tests` with `TestEZ`.
const RUNTIME_MAIN_SOURCE: &str = r#"/// <reference types="@rbxts/testez/globals" />

import { ServerScriptService } from "@rbxts/services";

const TestEZModule = game
	.GetService("ReplicatedStorage")
	.FindFirstChild("include")
	?.FindFirstChild("node_modules")
	?.FindFirstChild("@rbxts")
	?.FindFirstChild("testez")
	?.FindFirstChild("src") as ModuleScript | undefined;
assert(TestEZModule, "Unable to find @rbxts/testez!");
const TestEZ = require(TestEZModule) as typeof import("@rbxts/testez");

const results = TestEZ.TestBootstrap.run([ServerScriptService.tests]);
if (results.errors.size() > 0 || results.failureCount > 0) {
	error("Tests failed!");
}
"#;

/// `runtimeRojoConfig`
const RUNTIME_ROJO_CONFIG: &str = r#"{
	"name": "conformance-runtime",
	"tree": {
		"$className": "DataModel",
		"ReplicatedStorage": {
			"$className": "ReplicatedStorage",
			"include": {
				"$path": "include",
				"node_modules": {
					"$className": "Folder",
					"@rbxts": {
						"$path": "node_modules/@rbxts"
					}
				}
			}
		},
		"ServerScriptService": {
			"$className": "ServerScriptService",
			"main": {
				"$path": "out/main.server.luau"
			},
			"tests": {
				"$path": "out/tests"
			},
			"helpers": {
				"$className": "Folder",
				"util": {
					"$path": "out/helpers/util"
				}
			}
		},
		"StarterGui": {
			"$className": "StarterGui",
			"isolated": {
				"$path": "out/helpers/rojo/isolated.luau"
			}
		}
	}
}"#;

/// `compatibilityRuntimeRojoConfig`
const COMPATIBILITY_ROJO_CONFIG: &str = r#"{
	"name": "compatibility-runtime",
	"tree": {
		"$className": "DataModel",
		"ReplicatedStorage": {
			"$className": "ReplicatedStorage",
			"include": {
				"$path": "include",
				"node_modules": {
					"$className": "Folder",
					"@rbxts": {"$path": "node_modules/@rbxts"}
				}
			}
		},
		"ServerScriptService": {
			"$className": "ServerScriptService",
			"main": {"$path": "out/main.server.luau"},
			"tests": {"$path": "out/tests"}
		}
	}
}"#;

/// `fixtureRojoConfig`
const FIXTURE_ROJO_CONFIG: &str = r#"{"name":"conformance","tree":{"$path":"out","include":{"$path":"include"},"node_modules":{"$className":"Folder","@rbxts":{"$path":"node_modules/@rbxts"}}}}"#;

/// The JSX shim the runtime suites add to Roact's `init.lua` (`runtimeRoactCompatLua`): Roact 1.4
/// has no `Roact.jsx`, which the conformance project's `jsxFactory` names.
const ROACT_JSX_SHIM: &str = include_str!("roact_jsx_shim.lua");

/// `withRuntimeRoactCompat`: `init.lua` with the shim before its final `return Roact`.
fn with_roact_jsx_shim(text: &str) -> Result<String, String> {
	const MARKER: &str = "return Roact";
	let index = text
		.rfind(MARKER)
		.ok_or_else(|| format!("roact init missing {MARKER:?}"))?;
	if text.contains("Roact.jsx = function(component, props, ...)") {
		return Ok(text.to_owned());
	}
	Ok(format!("{}\n\n{ROACT_JSX_SHIM}\n{}", &text[..index], &text[index..]))
}

/// `stageRuntimeTypeRoots`: a copy of the conformance project's `@rbxts`, with the Roact shim.
fn stage_runtime_type_roots(base_project: &Path, project: &Path) {
	let types = project.join("node_modules/@rbxts");
	copy_tree(&base_project.join("node_modules/@rbxts"), &types);
	let init = types.join("roact/src/init.lua");
	let text = fs::read_to_string(&init).expect("Roact's init.lua reads");
	let patched = with_roact_jsx_shim(&text).unwrap_or_else(|error| panic!("{error}"));
	write_file(&init, patched);
}

/// `runBehavioralSuite`: every conformance spec, and the rotor-owned specs of
/// `testdata/conformance/rotor`, built into one place and run in Lune.
fn run_behavioral_suite(tools: &RuntimeTools) {
	let root = repo_root();
	let base_project = conformance_project();
	let temp = tempfile::Builder::new()
		.prefix(".phase5-runtime-")
		.tempdir_in(&base_project)
		.expect("a temp dir in the conformance project");
	let project = temp.path();
	copy_file(&base_project.join("package.json"), &project.join("package.json"));
	copy_file(&base_project.join("tsconfig.json"), &project.join("tsconfig.json"));
	write_file(&project.join("default.project.json"), RUNTIME_ROJO_CONFIG);
	stage_runtime_type_roots(&base_project, project);
	copy_tree(&base_project.join("src/helpers"), &project.join("src/helpers"));
	for rel in runtime_suite_source_rels(&base_project) {
		copy_file(&base_project.join("src").join(&rel), &project.join("src").join(&rel));
	}
	// Rotor extensions rbxtsc rejects (loop labels): runtime specs with no golden.
	let rotor_specs = root.join("testdata/conformance/rotor");
	for name in read_tree(&rotor_specs, &|path| path.ends_with(".spec.ts") && !path.contains('/')).keys() {
		copy_file(&rotor_specs.join(name), &project.join("src/tests").join(name));
	}
	write_file(&project.join("src/main.server.ts"), RUNTIME_MAIN_SOURCE);

	let project_arg = project.to_string_lossy().into_owned();
	let build = run_sloptor(
		&root,
		&["build", &project_arg, "--type", "game", "--allowCommentDirectives"],
	);
	expect_success(&build, "sloptor build of the conformance runtime project");
	let lune = run_place(
		tools,
		project,
		"conformance-tests.rbxlx",
		"the conformance behavioral suite",
	);
	eprintln!("{}", lune.combined());
}

/// Builds `project`'s place with Rojo and runs it in Lune; returns Lune's output.
fn run_place(tools: &RuntimeTools, project: &Path, place_name: &str, what: &str) -> Output {
	let place = project.join(place_name);
	let rojo = run(Command::new(&tools.rojo)
		.arg("build")
		.arg("--output")
		.arg(&place)
		.arg(project.join("default.project.json"))
		.current_dir(project));
	expect_success(&rojo, &format!("rojo build for {what}"));
	let lune = run(Command::new(&tools.lune)
		.arg("run")
		.arg(lune_runner())
		.arg(&place)
		.current_dir(repo_root()));
	if !lune.success() {
		let caller = fs::read_to_string(project.join("out/main.server.luau")).unwrap_or_default();
		panic!(
			"{what} failed in Lune (exit {}):\n{}\n--- out/main.server.luau ---\n{caller}",
			lune.code,
			lune.combined()
		);
	}
	lune
}

/// The user cache directory (Go's `os.UserCacheDir`).
fn user_cache_dir() -> PathBuf {
	let var = |name: &str| {
		std::env::var_os(name)
			.filter(|value| !value.is_empty())
			.map(PathBuf::from)
	};
	if cfg!(windows) {
		var("LocalAppData").expect("LocalAppData is set")
	} else if cfg!(target_os = "macos") {
		var("HOME").expect("HOME is set").join("Library/Caches")
	} else {
		var("XDG_CACHE_HOME").unwrap_or_else(|| var("HOME").expect("HOME is set").join(".cache"))
	}
}

fn npm() -> Command {
	if cfg!(windows) {
		let mut command = Command::new("cmd");
		command.args(["/c", "npm"]);
		command
	} else {
		Command::new("npm")
	}
}

/// `verifyPinnedCompiler`: `cli` runs and reports the pinned version.
fn verify_pinned_compiler(cli: &Path) {
	let output = run(Command::new("node").arg(cli).arg("--version"));
	expect_success(&output, "the pinned roblox-ts compiler");
	assert_eq!(
		output.stdout.trim(),
		PINNED_ROBLOX_TS_VERSION,
		"pinned roblox-ts version of {}",
		cli.display()
	);
}

/// `pinnedCompilerInstallValid`
fn pinned_install_valid(install: &Path, cli: &Path) -> bool {
	let commit = fs::read_to_string(install.join(".git-head")).unwrap_or_default();
	let types = fs::read_to_string(install.join("node_modules/@rbxts/types/package.json")).unwrap_or_default();
	commit.trim() == PINNED_ROBLOX_TS_COMMIT
		&& cli.is_file()
		&& serde_json::from_str::<serde_json::Value>(&types).is_ok_and(|package| {
			package.get("version").and_then(|version| version.as_str()) == Some(PINNED_ROBLOX_TYPES)
		})
}

/// `requirePinnedCompiler`: the pinned upstream `cli.js`, installed into the user cache on first
/// use after checking the registry's `gitHead`.
fn pinned_compiler() -> PathBuf {
	if let Some(cli) = std::env::var_os("ROTOR_PINNED_RBXTS_CLI").filter(|cli| !cli.is_empty()) {
		let cli = PathBuf::from(cli);
		verify_pinned_compiler(&cli);
		return cli;
	}
	let install = user_cache_dir().join(format!(
		"rotor/compatibility/roblox-ts-{PINNED_ROBLOX_TS_VERSION}-types-{PINNED_ROBLOX_TYPES}"
	));
	let cli_in = |install: &Path| install.join("node_modules/roblox-ts/out/CLI/cli.js");
	let cli = cli_in(&install);
	if pinned_install_valid(&install, &cli) {
		verify_pinned_compiler(&cli);
		return cli;
	}
	assert!(
		!install.exists(),
		"pinned compiler cache is incomplete: {}",
		install.display()
	);

	let parent = install.parent().expect("a parent");
	fs::create_dir_all(parent).expect("create the compatibility cache");
	let staging = tempfile::Builder::new()
		.prefix(".roblox-ts-install-")
		.tempdir_in(parent)
		.expect("a staging directory");
	let view = run(npm().args([
		"view",
		&format!("roblox-ts@{PINNED_ROBLOX_TS_VERSION}"),
		"gitHead",
		"--json",
	]));
	expect_success(&view, "npm view of the pinned roblox-ts");
	let commit: String = serde_json::from_str(view.stdout.trim()).expect("npm view prints a JSON string");
	assert_eq!(
		commit, PINNED_ROBLOX_TS_COMMIT,
		"roblox-ts@{PINNED_ROBLOX_TS_VERSION} gitHead"
	);
	let installed = run(npm().arg("install").arg("--prefix").arg(staging.path()).args([
		"--no-save",
		"--ignore-scripts",
		"--package-lock=false",
		&format!("roblox-ts@{PINNED_ROBLOX_TS_VERSION}"),
		&format!("@rbxts/types@{PINNED_ROBLOX_TYPES}"),
	]));
	expect_success(&installed, "npm install of the pinned roblox-ts");
	write_file(
		&staging.path().join(".git-head"),
		format!("{PINNED_ROBLOX_TS_COMMIT}\n"),
	);
	let staged = staging.keep();
	if let Err(error) = fs::rename(&staged, &install) {
		let _ = fs::remove_dir_all(&staged);
		// Another process may have published the cache first.
		assert!(
			pinned_install_valid(&install, &cli),
			"publish pinned compiler cache {}: {error}",
			install.display()
		);
	}
	verify_pinned_compiler(&cli);
	cli
}

/// The `node_modules` the pinned `cli.js` was installed into.
fn pinned_node_modules(cli: &Path) -> PathBuf {
	cli.ancestors()
		.nth(4)
		.expect("cli.js sits four levels inside node_modules")
		.to_path_buf()
}

/// `compatibilityRuntimeTSConfig`
fn compatibility_ts_config(general_loops: bool) -> String {
	let rbxts = if general_loops {
		"\n\t\"rbxts\": {\"optimizedLoops\": false},"
	} else {
		""
	};
	format!(
		r#"{{
	"compilerOptions": {{
		"allowSyntheticDefaultImports": true,
		"downlevelIteration": true,
		"module": "commonjs",
		"moduleResolution": "Node",
		"noLib": true,
		"forceConsistentCasingInFileNames": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"typeRoots": ["node_modules/@rbxts"],
		"rootDir": "src",
		"outDir": "out",
		"baseUrl": "src"
	}},{rbxts}
	"include": ["src"]
}}"#
	)
}

/// `compatibilityPackageTSConfig`
fn package_ts_config(declaration: bool) -> String {
	let declaration = if declaration {
		"\n\t\t\"declaration\": true,"
	} else {
		""
	};
	format!(
		r#"{{
	"compilerOptions": {{
		"allowSyntheticDefaultImports": true,{declaration}
		"module": "commonjs",
		"moduleResolution": "Node",
		"noLib": true,
		"forceConsistentCasingInFileNames": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"typeRoots": ["node_modules/@rbxts"],
		"rootDir": "src",
		"outDir": "out"
	}},
	"include": ["src"]
}}"#
	)
}

/// Who compiles a compatibility project.
#[derive(Clone, Copy, Debug)]
enum Compiler {
	Upstream,
	RotorOptimized,
	RotorGeneral,
}

/// Compiles `project` as `kind` (`game` or `package`) with `compiler`.
fn compile_with(compiler: Compiler, cli: &Path, project: &Path, kind: &str) {
	let root = repo_root();
	let project_arg = project.to_string_lossy().into_owned();
	let output = match compiler {
		Compiler::Upstream => run(Command::new("node")
			.arg(cli)
			.args(["build", "--project", &project_arg, "--type", kind])
			.current_dir(&root)),
		Compiler::RotorOptimized => run_sloptor(
			&root,
			&["build", &project_arg, "--type", kind, "--allowCommentDirectives"],
		),
		Compiler::RotorGeneral => run_sloptor(
			&root,
			&[
				"build",
				&project_arg,
				"--type",
				kind,
				"--allowCommentDirectives",
				"--optimizedLoops=false",
			],
		),
	};
	expect_success(&output, &format!("{compiler:?} compile of {}", project.display()));
}

/// `stageRuntimeProject`: one pinned runtime fixture as the only spec of a fresh project.
fn stage_compatibility_project(cli: &Path, fixture: &Path, general_loops: bool) -> tempfile::TempDir {
	let base_project = conformance_project();
	let temp = tempfile::tempdir().expect("a temp dir");
	let project = temp.path();
	copy_file(&base_project.join("package.json"), &project.join("package.json"));
	write_file(&project.join("tsconfig.json"), compatibility_ts_config(general_loops));
	write_file(&project.join("default.project.json"), COMPATIBILITY_ROJO_CONFIG);
	stage_runtime_type_roots(&base_project, project);
	copy_tree(
		&pinned_node_modules(cli).join("@rbxts/types"),
		&project.join("node_modules/@rbxts/types"),
	);
	copy_file(
		fixture,
		&project
			.join("src/tests")
			.join(fixture.file_name().expect("a file name")),
	);
	copy_file(
		&base_project.join("src/services.d.ts"),
		&project.join("src/services.d.ts"),
	);
	write_file(&project.join("src/main.server.ts"), RUNTIME_MAIN_SOURCE);
	temp
}

/// `runPinnedCompatibilityFixture`: the fixture must pass in Lune as compiled by the pinned
/// upstream compiler and by sloptor (and, for `loops`, by sloptor without loop optimization).
fn run_pinned_compatibility_fixture(tools: &RuntimeTools, cli: &Path, fixture: &str) {
	let path = repo_root().join(format!("testdata/compatibility/runtime/{fixture}.spec.ts"));
	assert!(path.is_file(), "compatibility fixture {} is missing", path.display());
	let mut compilers = vec![Compiler::Upstream, Compiler::RotorOptimized];
	if fixture == "loops" {
		compilers.push(Compiler::RotorGeneral);
	}
	for compiler in compilers {
		let project = stage_compatibility_project(cli, &path, matches!(compiler, Compiler::RotorGeneral));
		compile_with(compiler, cli, project.path(), "game");
		let lune = run_place(
			tools,
			project.path(),
			"compatibility-tests.rbxlx",
			&format!("{fixture} with {compiler:?}"),
		);
		eprintln!("{fixture} with {compiler:?}:\n{}", lune.combined());
	}
}

/// `stagePackageTypeRoots`: the conformance project's `@rbxts` plus the pinned `@rbxts/types`.
fn stage_package_type_roots(cli: &Path, project: &Path) {
	let base_project = conformance_project();
	copy_tree(
		&base_project.join("node_modules/@rbxts"),
		&project.join("node_modules/@rbxts"),
	);
	copy_tree(
		&pinned_node_modules(cli).join("@rbxts/types"),
		&project.join("node_modules/@rbxts/types"),
	);
}

/// `compilePackageLibrary`: `@rbxts/compat-library` compiled as a package by `compiler`.
fn compile_package_library(cli: &Path, compiler: Compiler) -> tempfile::TempDir {
	let fixture = repo_root().join("testdata/compatibility/packages/library");
	let temp = tempfile::tempdir().expect("a temp dir");
	let project = temp.path();
	copy_file(&fixture.join("package.json"), &project.join("package.json"));
	copy_tree(&fixture.join("src"), &project.join("src"));
	write_file(&project.join("tsconfig.json"), package_ts_config(true));
	stage_package_type_roots(cli, project);
	compile_with(compiler, cli, project, "package");
	for rel in ["out/init.luau", "out/index.d.ts"] {
		assert!(
			project.join(rel).is_file(),
			"{compiler:?} did not emit package artifact {rel}"
		);
	}
	temp
}

/// `compilePackageConsumer`: the consumer game compiled by `compiler` against `library`.
fn compile_package_consumer(cli: &Path, compiler: Compiler, library: &Path) -> tempfile::TempDir {
	let fixture = repo_root().join("testdata/compatibility/packages/consumer");
	let temp = tempfile::tempdir().expect("a temp dir");
	let project = temp.path();
	copy_file(&fixture.join("package.json"), &project.join("package.json"));
	copy_file(
		&fixture.join("default.project.json"),
		&project.join("default.project.json"),
	);
	copy_tree(&fixture.join("src"), &project.join("src"));
	write_file(&project.join("tsconfig.json"), package_ts_config(false));
	stage_package_type_roots(cli, project);
	let installed = project.join("node_modules/@rbxts/compat-library");
	copy_file(&library.join("package.json"), &installed.join("package.json"));
	copy_tree(&library.join("out"), &installed.join("out"));
	compile_with(compiler, cli, project, "game");
	temp
}

/// `diagnosticFixtureTSConfig`
const DIAGNOSTIC_TS_CONFIG: &str = r#"{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"downlevelIteration": true,
		"jsx": "react",
		"jsxFactory": "Roact.jsx",
		"module": "commonjs",
		"moduleResolution": "Node",
		"noLib": true,
		"resolveJsonModule": true,
		"forceConsistentCasingInFileNames": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"typeRoots": ["node_modules/@rbxts"],
		"experimentalDecorators": true,
		"rootDir": "src",
		"outDir": "out",
		"checkers": 1,
		"baseUrl": "src"
	},
	"include": ["src"]
}"#;

/// One diagnostic fixture and the diagnostic codes it must produce.
struct DiagnosticFixture {
	name: String,
	path: PathBuf,
	expected: Vec<String>,
	/// Whether `expected` is the exact list (a fork reclassification) rather than "only this code".
	exact: bool,
}

/// `forkDiagnosticExpectations`: fixtures whose rbxtsc expectation no longer holds for sloptor. An
/// empty list compiles clean. Loop labels are a sloptor extension, so `noLabeledStatement.1.ts`
/// compiles, and `.2.ts`/`.3.ts` (an unknown label behind `@ts-ignore`) get sloptor's own error.
const FORK_DIAGNOSTIC_EXPECTATIONS: [(&str, &[&str]); 6] = [
	("noFunctionExpressionName.ts", &[]),
	("noSpreadDestructuring.1.ts", &[]),
	("noSpreadDestructuring.2.ts", &["noRestSpreadingOfRobloxTypes"]),
	("noLabeledStatement.1.ts", &[]),
	("noLabeledStatement.2.ts", &["rotorUnknownLabel"]),
	("noLabeledStatement.3.ts", &["rotorUnknownLabel"]),
];

/// `diagnosticBaseName`: the file name without its extension and numeric case suffix.
fn diagnostic_base_name(name: &str) -> &str {
	let base = name
		.strip_suffix(".tsx")
		.or_else(|| name.strip_suffix(".ts"))
		.unwrap_or(name);
	match base.rsplit_once('.') {
		Some((head, case)) if !case.is_empty() && case.bytes().all(|byte| byte.is_ascii_digit()) => head,
		_ => base,
	}
}

/// `loadDiagnosticFixtures`
fn diagnostic_fixtures(dir: &Path) -> Vec<DiagnosticFixture> {
	read_tree(dir, &|path| {
		!path.contains('/') && (has_extension(path, "ts") || has_extension(path, "tsx"))
	})
	.into_keys()
	.map(|name| {
		let (expected, exact) = FORK_DIAGNOSTIC_EXPECTATIONS
			.iter()
			.find(|(fixture, _)| *fixture == name)
			.map_or_else(
				|| (vec![diagnostic_base_name(&name).to_owned()], false),
				|(_, codes)| (codes.iter().map(|code| (*code).to_owned()).collect(), true),
			);
		DiagnosticFixture {
			path: dir.join(&name),
			name,
			expected,
			exact,
		}
	})
	.collect()
}

/// `compileDiagnosticFixture`: the fixture alone in a project beside the conformance helpers,
/// compiled in-process; returns its diagnostic codes.
fn compile_diagnostic_fixture(fixture: &Path) -> Result<Vec<String>, String> {
	let root = repo_root();
	let base_project = conformance_project();
	let name = fixture.file_name().expect("a file name").to_string_lossy().into_owned();
	let (source_rel, rojo_config) = match name.as_str() {
		"noIsolatedImport.ts" | "noRojoData.ts" => (
			format!("src/diagnostics/{name}"),
			root.join("reference/roblox-ts/tests/default.project.json"),
		),
		_ => (
			format!("src/__diagnostics/{name}"),
			base_project.join("default.project.json"),
		),
	};
	let temp = tempfile::Builder::new()
		.prefix(".phase5-diagnostic-")
		.tempdir_in(&base_project)
		.expect("a temp dir in the conformance project");
	let project = temp.path();
	write_file(
		&project.join("package.json"),
		r#"{"name":"conformance-diagnostic-fixture"}"#,
	);
	write_file(&project.join("tsconfig.json"), DIAGNOSTIC_TS_CONFIG);
	copy_file(&rojo_config, &project.join("default.project.json"));
	fs::create_dir_all(project.join("include")).expect("create include");
	symlink_dir(
		&base_project.join("node_modules/@rbxts"),
		&project.join("node_modules/@rbxts"),
	);
	copy_tree(&base_project.join("src/helpers"), &project.join("src/helpers"));
	copy_file(
		&base_project.join("src/services.d.ts"),
		&project.join("src/services.d.ts"),
	);
	copy_file(fixture, &project.join(&source_rel));

	let mut options = ProjectOptions::default();
	options.allow_comment_directives = true;
	let project_dir = project.to_string_lossy().into_owned();
	let result =
		rotor_tsrs::with_large_stack(|| compile_file_detailed_with_options(&project_dir, &source_rel, &options));
	let diagnostics = match result {
		Ok(output) => output.diagnostics,
		Err(failure) if !failure.diagnostics.is_empty() => failure.diagnostics,
		Err(failure) => return Err(format!("compile {name}: {}", failure.error)),
	};
	Ok(diagnostics.into_iter().map(|diagnostic| diagnostic.code).collect())
}

/// `pinnedDiagnosticMessages`: upstream's message for each pinned diagnostic.
fn pinned_diagnostic_message(code: &str) -> Option<&'static str> {
	match code {
		"expectedFunctionGotMethod" => Some("Attempted to assign method where non-method was expected."),
		"expectedMethodGotFunction" => Some("Attempted to assign non-method where method was expected."),
		"noUnstableThisType" => Some("The generic this type can become void, changing whether a receiver is passed."),
		_ => None,
	}
}

/// `assertPinnedDiagnostic`: the pinned upstream compiler rejects `fixture` with `message`.
fn assert_pinned_diagnostic(cli: &Path, fixture: &Path, message: &str) {
	let base_project = conformance_project();
	let temp = tempfile::tempdir().expect("a temp dir");
	let project = temp.path();
	copy_file(&base_project.join("package.json"), &project.join("package.json"));
	write_file(&project.join("tsconfig.json"), compatibility_ts_config(false));
	write_file(&project.join("default.project.json"), FIXTURE_ROJO_CONFIG);
	stage_runtime_type_roots(&base_project, project);
	copy_tree(
		&pinned_node_modules(cli).join("@rbxts/types"),
		&project.join("node_modules/@rbxts/types"),
	);
	copy_file(
		fixture,
		&project.join("src").join(fixture.file_name().expect("a file name")),
	);
	let project_arg = project.to_string_lossy().into_owned();
	let output = run(Command::new("node")
		.arg(cli)
		.args(["build", "--project", &project_arg, "--type", "game"]));
	assert!(
		!output.success(),
		"pinned upstream compiled {} successfully:\n{}",
		fixture.display(),
		output.combined()
	);
	assert!(
		output.combined().contains(message),
		"pinned upstream output for {} lacks {message:?}:\n{}",
		fixture.display(),
		output.combined()
	);
}

/// `normalizeCompiledHeader`: the `-- Compiled with` line, whatever compiler and version wrote it.
fn normalize_compiled_header(text: &str) -> String {
	const PREFIX: &str = "-- Compiled with ";
	if !text.starts_with(PREFIX) {
		return text.to_owned();
	}
	match text.split_once('\n') {
		Some((_, rest)) => format!("-- Compiled with <normalized>\n{rest}"),
		None => "-- Compiled with <normalized>".to_owned(),
	}
}

/// `compareOutputTrees`: every file under `left` and `right` (each root keyed by its own name),
/// with Luau headers normalized and `.tsbuildinfo` ignored.
fn compare_output_trees(left: &[PathBuf], right: &[PathBuf]) -> Vec<String> {
	let collect = |roots: &[PathBuf]| {
		let mut files = BTreeMap::new();
		for root in roots {
			let base = root.file_name().expect("a root name").to_string_lossy().into_owned();
			for (rel, contents) in read_tree(root, &|path| !path.ends_with(".tsbuildinfo")) {
				files.insert(format!("{base}/{rel}"), contents);
			}
		}
		files
	};
	let (left, right) = (collect(left), collect(right));
	let keys: std::collections::BTreeSet<&String> = left.keys().chain(right.keys()).collect();
	keys.into_iter()
		.filter_map(|rel| match (left.get(rel), right.get(rel)) {
			(Some(a), Some(b)) => {
				let (mut a, mut b) = (
					String::from_utf8_lossy(a).into_owned(),
					String::from_utf8_lossy(b).into_owned(),
				);
				if has_extension(rel, "lua") || has_extension(rel, "luau") {
					a = normalize_compiled_header(&a);
					b = normalize_compiled_header(&b);
				}
				(a != b).then(|| format!("{rel} content mismatch"))
			}
			(a, b) => Some(format!(
				"{rel} presence mismatch (left={} right={})",
				a.is_some(),
				b.is_some()
			)),
		})
		.collect()
}

/// `stageAcceptanceProjectCopy`: the project without its outputs or `.git`, its `node_modules`
/// linked back.
fn stage_acceptance_copy(src: &Path, dst: &Path) {
	copy_tree_filtered(src, dst, &|rel| {
		let top = rel.split('/').next().unwrap_or_default();
		!matches!(top, "node_modules" | "out" | "include" | ".git")
	});
	let node_modules = src.join("node_modules");
	if node_modules.is_dir() {
		symlink_dir(&node_modules, &dst.join("node_modules"));
	}
}

/// `resolveRandomnessProjectPath`: `ROTOR_RANDOMNESS_PATH` as a project directory.
fn randomness_project() -> Result<PathBuf, String> {
	let raw = std::env::var_os("ROTOR_RANDOMNESS_PATH")
		.filter(|raw| !raw.is_empty())
		.ok_or("ROTOR_RANDOMNESS_PATH not set; point it at the randomness project root or its tsconfig.json")?;
	let mut path = PathBuf::from(raw);
	if path.is_file() {
		if path.file_name().is_none_or(|name| name != "tsconfig.json") {
			return Err(format!(
				"ROTOR_RANDOMNESS_PATH {} must be a project directory or tsconfig.json",
				path.display()
			));
		}
		path.pop();
	}
	if !path.join("tsconfig.json").is_file() {
		return Err(format!(
			"ROTOR_RANDOMNESS_PATH {} does not contain tsconfig.json",
			path.display()
		));
	}
	Ok(path)
}

#[test]
#[ignore = "builds a place and runs it in Lune; mise run rust:forkparity"]
fn behavioral_suite() {
	if let Some(tools) = tools_or_skip("behavioral_suite") {
		run_behavioral_suite(&tools);
	}
}

fn pinned_compatibility(test: &str, fixture: &str) {
	if let Some(tools) = tools_or_skip(test) {
		run_pinned_compatibility_fixture(&tools, &pinned_compiler(), fixture);
	}
}

#[test]
#[ignore = "runs the pinned upstream compiler and Lune; mise run rust:forkparity"]
fn pinned_compatibility_loops() {
	pinned_compatibility("pinned_compatibility_loops", "loops");
}

#[test]
#[ignore = "runs the pinned upstream compiler and Lune; mise run rust:forkparity"]
fn pinned_compatibility_calling_conventions() {
	pinned_compatibility("pinned_compatibility_calling_conventions", "calling-conventions");
}

#[test]
#[ignore = "runs the pinned upstream compiler and Lune; mise run rust:forkparity"]
fn pinned_compatibility_iterator_rest() {
	pinned_compatibility("pinned_compatibility_iterator_rest", "iterator-rest");
}

#[test]
#[ignore = "runs the pinned upstream compiler and Lune; mise run rust:forkparity"]
fn pinned_compatibility_switch() {
	pinned_compatibility("pinned_compatibility_switch", "switch");
}

/// `TestPinnedPackageInterop`: a package compiled by one compiler, consumed by a game compiled by
/// the other, both ways round.
#[test]
#[ignore = "runs the pinned upstream compiler and Lune; mise run rust:forkparity"]
fn pinned_package_interop() {
	let Some(tools) = tools_or_skip("pinned_package_interop") else {
		return;
	};
	let cli = pinned_compiler();
	let upstream_library = compile_package_library(&cli, Compiler::Upstream);
	let rotor_library = compile_package_library(&cli, Compiler::RotorOptimized);
	for (compiler, library, what) in [
		(
			Compiler::RotorOptimized,
			upstream_library.path(),
			"sloptor caller, upstream library",
		),
		(
			Compiler::Upstream,
			rotor_library.path(),
			"upstream caller, sloptor library",
		),
	] {
		let consumer = compile_package_consumer(&cli, compiler, library);
		run_place(&tools, consumer.path(), "package-interop.rbxlx", what);
	}
}

/// `TestPinnedCompatibilityDiagnostics`: sloptor reports exactly the fixture's diagnostic, and the
/// pinned upstream compiler rejects the fixture with the same diagnostic's message.
#[test]
#[ignore = "runs the pinned upstream compiler; mise run rust:forkparity"]
fn pinned_compatibility_diagnostics() {
	let cli = pinned_compiler();
	let fixtures = diagnostic_fixtures(&repo_root().join("testdata/compatibility/diagnostics"));
	assert!(!fixtures.is_empty(), "no pinned compatibility diagnostic fixtures");
	for fixture in fixtures {
		let [expected] = fixture.expected.as_slice() else {
			panic!("{}: expected codes {:?}, want one", fixture.name, fixture.expected);
		};
		let got = compile_diagnostic_fixture(&fixture.path).unwrap_or_else(|error| panic!("{error}"));
		assert_eq!(
			got,
			std::slice::from_ref(expected),
			"{}: sloptor diagnostics",
			fixture.name
		);
		let message = pinned_diagnostic_message(expected)
			.unwrap_or_else(|| panic!("no pinned message for diagnostic {expected}"));
		assert_pinned_diagnostic(&cli, &fixture.path, message);
	}
}

/// `TestRandomnessAcceptance`: a real project at `ROTOR_RANDOMNESS_PATH` builds to the same tree
/// as rbxtsc, headers aside.
#[test]
#[ignore = "needs ROTOR_RANDOMNESS_PATH and rbxtsc; mise run rust:forkparity"]
fn randomness_acceptance() {
	let project = match randomness_project() {
		Ok(project) => project,
		Err(reason) => {
			eprintln!("SKIPPED randomness_acceptance: {reason}");
			return;
		}
	};
	let temp = tempfile::tempdir().expect("a temp dir");
	let rotor_copy = temp.path().join("rotor");
	let rbxtsc_copy = temp.path().join("rbxtsc");
	stage_acceptance_copy(&project, &rotor_copy);
	stage_acceptance_copy(&project, &rbxtsc_copy);

	let mut options = ProjectOptions::default();
	options.emit_include_files = true;
	options.allow_comment_directives = true;
	let rotor_dir = rotor_copy.to_string_lossy().into_owned();
	let result = rotor_tsrs::with_large_stack(|| build_project_with_options(&rotor_dir, &options))
		.unwrap_or_else(|failure| panic!("build: {} ({:?})", failure.failure.error, failure.failure.messages()));
	assert!(
		result.diagnostics.is_empty(),
		"unexpected diagnostics: {:?}",
		result.diagnostics
	);
	assert!(!result.outputs.is_empty(), "no emitted files");

	let local = rbxtsc_copy.join("node_modules/.bin/rbxtsc.cmd");
	let mut rbxtsc = if cfg!(windows) && local.is_file() {
		let mut command = Command::new("cmd");
		command.arg("/c").arg(local);
		command
	} else {
		let mut command = Command::new(if cfg!(windows) { "npx.cmd" } else { "npx" });
		command.arg("rbxtsc");
		command
	};
	expect_success(&run(rbxtsc.current_dir(&rbxtsc_copy)), "rbxtsc build");

	let output_dir = PathBuf::from(&result.output_dir);
	let output_rel = output_dir
		.strip_prefix(&rotor_copy)
		.expect("the output directory is inside the project");
	let diffs = compare_output_trees(
		&[output_dir.clone(), rotor_copy.join("include")],
		&[rbxtsc_copy.join(output_rel), rbxtsc_copy.join("include")],
	);
	assert!(
		diffs.is_empty(),
		"randomness output differs from rbxtsc:\n{}",
		diffs.join("\n")
	);
}

/// `TestDiagnosticsCorpus`, `TestNoRobloxSymbolInstanceofFixture` and
/// `TestRojoTopologyDiagnosticFixtures`: every excluded rbxtsc diagnostic fixture reports its
/// diagnostic, and only that one.
#[test]
fn diagnostics_corpus() {
	let fixtures = diagnostic_fixtures(&repo_root().join("testdata/conformance/excluded/diagnostics"));
	assert!(!fixtures.is_empty(), "no diagnostic fixtures found");
	let mut failures = Vec::new();
	for fixture in &fixtures {
		let got = match compile_diagnostic_fixture(&fixture.path) {
			Ok(got) => got,
			Err(error) => {
				failures.push(error);
				continue;
			}
		};
		let exactly_one =
			["noRobloxSymbolInstanceof.1.ts", "noIsolatedImport.ts", "noRojoData.ts"].contains(&fixture.name.as_str());
		let ok = if fixture.exact || exactly_one {
			got == fixture.expected
		} else {
			!got.is_empty() && got.iter().all(|code| *code == fixture.expected[0])
		};
		if !ok {
			failures.push(format!("{}: got {got:?}, want {:?}", fixture.name, fixture.expected));
		}
	}
	assert!(
		failures.is_empty(),
		"diagnostic corpus failures:\n{}",
		failures.join("\n")
	);
}

#[test]
fn ledger_tests_name_this_module() {
	assert!(is_behavioral_test(
		"rotor_cli::forkparity::runtime::pinned_compatibility_iterator_rest"
	));
	assert!(!is_behavioral_test("TestPinnedCompatibility/iterator-rest"));
	assert!(!is_behavioral_test(
		"rotor_cli::forkparity::runtime::pinned_package_interop"
	));
}

#[test]
fn missing_tools_reason_names_the_missing_tool_and_its_override() {
	let reason = missing_tools_reason(&Tools {
		rojo: Some(PathBuf::from("rojo")),
		lune: None,
	})
	.expect("lune is missing");
	assert!(
		reason.contains("lune") && reason.contains("ROTOR_LUNE_PATH"),
		"{reason}"
	);
	assert!(!reason.contains("requires rojo"), "{reason}");
	assert!(
		missing_tools_reason(&Tools {
			rojo: Some(PathBuf::from("rojo")),
			lune: Some(PathBuf::from("lune"))
		})
		.is_none()
	);
}

#[test]
fn roact_jsx_shim_goes_before_the_final_return() {
	let patched = with_roact_jsx_shim("local Roact = {}\n\nreturn Roact\n").expect("the shim applies");
	assert!(patched.starts_with("local Roact = {}\n"));
	assert!(patched.ends_with("return Roact\n"));
	assert!(patched.contains(r#"rawset(Roact, "jsx", function(component, props, ...)"#));
	assert!(with_roact_jsx_shim("local Roact = {}\n").is_err());
}

#[test]
fn diagnostic_base_name_drops_the_case_number() {
	assert_eq!(diagnostic_base_name("noAny.12.ts"), "noAny");
	assert_eq!(diagnostic_base_name("noRojoData.ts"), "noRojoData");
	assert_eq!(diagnostic_base_name("noJsx.tsx"), "noJsx");
}

#[test]
fn output_tree_comparison_ignores_the_compiler_header() {
	let temp = tempfile::tempdir().expect("a temp dir");
	for (side, header) in [("left", "rotor v0.1.0-dev"), ("right", "sloptor v3.0.0-beta.2")] {
		write_file(
			&temp.path().join(side).join("out/main.luau"),
			format!("-- Compiled with {header}\nprint(\"ok\")\n"),
		);
		write_file(&temp.path().join(side).join("include/RuntimeLib.lua"), "return {}\n");
	}
	let roots = |side: &str| {
		[
			temp.path().join(side).join("out"),
			temp.path().join(side).join("include"),
		]
	};
	assert_eq!(
		compare_output_trees(&roots("left"), &roots("right")),
		Vec::<String>::new()
	);
	write_file(
		&temp.path().join("right/out/main.luau"),
		"-- Compiled with x\nprint(\"no\")\n",
	);
	assert_eq!(
		compare_output_trees(&roots("left"), &roots("right")),
		["out/main.luau content mismatch"]
	);
}

#[test]
fn acceptance_copy_skips_generated_directories() {
	let temp = tempfile::tempdir().expect("a temp dir");
	let src = temp.path().join("src-project");
	for (rel, text) in [
		("tsconfig.json", "{}"),
		("src/main.ts", "export {};\n"),
		("node_modules/sentinel.txt", "node_modules\n"),
		("out/stale.luau", "stale\n"),
		("include/RuntimeLib.lua", "stale\n"),
	] {
		write_file(&src.join(rel), text);
	}
	let dst = temp.path().join("staged");
	stage_acceptance_copy(&src, &dst);
	assert!(dst.join("src/main.ts").is_file());
	assert!(!dst.join("out").exists());
	assert!(!dst.join("include").exists());
	assert!(dst.join("node_modules/sentinel.txt").is_file());
}
