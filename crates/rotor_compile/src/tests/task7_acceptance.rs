//! `task7_acceptance_test.go`

use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

use rotor_config::config::{FlameworkConfig, FlameworkOptimizations};
use sha2::{Digest, Sha256};

use super::output::enable_incremental_builds;
use super::sidecar::build;
use super::{Temp, large_stack, write_project};
use crate::flamework_pipeline::{FlameworkPipeline, prepare_flamework_pipeline};
use crate::project::{ProjectOptions, lock, new_project_program_with_options};
use crate::sidecar::set_test_env;
use crate::timings::BuildTimings;

#[test]
fn task7_flamework_clean_warm_incremental_and_recovery_are_deterministic() {
	// Given: an incremental native-only project with every non-ordering option set.
	let project = task7_flamework_project(TASK7_ALL_OPTIONS);
	let dir = project.dir();
	set_test_env("ROTOR_NODE_PATH", &project.join("node-must-not-run").to_string_lossy());

	// When: it builds clean, warm, after one source edit, fails, and resumes.
	let clean_timings = Arc::new(BuildTimings::new());
	task7_build(&dir, Some(&clean_timings));
	let clean_hash = task7_artifact_hash(&dir);
	task7_write(&project.join("out/task7-unexpected.txt"), "unexpected\n");
	assert_ne!(
		task7_artifact_hash(&dir),
		clean_hash,
		"complete artifact hash ignored unexpected output file"
	);
	task7_remove(&project.join("out/task7-unexpected.txt"));
	assert_eq!(
		task7_artifact_hash(&dir),
		clean_hash,
		"artifact hash did not recover after unexpected output removal"
	);
	let warm_timings = Arc::new(BuildTimings::new());
	task7_build(&dir, Some(&warm_timings));
	let warm_hash = task7_artifact_hash(&dir);
	let good_source = "export const task7Value = 2;\n";
	task7_write(&project.join("src/main.ts"), good_source);
	let incremental_timings = Arc::new(BuildTimings::new());
	task7_build(&dir, Some(&incremental_timings));
	let incremental_hash = task7_artifact_hash(&dir);
	task7_write(&project.join("src/main.ts"), "export const broken: = 1;\n");
	let (_, diagnostics, error) = build(&dir, &ProjectOptions::default());
	assert!(
		error.is_some() && !diagnostics.is_empty(),
		"malformed build = ({diagnostics:?}, {error:?}), want failure with diagnostics"
	);
	let failed_hash = task7_artifact_hash(&dir);
	task7_write(&project.join("src/main.ts"), good_source);
	let resume_timings = Arc::new(BuildTimings::new());
	task7_build(&dir, Some(&resume_timings));

	// Then: counts and complete artifact hashes prove deterministic publication.
	let clean = clean_timings.report().counts;
	assert_eq!(
		[clean.selected_sources, clean.actual_writes],
		[1, 1],
		"clean selected/writes"
	);
	let warm = warm_timings.report().counts;
	assert!(
		warm_hash == clean_hash && warm.selected_sources == 0 && warm.actual_writes == 0,
		"warm hash/counts = {warm_hash}/{}/{}, want {clean_hash}/0/0",
		warm.selected_sources,
		warm.actual_writes
	);
	let incremental = incremental_timings.report().counts;
	assert!(
		incremental_hash != clean_hash && incremental.selected_sources == 1 && incremental.actual_writes == 1,
		"incremental hash/counts = {incremental_hash}/{}/{}, clean {clean_hash}",
		incremental.selected_sources,
		incremental.actual_writes
	);
	let resume = resume_timings.report().counts;
	let resumed_hash = task7_artifact_hash(&dir);
	assert!(
		failed_hash == incremental_hash
			&& resumed_hash == incremental_hash
			&& resume.selected_sources == 0
			&& resume.actual_writes == 0,
		"failure/resume changed last-good artifacts or cache: failed={failed_hash} resumed={resumed_hash} selected={} writes={}",
		resume.selected_sources,
		resume.actual_writes
	);
}

