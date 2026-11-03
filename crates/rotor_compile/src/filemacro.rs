//! `filemacro.go`: the build-time data-file reader of the `$file` macro, rooted at the project.
//!
//! A path resolves from the project root, or from the importing file when it begins with `./` or
//! `../`. Reads are capped so a stray huge file cannot blow up the compile.

use std::io;

use rotor_support::gopath;

/// `maxFileMacroBytes`: 8 MiB.
const MAX_FILE_MACRO_BYTES: u64 = 8 << 20;

/// `fileResolver`
#[derive(Clone, Debug)]
pub struct FileResolver {
	/// The absolute slash-separated project directory.
	project_dir: String,
}

impl FileResolver {
	/// `newFileResolver`
	#[must_use]
	pub fn new(project_dir: &str) -> Self {
		Self {
			project_dir: gopath::to_slash(project_dir).into_owned(),
		}
	}

	/// `resolvePath` for the macro input scan: `None` where Go returns an error.
	pub(crate) fn resolve_path_public(&self, importer_path: &str, path: &str) -> Option<String> {
		self.resolve_path(importer_path, path).ok()
	}

	/// `resolvePath`: the absolute path of the macro argument.
	fn resolve_path(&self, importer_path: &str, path: &str) -> io::Result<String> {
		let trimmed = path.trim();
		if trimmed.is_empty() {
			return Err(io::Error::new(io::ErrorKind::NotFound, "file not found: (empty path)"));
		}
		let slash = gopath::to_slash(trimmed);
		if slash.starts_with("./") || slash.starts_with("../") {
			let base = if importer_path.is_empty() {
				self.project_dir.clone()
			} else {
				gopath::to_slash(&gopath::dir(&gopath::from_slash(importer_path))).into_owned()
			};
			return Ok(gopath::join(&[&gopath::from_slash(&base), &gopath::from_slash(&slash)]));
		}
		Ok(gopath::join(&[
			&gopath::from_slash(&self.project_dir),
			&gopath::from_slash(&slash),
		]))
	}
}

impl rotor_transformer::FileResolver for FileResolver {
	/// `Read`: the raw bytes of the file `path` names. A missing or unreadable file is
	/// `ErrorKind::NotFound` (Go's `ErrFileNotFound`); an oversized one is another error.
	fn read(&self, importer_path: &str, path: &str) -> io::Result<Vec<u8>> {
		let abs_path = self.resolve_path(importer_path, path)?;
		let not_found = || io::Error::new(io::ErrorKind::NotFound, format!("file not found: {path}"));
		let metadata = std::fs::metadata(&abs_path).map_err(|_| not_found())?;
		if metadata.is_dir() {
			return Err(not_found());
		}
		if metadata.len() > MAX_FILE_MACRO_BYTES {
			return Err(io::Error::other(format!(
				"$file {} is too large ({} bytes; limit {MAX_FILE_MACRO_BYTES})",
				rotor_support::gostr::quote(path),
				metadata.len()
			)));
		}
		std::fs::read(&abs_path).map_err(|_| not_found())
	}
}
