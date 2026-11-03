//! `artifacts_temp.go`: uniquely named temporary files beside an artifact.

use std::fs::File;
use std::io::Read;

use rotor_support::gopath;

use crate::artifacts::ArtifactRoot;
use crate::id::{CryptoReader, hex};

/// `createArtifactTemp`: a new file named `<prefix><16 hex>.tmp` in `directory`.
pub(crate) fn create_artifact_temp(
	root: &ArtifactRoot,
	directory: &str,
	prefix: &str,
) -> std::io::Result<(File, String)> {
	let mut random = [0u8; 8];
	for _ in 0..100 {
		CryptoReader.read_exact(&mut random)?;
		let path = gopath::join(&[directory, &format!("{prefix}{}.tmp", hex(&random))]);
		match root.create_exclusive(&path, 0o600) {
			Ok(file) => return Ok((file, path)),
			Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
			Err(error) => return Err(error),
		}
	}
	Err(std::io::Error::other("artifact temporary name collision limit reached"))
}