#[test]
#[expect(
	clippy::significant_drop_tightening,
	reason = "the lock guards what the assertions read"
)]
fn task7_flamework_public_options_and_defaults_reach_real_compiler_pipeline() {
	// Given: native projects using the empty defaults and every public option.
	let default_project = task7_flamework_project("[flamework]\n");
	let configured_project = task7_flamework_project(TASK7_ALL_OPTIONS);
	let node = Temp::new();
	set_test_env("ROTOR_NODE_PATH", &node.join("node-must-not-run").to_string_lossy());

	// When: both projects build and their real compiler pipelines are prepared.
	task7_build(&default_project.dir(), None);
	task7_build(&configured_project.dir(), None);
	large_stack(|| {
		let default_pipeline = task7_pipeline(&default_project.dir());
		let configured_pipeline = task7_pipeline(&configured_project.dir());

		// Then: upstream defaults and every typed public field arrive unchanged.
		let got = &default_pipeline.config;
		assert!(
			got.id_generation_mode == "full"
				&& got.after.is_empty()
				&& !got.obfuscation
				&& !got.no_semantic_diagnostics
				&& got.hash_prefix.is_empty()
				&& got.salt.is_empty()
				&& !got.preload_ids
				&& got.optimizations.guard_generation_dedup_limit.is_none(),
			"default Flamework config = {got:?}"
		);
		assert_eq!(
			configured_pipeline.config,
			task7_configured_options(0),
			"configured Flamework options"
		);
		let project = lock(&configured_pipeline.project);
		assert!(
			project.id_mode().as_str() == "tiny"
				&& project.hash_prefix() == "task7"
				&& configured_pipeline.prefix.is_empty()
				&& configured_pipeline.suffix.is_empty(),
			"configured native pipeline = mode {:?} prefix {:?} stages {}/{}",
			project.id_mode().as_str(),
			project.hash_prefix(),
			configured_pipeline.prefix.len(),
			configured_pipeline.suffix.len()
		);
	});
	for mode in ["full", "obfuscated", "short", "tiny"] {
		let project = task7_flamework_project(&format!(
			"[flamework]\nidGenerationMode = \"{mode}\"\nsalt = \"fixed\"\n"
		));
		task7_build(&project.dir(), None);
		let got = large_stack(|| {
			lock(&task7_pipeline(&project.dir()).project)
				.id_mode()
				.as_str()
				.to_owned()
		});
		assert_eq!(got, mode, "{mode}: ID mode");
	}
	// obfuscation default
	let project = task7_flamework_project("[flamework]\nobfuscation = true\nsalt = \"fixed\"\n");
	task7_build(&project.dir(), None);
	let got = large_stack(|| {
		lock(&task7_pipeline(&project.dir()).project)
			.id_mode()
			.as_str()
			.to_owned()
	});
	assert_eq!(got, "obfuscated", "obfuscation default ID mode");
}

