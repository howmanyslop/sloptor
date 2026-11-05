//! `path.go`: project-relative path translation.

use rotor_rojo::PathTranslator;
use rotor_support::gopath;
use rotor_support::gostr::quote;

use crate::{Error, ErrorKind};

/// `TranslateOutputPath`: applies the project's roblox-ts path translator and returns a
/// slash-separated path relative to `root`.
///
/// # Errors
///
/// The input or output escapes `root`.
pub fn translate_output_path(root: &str, translator: &PathTranslator, input_path: &str) -> Result<String, Error> {
	let input = project_path(root, input_path)?;
	let output = translator.get_output_path(&input);
	project_relative_path(root, &output)
}

pub(crate) fn project_path(root: &str, file_path: &str) -> Result<String, Error> {
	let abs_root = crate::abs(root).map_err(|error| Error::msg(format!("flamework: resolve project root: {error}")))?;
	let mut abs_path = file_path.to_owned();
	if !gopath::is_abs(&abs_path) {
		abs_path = gopath::join(&[&abs_root, &gopath::from_slash(&abs_path)]);
	}
	let abs_path = crate::abs(&abs_path)
		.map_err(|error| Error::msg(format!("flamework: resolve path {}: {error}", quote(file_path))))?;
	project_relative_path(&abs_root, &abs_path)?;
	Ok(abs_path)
}

pub(crate) fn project_relative_path(root: &str, file_path: &str) -> Result<String, Error> {
	let abs_root = crate::abs(root).map_err(|error| Error::msg(format!("flamework: resolve project root: {error}")))?;
	let abs_path = crate::abs(file_path)
		.map_err(|error| Error::msg(format!("flamework: resolve path {}: {error}", quote(file_path))))?;
	let rel = gopath::rel(&abs_root, &abs_path).map_err(|error| {
		Error::msg(format!(
			"flamework: make {} project-relative: {error}",
			quote(file_path)
		))
	})?;
	if rel == ".." || rel.starts_with(&format!("..{}", std::path::MAIN_SEPARATOR)) || gopath::is_abs(&rel) {
		return Err(Error::kinded(ErrorKind::PathEscape, &format!(": {}", quote(file_path))));
	}
	Ok(gopath::to_slash(&rel).into_owned())
}
