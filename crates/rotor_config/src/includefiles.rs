//! Embeds the roblox-ts runtime library.
//!
//! These are the files upstream ships in its package's `include/` directory
//! (`Shared/constants.ts` L5: `INCLUDE_PATH` = `<package root>/include`), so
//! the rotor binary stays a standalone single executable.
//!
//! The `.lua` files are the repo-root `include/` directory, which is
//! byte-for-byte a copy of `reference/roblox-ts/include/`. The tests fail if
//! the copies drift.

use std::fmt;
use std::io;

use rotor_support::fsutil;
use rotor_support::gopath;

/// The runtime library. Upstream's `include/` directory contains exactly
/// `RuntimeLib.lua` and `Promise.lua`; the list is explicit so an accidental
/// extra file is a compile error, not a silent payload. Sorted by name.
const FILES: [(&str, &[u8]); 2] = [
	("Promise.lua", include_bytes!("../../../include/Promise.lua")),
	("RuntimeLib.lua", include_bytes!("../../../include/RuntimeLib.lua")),
];

/// The error [`read`] returns for a name that is not embedded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotFound(String);

impl fmt::Display for NotFound {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "open {}: file does not exist", self.0)
	}
}

impl std::error::Error for NotFound {}

/// The embedded file names in sorted order.
#[must_use]
pub fn names() -> Vec<&'static str> {
	FILES.iter().map(|(name, _)| *name).collect()
}

/// The contents of one embedded runtime file.
///
/// # Errors
///
/// Returns [`NotFound`] when `name` is not an embedded file.
pub fn read(name: &str) -> Result<&'static [u8], NotFound> {
	FILES
		.iter()
		.find(|(file, _)| *file == name)
		.map(|(_, data)| *data)
		.ok_or_else(|| NotFound(name.to_owned()))
}

/// Ports `Project/functions/copyInclude.ts` L12-14's
/// `fs.copySync(INCLUDE_PATH, includePath, { dereference: true })`.
///
/// It writes every runtime file into `include_path`, creating the directory if
/// needed and overwriting same-named files whose contents differ.
///
/// Like `fs.copySync`, it merges into an existing directory: extra files the
/// user put there are left alone. (`dereference` only matters for symlinks
/// inside the source tree; the embedded files have none.) The gating, where
/// `--noInclude` and package projects skip the copy (`copyInclude.ts` L7-11),
/// is the caller's responsibility.
///
/// # Errors
///
/// Returns the I/O error from creating the directory or writing a file.
pub fn copy(include_path: &str) -> io::Result<()> {
	fsutil::mkdir_all(include_path, 0o755)?;
	for (name, data) in FILES {
		fsutil::write_file_if_changed(gopath::join(&[include_path, name]), data, 0o644)?;
	}
	Ok(())
}

#[cfg(test)]
mod tests;
