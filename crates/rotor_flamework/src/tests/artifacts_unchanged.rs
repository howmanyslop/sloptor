//! `artifacts_unchanged_test.go`

use std::time::{Duration, SystemTime};

use super::{join, temp_dir};
use crate::artifacts::{Artifact, persist_artifacts};

#[test]
fn persist_artifacts_keeps_unchanged_mtimes() {
	let (_dir, root) = temp_dir();
	let mut artifacts = vec![
		Artifact {
			path: join(&["include", "flamework", "config.json"]),
			data: Some(br#"{"a":1}"#.to_vec()),
		},
		Artifact {
			path: join(&["include", "flamework", "globs.json"]),
			data: Some(b"{}".to_vec()),
		},
	];
	persist_artifacts(&root, &artifacts).unwrap();
	let old = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
	for artifact in &artifacts {
		let file = std::fs::File::options()
			.write(true)
			.open(join(&[&root, &artifact.path]))
			.unwrap();
		file.set_modified(old).unwrap();
	}
	artifacts[1].data = Some(br#"{"b":2}"#.to_vec());
	persist_artifacts(&root, &artifacts).unwrap();
	for (index, want_old) in [true, false].into_iter().enumerate() {
		let path = join(&[&root, &artifacts[index].path]);
		let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
		assert_eq!(modified == old, want_old, "{}", artifacts[index].path);
		assert_eq!(Some(std::fs::read(&path).unwrap()), artifacts[index].data);
	}
}

#[test]
fn persist_artifacts_rejects_unchanged_duplicate_paths() {
	let (_dir, root) = temp_dir();
	let artifact = Artifact {
		path: join(&["include", "flamework", "config.json"]),
		data: Some(br#"{"a":1}"#.to_vec()),
	};
	persist_artifacts(&root, std::slice::from_ref(&artifact)).unwrap();
	let error = persist_artifacts(&root, &[artifact.clone(), artifact]).unwrap_err();
	assert!(error.message().contains("duplicate artifact path"), "{error}");
}
