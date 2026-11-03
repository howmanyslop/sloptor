//! `artifacts_stage.go`: staging an artifact's bytes beside its destination.

use std::io::Write as _;

use rotor_support::gopath;
use rotor_support::gostr::quote;

use crate::Error;
use crate::artifacts::{Artifact, ArtifactRoot};
use crate::artifacts_identity::{
	ArtifactIdentity, artifact_directory_identity, read_artifact_identity, require_artifact_missing, sha256,
};
use crate::artifacts_temp::create_artifact_temp;

/// `stagedArtifact`
#[derive(Debug, Default)]
pub(crate) struct StagedArtifact {
	pub(crate) artifact: Artifact,
	pub(crate) temp_path: String,
	pub(crate) prior: Vec<u8>,
	pub(crate) mode: u32,
	pub(crate) existed: bool,
	pub(crate) committed: bool,
	pub(crate) created_directories: Vec<String>,
	pub(crate) parent: ArtifactIdentity,
	pub(crate) destination: ArtifactIdentity,
	pub(crate) temp: ArtifactIdentity,
	pub(crate) committed_identity: ArtifactIdentity,
}

fn text(error: &std::io::Error) -> String {
	rotor_support::gostr::io_error_text(error)
}

/// `stageArtifact`
pub(crate) fn stage_artifact(root: &ArtifactRoot, artifact: Artifact) -> Result<StagedArtifact, Error> {
	let mut entry = StagedArtifact {
		artifact,
		mode: 0o644,
		..StagedArtifact::default()
	};
	let path = entry.artifact.path.clone();
	match root.lstat(&path) {
		Ok(info) => {
			if !info.is_regular() {
				return Err(Error::msg(format!(
					"flamework: artifact destination {} is not a regular file",
					quote(&path)
				)));
			}
			let mode = info.permissions();
			entry.destination = ArtifactIdentity {
				path: path.clone(),
				info: Some(info),
				digest: None,
			};
			entry.prior = read_artifact_identity(root, &entry.destination).map_err(|error| {
				Error::msg(format!(
					"flamework: read prior artifact {}: {}",
					quote(&path),
					text(&error)
				))
			})?;
			entry.destination.digest = Some(sha256(&entry.prior));
			entry.existed = true;
			entry.mode = mode;
		}
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
		Err(error) => {
			return Err(Error::msg(format!(
				"flamework: stat artifact {}: {}",
				quote(&path),
				text(&error)
			)));
		}
	}
	let directory = gopath::dir(&path);
	entry.created_directories = missing_artifact_directories(root, &directory)
		.map_err(|error| Error::msg(format!("flamework: inspect artifact directory: {}", text(&error))))?;
	if let Err(error) = root.mkdir_all(&directory, 0o755) {
		remove_empty_artifact_directories(root, &entry.created_directories);
		return Err(Error::msg(format!(
			"flamework: create artifact directory: {}",
			text(&error)
		)));
	}
	match artifact_directory_identity(root, &directory) {
		Ok(parent) => entry.parent = parent,
		Err(error) => {
			remove_empty_artifact_directories(root, &entry.created_directories);
			return Err(Error::msg(format!(
				"flamework: identify artifact parent: {}",
				text(&error)
			)));
		}
	}
	let unchanged = if entry.existed {
		entry.destination.revalidate_regular(root)
	} else {
		require_artifact_missing(root, &path)
	};
	if let Err(error) = unchanged {
		remove_empty_artifact_directories(root, &entry.created_directories);
		return Err(Error::msg(format!(
			"flamework: artifact destination changed: {}",
			text(&error)
		)));
	}
	if entry.artifact.data.is_none() {
		return Ok(entry);
	}
	stage_artifact_data(root, entry)
}

fn stage_artifact_data(root: &ArtifactRoot, mut entry: StagedArtifact) -> Result<StagedArtifact, Error> {
	let path = entry.artifact.path.clone();
	let fail = |root: &ArtifactRoot, entry: &StagedArtifact, temp: Option<&str>, message: String| {
		if let Some(temp) = temp {
			let _ = root.remove(temp);
		}
		remove_empty_artifact_directories(root, &entry.created_directories);
		Error::msg(message)
	};
	let (mut temp, temp_path) = match create_artifact_temp(root, &gopath::dir(&path), ".flamework-artifact-") {
		Ok(created) => created,
		Err(error) => {
			return Err(fail(
				root,
				&entry,
				None,
				format!("flamework: stage artifact {}: {}", quote(&path), text(&error)),
			));
		}
	};
	entry.temp_path.clone_from(&temp_path);
	let temp_info = match ArtifactRoot::file_info(&temp) {
		Ok(info) => info,
		Err(error) => {
			drop(temp);
			return Err(fail(
				root,
				&entry,
				Some(&temp_path),
				format!("flamework: identify staged artifact {}: {}", quote(&path), text(&error)),
			));
		}
	};
	entry.temp = ArtifactIdentity {
		path: temp_path.clone(),
		info: Some(temp_info),
		digest: None,
	};
	let data = entry.artifact.data.clone().unwrap_or_default();
	if let Err(error) = temp.write_all(&data) {
		drop(temp);
		return Err(fail(
			root,
			&entry,
			Some(&temp_path),
			format!("flamework: stage artifact {}: {}", quote(&path), text(&error)),
		));
	}
	if let Err(error) = ArtifactRoot::chmod(&temp, entry.mode) {
		drop(temp);
		return Err(fail(
			root,
			&entry,
			Some(&temp_path),
			format!("flamework: set artifact mode {}: {}", quote(&path), text(&error)),
		));
	}
	if let Err(error) = temp.sync_all().and(Ok(())) {
		drop(temp);
		return Err(fail(
			root,
			&entry,
			Some(&temp_path),
			format!("flamework: close staged artifact {}: {}", quote(&path), text(&error)),
		));
	}
	drop(temp);
	entry.temp.digest = Some(sha256(&data));
	Ok(entry)
}

/// `missingArtifactDirectories`: `directory` and its ancestors that do not exist yet,
/// deepest first.
pub(crate) fn missing_artifact_directories(root: &ArtifactRoot, directory: &str) -> std::io::Result<Vec<String>> {
	let mut missing = Vec::new();
	let mut current = gopath::clean(directory);
	loop {
		match root.stat(&current) {
			Ok(_) => return Ok(missing),
			Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
			Err(error) => return Err(error),
		}
		missing.push(current.clone());
		let parent = gopath::dir(&current);
		if parent == current {
			return Ok(missing);
		}
		current = parent;
	}
}

pub(crate) fn remove_empty_artifact_directories(root: &ArtifactRoot, directories: &[String]) {
	for directory in directories {
		let _ = root.remove(directory);
	}
}
