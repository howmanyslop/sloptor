//! `artifacts_test.go` (with `nonregular_test_unix_test.go`'s helpers)

use std::cell::Cell;
use std::rc::Rc;

use super::{join, temp_dir};
use crate::artifacts::{
	Artifact, ArtifactRoot, open_artifact_root, persist_artifacts, prepare_artifacts, set_rename_hook,
};
use crate::artifacts_commit::{cleanup_staged_artifacts, commit_artifact, rollback_artifacts};
use crate::artifacts_stage::stage_artifact;
use crate::build_info::BuildInfo;

fn artifact(path: &str, data: Option<&[u8]>) -> Artifact {
	Artifact {
		path: path.to_owned(),
		data: data.map(<[u8]>::to_vec),
	}
}

/// Fails the `fail_on`-th rename with `message`; the others rename for real.
pub(super) fn fail_nth_rename(fail_on: usize, before_failing: impl Fn() + 'static) {
	let count = Rc::new(Cell::new(0));
	set_rename_hook(Some(Box::new(move |root: &ArtifactRoot, old: &str, new: &str| {
		count.set(count.get() + 1);
		if count.get() == fail_on {
			before_failing();
			return Err(std::io::Error::other("injected late failure"));
		}
		root.rename(old, new)
	})));
}

#[cfg(unix)]
fn permissions(path: &str) -> u32 {
	use std::os::unix::fs::PermissionsExt as _;
	std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn prepare_artifacts_when_optional_metadata_is_absent() {
	let (_dir, root) = temp_dir();
	let info = BuildInfo::new(&join(&[&root, "flamework.build"]), "1.3.2");
	let artifacts = prepare_artifacts(Some(&info), &join(&[&root, "include", "flamework"]), None, None).unwrap();
	let got: Vec<String> = artifacts
		.iter()
		.map(|artifact| rotor_support::gopath::base(&artifact.path))
		.collect();
	assert_eq!(got, ["config.json", "globs.json", "flamework.build"]);
	assert!(artifacts[0].data.is_none() && artifacts[1].data.is_none() && artifacts[2].data.is_some());
}

#[test]
fn persist_artifacts_when_late_commit_fails() {
	let (_dir, root) = temp_dir();
	let first_path = join(&[&root, "first.json"]);
	let second_path = join(&[&root, "second.json"]);
	let prior = b"prior bytes";
	for path in [&first_path, &second_path] {
		std::fs::write(path, prior).unwrap();
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt as _;
			std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o640)).unwrap();
		}
	}
	#[cfg(unix)]
	let prior_mode = permissions(&first_path);
	fail_nth_rename(2, || {});
	let result = persist_artifacts(
		&root,
		&[
			artifact("first.json", Some(b"replacement")),
			artifact("second.json", Some(b"replacement")),
		],
	);
	set_rename_hook(None);
	assert!(result.is_err());
	assert_eq!(std::fs::read(&first_path).unwrap(), prior);
	assert_eq!(std::fs::read(&second_path).unwrap(), prior);
	#[cfg(unix)]
	assert_eq!(permissions(&first_path), prior_mode);
}

#[test]
fn persist_artifacts_when_deleting_and_writing() {
	let (_dir, root) = temp_dir();
	for name in ["config.json", "globs.json"] {
		std::fs::write(join(&[&root, name]), "stale").unwrap();
	}
	persist_artifacts(
		&root,
		&[
			artifact("config.json", None),
			artifact("globs.json", Some(b"")),
			artifact("flamework.build", Some(b"build")),
		],
	)
	.unwrap();
	assert!(!std::path::Path::new(&join(&[&root, "config.json"])).exists());
	assert_eq!(std::fs::read(join(&[&root, "globs.json"])).unwrap(), b"");
	assert_eq!(std::fs::read(join(&[&root, "flamework.build"])).unwrap(), b"build");
}

#[test]
fn persist_artifacts_when_staging_fails_after_creating_parent() {
	let (_dir, root) = temp_dir();
	std::fs::write(join(&[&root, "blocker"]), "file").unwrap();
	let result = persist_artifacts(
		&root,
		&[
			artifact(&join(&["include", "flamework", "config.json"]), Some(b"{}")),
			artifact(&join(&["blocker", "globs.json"]), Some(b"{}")),
		],
	);
	assert!(result.is_err());
	assert!(!std::path::Path::new(&join(&[&root, "include"])).exists());
}

