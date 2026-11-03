//! `artifacts_identity.go`: file identities that detect a swapped or rewritten artifact.

use sha2::{Digest, Sha256};

use crate::artifacts::{ArtifactInfo, ArtifactRoot};

/// `artifactIdentity`
#[derive(Clone, Debug, Default)]
pub(crate) struct ArtifactIdentity {
	pub(crate) path: String,
	pub(crate) info: Option<ArtifactInfo>,
	pub(crate) digest: Option<[u8; 32]>,
}

fn other(message: &str) -> std::io::Error {
	std::io::Error::other(message.to_owned())
}

pub(crate) fn sha256(data: &[u8]) -> [u8; 32] {
	Sha256::digest(data).into()
}

pub(crate) fn artifact_regular_identity(root: &ArtifactRoot, path: &str) -> std::io::Result<ArtifactIdentity> {
	let info = root.lstat(path)?;
	if !info.is_regular() {
		return Err(other("not a regular file"));
	}
	Ok(ArtifactIdentity {
		path: path.to_owned(),
		info: Some(info),
		digest: None,
	})
}

pub(crate) fn artifact_directory_identity(root: &ArtifactRoot, path: &str) -> std::io::Result<ArtifactIdentity> {
	let info = root.lstat(path)?;
	if !info.is_dir() {
		return Err(other("not a directory"));
	}
	Ok(ArtifactIdentity {
		path: path.to_owned(),
		info: Some(info),
		digest: None,
	})
}

impl ArtifactIdentity {
	pub(crate) fn same_file(&self, other_info: &ArtifactInfo) -> bool {
		self.info.as_ref().is_some_and(|info| info.same_file(other_info))
	}

	pub(crate) fn revalidate_regular(&self, root: &ArtifactRoot) -> std::io::Result<()> {
		let current = artifact_regular_identity(root, &self.path);
		let Ok(current) = current else {
			return Err(other("artifact identity changed"));
		};
		if !current.info.as_ref().is_some_and(|info| self.same_file(info)) {
			return Err(other("artifact identity changed"));
		}
		let Some(digest) = self.digest else {
			return Ok(());
		};
		let contents = read_artifact_bytes(root, &current)?;
		if sha256(&contents) != digest {
			return Err(other("artifact contents changed"));
		}
		Ok(())
	}

	pub(crate) fn revalidate_directory(&self, root: &ArtifactRoot) -> std::io::Result<()> {
		match artifact_directory_identity(root, &self.path) {
			Ok(current) if current.info.as_ref().is_some_and(|info| self.same_file(info)) => Ok(()),
			_ => Err(other("artifact parent identity changed")),
		}
	}
}

pub(crate) fn read_artifact_identity(root: &ArtifactRoot, identity: &ArtifactIdentity) -> std::io::Result<Vec<u8>> {
	read_artifact_bytes(root, identity)
}

pub(crate) fn read_artifact_bytes(root: &ArtifactRoot, identity: &ArtifactIdentity) -> std::io::Result<Vec<u8>> {
	let (contents, info) = root.read_with_info(&identity.path)?;
	if !info.is_regular() || !identity.same_file(&info) {
		return Err(other("opened artifact identity changed"));
	}
	Ok(contents)
}

pub(crate) fn require_artifact_missing(root: &ArtifactRoot, path: &str) -> std::io::Result<()> {
	match root.lstat(path) {
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
		Err(error) => Err(error),
		Ok(_) => Err(other("artifact destination appeared")),
	}
}

pub(crate) fn remove_owned_artifact(root: &ArtifactRoot, identity: &ArtifactIdentity) -> std::io::Result<()> {
	identity.revalidate_regular(root)?;
	root.remove(&identity.path)
}
