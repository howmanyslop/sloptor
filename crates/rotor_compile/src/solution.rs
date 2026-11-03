//! `solution.go`: the solution coordinator is Stage 5b. Stage 5a ports only the path helpers the
//! single-project pipeline calls.

use rotor_support::gopath;

/// `canonicalExplicitPath`: spells a user-given path like solution config paths, so it compares
/// equal to paths derived from them.
pub(crate) fn canonical_explicit_path(path: &str) -> String {
	if !path_has_explicit_symlink(path) {
		return path.to_owned();
	}
	gopath::clean(&gopath::from_slash(&crate::sidecar::realpath_or(path)))
}

/// `pathHasExplicitSymlink`: whether any component of `path` is a symbolic link.
pub(crate) fn path_has_explicit_symlink(path: &str) -> bool {
	let separator = gopath::NATIVE.separator();
	let volume = gopath::volume_name(path);
	let mut current = format!("{volume}{separator}");
	let remainder = path.strip_prefix(&current).unwrap_or(path);
	for part in remainder.split(separator) {
		if part.is_empty() {
			continue;
		}
		current = gopath::join(&[&current, part]);
		match std::fs::symlink_metadata(&current) {
			Ok(metadata) if metadata.file_type().is_symlink() => return true,
			Ok(_) => {}
			Err(_) => return false,
		}
	}
	false
}