#[cfg(unix)]
#[test]
fn persist_artifacts_when_existing_artifact_is_external_symlink() {
	let (_dir, root) = temp_dir();
	let (_external_dir, external_root) = temp_dir();
	let external_path = join(&[&external_root, "secret.json"]);
	let artifact_path = join(&[&root, "first.json"]);
	std::fs::write(&external_path, "external-private-bytes").unwrap();
	std::os::unix::fs::symlink(&external_path, &artifact_path).unwrap();
	std::fs::write(join(&[&root, "second.json"]), "second-prior").unwrap();
	fail_nth_rename(2, || {});
	let result = persist_artifacts(
		&root,
		&[
			artifact("first.json", Some(b"replacement")),
			artifact("second.json", Some(b"replacement")),
		],
	);
	set_rename_hook(None);
	assert!(result.is_err());
	assert!(
		std::fs::symlink_metadata(&artifact_path)
			.unwrap()
			.file_type()
			.is_symlink()
	);
	assert_eq!(std::fs::read(&external_path).unwrap(), b"external-private-bytes");
}

#[cfg(unix)]
#[test]
fn persist_artifacts_when_destination_is_dangling_symlink() {
	let (_dir, root) = temp_dir();
	let path = join(&[&root, "artifact.json"]);
	std::os::unix::fs::symlink(join(&[&root, "missing"]), &path).unwrap();
	assert!(persist_artifacts(&root, &[artifact("artifact.json", Some(b"replacement"))]).is_err());
	assert!(std::fs::symlink_metadata(&path).unwrap().file_type().is_symlink());
}

#[cfg(unix)]
#[test]
fn persist_artifacts_when_destination_is_non_regular() {
	use std::os::unix::fs::FileTypeExt as _;
	let (_dir, root) = temp_dir();
	let path = join(&[&root, "artifact.fifo"]);
	let status = std::process::Command::new("mkfifo").arg(&path).status().unwrap();
	assert!(status.success());
	assert!(persist_artifacts(&root, &[artifact("artifact.fifo", Some(b"replacement"))]).is_err());
	assert!(std::fs::symlink_metadata(&path).unwrap().file_type().is_fifo());
}

#[test]
fn commit_artifact_when_destination_leaf_is_swapped() {
	let (_dir, root) = temp_dir();
	let path = join(&[&root, "artifact.json"]);
	std::fs::write(&path, "prior").unwrap();
	let transaction = open_artifact_root(&root).unwrap();
	let mut entry = stage_artifact(&transaction, artifact("artifact.json", Some(b"replacement"))).unwrap();
	std::fs::remove_file(&path).unwrap();
	std::fs::write(&path, "attacker").unwrap();
	assert!(commit_artifact(&transaction, &mut entry).is_err());
	assert_eq!(std::fs::read(&path).unwrap(), b"attacker");
	cleanup_staged_artifacts(&transaction, &[entry]);
}

#[test]
fn commit_artifact_when_parent_is_swapped() {
	let (_dir, root) = temp_dir();
	let parent = join(&[&root, "metadata"]);
	std::fs::create_dir(&parent).unwrap();
	let transaction = open_artifact_root(&root).unwrap();
	let mut entry = stage_artifact(
		&transaction,
		artifact(&join(&["metadata", "artifact.json"]), Some(b"replacement")),
	)
	.unwrap();
	std::fs::rename(&parent, join(&[&root, "old-metadata"])).unwrap();
	std::fs::create_dir(&parent).unwrap();
	assert!(commit_artifact(&transaction, &mut entry).is_err());
	assert!(!std::path::Path::new(&join(&[&parent, "artifact.json"])).exists());
}

#[test]
fn commit_artifact_when_staged_leaf_is_swapped() {
	let (_dir, root) = temp_dir();
	let transaction = open_artifact_root(&root).unwrap();
	let mut entry = stage_artifact(&transaction, artifact("artifact.json", Some(b"replacement"))).unwrap();
	let temp = join(&[&root, &entry.temp_path]);
	std::fs::remove_file(&temp).unwrap();
	std::fs::write(&temp, "attacker").unwrap();
	assert!(commit_artifact(&transaction, &mut entry).is_err());
	assert!(!std::path::Path::new(&join(&[&root, "artifact.json"])).exists());
}

#[test]
fn rollback_artifacts_when_committed_destination_is_swapped() {
	let (_dir, root) = temp_dir();
	let path = join(&[&root, "artifact.json"]);
	std::fs::write(&path, "prior").unwrap();
	let transaction = open_artifact_root(&root).unwrap();
	let mut entry = stage_artifact(&transaction, artifact("artifact.json", Some(b"replacement"))).unwrap();
	commit_artifact(&transaction, &mut entry).unwrap();
	entry.committed = true;
	std::fs::remove_file(&path).unwrap();
	std::fs::write(&path, "attacker").unwrap();
	assert!(rollback_artifacts(&transaction, &[entry]).is_err());
	assert_eq!(std::fs::read(&path).unwrap(), b"attacker");
}

