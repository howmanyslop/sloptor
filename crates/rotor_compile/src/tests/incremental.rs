//! `incremental_test.go` and `skip_identical_writes_test.go`

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use super::output::{build, enable_incremental_builds, patch_out_dir, set_mtime};
use super::{Temp, large_stack, write_project};
#[cfg(unix)]
use super::{expect_compiled, output};
use crate::incremental_manifest::{normalize_source_file_path, read_incremental_manifest, write_incremental_manifest};
#[cfg(unix)]
use crate::project::compile_project;
use crate::project::new_project_program_with_options;
use crate::timings::{BuildTimingCounts, BuildTimings};
use crate::{BuildResult, ProjectOptions};

fn old() -> SystemTime {
	SystemTime::UNIX_EPOCH + Duration::from_secs(100)
}

fn mtime(path: &Path) -> Option<SystemTime> {
	fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

/// A build with fresh timings; returns the result and the counts.
fn timed_build(dir: &str) -> (BuildResult, BuildTimingCounts) {
	let timings = Arc::new(BuildTimings::new());
	let result = build(
		dir,
		&ProjectOptions {
			timings: Some(Arc::clone(&timings)),
			..ProjectOptions::default()
		},
	);
	(result, timings.report().counts)
}

/// `writeIncrementalFixture`
fn write_incremental_fixture(temp: &Temp) {
	temp.write(
		"src/main.ts",
		"import { VALUE } from \"./util\";\nexport const main = VALUE;\n",
	);
	temp.write("src/util.ts", "export const VALUE = 1;\n");
	temp.write("src/side.ts", "export const side = 1;\n");
}

/// `emittedFileBases`
pub fn emitted_file_bases(result: &BuildResult) -> Vec<String> {
	let mut bases: Vec<String> = result
		.emitted_files
		.iter()
		.filter_map(|path| Path::new(path).file_name())
		.map(|name| name.to_string_lossy().into_owned())
		.collect();
	bases.sort();
	bases
}

#[test]
fn incremental_manifest_v1_hard_cutover() {
	let temp = Temp::new();
	temp.write("manifest.json", r#"{"salt":"old","files":{}}"#);
	let manifest = read_incremental_manifest(&temp.join("manifest.json").to_string_lossy(), None)
		.unwrap_or_else(|error| panic!("{error}"));
	assert!(manifest.is_none(), "v1 manifest accepted: {manifest:?}");
}

#[test]
fn build_project_incremental_rebuilds_changed_files_and_importers() {
	let temp = write_project("@scope/incremental-fixture", "");
	enable_incremental_builds(&temp);
	write_incremental_fixture(&temp);

	let first = build(&temp.dir(), &ProjectOptions::default());
	assert!(
		first.diagnostics.is_empty(),
		"first build diagnostics: {:?}",
		first.diagnostics
	);

	let build_info = fs::read_to_string(temp.join("out/cache.rbxtsc.tsbuildinfo")).expect("read build info");
	assert!(
		build_info.contains("\"salt\""),
		"build info = {build_info:?}, want rotor incremental manifest JSON"
	);

	for rel in ["out/main.luau", "out/util.luau", "out/side.luau"] {
		set_mtime(&temp.join(rel), old());
	}
	temp.write("src/util.ts", "export const VALUE = 2;\n");

	let second = build(&temp.dir(), &ProjectOptions::default());
	assert!(
		second.diagnostics.is_empty(),
		"second build diagnostics: {:?}",
		second.diagnostics
	);
	assert_eq!(emitted_file_bases(&second), ["util.luau"], "second build emitted files");
	assert_eq!(
		mtime(&temp.join("out/side.luau")),
		Some(old()),
		"side.luau modtime, want preserved"
	);
}

#[test]
fn build_project_incremental_recreates_missing_outputs() {
	let all_outputs = [
		"out/main.luau",
		"out/main.luau.map",
		"out/main.d.ts",
		"out/main.d.ts.map",
	];
	for relative_path in all_outputs {
		let temp = write_project("@scope/incremental-missing-output-fixture", "");
		enable_incremental_builds(&temp);
		patch_out_dir(
			&temp,
			r#", "sourceMap": true, "declaration": true, "declarationMap": true"#,
		);
		build(&temp.dir(), &ProjectOptions::default());
		for output in all_outputs {
			set_mtime(&temp.join(output), old());
		}

		let path = temp.join(relative_path);
		let want = fs::read(&path).expect("the output reads");
		fs::remove_file(&path).expect("the output removes");

		let (_, counts) = timed_build(&temp.dir());
		let got = fs::read(&path)
			.unwrap_or_else(|error| panic!("{relative_path}: missing output was not recreated: {error}"));
		assert_eq!(
			got, want,
			"{relative_path}: recreated output differs from the first build"
		);
		if counts.actual_writes != 1 {
			let changed: Vec<&str> = all_outputs
				.into_iter()
				.filter(|output| mtime(&temp.join(output)) != Some(old()))
				.collect();
			panic!(
				"{relative_path}: actual writes = {}, want 1; changed outputs = {changed:?}",
				counts.actual_writes
			);
		}
	}
}

#[test]
fn build_project_incremental_repairs_externally_corrupted_regular_output() {
	let temp = write_project("@scope/incremental-corrupt-output-fixture", "");
	enable_incremental_builds(&temp);
	let output_path = temp.join("out/main.luau");
	build(&temp.dir(), &ProjectOptions::default());
	let want = fs::read(&output_path).expect("read seed output");
	let mut corrupt = want.clone();
	corrupt.extend_from_slice(b"\nSTALE-MARKER\n");
	fs::write(&output_path, corrupt).expect("corrupt output");

	let (result, counts) = timed_build(&temp.dir());

	let got = fs::read(&output_path).expect("read repaired output");
	assert_eq!(got, want, "repaired output differs from seed output");
	assert_ne!(counts.actual_writes, 0, "actual writes = 0, want nonzero");
	assert!(
		!result.outputs.is_empty(),
		"compiled outputs = none, want selected repaired output"
	);
	assert!(
		!result.emitted_files.is_empty(),
		"emitted files = none, want repaired output"
	);
}

#[test]
fn build_project_incremental_no_change_keeps_manifest_and_outputs() {
	let temp = write_project("@scope/incremental-nochange-fixture", "");
	enable_incremental_builds(&temp);
	write_incremental_fixture(&temp);
	build(&temp.dir(), &ProjectOptions::default());

	let manifest_path = temp.join("out/cache.rbxtsc.tsbuildinfo");
	let before = fs::read(&manifest_path).expect("the manifest reads");
	let output_paths = ["out/main.luau", "out/util.luau", "out/side.luau"];
	for rel in output_paths {
		set_mtime(&temp.join(rel), old());
	}

	let (result, counts) = timed_build(&temp.dir());
	assert_eq!(counts.selected_sources, 0, "selected sources");
	assert_eq!(counts.actual_writes, 0, "actual writes");
	assert!(
		result.emitted_files.is_empty(),
		"emitted files = {:?}, want none",
		result.emitted_files
	);
	let after = fs::read(&manifest_path).expect("the manifest reads");
	assert_eq!(before, after, "no-change build rewrote the incremental manifest");
	for rel in output_paths {
		assert_eq!(mtime(&temp.join(rel)), Some(old()), "{rel} modtime, want preserved");
	}
}

#[test]
fn build_incremental_manifest_recomputes_all_references_after_source_change() {
	let temp = write_project("@scope/incremental-refs-fixture", "");
	enable_incremental_builds(&temp);
	temp.write(
		"src/main.ts",
		"import { VALUE } from \"./util\";\nexport const main = VALUE;\n",
	);
	temp.write("src/util.ts", "export const VALUE = 1;\n");
	temp.write(
		"src/side.ts",
		"import { VALUE } from \"./util\";\nexport const side = VALUE;\n",
	);
	build(&temp.dir(), &ProjectOptions::default());

	let manifest_path = temp.join("out/cache.rbxtsc.tsbuildinfo").to_string_lossy().into_owned();
	let key = |rel: &str| normalize_source_file_path(&temp.join(rel).to_string_lossy());
	let (main_key, util_key, side_key) = (key("src/main.ts"), key("src/util.ts"), key("src/side.ts"));
	let dir = temp.dir();
	let (expected_refs, rebuilt) = large_stack(|| {
		let (_, program) = match new_project_program_with_options(&dir, "", &ProjectOptions::default()) {
			Ok(loaded) => loaded,
			Err(failure) => panic!("read project: {failure} (diags: {:?})", failure.messages()),
		};
		let mut seeded = read_incremental_manifest(&manifest_path, Some(program))
			.ok()
			.flatten()
			.expect("read seeded manifest");
		let side_state = seeded.files.get_mut(&side_key).expect("side.ts state");
		assert!(
			side_state.refs.contains(&util_key),
			"seeded side.ts refs = {:?}, want dependency {util_key}",
			side_state.refs
		);
		let expected_refs = side_state.refs.clone();
		side_state.refs = vec!["sentinel".to_owned()];
		write_incremental_manifest(&manifest_path, &seeded, program).unwrap_or_else(|error| panic!("{error}"));

		std::fs::write(Path::new(&dir).join("src/util.ts"), "export const VALUE = 2;\n").expect("util.ts writes");
		if let Err(failure) = crate::output::build_project_with_options(&dir, &ProjectOptions::default()) {
			panic!(
				"second build: {} (diags: {:?})",
				failure.failure,
				failure.failure.messages()
			);
		}
		let rebuilt = read_incremental_manifest(&manifest_path, Some(program))
			.ok()
			.flatten()
			.expect("read rebuilt manifest");
		(expected_refs, rebuilt)
	});
	assert_eq!(
		rebuilt.files[&side_key].refs, expected_refs,
		"side.ts refs, want recomputed"
	);
	assert_eq!(
		rebuilt.files[&main_key].refs, expected_refs,
		"main.ts refs, want recomputed"
	);
}

#[test]
fn build_project_incremental_invalidates_emit_option_changes() {
	let temp = write_project("@scope/incremental-options-fixture", "");
	enable_incremental_builds(&temp);
	build(&temp.dir(), &ProjectOptions::default());

	patch_out_dir(&temp, r#", "sourceMap": true"#);
	let (_, counts) = timed_build(&temp.dir());
	assert_eq!(counts.selected_sources, 1, "selected sources");
	assert!(
		temp.join("out/main.luau.map").exists(),
		"source map missing after sourceMap option change"
	);
}

#[test]
fn build_project_incremental_invalidates_path_changes() {
	let temp = write_project("@scope/incremental-paths-fixture", "");
	enable_incremental_builds(&temp);
	temp.write(
		"src/main.ts",
		"import { value } from \"alias\";\nexport const result = value;\n",
	);
	temp.write("src/one.ts", "export const value = 1;\n");
	temp.write("src/two.ts", "export const value = 2;\n");
	patch_out_dir(&temp, r#", "baseUrl": ".", "paths": {"alias": ["src/one"]}"#);
	build(&temp.dir(), &ProjectOptions::default());

	let config = fs::read_to_string(temp.join("tsconfig.json")).expect("the config reads");
	temp.write("tsconfig.json", config.replacen(r#""src/one""#, r#""src/two""#, 1));
	let (_, counts) = timed_build(&temp.dir());
	assert_ne!(counts.selected_sources, 0, "paths option change selected no sources");
}

#[test]
#[cfg(unix)]
fn build_project_incremental_invalidates_dependency_rojo_name_changes() {
	let temp = super::pnpm::write_pnpm_fixture();
	enable_incremental_builds(&temp);
	build(&temp.dir(), &ProjectOptions::default());
	let (_, unchanged) = timed_build(&temp.dir());
	assert_eq!(unchanged.selected_sources, 0, "unchanged build selected sources");

	let package_config =
		temp.join("node_modules/.pnpm/@rbxts+linked@1.0.0/node_modules/@rbxts/linked/default.project.json");
	let modified = mtime(&package_config).expect("the package config's mtime");
	fs::write(&package_config, r#"{"name":"renamed","tree":{"$path":"src"}}"#).expect("the package config writes");
	set_mtime(&package_config, modified);
	let (_, counts) = timed_build(&temp.dir());
	let got = fs::read_to_string(temp.join("out/main.luau")).expect("incremental build left no out/main.luau");

	let clean = expect_compiled(large_stack(|| compile_project(&temp.dir())));
	assert_eq!(
		got,
		output(&clean, "out/main.luau"),
		"incremental out/main.luau is stale"
	);
	assert_ne!(counts.selected_sources, 0, "Rojo project rename selected no sources");
}

/// `TestBuildProjectRebuildLeavesUnchangedOutputMtimes` (`skip_identical_writes_test.go`).
#[test]
fn build_project_rebuild_leaves_unchanged_output_mtimes() {
	for incremental in [false, true] {
		let temp = write_project("@scope/skip-writes-fixture", "");
		if incremental {
			enable_incremental_builds(&temp);
		}
		assert_rebuild_keeps_output_mtimes(&temp, incremental);
	}
}

fn assert_rebuild_keeps_output_mtimes(temp: &Temp, incremental: bool) {
	write_incremental_fixture(temp);
	for _ in 0..2 {
		let result = build(&temp.dir(), &ProjectOptions::default());
		assert!(
			result.diagnostics.is_empty(),
			"seed build diagnostics: {:?}",
			result.diagnostics
		);
	}

	let outputs = age_outputs(temp, old());
	build(&temp.dir(), &ProjectOptions::default());
	let changed = changed_outputs(temp, &outputs, old());
	assert!(
		changed.is_empty(),
		"incremental={incremental}: no-change build rewrote {changed:?}"
	);

	temp.write("src/side.ts", "export const side = 2;\n");
	build(&temp.dir(), &ProjectOptions::default());
	for rel in changed_outputs(temp, &outputs, old()) {
		assert!(
			rel.starts_with("out/side.") || rel.ends_with(".tsbuildinfo"),
			"incremental={incremental}: edit build rewrote unrelated output {rel}"
		);
	}
}

/// `ageOutputs`
fn age_outputs(temp: &Temp, at: SystemTime) -> Vec<String> {
	fn walk(root: &Path, dir: &Path, at: SystemTime, outputs: &mut Vec<String>) {
		let mut entries: Vec<_> = fs::read_dir(dir).expect("the directory reads").flatten().collect();
		entries.sort_by_key(fs::DirEntry::file_name);
		for entry in entries {
			let path = entry.path();
			let rel = path
				.strip_prefix(root)
				.map(|rel| rel.to_string_lossy().replace('\\', "/"))
				.unwrap_or_default();
			if path.is_dir() {
				if rel != "src" && rel != "node_modules" {
					walk(root, &path, at, outputs);
				}
				continue;
			}
			if rel.starts_with("out/") || rel.starts_with("include/") {
				set_mtime(&path, at);
				outputs.push(rel);
			}
		}
	}
	let mut outputs = Vec::new();
	walk(temp.join("").as_path(), temp.join("").as_path(), at, &mut outputs);
	assert!(!outputs.is_empty(), "no outputs found");
	outputs
}

/// `changedOutputs`
fn changed_outputs(temp: &Temp, outputs: &[String], at: SystemTime) -> Vec<String> {
	let mut changed: Vec<String> = outputs
		.iter()
		.filter(|rel| mtime(&temp.join(rel)) != Some(at))
		.cloned()
		.collect();
	changed.sort();
	changed
}

/// The salt reads path options relative to the project (`portableIncrementalOptions`), so the
/// same project in two checkouts gets the same salt, `paths` targets included.
#[test]
fn incremental_salt_survives_a_checkout_move() {
	let salt = || {
		let temp = write_project("@scope/portable-salt", "");
		enable_incremental_builds(&temp);
		patch_out_dir(&temp, r#", "baseUrl": ".", "paths": {"alias/*": ["src/*"]}"#);
		temp.write(
			"src/main.ts",
			"import { value } from \"alias/value\";\nexport const result = value;\n",
		);
		temp.write("src/value.ts", "export const value = 1;\n");
		build(&temp.dir(), &ProjectOptions::default());
		let manifest: serde_json::Value =
			serde_json::from_slice(&fs::read(temp.join("out/cache.rbxtsc.tsbuildinfo")).expect("the manifest reads"))
				.expect("the manifest parses");
		manifest["salt"].as_str().unwrap_or_default().to_owned()
	};
	let (first, second) = (salt(), salt());
	assert!(!first.is_empty(), "the manifest has no salt");
	assert_eq!(first, second, "the salt depends on the checkout path");
}