#[test]
fn task7_flamework_invalid_inputs_fail_closed_and_preserve_last_good_artifacts() {
	let invalid: Vec<Task7InvalidCase> = vec![
		// Go's tuple is BurntSushi's wording, "config: rotor.toml: toml: line 2: expected '.' or
		// ']' to end table name, but got '\\n' instead"; rotor_config's TOML parser words syntax
		// errors its own way (`rotor_config::config::decode`).
		(
			"malformed toml",
			"config: rotor.toml: toml: line 1: unclosed table, expected `]`\nerror: config: rotor.toml: toml: line 1: unclosed table, expected `]`",
			task7_write_mutation("rotor.toml", "[flamework\n"),
		),
		(
			"unknown option",
			"config: rotor.toml: flamework: unknown key \"flamework.unknown\"\nerror: config: rotor.toml: flamework: unknown key \"flamework.unknown\"",
			task7_write_mutation("rotor.toml", "[flamework]\nunknown = true\n"),
		),
		(
			"unknown optimization",
			"config: rotor.toml: flamework: unknown key \"flamework.optimizations.unknown\"\nerror: config: rotor.toml: flamework: unknown key \"flamework.optimizations.unknown\"",
			task7_write_mutation("rotor.toml", "[flamework.optimizations]\nunknown = 1\n"),
		),
		(
			"invalid id mode",
			"config: flamework.idGenerationMode must be one of full, obfuscated, short, tiny; got \"random\"\nerror: compile: invalid Flamework configuration",
			task7_write_mutation("rotor.toml", "[flamework]\nidGenerationMode = \"random\"\n"),
		),
		(
			"reserved hash prefix",
			"config: flamework.hashPrefix must not start with reserved prefix \"$\": \"$reserved\"\nerror: compile: invalid Flamework configuration",
			task7_write_mutation("rotor.toml", "[flamework]\nhashPrefix = \"$reserved\"\n"),
		),
		(
			"negative dedup limit",
			"config: flamework.optimizations.guardGenerationDedupLimit must be >= 0, got -1\nerror: compile: invalid Flamework configuration",
			task7_write_mutation(
				"rotor.toml",
				"[flamework.optimizations]\nguardGenerationDedupLimit = -1\n",
			),
		),
		(
			"missing after anchor",
			"<ROOT>/rotor.toml: flamework.after \"missing-transformer\" does not match an effective tsconfig transformer plugin\nerror: compile: invalid flamework.after",
			task7_write_mutation("rotor.toml", "[flamework]\nafter = \"missing-transformer\"\n"),
		),
		(
			"self after anchor",
			"<ROOT>/rotor.toml: flamework.after cannot anchor native Flamework to itself: \"rbxts-transformer-flamework\"\nerror: compile: invalid flamework.after",
			task7_write_mutation("rotor.toml", "[flamework]\nafter = \"rbxts-transformer-flamework\"\n"),
		),
	];
	task7_invalid_cases(invalid);
}

