//! `output_test.go`

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use rotor_rojo::PathTranslator;

use super::{Temp, large_stack, testdata, write_project};
use crate::declpaths::apply_text_edits;
use crate::output::{
	build_project_with_options, is_output_file_orphaned, reject_duplicate_output_paths, type_reference_edits,
};
use crate::rotortypes::ROTOR_TYPES_FILE_NAME;
use crate::timings::BuildTimings;
use crate::{BuildFailure, BuildResult, ProjectOptions};

pub fn try_build(dir: &str, opts: &ProjectOptions) -> Result<BuildResult, BuildFailure> {
	large_stack(|| build_project_with_options(dir, opts))
}

pub fn build(dir: &str, opts: &ProjectOptions) -> BuildResult {
	match try_build(dir, opts) {
		Ok(result) => result,
		Err(failure) => panic!(
			"BuildProjectWithOptions: {} (diags: {:?})",
			failure.failure,
			failure.failure.messages()
		),
	}
}

/// Rewrites the config's `"outDir": "out"` to `"outDir": "out"` plus `extra`.
pub fn patch_out_dir(temp: &Temp, extra: &str) {
	let config = fs::read_to_string(temp.join("tsconfig.json")).expect("the config reads");
	let patched = config.replacen(r#""outDir": "out""#, &format!(r#""outDir": "out"{extra}"#), 1);
	assert_ne!(patched, config, "the config has no outDir to patch");
	temp.write("tsconfig.json", patched);
}

/// `enableIncrementalBuilds`
pub fn enable_incremental_builds(temp: &Temp) {
	patch_out_dir(
		temp,
		",\n\t\t\"incremental\": true,\n\t\t\"tsBuildInfoFile\": \"out/cache.tsbuildinfo\"",
	);
}

pub fn set_mtime(path: &Path, time: SystemTime) {
	let file = fs::File::options().write(true).open(path).expect("the file opens");
	file.set_times(fs::FileTimes::new().set_accessed(time).set_modified(time))
		.expect("the times set");
}

fn base_name(path: &str) -> &str {
	Path::new(path).file_name().and_then(|name| name.to_str()).unwrap_or("")
}

#[test]
fn build_project_output_pipeline() {
	let temp = write_project("@scope/output-fixture", "");
	enable_incremental_builds(&temp);
	temp.write("src/data.json", "{\"ok\":true}\n");
	temp.write("src/script.luau", "print(\"copied\")\n");
	temp.write("src/types.d.ts", "declare const TAG: string;\n");
	fs::create_dir_all(temp.join("out/.git")).expect("out/.git");
	for (path, text) in [
		("out/stale.luau", "-- stale\n"),
		("out/nested/old.json", "{}\n"),
		("out/.git/keep", "keep\n"),
		("out/cache.rbxtsc.tsbuildinfo", "{}\n"),
	] {
		temp.write(path, text);
	}

	let result = build(&temp.dir(), &ProjectOptions::default());
	assert!(result.diagnostics.is_empty(), "diagnostics: {:?}", result.diagnostics);

	for rel in ["out/main.luau", "out/data.json", "out/script.luau"] {
		assert!(temp.join(rel).exists(), "{rel} missing after build");
	}
	assert!(
		!temp.join("out/types.d.ts").exists(),
		"out/types.d.ts exists, want not-exist"
	);
	assert!(
		!temp.join("out/stale.luau").exists(),
		"stale output exists, want removed"
	);
	assert!(
		!temp.join("out/nested/old.json").exists(),
		"nested orphan exists, want removed"
	);
	assert!(temp.join("out/.git/keep").exists(), ".git sentinel missing");
	assert!(temp.join("out/cache.rbxtsc.tsbuildinfo").exists(), "build info missing");
	assert!(
		result.emitted_files.len() == 1 && base_name(&result.emitted_files[0]) == "main.luau",
		"EmittedFiles = {:?}, want only compiled main.luau",
		result.emitted_files
	);
}

#[test]
fn build_project_reports_complete_owned_outputs_across_no_change_builds() {
	let temp = write_project("@scope/owned-outputs-fixture", "");
	patch_out_dir(
		&temp,
		r#", "declaration": true, "declarationMap": true, "incremental": true, "sourceMap": true, "tsBuildInfoFile": "out/cache.tsbuildinfo""#,
	);
	temp.write("src/data.json", "{\"ok\":true}\n");

	let want = [
		"out/data.json",
		"out/globals.d.ts",
		"out/main.d.ts",
		"out/main.d.ts.map",
		"out/main.luau",
		"out/main.luau.map",
	];
	for attempt in 1..=2 {
		let result = build(&temp.dir(), &ProjectOptions::default());
		assert_eq!(result.owned_outputs, want, "build {attempt} OwnedOutputs");
	}
}

#[test]
fn source_map_emission_enabled() {
	let temp = write_project("@scope/source-map-fixture", "");
	let source = "export const answer = 42;\n";
	temp.write("src/main.ts", source);
	patch_out_dir(&temp, r#", "sourceMap": true"#);

	let result = build(&temp.dir(), &ProjectOptions::default());

	let map_path = temp.join("out/main.luau.map");
	let map_bytes = fs::read(&map_path).expect("read emitted source map");
	let fields: serde_json::Map<String, serde_json::Value> =
		serde_json::from_slice(&map_bytes).expect("unmarshal source map fields");
	assert_eq!(
		fields.len(),
		5,
		"source map fields = {fields:?}, want exactly version, file, sources, sourcesContent, mappings"
	);
	assert_eq!(fields["version"], 3, "version");
	assert_eq!(fields["file"], "main.luau", "file");
	assert_eq!(fields["sources"], serde_json::json!(["../src/main.ts"]), "sources");
	assert_eq!(fields["sourcesContent"], serde_json::json!([source]), "sourcesContent");
	assert!(
		fields["mappings"].as_str().is_some_and(|mappings| !mappings.is_empty()),
		"mappings is empty"
	);
	for emitted in &result.emitted_files {
		assert!(
			Path::new(emitted)
				.extension()
				.is_none_or(|extension| extension != "map"),
			"EmittedFiles includes source map {emitted:?}"
		);
	}

	let src_dir = temp.join("src");
	let src_mtime = fs::metadata(&src_dir)
		.and_then(|meta| meta.modified())
		.expect("src mtime");
	fs::remove_file(src_dir.join("main.ts")).expect("main.ts removes");
	fs::File::open(&src_dir)
		.and_then(|dir| dir.set_modified(src_mtime))
		.expect("src mtime restores");
	build(&temp.dir(), &ProjectOptions::default());
	assert!(!map_path.exists(), "orphaned source map exists, want not-exist");
}

#[test]
fn source_map_emission_is_independent_of_the_checkout_path() {
	let build_map = |name: &str| {
		let temp = write_project(name, "");
		temp.write("src/main.ts", "export const answer = 42;\n");
		patch_out_dir(&temp, r#", "sourceMap": true"#);
		build(&temp.dir(), &ProjectOptions::default());
		fs::read(temp.join("out/main.luau.map")).expect("the map reads")
	};
	let first = build_map("@scope/source-map-portable-one");
	let second = build_map("@scope/source-map-portable-two");
	assert_eq!(
		String::from_utf8_lossy(&first),
		String::from_utf8_lossy(&second),
		"source maps differ across checkout paths"
	);
}

#[test]
fn source_map_emission_disabled() {
	let temp = write_project("@scope/source-map-disabled-fixture", "");
	build(&temp.dir(), &ProjectOptions::default());
	assert!(
		!temp.join("out/main.luau.map").exists(),
		"source map with sourceMap disabled exists, want not-exist"
	);
}

#[test]
fn rbxtsc_build_info_path() {
	let temp = write_project("@scope/rbxtsc-build-info-fixture", "");
	enable_incremental_builds(&temp);
	build(&temp.dir(), &ProjectOptions::default());
	assert!(
		temp.join("out/cache.rbxtsc.tsbuildinfo").exists(),
		"suffixed build info missing"
	);
	assert!(
		!temp.join("out/cache.tsbuildinfo").exists(),
		"unsuffixed build info exists, want not-exist"
	);
}

#[test]
fn build_info_failure_rollback() {
	let temp = write_project("@scope/build-info-rollback-fixture", "");
	enable_incremental_builds(&temp);
	build(&temp.dir(), &ProjectOptions::default());

	let build_info_path = temp.join("out/cache.rbxtsc.tsbuildinfo");
	let prior = fs::read(&build_info_path).expect("read prior build info");
	let output_path = temp.join("out/main.luau");
	fs::remove_file(&output_path).expect("remove output");
	fs::create_dir(&output_path).expect("make output directory");
	temp.write("src/main.ts", "export const value = 2;\n");

	assert!(
		try_build(&temp.dir(), &ProjectOptions::default()).is_err(),
		"expected emit failure"
	);
	let after = fs::read(&build_info_path).expect("read rolled-back build info");
	assert_eq!(after, prior, "build info changed after failed emit");
}

#[test]
fn build_info_failure_after_output_persistence_rollback() {
	let temp = write_project("@scope/build-info-persistence-rollback-fixture", "");
	enable_incremental_builds(&temp);
	build(&temp.dir(), &ProjectOptions::default());

	let build_info_path = temp.join("out/cache.rbxtsc.tsbuildinfo");
	let prior = fs::read(&build_info_path).expect("read prior build info");
	temp.write(
		"src/main.ts",
		"export const value = 1;\nexport const name = $nameof(value);\n",
	);
	fs::create_dir(temp.join(ROTOR_TYPES_FILE_NAME)).expect("a directory where rotor.d.ts goes");

	assert!(
		try_build(&temp.dir(), &ProjectOptions::default()).is_err(),
		"expected rotor types persistence failure"
	);
	let after = fs::read(&build_info_path).expect("read build info");
	assert_eq!(after, prior, "build info changed after post-output persistence failure");
}

#[test]
fn duplicate_output_guard() {
	let temp = Temp::new();
	let first = temp.join("out/same.luau").to_string_lossy().into_owned();
	let second = temp.join("out/./same.luau").to_string_lossy().into_owned();
	assert!(
		reject_duplicate_output_paths(&[first, second]).is_err(),
		"expected duplicate output error"
	);
	assert!(!temp.join("out").exists(), "output directory exists, want not-exist");
}

#[test]
fn build_project_write_only_changed_skips_unchanged_outputs() {
	let temp = write_project("@scope/write-only-fixture", "");
	temp.write("src/data.json", "{\"same\":true}\n");

	let opts = ProjectOptions {
		write_only_changed: true,
		..ProjectOptions::default()
	};
	let result = build(&temp.dir(), &opts);
	assert!(
		result.diagnostics.is_empty(),
		"first build diagnostics: {:?}",
		result.diagnostics
	);
	assert_eq!(
		result.emitted_files.len(),
		1,
		"first build EmittedFiles = {:?}",
		result.emitted_files
	);

	let old = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
	let main_out = temp.join("out/main.luau");
	let json_out = temp.join("out/data.json");
	for path in [&main_out, &json_out] {
		set_mtime(path, old);
	}

	let result = build(&temp.dir(), &opts);
	assert!(
		result.diagnostics.is_empty(),
		"second build diagnostics: {:?}",
		result.diagnostics
	);
	assert!(
		result.emitted_files.is_empty(),
		"second build EmittedFiles = {:?}, want none",
		result.emitted_files
	);
	for path in [&main_out, &json_out] {
		let modified = fs::metadata(path)
			.and_then(|meta| meta.modified())
			.expect("the output's mtime");
		assert_eq!(modified, old, "{} modtime, want preserved", path.display());
	}
}

#[test]
fn build_project_hash_match_recreates_missing_output() {
	let temp = write_project("@scope/missing-output-fixture", "");
	build(&temp.dir(), &ProjectOptions::default());
	let output_path = temp.join("out/main.luau");
	fs::remove_file(&output_path).expect("the output removes");

	let result = build(&temp.dir(), &ProjectOptions::default());
	assert_eq!(
		result.emitted_files,
		[output_path.to_string_lossy().into_owned()],
		"emitted files"
	);
	assert!(output_path.exists(), "recreated output missing");
}

#[test]
fn build_project_lua_extension() {
	let temp = write_project("@scope/lua-ext-fixture", "");
	patch_out_dir(&temp, r#", "sourceMap": true"#);

	let result = build(
		&temp.dir(),
		&ProjectOptions {
			lua_extension: true,
			..ProjectOptions::default()
		},
	);
	assert!(result.diagnostics.is_empty(), "diagnostics: {:?}", result.diagnostics);
	assert!(temp.join("out/main.lua").exists(), "out/main.lua missing");
	assert!(
		!temp.join("out/main.luau").exists(),
		"out/main.luau exists, want not-exist"
	);
	let map: serde_json::Value =
		serde_json::from_slice(&fs::read(temp.join("out/main.lua.map")).expect("read source map"))
			.expect("unmarshal source map");
	assert_eq!(map["file"], "main.lua", "source map file");
}

#[test]
fn build_project_emits_declarations_for_package() {
	let temp = write_project("@scope/declaration-fixture", "");
	patch_out_dir(&temp, ",\n\t\t\"declaration\": true,\n\t\t\"types\": [\"types\"]");
	temp.write(
		"node_modules/@rbxts/types/package.json",
		"{\"name\":\"@rbxts/types\",\"types\":\"index.d.ts\"}\n",
	);
	temp.write(
		"node_modules/@rbxts/types/index.d.ts",
		"interface TypesBox {\n\tmarker: string;\n}\n",
	);
	temp.write(
		"src/main.ts",
		"export interface Box {\n\tvalue: TypesBox;\n}\nexport const value = undefined as unknown as TypesBox;\n",
	);

	let result = build(&temp.dir(), &ProjectOptions::default());
	assert!(result.diagnostics.is_empty(), "diagnostics: {:?}", result.diagnostics);

	let decl_text = fs::read_to_string(temp.join("out/main.d.ts")).expect("read declaration output");
	assert!(
		decl_text.contains("export interface Box") && decl_text.contains("export declare const value: TypesBox;"),
		"unexpected declaration output:\n{decl_text}"
	);
	assert_eq!(
		result.emitted_files.len(),
		2,
		"EmittedFiles = {:?}, want compiled file + declaration",
		result.emitted_files
	);

	let timings = Arc::new(BuildTimings::new());
	let warm = build(
		&temp.dir(),
		&ProjectOptions {
			timings: Some(Arc::clone(&timings)),
			..ProjectOptions::default()
		},
	);
	let counts = timings.report().counts;
	assert!(
		warm.emitted_files.is_empty() && counts.actual_writes == 0 && counts.hash_skips >= 2,
		"warm emitted={:?} counts={counts:?}",
		warm.emitted_files
	);
}

#[test]
fn build_project_declaration_only_emits_declarations() {
	let temp = write_project("@scope/declaration-only-fixture", "");
	patch_out_dir(&temp, r#", "declaration": true"#);

	let result = build(
		&temp.dir(),
		&ProjectOptions {
			emit_declaration_only: true,
			..ProjectOptions::default()
		},
	);
	assert!(
		result.emitted_files.len() == 1 && base_name(&result.emitted_files[0]) == "main.d.ts",
		"EmittedFiles = {:?}, want main.d.ts",
		result.emitted_files
	);
	assert!(temp.join("out/main.d.ts").exists(), "declaration output missing");
	assert!(
		!temp.join("out/main.luau").exists(),
		"Luau output exists, want not exists"
	);
}

#[test]
fn build_result_carries_structured_diagnostics() {
	let Err(failure) = try_build(&testdata("env_diag_model"), &ProjectOptions::default()) else {
		panic!("expected a diagnostic error");
	};
	let result = failure.result.expect("BuildResult with the diagnostics");
	assert!(!result.diagnostics.is_empty(), "BuildResult.Diagnostics empty");
	assert_eq!(
		failure.failure.messages().len(),
		result.diagnostics.len(),
		"msgs and structured diags length mismatch"
	);
	assert!(
		result
			.diagnostics
			.iter()
			.any(|info| !info.file_name.is_empty() && info.len > 0),
		"no structured diagnostic carried a location: {:?}",
		result.diagnostics
	);
}

#[test]
fn rewrite_declaration_type_references() {
	let input = "/// <reference types=\"types\" />\n/// <reference types=\"other\" />\n";
	let got = apply_text_edits(input, &type_reference_edits(input));
	assert!(
		got.contains("/// <reference types=\"@rbxts/types\" />"),
		"got {got:?}, want rewritten @rbxts/types reference"
	);
	assert!(
		got.contains("/// <reference types=\"other\" />"),
		"got {got:?}, want unrelated reference preserved"
	);
	assert!(
		!got.contains("/// <reference types=\"types\" />"),
		"got {got:?}, want raw types reference removed"
	);
}

#[test]
fn is_output_file_orphaned_dts_map() {
	let temp = Temp::new();
	let src_dir = temp.join("src").to_string_lossy().into_owned();
	let out_dir = temp.join("out").to_string_lossy().into_owned();
	temp.write("src/foo.ts", "export {};\n");
	temp.write("out/foo.d.ts.map", "{}");
	let map_path = temp.join("out/foo.d.ts.map").to_string_lossy().into_owned();
	let translator = PathTranslator::new(&src_dir, &out_dir, "", true, true);

	assert!(
		!is_output_file_orphaned(&translator, &map_path),
		"expected .d.ts.map to be preserved when its source exists"
	);
	fs::remove_file(temp.join("src/foo.ts")).expect("the source removes");
	assert!(
		is_output_file_orphaned(&translator, &map_path),
		"expected .d.ts.map to be orphaned when its source was deleted"
	);
}

/// Cleanup removes a copied nested Rojo project whose source is gone. The build's Rojo cache,
/// loaded before cleanup and trusted for the build, must see the tree without it by project
/// context, as Go's load there does; otherwise an import into the old partition keeps its path.
#[test]
fn cleanup_of_a_copied_rojo_project_reaches_project_context() {
	let temp = write_project(
		"cleanup-rojo-project",
		r#"{"name":"game","tree":{"$className":"DataModel","ReplicatedStorage":{"out":{"$path":"out"},"include":{"$path":"include"}}}}"#,
	);
	enable_incremental_builds(&temp);
	temp.write(
		"src/sub/default.project.json",
		r#"{"name":"inner","tree":{"$path":"code"}}"#,
	);
	temp.write("src/sub/code/x.ts", "export const x = 1;\n");
	temp.write(
		"src/main.ts",
		"import { x } from \"./sub/code/x\";\nexport const y = x;\n",
	);
	build(&temp.dir(), &ProjectOptions::default());
	let first = fs::read_to_string(temp.join("out/main.luau")).expect("main.luau");
	assert!(
		temp.join("out/sub/default.project.json").exists(),
		"the nested project was not copied"
	);

	fs::remove_file(temp.join("src/sub/default.project.json")).expect("remove the nested project");
	temp.write(
		"src/main.ts",
		"import { x } from \"./sub/code/x\";\nexport const y = x;\nexport const z = x;\n",
	);
	build(&temp.dir(), &ProjectOptions::default());
	let second = fs::read_to_string(temp.join("out/main.luau")).expect("main.luau");

	assert!(
		!temp.join("out/sub/default.project.json").exists(),
		"cleanup kept the orphaned nested project"
	);
	let import = |text: &str| text.lines().find(|line| line.contains("TS.import")).map(str::to_owned);
	assert_eq!(
		import(&second),
		import(&first),
		"the import resolved through the removed nested project:\n{second}"
	);
}
