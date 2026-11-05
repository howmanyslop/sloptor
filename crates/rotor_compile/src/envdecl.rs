//! `envdecl.go`.
//!
//! The synthetic `$env` ambient declaration. The in-memory file system that serves it, and the
//! on-disk coexistence guard, are `rotor_tsrs::project` (the program loader); this module keeps the
//! names, the body and the atomic writer the editor companions use.

use std::io::Write as _;

use rotor_support::gopath;
use rotor_tsrs::tspath;

pub use rotor_tsrs::project::{ENV_DECL_BODY, ENV_DECL_FILE_NAME};

/// `EnvDeclFileName`: the legacy per-macro editor companion, superseded by `rotor.d.ts` but still
/// recognised.
pub const ENV_DECL_COMPANION_FILE_NAME: &str = rotor_tsrs::project::ENV_DECL_COMPANION;

/// `atomicWriteFile`: a temp file in the target directory plus a rename, so concurrent readers
/// never see partial content.
///
/// # Errors
///
/// The temp file cannot be created, written or renamed into place.
pub fn atomic_write_file(path: &str, data: &[u8]) -> std::io::Result<()> {
	use rotor_support::gostr::with_path;
	let directory = gopath::dir(path);
	let pattern = format!("{}.tmp-", gopath::base(path));
	let mut temp = tempfile::Builder::new()
		.prefix(&pattern)
		.tempfile_in(&directory)
		.map_err(|error| with_path("open", &gopath::join(&[&directory, &format!("{pattern}*")]), &error))?;
	let temp_name = temp.path().to_string_lossy().into_owned();
	temp.write_all(data)
		.map_err(|error| with_path("write", &temp_name, &error))?;
	// Go's `*os.LinkError`: `rename <old> <new>: <error>`.
	temp.persist(path)
		.map_err(|error| with_path("rename", &format!("{temp_name} {path}"), &error.error))?;
	Ok(())
}

/// `envDeclPath`: the synthetic declaration next to the tsconfig (slash separated).
#[must_use]
pub fn env_decl_path(config_path: &str) -> String {
	format!("{}/{ENV_DECL_FILE_NAME}", tspath::get_directory_path(config_path))
}
