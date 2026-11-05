//! `artifacts_root_test.go`

#[cfg(unix)]
use std::path::Path;

#[cfg(unix)]
use sha2::{Digest, Sha256};

#[cfg(unix)]
use super::artifacts::fail_nth_rename;
use super::{join, temp_dir};
use crate::artifacts::{Artifact, persist_artifacts};
#[cfg(unix)]
use crate::artifacts::{open_artifact_root, set_rename_hook};
#[cfg(unix)]
use crate::artifacts_commit::commit_artifact;
#[cfg(unix)]
use crate::artifacts_stage::stage_artifact;

#[cfg(unix)]
/// `snapshotArtifactTree`: every entry under `root` with its kind and, for files, a digest.
fn snapshot_artifact_tree(root: &str) -> Vec<String> {
	fn walk(root: &Path, path: &Path, out: &mut Vec<String>) {
		let metadata = std::fs::symlink_metadata(path).unwrap();
		let relative = path.strip_prefix(root).unwrap().to_string_lossy().into_owned();
		let relative = if relative.is_empty() { ".".to_owned() } else { relative };
		let mut record = format!("{relative}:{:?}", metadata.file_type());
		if metadata.is_file() {
			let digest = Sha256::digest(std::fs::read(path).unwrap());
			record.push(':');
			record.push_str(&crate::id::hex(&digest));
		}
		out.push(record);
		if metadata.is_dir() {
			let mut entries: Vec<_> = std::fs::read_dir(path)
				.unwrap()
				.map(|entry| entry.unwrap().path())
				.collect();
			entries.sort();
			for entry in entries {
				walk(root, &entry, out);
			}
		}
	}
	let mut out = Vec::new();
	walk(Path::new(root), Path::new(root), &mut out);
	out
}

#[cfg(unix)]
#[test]
fn persist_artifacts_rejects_intermediate_symlink_to_external_tree() {
	type Case = (Option<&'static [u8]>, Option<&'static [u8]>);
	let cases: [Case; 3] = [
		(None, Some(b"private")),
		(Some(b"external-private"), Some(b"replacement")),
		(Some(b"external-private"), None),
	];
	for (existing, data) in cases {
		let (_dir, project_root) = temp_dir();
		let (_external_dir, external_root) = temp_dir();
		let nested = join(&[&external_root, "nested"]);
		std::fs::create_dir(&nested).unwrap();
		if let Some(existing) = existing {
			std::fs::write(join(&[&nested, "artifact.json"]), existing).unwrap();
		}
		std::os::unix::fs::symlink(&external_root, join(&[&project_root, "metadata"])).unwrap();
		let before = snapshot_artifact_tree(&external_root);
		let result = persist_artifacts(
			&project_root,
			&[Artifact {
				path: join(&["metadata", "nested", "artifact.json"]),
				data: data.map(<[u8]>::to_vec),
			}],
		);
		assert!(result.is_err());
		assert_eq!(snapshot_artifact_tree(&external_root), before);
	}
}

#[test]
fn persist_artifacts_rejects_paths_outside_project_root() {
	let (_dir, project_root) = temp_dir();
	for path in [
		"../escape.json".to_owned(),
		join(&["nested", "..", "..", "escape.json"]),
		join(&[&project_root, "absolute.json"]),
	] {
		let result = persist_artifacts(
			&project_root,
			&[Artifact {
				path,
				data: Some(b"private".to_vec()),
			}],
		);
		assert!(result.is_err());
	}
}

#[cfg(unix)]
#[test]
fn artifact_transaction_remains_bound_when_trusted_root_moves_and_is_replaced_by_symlink() {
	let (_dir, parent) = temp_dir();
	let project_root = join(&[&parent, "project"]);
	let moved_root = join(&[&parent, "moved-project"]);
	let (_external_dir, external_root) = temp_dir();
	std::fs::create_dir(&project_root).unwrap();
	let transaction = open_artifact_root(&project_root).unwrap();
	std::fs::rename(&project_root, &moved_root).unwrap();
	std::os::unix::fs::symlink(&external_root, &project_root).unwrap();
	let mut entry = stage_artifact(
		&transaction,
		Artifact {
			path: "artifact.json".to_owned(),
			data: Some(b"bound".to_vec()),
		},
	)
	.unwrap();
	commit_artifact(&transaction, &mut entry).unwrap();
	assert_eq!(std::fs::read(join(&[&moved_root, "artifact.json"])).unwrap(), b"bound");
	assert_eq!(snapshot_artifact_tree(&external_root).len(), 1);
}

#[cfg(unix)]
#[test]
fn persist_artifacts_refuses_intermediate_symlink_during_rollback() {
	let (_dir, project_root) = temp_dir();
	let metadata = join(&[&project_root, "metadata"]);
	let moved_metadata = join(&[&project_root, "moved-metadata"]);
	let (_external_dir, external_root) = temp_dir();
	std::fs::create_dir(&metadata).unwrap();
	for name in ["first.json", "second.json"] {
		std::fs::write(join(&[&metadata, name]), "prior").unwrap();
	}
	let external_before = snapshot_artifact_tree(&external_root);
	let (metadata_for_hook, external_for_hook) = (metadata, external_root.clone());
	fail_nth_rename(2, move || {
		std::fs::rename(&metadata_for_hook, &moved_metadata).unwrap();
		std::os::unix::fs::symlink(&external_for_hook, &metadata_for_hook).unwrap();
	});
	let result = persist_artifacts(
		&project_root,
		&[
			Artifact {
				path: join(&["metadata", "first.json"]),
				data: Some(b"replacement".to_vec()),
			},
			Artifact {
				path: join(&["metadata", "second.json"]),
				data: Some(b"replacement".to_vec()),
			},
		],
	);
	set_rename_hook(None);
	assert!(result.is_err());
	assert_eq!(snapshot_artifact_tree(&external_root), external_before);
}