#[test]
fn cleanup_staged_artifacts_when_temp_path_collides() {
	let (_dir, root) = temp_dir();
	let transaction = open_artifact_root(&root).unwrap();
	let entry = stage_artifact(&transaction, artifact("artifact.json", Some(b"replacement"))).unwrap();
	let temp = join(&[&root, &entry.temp_path]);
	std::fs::remove_file(&temp).unwrap();
	std::fs::write(&temp, "attacker").unwrap();
	cleanup_staged_artifacts(&transaction, &[entry]);
	assert_eq!(std::fs::read(&temp).unwrap(), b"attacker");
}

#[test]
fn persist_artifacts_names_the_root_operation_that_failed() {
	// `os.Root`'s `*PathError` text: the operation and the whole path.
	let (_dir, root) = temp_dir();
	std::fs::create_dir(join(&[&root, "include"])).unwrap();
	std::fs::write(join(&[&root, "include", "flamework"]), "file").unwrap();
	let error = persist_artifacts(&root, &[artifact("include/flamework/config.json", Some(b"{}"))])
		.unwrap_err()
		.to_string();
	assert_eq!(
		error,
		r#"flamework: stat artifact "include/flamework/config.json": statat include/flamework/config.json: not a directory"#
	);
}

#[cfg(unix)]
#[test]
fn persist_artifacts_follows_symlinks_inside_the_root() {
	// `os.Root` follows a symlinked directory whose target stays inside the root.
	let (_dir, root) = temp_dir();
	std::fs::create_dir_all(join(&[&root, "vendor", "include"])).unwrap();
	std::os::unix::fs::symlink(join(&["vendor", "include"]), join(&[&root, "include"])).unwrap();
	persist_artifacts(&root, &[artifact("include/flamework/config.json", Some(b"{}"))]).unwrap();
	assert_eq!(
		std::fs::read(join(&[&root, "vendor", "include", "flamework", "config.json"])).unwrap(),
		b"{}"
	);
	assert!(
		std::fs::symlink_metadata(join(&[&root, "include"]))
			.unwrap()
			.file_type()
			.is_symlink()
	);

	// A relative target that climbs out of the root is refused, as is an absolute one.
	let (_outside_dir, outside) = temp_dir();
	std::fs::remove_file(join(&[&root, "include"])).unwrap();
	std::os::unix::fs::symlink(join(&["..", ".."]), join(&[&root, "include"])).unwrap();
	let error = persist_artifacts(&root, &[artifact("include/config.json", Some(b"{}"))])
		.unwrap_err()
		.to_string();
	assert!(error.contains("path escapes from parent"), "{error}");
	std::fs::remove_file(join(&[&root, "include"])).unwrap();
	std::os::unix::fs::symlink(&outside, join(&[&root, "include"])).unwrap();
	let error = persist_artifacts(&root, &[artifact("include/config.json", Some(b"{}"))])
		.unwrap_err()
		.to_string();
	assert!(error.contains("path escapes from parent"), "{error}");
	assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn artifact_root_errors_read_as_go_os_root_errors() {
	// The texts Go 1.26's `os.Root` gives for the same operations.
	let (_dir, root) = temp_dir();
	std::fs::create_dir(join(&[&root, "include"])).unwrap();
	std::fs::write(join(&[&root, "include", "flamework"]), "file").unwrap();
	std::os::unix::fs::symlink(join(&["..", ".."]), join(&[&root, "up"])).unwrap();
	let root = open_artifact_root(&root).unwrap();
	let text = |error: std::io::Error| rotor_support::gostr::io_error_text(&error);
	assert_eq!(
		text(root.lstat("include/flamework/config.json").unwrap_err()),
		"statat include/flamework/config.json: not a directory"
	);
	assert_eq!(
		text(root.mkdir_all("include/flamework/x", 0o755).unwrap_err()),
		"openat include/flamework: not a directory"
	);
	assert_eq!(
		text(root.mkdir_all("include/flamework", 0o755).unwrap_err()),
		"mkdirat include/flamework: file exists"
	);
	assert_eq!(
		text(root.remove("include/flamework/x").unwrap_err()),
		"removeat include/flamework/x: not a directory"
	);
	assert_eq!(
		text(root.rename("include/nope", "include/flamework/y").unwrap_err()),
		"renameat include/nope include/flamework/y: not a directory"
	);
	assert_eq!(
		text(root.create_exclusive("include/flamework/t", 0o600).unwrap_err()),
		"openat include/flamework/t: not a directory"
	);
	assert_eq!(
		text(root.lstat("up/x").unwrap_err()),
		"statat up/x: path escapes from parent"
	);
}
