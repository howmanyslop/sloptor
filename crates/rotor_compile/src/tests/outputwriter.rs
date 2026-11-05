//! `outputwriter_test.go`. Go's writer takes injectable file operations
//! (`newOutputWriterWithOperations`) and several tests count the calls; the Rust writer calls
//! `std::fs` directly. The tests that only observe the file system port with their
//! operation counters dropped; the ones that exist to count or fail operations are ignored.

use std::collections::BTreeMap;
use std::fs;

use super::Temp;
use crate::outputwriter::OutputWriter;
use crate::package_layout::hex_sha256;

fn confined(temp: &Temp, previous: &[(&str, &str)]) -> OutputWriter {
	let mut writer = OutputWriter::new();
	let previous: BTreeMap<String, String> = previous
		.iter()
		.map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
		.collect();
	writer
		.use_hashes(&temp.dir(), &previous, BTreeMap::new())
		.unwrap_or_else(|error| panic!("{error}"));
	writer
}

#[test]
fn output_hash_match_uses_validated_contents() {
	let temp = Temp::new();
	let contents = b"output";
	temp.write("out/main.luau", contents);
	let hash = hex_sha256(contents);
	let writer = confined(&temp, &[("out/main.luau", &hash)]);

	let path = temp.join("out/main.luau").to_string_lossy().into_owned();
	let wrote = writer.write(&path, contents).unwrap_or_else(|error| panic!("{error}"));
	assert!(!wrote, "wrote = true, want a hash skip");
	let current = writer.current_outputs();
	assert!(
		current.get("out/main.luau") == Some(&hash) && writer.hash_skip_count() == 1,
		"current hashes = {current:?}, skips = {}",
		writer.hash_skip_count()
	);
}

/// Another process rewrites an output after validation confirmed its hash. This build still skips
/// the write (as Go does), but the stamp it keeps is the one validation saw, so the next build
/// hashes the file again and restores it. Recording the rewritten file's stamp would keep the
/// stale hash trusted from then on.
#[test]
fn output_rewritten_mid_build_is_restored_by_the_next_build() {
	let temp = Temp::new();
	let contents = b"output";
	temp.write("out/main.luau", contents);
	let hash = hex_sha256(contents);
	let path = temp.join("out/main.luau").to_string_lossy().into_owned();
	let writer = confined(&temp, &[("out/main.luau", &hash)]);

	fs::write(&path, b"rewritten by another process").expect("rewrite");
	let wrote = writer.write(&path, contents).unwrap_or_else(|error| panic!("{error}"));
	assert!(!wrote, "wrote = true, want this build's hash skip");
	writer.persist_stamps();

	let next = confined(&temp, &[("out/main.luau", &hash)]);
	let wrote = next.write(&path, contents).unwrap_or_else(|error| panic!("{error}"));
	assert!(
		wrote && fs::read(&path).expect("the output reads") == contents,
		"the next build trusted the rewritten output (wrote = {wrote})"
	);
}

#[test]
fn output_hash_mismatch_does_not_skip_write() {
	let temp = Temp::new();
	temp.write("out/main.luau", "STALE-MARKER");
	let want = b"expected output";
	let writer = confined(&temp, &[("out/main.luau", &hex_sha256(want))]);

	let path = temp.join("out/main.luau").to_string_lossy().into_owned();
	let wrote = writer.write(&path, want).unwrap_or_else(|error| panic!("{error}"));
	let got = fs::read(&path).expect("the output reads");
	assert!(
		wrote && got == want && writer.hash_skip_count() == 0,
		"wrote={wrote} output={:?} skips={}",
		String::from_utf8_lossy(&got),
		writer.hash_skip_count()
	);
}

#[test]
fn output_writer_prepare_rejects_path_outside_project() {
	let project = Temp::new();
	let writer = confined(&project, &[]);
	let outside = project.join("../outside/main.luau").to_string_lossy().into_owned();
	assert!(
		writer.prepare(&[outside]).is_err(),
		"prepare accepted an output outside the project"
	);
	assert!(
		!project.join("../outside").exists(),
		"prepare created the outside directory"
	);
}

#[test]
fn output_writer_lstat_rejects_path_outside_project() {
	let temp = Temp::new();
	let writer = confined(&temp, &[]);
	let outside = temp.join("../outside.luau").to_string_lossy().into_owned();
	assert!(
		writer.lstat(&outside).is_err(),
		"lstat accepted an output outside the project"
	);
}

