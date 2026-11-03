//! `incremental_flamework_build_test.go`

use std::sync::Arc;

use sha2::{Digest, Sha256};

use super::output::enable_incremental_builds;
use super::sidecar::build;
use super::write_project;
use crate::incremental_manifest::read_incremental_manifest;
use crate::project::ProjectOptions;
use crate::timings::BuildTimings;

/// `writeFlameworkIncrementalFile`
fn write_flamework_incremental_file(path: &std::path::Path, contents: &str) {
	if let Some(parent) = path.parent() {
		std::fs::create_dir_all(parent).expect("mkdir");
	}
	std::fs::write(path, contents).expect("write");
}

fn timed_build(dir: &str) -> (Arc<BuildTimings>, Option<crate::Error>, Vec<String>) {
	let timings = Arc::new(BuildTimings::new());
	let opts = ProjectOptions {
		timings: Some(Arc::clone(&timings)),
		..ProjectOptions::default()
	};
	let (_, diags, error) = build(dir, &opts);
	(timings, error, diags)
}

type Mutation = Box<dyn Fn()>;

#[test]
fn build_project_flamework_incremental_inputs_rebuild_once() {
	// Given: a native Flamework project with incremental output and one persisted glob.
	let temp = write_project("flamework-incremental-inputs", "");
	let dir = temp.dir();
	enable_incremental_builds(&temp);
	write_flamework_incremental_file(
		&temp.join("default.project.json"),
		r#"{"name":"fixture","tree":{"$className":"DataModel","ReplicatedStorage":{"TS":{"$path":"out"},"rbxts_include":{"$path":"include"}}}}"#,
	);
	write_flamework_incremental_file(
		&temp.join(rotor_config::config::CONFIG_FILE_NAME),
		"[flamework]\nidGenerationMode = \"full\"\n",
	);
	write_flamework_incremental_file(
		&temp.join("flamework.build"),
		r#"{"version":1,"flameworkVersion":"1.3.2","identifiers":{},"metadata":{"globs":{"paths":{"assets/*.txt":[]},"origins":{"manual":["assets/*.txt"]}}}}"#,
	);
	let (_, diags, error) = build(&dir, &ProjectOptions::default());
	assert!(error.is_none(), "seed build: {error:?} (diags: {diags:?})");
	let manifest_path = temp.join("out/cache.rbxtsc.tsbuildinfo").to_string_lossy().into_owned();
	let mut previous_salt = incremental_manifest_salt(&manifest_path);

	let config_path = temp.join(rotor_config::config::CONFIG_FILE_NAME);
	let package_path = temp.join("package.json");
	let asset_path = temp.join("assets/added.txt");
	let mutations: [(&str, Mutation); 3] = [
		(
			"effective config",
			Box::new(move || {
				write_flamework_incremental_file(
					&config_path,
					"[flamework]\nidGenerationMode = \"full\"\nhashPrefix = \"changed\"\n",
				);
			}),
		),
		(
			"package state",
			Box::new(move || {
				write_flamework_incremental_file(
					&package_path,
					r#"{"name":"flamework-incremental-inputs","version":"2.0.0"}"#,
				);
			}),
		),
		(
			"glob matches",
			Box::new(move || write_flamework_incremental_file(&asset_path, "asset\n")),
		),
	];
	for (name, mutate) in mutations {
		mutate();

		// When: one real Flamework input changes and the project builds twice.
		let (rebuild_timings, error, diags) = timed_build(&dir);
		assert!(error.is_none(), "{name}: input rebuild: {error:?} (diags: {diags:?})");
		let current_salt = incremental_manifest_salt(&manifest_path);
		let (warm_timings, error, diags) = timed_build(&dir);
		assert!(error.is_none(), "{name}: warm build: {error:?} (diags: {diags:?})");

		// Then: the input changes the output salt once and the next build is a cache hit.
		assert_ne!(
			current_salt, previous_salt,
			"{name} preserved incremental salt {previous_salt}"
		);
		assert_ne!(
			rebuild_timings.report().counts.selected_sources,
			0,
			"{name} selected no sources"
		);
		let warm = warm_timings.report().counts;
		assert!(
			warm.selected_sources == 0 && warm.actual_writes == 0,
			"{name} warm build selected={} writes={}, want cache hit",
			warm.selected_sources,
			warm.actual_writes
		);
		previous_salt = current_salt;
	}
}

#[test]
fn build_project_flamework_failure_preserves_artifacts_and_incremental_state() {
	// Given: a successful native Flamework incremental build.
	let temp = write_project("flamework-incremental-failure", "");
	let dir = temp.dir();
	enable_incremental_builds(&temp);
	write_flamework_incremental_file(
		&temp.join("default.project.json"),
		r#"{"name":"fixture","tree":{"$className":"DataModel","ReplicatedStorage":{"TS":{"$path":"out"},"rbxts_include":{"$path":"include"}}}}"#,
	);
	write_flamework_incremental_file(&temp.join(rotor_config::config::CONFIG_FILE_NAME), "[flamework]\n");
	let (_, diags, error) = build(&dir, &ProjectOptions::default());
	assert!(error.is_none(), "seed build: {error:?} (diags: {diags:?})");
	let paths = [
		temp.join("out/main.luau"),
		temp.join("out/cache.rbxtsc.tsbuildinfo"),
		temp.join("flamework.build"),
	];
	let before = file_hashes(&paths);
	let original_source = std::fs::read_to_string(temp.join("src/main.ts")).expect("read main.ts");

	// When: a malformed source build fails, then the exact successful source resumes.
	write_flamework_incremental_file(&temp.join("src/main.ts"), "export const broken: = 1;\n");
	let (_, _, error) = build(&dir, &ProjectOptions::default());
	assert!(error.is_some(), "malformed native Flamework build succeeded");
	let after_failure = file_hashes(&paths);
	write_flamework_incremental_file(&temp.join("src/main.ts"), &original_source);
	let (resume_timings, error, diags) = timed_build(&dir);
	assert!(error.is_none(), "resume build: {error:?} (diags: {diags:?})");

	// Then: failure publishes nothing and the last successful cache state remains reusable.
	assert_eq!(before, after_failure, "failed build changed artifact hashes");
	let resume = resume_timings.report().counts;
	assert!(
		resume.selected_sources == 0 && resume.actual_writes == 0,
		"resume selected={} writes={}, want last-success cache hit",
		resume.selected_sources,
		resume.actual_writes
	);
}

/// `incrementalManifestSalt`
fn incremental_manifest_salt(path: &str) -> String {
	read_incremental_manifest(path, None)
		.ok()
		.flatten()
		.unwrap_or_else(|| panic!("read incremental manifest {path}"))
		.salt
}

/// `fileHashes`
pub fn file_hashes<P: AsRef<std::path::Path>>(paths: &[P]) -> Vec<u8> {
	let mut hashes = Vec::new();
	for path in paths {
		let contents = std::fs::read(path).expect("read");
		hashes.extend_from_slice(&Sha256::digest(&contents));
	}
	hashes
}
