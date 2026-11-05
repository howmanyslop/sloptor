//! `sidecar_install.go`: where the Node worker comes from.
//!
//! `ROTOR_SIDECAR_PATH` overrides; otherwise the embedded worker is extracted once into a
//! content-addressed cache directory, the one Go's rotor uses for the same worker.

use rotor_support::{fsutil, gopath};
use sha2::{Digest, Sha256};

use crate::compile::Error;

/// The worker files Go's `tools/sidecar` package embeds (`main.js index.js lib/*.js
/// package.json`), sorted by name.
pub(crate) const EMBEDDED_SIDECAR: &[(&str, &[u8])] = &[
	("index.js", include_bytes!("../../../tools/sidecar/index.js")),
	(
		"lib/diagnostics.js",
		include_bytes!("../../../tools/sidecar/lib/diagnostics.js"),
	),
	(
		"lib/original-react-syntax.js",
		include_bytes!("../../../tools/sidecar/lib/original-react-syntax.js"),
	),
	(
		"lib/parallel.js",
		include_bytes!("../../../tools/sidecar/lib/parallel.js"),
	),
	(
		"lib/plugins.js",
		include_bytes!("../../../tools/sidecar/lib/plugins.js"),
	),
	(
		"lib/react-syntax-profile.js",
		include_bytes!("../../../tools/sidecar/lib/react-syntax-profile.js"),
	),
	(
		"lib/resolve.js",
		include_bytes!("../../../tools/sidecar/lib/resolve.js"),
	),
	(
		"lib/session.js",
		include_bytes!("../../../tools/sidecar/lib/session.js"),
	),
	("main.js", include_bytes!("../../../tools/sidecar/main.js")),
	("package.json", include_bytes!("../../../tools/sidecar/package.json")),
];

/// `resolveSidecarDir`
///
/// # Errors
///
/// The user cache directory is unknown, or the extraction fails.
pub(crate) fn resolve_sidecar_dir() -> Result<String, Error> {
	if let Some(dir) = crate::sidecar::getenv("ROTOR_SIDECAR_PATH")
		&& !dir.is_empty()
	{
		return Ok(dir);
	}
	let cache_root = user_cache_dir().map_err(|error| {
		Error::message(format!(
			"compile: cannot locate user cache dir for sidecar extraction: {error}"
		))
	})?;
	extract_sidecar_into(&cache_root)
}

/// The extraction half of `resolveSidecarDir`: the embedded worker under `cache_root`.
pub(crate) fn extract_sidecar_into(cache_root: &str) -> Result<String, Error> {
	let hash = embedded_sidecar_hash();
	let dir = gopath::join(&[cache_root, "rotor", &format!("sidecar-{hash}")]);
	let marker = gopath::join(&[&dir, ".complete"]);
	if std::fs::metadata(&marker).is_ok() {
		return Ok(dir);
	}
	let io = |error: std::io::Error| Error::message(rotor_support::gostr::io_error_text(&error));
	let tmp = format!("{dir}.tmp-{}", std::process::id());
	match std::fs::remove_dir_all(&tmp) {
		Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(io(error)),
		_ => {}
	}
	// User-private: Node executes the extracted worker.
	for (name, data) in EMBEDDED_SIDECAR {
		let target = gopath::join(&[&tmp, &gopath::from_slash(name)]);
		fsutil::mkdir_all(gopath::dir(&target), 0o700).map_err(io)?;
		fsutil::write_file(std::path::Path::new(&target), data, 0o600).map_err(io)?;
	}
	fsutil::write_file(std::path::Path::new(&gopath::join(&[&tmp, ".complete"])), &[], 0o600).map_err(io)?;
	if let Err(error) = std::fs::rename(&tmp, &dir) {
		// Lost a race with a concurrent extraction; accept the winner.
		if std::fs::metadata(&marker).is_ok() {
			let _ = std::fs::remove_dir_all(&tmp);
			return Ok(dir);
		}
		return Err(io(error));
	}
	Ok(dir)
}

/// `embeddedSidecarManifest`'s hash: SHA-256 over each name and file, NUL-terminated, the first
/// 16 hex digits.
pub(crate) fn embedded_sidecar_hash() -> String {
	let mut hasher = Sha256::new();
	for (name, data) in EMBEDDED_SIDECAR {
		hasher.update(name.as_bytes());
		hasher.update([0]);
		hasher.update(data);
		hasher.update([0]);
	}
	let digest = hasher.finalize();
	let mut hex = String::with_capacity(16);
	for byte in &digest[..8] {
		use std::fmt::Write as _;
		let _ = write!(hex, "{byte:02x}");
	}
	hex
}

/// Go's `os.UserCacheDir`.
fn user_cache_dir() -> Result<String, String> {
	let var = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
	if cfg!(windows) {
		return var("LocalAppData").ok_or_else(|| "%LocalAppData% is not defined".to_owned());
	}
	if cfg!(target_os = "macos") {
		let home = var("HOME").ok_or_else(|| "$HOME is not defined".to_owned())?;
		return Ok(gopath::join(&[&home, "Library", "Caches"]));
	}
	if let Some(dir) = var("XDG_CACHE_HOME") {
		if !gopath::is_abs(&dir) {
			return Err("path in $XDG_CACHE_HOME is relative".to_owned());
		}
		return Ok(dir);
	}
	let home = var("HOME").ok_or_else(|| "neither $XDG_CACHE_HOME nor $HOME are defined".to_owned())?;
	Ok(gopath::join(&[&home, ".cache"]))
}
