//! `selectsymlinkpath.go`

use rotor_support::gopath;

/// `selectSymlinkPath`: the first symlink inside `node_modules_path`, else the first symlink.
#[must_use]
pub fn select_symlink_path(symlink_paths: &[String], node_modules_path: &str) -> String {
	if !node_modules_path.is_empty() {
		for symlink_path in symlink_paths {
			if let Ok(relative) = gopath::rel(node_modules_path, symlink_path)
				&& relative != ".."
				&& !relative.starts_with(&format!("..{}", std::path::MAIN_SEPARATOR))
			{
				return symlink_path.clone();
			}
		}
	}
	symlink_paths.first().cloned().unwrap_or_default()
}
