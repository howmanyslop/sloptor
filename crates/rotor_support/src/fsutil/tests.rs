//! Port of `internal/fsutil/realpath_test.go` and
//! `internal/fsutil/same_test.go`.

use std::fs;
use std::time::{Duration, SystemTime};

use super::{eval_symlinks, write_file_if_changed};

fn text(path: &std::path::Path) -> String {
	path.to_str().expect("temp paths are UTF-8").to_owned()
}

#[test]
fn eval_symlinks_resolves_aliased_directories() {
	for kind in ["junction", "symlink"] {
		// Given a directory reached through an alias
		let dir = tempfile::tempdir().expect("temp dir");
		let real = dir.path().join("real");
		let alias = dir.path().join("alias");
		fs::create_dir(&real).expect("mkdir");
		fs::write(real.join("a.ts"), []).expect("write");
		if kind == "junction" {
			if !cfg!(windows) {
				// Directory junctions are Windows-only.
				continue;
			}
			let status = std::process::Command::new("cmd")
				.args(["/c", "mklink", "/J"])
				.arg(&alias)
				.arg(&real)
				.status();
			if !status.is_ok_and(|status| status.success()) {
				// Directory junctions unavailable.
				continue;
			}
		} else {
			#[cfg(unix)]
			if std::os::unix::fs::symlink(&real, &alias).is_err() {
				continue;
			}
			#[cfg(windows)]
			if std::os::windows::fs::symlink_dir(&real, &alias).is_err() {
				continue;
			}
		}
		let want = eval_symlinks(&text(&real.join("a.ts"))).expect("real path resolves");

		// When a file is resolved through the alias
		let got = eval_symlinks(&text(&alias.join("a.ts"))).expect("alias path resolves");

		// Then it has the real spelling
		assert_eq!(got, want, "EvalSymlinks through {kind}");
	}
}

#[test]
fn eval_symlinks_fails_on_missing_path() {
	let dir = tempfile::tempdir().expect("temp dir");
	let missing = text(&dir.path().join("missing"));
	assert!(
		eval_symlinks(&missing).is_err(),
		"EvalSymlinks of a missing path succeeded"
	);
}

/// `os.SameFile`'s contract: a file stays itself while it is written and renamed, and a directory
/// while its entries change. Size and times cannot say that.
#[cfg(windows)]
#[test]
fn file_identity_follows_the_file() {
	use super::{file_identity, open_for_metadata};

	let identity =
		|path: &std::path::Path| file_identity(&open_for_metadata(path, false).expect("open")).expect("identity");
	let dir = tempfile::tempdir().expect("temp dir");
	let path = dir.path().join("a.txt");
	fs::write(&path, "a").expect("write");
	let (file, directory) = (identity(&path), identity(dir.path()));
	fs::write(&path, "longer contents").expect("rewrite");
	assert_eq!(identity(&path), file, "after a write");
	let renamed = dir.path().join("b.txt");
	fs::rename(&path, &renamed).expect("rename");
	assert_eq!(identity(&renamed), file, "after a rename");
	assert_eq!(identity(dir.path()), directory, "after its entries changed");
	fs::write(&path, "a").expect("write");
	assert_ne!(identity(&path), file, "another file");
}

#[test]
fn write_file_if_changed_keeps_identical_file() {
	let dir = tempfile::tempdir().expect("temp dir");
	let path = dir.path().join("a.txt");
	fs::write(&path, "abc").expect("write");
	let old = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
	fs::File::options()
		.write(true)
		.open(&path)
		.expect("open")
		.set_modified(old)
		.expect("set mtime");
	let wrote = write_file_if_changed(&path, b"abc", 0o644).expect("write_file_if_changed");
	assert!(!wrote, "wrote = {wrote}; want skip");
	let modified = fs::metadata(&path).expect("stat").modified().expect("mtime");
	assert_eq!(modified, old, "modtime changed to {modified:?}");
	for next in ["xyz", "wxyz"] {
		let wrote = write_file_if_changed(&path, next.as_bytes(), 0o644).expect("write_file_if_changed");
		let got = fs::read_to_string(&path).unwrap_or_default();
		assert!(wrote && got == next, "write {next:?}: wrote = {wrote}, got {got:?}");
	}
}
