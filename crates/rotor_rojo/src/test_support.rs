//! Helpers shared by the module tests, mirroring the Go tests' fixtures.

use std::fs;
use std::time::SystemTime;

use rotor_support::gopath;
use tempfile::TempDir;

use crate::{RbxPath, RojoResolver};

/// A project written under a temporary directory that is removed on drop.
pub struct Project {
	_guard: TempDir,
	/// The project directory.
	pub dir: String,
}

impl Project {
	/// `filepath.Join(dir, filepath.FromSlash(rel))`.
	pub fn path(&self, rel: &str) -> String {
		join_slashed(&self.dir, rel)
	}
}

/// `filepath.Join(dir, filepath.FromSlash(rel))`.
pub fn join_slashed(dir: &str, rel: &str) -> String {
	gopath::join(&[dir, &gopath::from_slash(rel)])
}

/// Creates files (path to contents) under a temp dir. Keys use forward
/// slashes and directories are created as needed; a trailing `/` key creates
/// an empty directory.
pub fn write_project(files: &[(&str, &str)]) -> Project {
	let guard = tempfile::tempdir().expect("temp dir");
	let dir = guard.path().to_str().expect("temp paths are UTF-8").to_owned();
	for (rel, contents) in files {
		let abs = join_slashed(&dir, rel);
		if rel.ends_with('/') {
			fs::create_dir_all(&abs).expect("mkdir");
			continue;
		}
		fs::create_dir_all(gopath::dir(&abs)).expect("mkdir");
		fs::write(&abs, contents).expect("write");
	}
	Project { _guard: guard, dir }
}

/// The rbx path for `file_path`, failing the test when no Rojo data covers it.
pub fn must_rbx_path(r: &RojoResolver, file_path: &str) -> RbxPath {
	r.get_rbx_path_from_file_path(file_path)
		.unwrap_or_else(|| panic!("get_rbx_path_from_file_path({file_path:?}): no Rojo data"))
}

/// Builds an rbx path from segments.
pub fn rbx(segments: &[&str]) -> RbxPath {
	segments.iter().map(|&s| s.to_owned()).collect()
}

/// `os.Chtimes` for the modification time only.
pub fn set_mtime(path: &str, time: SystemTime) {
	#[cfg(windows)]
	let file = {
		use std::os::windows::fs::OpenOptionsExt;
		// FILE_WRITE_ATTRIBUTES with FILE_FLAG_BACKUP_SEMANTICS opens directories too.
		fs::OpenOptions::new()
			.access_mode(0x100)
			.custom_flags(0x0200_0000)
			.open(path)
			.expect("open")
	};
	#[cfg(not(windows))]
	let file = fs::File::open(path).expect("open");
	file.set_modified(time).expect("set mtime");
}

/// Creates a directory link of `kind` (`junction` or `symlink`) at `link`
/// pointing at `target`. Returns false when the platform cannot make one,
/// which the Go tests report as a skip.
pub fn directory_link(kind: &str, link: &str, target: &str) -> bool {
	if kind == "junction" {
		if !cfg!(windows) {
			// Directory junctions are Windows-only.
			return false;
		}
		let output = std::process::Command::new("cmd")
			.args(["/c", "mklink", "/J", link, target])
			.output();
		assert!(
			output.is_ok_and(|output| output.status.success()),
			"create directory junction"
		);
		return true;
	}
	#[cfg(unix)]
	let made = std::os::unix::fs::symlink(target, link);
	#[cfg(windows)]
	let made = std::os::windows::fs::symlink_dir(target, link);
	made.is_ok()
}

/// Creates a file symlink at `link` pointing at `target`; false when the
/// platform cannot make one.
pub fn file_link(link: &str, target: &str) -> bool {
	#[cfg(unix)]
	let made = std::os::unix::fs::symlink(target, link);
	#[cfg(windows)]
	let made = std::os::windows::fs::symlink_file(target, link);
	made.is_ok()
}