#[test]
#[cfg(unix)]
fn output_writer_rejects_symlink_escape() {
	let project = Temp::new();
	let outside = Temp::new();
	std::os::unix::fs::symlink(outside.join(""), project.join("out")).expect("the out symlink");

	let writer = confined(&project, &[]);
	let path = project.join("out/main.luau").to_string_lossy().into_owned();
	assert!(
		writer.prepare(&[path]).is_err(),
		"prepare accepted a parent symlink outside the project"
	);
	assert!(
		!outside.join("main.luau").exists(),
		"outside output exists, want not-exist"
	);
}

#[test]
#[cfg(unix)]
fn output_writer_rejects_final_symlink_escape() {
	let project = Temp::new();
	fs::create_dir(project.join("out")).expect("out");
	let outside = Temp::new();
	outside.write("outside.luau", "original");
	let output_path = project.join("out/main.luau");
	std::os::unix::fs::symlink(outside.join("outside.luau"), &output_path).expect("the output symlink");

	let writer = confined(&project, &[]);
	assert!(
		writer.write(&output_path.to_string_lossy(), b"replacement").is_err(),
		"write accepted a final symlink outside the project"
	);
	let contents = fs::read_to_string(outside.join("outside.luau")).expect("the outside file reads");
	assert_eq!(contents, "original", "outside output");
}

#[test]
#[ignore = "counts injected mkdir calls (Go's outputWriterOperations); the Rust writer calls std::fs directly"]
fn output_directory_created_once() {}

#[test]
#[ignore = "counts injected mkdir calls (Go's outputWriterOperations); the Rust writer calls std::fs directly"]
fn output_directory_unchanged_does_not_create() {}

#[test]
#[ignore = "injects a failing mkdir (Go's outputWriterOperations); the Rust writer calls std::fs directly"]
fn output_directory_failure_memoized() {}

#[test]
#[ignore = "injects a case-insensitive file system (Go's outputWriterOperations); the Rust writer calls std::fs directly"]
fn output_directory_case_canonicalized() {}

#[test]
#[cfg(unix)]
fn output_presence_index() {
	let temp = Temp::new();
	temp.write("out/a.luau", "x");
	temp.write("out/b.luau", "x");
	fs::create_dir(temp.join("out/adir")).expect("adir");
	std::os::unix::fs::symlink(temp.join("out/a.luau"), temp.join("out/alink.luau")).expect("alink");

	let writer = confined(&temp, &[]);
	let outputs: Vec<String> = [
		"out/a.luau",
		"out/b.luau",
		"out/alink.luau",
		"out/adir",
		"out/missing.luau",
		"../outside.luau",
	]
	.map(str::to_owned)
	.to_vec();
	let index = writer.new_output_presence_index(&outputs);
	for present in ["out/a.luau", "out/b.luau"] {
		assert!(index.has_regular(present), "hasRegular({present:?}) = false, want true");
	}
	for absent in ["out/alink.luau", "out/adir", "out/missing.luau", "../outside.luau"] {
		assert!(!index.has_regular(absent), "hasRegular({absent:?}) = true, want false");
	}
}

#[test]
fn output_presence_index_case_normalization() {
	let temp = Temp::new();
	temp.write("out/Shared/X.luau", "x");
	temp.write("out/deep/x.luau", "x");

	let mut sensitive = confined(&temp, &[]);
	sensitive.case_sensitive = true;
	let outputs = ["out/Shared/X.luau", "out/deep/x.luau"].map(str::to_owned).to_vec();
	let sensitive_index = sensitive.new_output_presence_index(&outputs);
	for present in ["out/Shared/X.luau", "out/deep/x.luau"] {
		assert!(
			sensitive_index.has_regular(present),
			"case-sensitive hasRegular({present:?}) = false, want true"
		);
	}
	assert!(
		!sensitive_index.has_regular("out/Shared/x.luau"),
		"case-sensitive hasRegular(out/Shared/x.luau) = true, want false"
	);

	let mut insensitive = confined(&temp, &[]);
	insensitive.case_sensitive = false;
	let outputs = vec!["out/deep/x.luau".to_owned()];
	let insensitive_index = insensitive.new_output_presence_index(&outputs);
	for present in ["out/deep/x.luau", "out/DEEP/x.luau", "out/deep/X.luau"] {
		assert!(
			insensitive_index.has_regular(present),
			"case-insensitive hasRegular({present:?}) = false, want true"
		);
	}
	assert!(
		!insensitive_index.has_regular("out/deep/missing.luau"),
		"case-insensitive hasRegular(out/deep/missing.luau) = true, want false"
	);
}

#[test]
#[ignore = "injects a failing mkdir (Go's outputWriterOperations); the Rust writer calls std::fs directly"]
fn batch_prepare_failure_prevents_writes() {}