#[test]
fn task7_flamework_invalid_project_state_fails_closed_and_preserves_artifacts() {
	let invalid: Vec<Task7InvalidCase> = vec![
		(
			"missing package",
			"package.json not found in <ROOT>\nerror: compile: open native Flamework project: package.json not found in <ROOT>",
			Box::new(|dir: &str| task7_remove(&Path::new(dir).join("package.json"))),
		),
		(
			"invalid package",
			"invalid Flamework package.json at <ROOT>/package.json\nerror: compile: open native Flamework project: invalid Flamework package.json at <ROOT>/package.json",
			task7_write_mutation("package.json", "null\n"),
		),
		(
			"invalid build info",
			"load project Flamework build info: flamework: invalid flamework.build: SyntaxError: Expected property name or '}' in JSON at position 2 (line 2 column 1)\nerror: compile: open native Flamework project: load project Flamework build info: flamework: invalid flamework.build: SyntaxError: Expected property name or '}' in JSON at position 2 (line 2 column 1)",
			task7_write_mutation("flamework.build", "{\n"),
		),
		(
			"runtime nonobject",
			"flamework: invalid flamework.json: expected JSON object\nerror: compile: open native Flamework project: flamework: invalid flamework.json: expected JSON object",
			task7_write_mutation("flamework.json", "[]"),
		),
		(
			"runtime trailing value",
			"flamework: invalid flamework.json: SyntaxError: invalid character '{' after top-level value in JSON at position 4 (line 1 column 5)\nerror: compile: open native Flamework project: flamework: invalid flamework.json: SyntaxError: invalid character '{' after top-level value in JSON at position 4 (line 1 column 5)",
			task7_write_mutation("flamework.json", "{} {}"),
		),
		(
			"runtime log level",
			"Malformed flamework.json\nenum /logLevel: must be equal to one of the allowed values {\"allowedValues\":[\"none\",\"verbose\"]}\nerror: compile: open native Flamework project: Malformed flamework.json\nenum /logLevel: must be equal to one of the allowed values {\"allowedValues\":[\"none\",\"verbose\"]}",
			task7_write_mutation("flamework.json", r#"{"logLevel":"debug"}"#),
		),
		(
			"runtime profiling",
			"Malformed flamework.json\ntype /profiling: must be boolean {\"type\":\"boolean\"}\nerror: compile: open native Flamework project: Malformed flamework.json\ntype /profiling: must be boolean {\"type\":\"boolean\"}",
			task7_write_mutation("flamework.json", r#"{"profiling":"yes"}"#),
		),
		(
			"runtime dependency warnings",
			"Malformed flamework.json\ntype /disableDependencyWarnings: must be boolean {\"type\":\"boolean\"}\nerror: compile: open native Flamework project: Malformed flamework.json\ntype /disableDependencyWarnings: must be boolean {\"type\":\"boolean\"}",
			task7_write_mutation("flamework.json", r#"{"disableDependencyWarnings":1}"#),
		),
		(
			"native and legacy plugin",
			"[flamework] cannot be combined with the legacy rbxts-transformer-flamework plugin; remove it from tsconfig.json\nerror: [flamework] cannot be combined with the legacy rbxts-transformer-flamework plugin; remove it from tsconfig.json",
			Box::new(|dir: &str| task7_set_plugins(dir, r#"[{"transform":"rbxts-transformer-flamework"}]"#)),
		),
		(
			"duplicate after anchor",
			"<ROOT>/rotor.toml: flamework.after \"duplicate\" matches 2 effective tsconfig transformer plugins; anchor must be unique\nerror: compile: invalid flamework.after",
			Box::new(|dir: &str| {
				task7_write(
					&Path::new(dir).join("rotor.toml"),
					"[flamework]\nafter = \"duplicate\"\n",
				);
				task7_set_plugins(dir, r#"[{"transform":"duplicate"},{"transform":"duplicate"}]"#);
			}),
		),
	];
	task7_invalid_cases(invalid);
}

/// `task7InvalidCase`: the name, the diagnostic tuple and the mutation.
pub type Task7InvalidCase = (&'static str, &'static str, Box<dyn Fn(&str)>);

/// `task7InvalidCases`
pub fn task7_invalid_cases(cases: Vec<Task7InvalidCase>) {
	for (name, tuple, mutate) in cases {
		let project = task7_flamework_project("[flamework]\n");
		let dir = project.dir();
		task7_build(&dir, None);
		mutate(&dir);
		let before = task7_artifact_hash(&dir);
		let (_, diagnostics, error) = build(&dir, &ProjectOptions::default());
		let got = task7_diagnostic_tuple(&dir, &diagnostics, error.as_ref());
		assert!(
			error.is_some() && got == tuple,
			"{name}: invalid build tuple = {got:?}, want {tuple:?}"
		);
		let after = task7_artifact_hash(&dir);
		assert_eq!(
			after, before,
			"{name}: invalid build published artifacts: before={before} after={after}"
		);
	}
}

/// `task7AllOptions`
pub const TASK7_ALL_OPTIONS: &str = "[flamework]\nafter = \"\"\nnoSemanticDiagnostics = true\nobfuscation = true\nidGenerationMode = \"tiny\"\nhashPrefix = \"task7\"\nsalt = \"task7-fixed-salt\"\npreloadIds = true\n\n[flamework.optimizations]\nguardGenerationDedupLimit = 0\n";

/// `task7ConfiguredOptions`
pub fn task7_configured_options(limit: i64) -> FlameworkConfig {
	FlameworkConfig {
		after: String::new(),
		no_semantic_diagnostics: true,
		obfuscation: true,
		id_generation_mode: "tiny".to_owned(),
		hash_prefix: "task7".to_owned(),
		salt: "task7-fixed-salt".to_owned(),
		preload_ids: true,
		optimizations: FlameworkOptimizations {
			guard_generation_dedup_limit: Some(limit),
		},
		..FlameworkConfig::default()
	}
}

/// `task7FlameworkProject`
pub fn task7_flamework_project(config_text: &str) -> Temp {
	let temp = write_project("@scope/task7-acceptance", "");
	enable_incremental_builds(&temp);
	task7_write(&temp.join("rotor.toml"), config_text);
	temp
}

/// `task7Pipeline`: the project's prepared native pipeline. Runs on the caller's thread, which
/// must have the checker's stack.
pub fn task7_pipeline(dir: &str) -> FlameworkPipeline {
	let (abs_dir, program) = new_project_program_with_options(dir, "", &ProjectOptions::default())
		.unwrap_or_else(|failure| panic!("newProjectProgram: {} ({:?})", failure.error, failure.messages()));
	match prepare_flamework_pipeline(&abs_dir, program, &ProjectOptions::default()) {
		Ok(Some(pipeline)) => pipeline,
		Ok(None) => panic!("prepareFlameworkPipeline = (nil, [], nil)"),
		Err(failure) => panic!(
			"prepareFlameworkPipeline = (nil, {:?}, {})",
			failure.messages(),
			failure.error
		),
	}
}

/// `task7Build`
pub fn task7_build(dir: &str, timings: Option<&Arc<BuildTimings>>) {
	let opts = ProjectOptions {
		timings: timings.cloned(),
		..ProjectOptions::default()
	};
	let (_, diagnostics, error) = build(dir, &opts);
	assert!(
		error.is_none() && diagnostics.is_empty(),
		"BuildProjectWithOptions = ({diagnostics:?}, {error:?})"
	);
}

/// `task7ArtifactHash`: the hash of every build-owned artifact (`flamework.build`, `out/`,
/// `include/`).
pub fn task7_artifact_hash(dir: &str) -> String {
	fn walk(root: &Path, directory: &Path, records: &mut Vec<String>) {
		let Ok(entries) = std::fs::read_dir(directory) else {
			return;
		};
		for entry in entries.flatten() {
			let path = entry.path();
			let Ok(file_type) = entry.file_type() else {
				continue;
			};
			if file_type.is_dir() {
				walk(root, &path, records);
				continue;
			}
			if !file_type.is_file() {
				continue;
			}
			let relative = path
				.strip_prefix(root)
				.map(|relative| relative.to_string_lossy().replace('\\', "/"))
				.unwrap_or_default();
			if relative != "flamework.build" && !relative.starts_with("out/") && !relative.starts_with("include/") {
				continue;
			}
			let data = std::fs::read(&path).expect("read artifact");
			records.push(format!("{relative} {}", hex(&Sha256::digest(&data))));
		}
	}
	let mut records = Vec::new();
	walk(Path::new(dir), Path::new(dir), &mut records);
	records.sort();
	hex(&Sha256::digest(records.join("\n").as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
	bytes
		.iter()
		.fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
			let _ = write!(out, "{byte:02x}");
			out
		})
}

/// `task7DiagnosticTuple`
pub fn task7_diagnostic_tuple(dir: &str, diagnostics: &[String], error: Option<&crate::Error>) -> String {
	let mut parts = diagnostics.to_vec();
	if let Some(error) = error {
		parts.push(format!("error: {error}"));
	}
	let mut value = parts.join("\n");
	for root in [
		dir.to_owned(),
		rotor_support::gopath::clean(dir),
		rotor_support::gopath::to_slash(dir).into_owned(),
	] {
		value = value.replace(&root, "<ROOT>");
	}
	value.replace("<ROOT>\\", "<ROOT>/")
}

/// `task7Write`
pub fn task7_write(path: &Path, contents: &str) {
	if let Some(parent) = path.parent() {
		std::fs::create_dir_all(parent).expect("mkdir");
	}
	std::fs::write(path, contents).expect("write");
}

/// `task7Remove`
pub fn task7_remove(path: &Path) {
	std::fs::remove_file(path).expect("remove");
}

/// `task7WriteMutation`
pub fn task7_write_mutation(relative: &'static str, contents: &'static str) -> Box<dyn Fn(&str)> {
	Box::new(move |dir: &str| task7_write(&Path::new(dir).join(relative), contents))
}

/// `task7SetPlugins`
pub fn task7_set_plugins(dir: &str, plugins: &str) {
	let path = Path::new(dir).join("tsconfig.json");
	let data = std::fs::read_to_string(&path).expect("read tsconfig");
	let updated = data.replacen(
		r#""compilerOptions": {"#,
		&format!(r#""compilerOptions": {{"plugins": {plugins},"#),
		1,
	);
	task7_write(&path, &updated);
}
