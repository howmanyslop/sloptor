//! `artifacts_commit.go`: committing staged artifacts, and rolling them back.

use std::io::Write as _;

use rotor_support::gopath;

use crate::artifacts::{ArtifactRoot, rename_artifact};
use crate::artifacts_identity::{
	ArtifactIdentity, artifact_regular_identity, remove_owned_artifact, require_artifact_missing, sha256,
};
use crate::artifacts_stage::{StagedArtifact, remove_empty_artifact_directories};
use crate::artifacts_temp::create_artifact_temp;

fn other(message: &str) -> std::io::Error {
	std::io::Error::other(message.to_owned())
}

/// `commitArtifact`
pub(crate) fn commit_artifact(root: &ArtifactRoot, entry: &mut StagedArtifact) -> std::io::Result<()> {
	entry.parent.revalidate_directory(root)?;
	if entry.existed {
		entry.destination.revalidate_regular(root)?;
	} else {
		require_artifact_missing(root, &entry.artifact.path)?;
	}
	let Some(data) = &entry.artifact.data else {
		if !entry.existed {
			return require_artifact_missing(root, &entry.artifact.path);
		}
		return remove_owned_artifact(root, &entry.destination);
	};
	entry.temp.revalidate_regular(root)?;
	rename_artifact(root, &entry.temp_path, &entry.artifact.path)?;
	let committed = artifact_regular_identity(root, &entry.artifact.path);
	let mut committed = match committed {
		Ok(committed) if committed.info.as_ref().is_some_and(|info| entry.temp.same_file(info)) => committed,
		_ => return Err(other("committed artifact identity changed")),
	};
	committed.digest = Some(sha256(data));
	if let Err(error) = committed.revalidate_regular(root) {
		return Err(other(&format!(
			"committed artifact contents changed: {}",
			rotor_support::gostr::io_error_text(&error)
		)));
	}
	entry.committed_identity = committed;
	entry.temp_path.clear();
	Ok(())
}

/// `rollbackArtifacts`: restores every committed artifact, last first.
pub(crate) fn rollback_artifacts(root: &ArtifactRoot, staged: &[StagedArtifact]) -> Result<(), String> {
	let mut errors: Vec<String> = Vec::new();
	for entry in staged.iter().rev() {
		if !entry.committed {
			continue;
		}
		if !entry.existed {
			if let Err(error) = remove_owned_artifact(root, &entry.committed_identity) {
				errors.push(rotor_support::gostr::io_error_text(&error));
			}
			continue;
		}
		let unchanged = if entry.artifact.data.is_none() {
			require_artifact_missing(root, &entry.artifact.path)
		} else {
			entry.committed_identity.revalidate_regular(root)
		};
		if let Err(error) = unchanged {
			errors.push(rotor_support::gostr::io_error_text(&error));
			continue;
		}
		if let Err(error) = restore_artifact(root, entry) {
			errors.push(rotor_support::gostr::io_error_text(&error));
		}
	}
	if errors.is_empty() {
		return Ok(());
	}
	Err(format!("flamework: rollback artifacts: {}", errors.join("\n")))
}

fn restore_artifact(root: &ArtifactRoot, entry: &StagedArtifact) -> std::io::Result<()> {
	entry.parent.revalidate_directory(root)?;
	let (mut temp, temp_path) = create_artifact_temp(root, &gopath::dir(&entry.artifact.path), ".flamework-rollback-")?;
	let temp_info = ArtifactRoot::file_info(&temp)?;
	let identity = ArtifactIdentity {
		path: temp_path.clone(),
		info: Some(temp_info),
		digest: Some(sha256(&entry.prior)),
	};
	let result = (|| {
		temp.write_all(&entry.prior)?;
		ArtifactRoot::chmod(&temp, entry.mode)?;
		temp.sync_all()?;
		drop(temp);
		entry.parent.revalidate_directory(root)?;
		identity.revalidate_regular(root)?;
		rename_artifact(root, &temp_path, &entry.artifact.path)
	})();
	let _ = remove_owned_artifact(root, &identity);
	result
}

/// `cleanupStagedArtifacts`: removes staged temporaries and created directories, last first.
pub(crate) fn cleanup_staged_artifacts(root: &ArtifactRoot, staged: &[StagedArtifact]) {
	for entry in staged.iter().rev() {
		if !entry.temp_path.is_empty() {
			let _ = remove_owned_artifact(root, &entry.temp);
		}
		remove_empty_artifact_directories(root, &entry.created_directories);
	}
}
