//! `copyfilesgate_test.go`

use std::collections::HashSet;
use std::time::{Duration, SystemTime};

use rotor_rojo::PathTranslator;

use super::Temp;
use super::output::set_mtime;
use crate::copyfilesgate::{
	BuildStateSnapshot, COPY_FILES_CACHE_FILENAME, COPY_FILES_RESOLVER_CACHE_FILENAME,
	COPY_FILES_TYPE_ROOTS_CACHE_FILENAME, CopyFilesGateInputs, CopyFilesManifest, build_copy_files_manifest,
	build_copy_files_manifest_key, load_copy_files_gate_pre_build,
};
use crate::output::{copy_non_compiled_files, is_output_file_orphaned};

fn path(temp: &Temp, rel: &str) -> String {
	temp.join(rel).to_string_lossy().into_owned()
}

#[test]
fn copy_files_manifest() {
	let temp = Temp::new();
	let (root_dir, out_dir) = (path(&temp, "src"), path(&temp, "out"));
	temp.write("src/asset.luau", "asset");
	temp.write("src/nested/data.json", "{}");
	temp.write("src/source.ts", "export {}");
	temp.write("src/node_modules/pkg/ignored.luau", "ignored");
	temp.write("out/asset.luau", "asset");
	std::fs::create_dir_all(temp.join("out/nested")).expect("out/nested");

	let translator = PathTranslator::new(&root_dir, &out_dir, "", false, true);
	let roots = vec![root_dir];
	let key = build_copy_files_manifest_key(&out_dir, &roots);
	let manifest = build_copy_files_manifest(&roots, &out_dir, false, &key, &translator);

	assert_eq!(manifest.files.len(), 2, "manifest files");
	assert_eq!(manifest.dirs.len(), 2, "manifest dirs, want root and nested");
	let asset = path(&temp, "src/asset.luau");
	for file in &manifest.files {
		assert!(
			file.src_mtime_ms > 0,
			"source mtime = {}, want positive",
			file.src_mtime_ms
		);
		if file.src == asset {
			assert!(
				file.dest_mtime_ms > 0,
				"destination mtime = {}, want positive",
				file.dest_mtime_ms
			);
		}
	}
	let decoded = CopyFilesManifest::from_json(manifest.to_json().as_bytes()).map(|(decoded, _, _)| decoded);
	let decoded = decoded.expect("the manifest decodes");
	assert!(
		decoded.files.len() == manifest.files.len() && decoded.dirs.len() == manifest.dirs.len(),
		"decoded manifest = {decoded:?}, want same entry counts"
	);
}

#[test]
fn copy_files_gate() {
	let temp = Temp::new();
	let (root_dir, out_dir) = (path(&temp, "src"), path(&temp, "out"));
	temp.write("src/asset.luau", "v1");
	temp.write("out/asset.luau", "v1");
	let translator = PathTranslator::new(&root_dir, &out_dir, "", false, true);
	let inputs = |changed: HashSet<String>| CopyFilesGateInputs {
		root_dirs: vec![root_dir.clone()],
		out_dir: out_dir.clone(),
		declaration: false,
		path_translator: &translator,
		snapshot: BuildStateSnapshot {
			changed_file_paths: changed,
		},
	};

	let cold = load_copy_files_gate_pre_build(&inputs(HashSet::new()));
	assert!(
		!cold.skip_cleanup && !cold.skip_copy_files,
		"cold gate, want both skips false"
	);
	cold.persist().unwrap_or_else(|error| panic!("{error}"));
	let warm = load_copy_files_gate_pre_build(&inputs(HashSet::new()));
	assert!(
		warm.skip_cleanup && warm.skip_copy_files,
		"warm gate, want both skips true"
	);
	assert!(warm.persist.is_none(), "warm gate rewrites an unchanged manifest");

	let changed = load_copy_files_gate_pre_build(&inputs(HashSet::from([path(&temp, "src/source.ts")])));
	assert!(
		!changed.skip_cleanup && changed.skip_copy_files,
		"changed TS gate, want cleanup false and copy true"
	);

	let future = SystemTime::now() + Duration::from_secs(60);
	set_mtime(&temp.join("src/asset.luau"), future);
	let drifted = load_copy_files_gate_pre_build(&inputs(HashSet::new()));
	assert!(!drifted.skip_copy_files, "source mtime drift incorrectly skipped copy");

	std::fs::remove_file(temp.join("out/asset.luau")).expect("the copy removes");
	let missing_dest = load_copy_files_gate_pre_build(&inputs(HashSet::new()));
	assert!(
		!missing_dest.skip_copy_files,
		"missing destination incorrectly skipped copy"
	);
}

#[test]
fn copy_files_gate_persist_propagates_manifest_write_errors() {
	let temp = Temp::new();
	let (root_dir, out_dir) = (path(&temp, "src"), path(&temp, "out"));
	temp.write("src/asset.luau", "asset");
	temp.write("out", "not a directory");
	let translator = PathTranslator::new(&root_dir, &out_dir, "", false, true);

	let gate = load_copy_files_gate_pre_build(&CopyFilesGateInputs {
		root_dirs: vec![root_dir],
		out_dir,
		declaration: false,
		path_translator: &translator,
		snapshot: BuildStateSnapshot::default(),
	});
	assert!(
		gate.persist().is_err(),
		"Persist succeeded with a file in place of the output directory"
	);
}

#[test]
fn copy_output_recursion_guard() {
	let temp = Temp::new();
	let (root_dir, out_dir) = (temp.dir(), path(&temp, "out"));
	temp.write("asset.luau", "asset");
	temp.write("out/stale.luau", "stale");
	temp.write("node_modules/pkg/ignored.luau", "ignored");
	let translator = PathTranslator::new(&root_dir, &out_dir, "", false, true);

	copy_non_compiled_files(&translator, &[root_dir]).unwrap_or_else(|error| panic!("{error}"));
	assert!(temp.join("out/asset.luau").exists(), "asset was not copied");
	assert!(
		!temp.join("out/out").exists(),
		"recursive outDir exists, want not-exist"
	);
	assert!(
		!temp.join("out/node_modules").exists(),
		"node_modules output exists, want not-exist"
	);
}

#[test]
fn protected_outputs() {
	let temp = Temp::new();
	let translator = PathTranslator::new(&path(&temp, "src"), &path(&temp, "out"), "", false, true);
	for name in [
		COPY_FILES_CACHE_FILENAME,
		COPY_FILES_RESOLVER_CACHE_FILENAME,
		COPY_FILES_TYPE_ROOTS_CACHE_FILENAME,
	] {
		assert!(
			!is_output_file_orphaned(&translator, &path(&temp, &format!("out/{name}"))),
			"{name} is orphaned, want protected"
		);
	}
	assert!(
		is_output_file_orphaned(&translator, &path(&temp, "out/ordinary.json")),
		"ordinary output is not orphaned, want orphaned"
	);
}
