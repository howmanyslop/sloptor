//! Reading `rotor.toml` from a project directory.

use std::fmt;
use std::fs;

use rotor_support::gopath;
use rotor_support::gostr::{io_error_text, path_error, quote};

use super::decode::{self, key_string};
use super::model::Config;

/// The primary config file rotor loads.
pub const CONFIG_FILE_NAME: &str = "rotor.toml";

/// Why [`load`] did not produce a [`Config`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
	/// The project has no `rotor.toml`. Callers treat the config as optional.
	NotFound,
	/// The file exists but could not be read or decoded; the text is the full
	/// message to print.
	Failed(String),
}

impl fmt::Display for LoadError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::NotFound => f.write_str("rotor.toml not found"),
			Self::Failed(message) => f.write_str(message),
		}
	}
}

impl std::error::Error for LoadError {}

/// Reads the project's `rotor.toml` into a [`Config`].
///
/// Decoding is typed: unknown keys are tolerated for forward compatibility
/// and recorded as sorted warnings on the returned config rather than failing,
/// except under `flamework`, where an unknown key is an error. Validation of
/// compiler settings is left to [`Config::validate`].
///
/// # Errors
///
/// Returns [`LoadError::NotFound`] when `rotor.toml` does not exist, and
/// [`LoadError::Failed`] when it cannot be read, parsed, or decoded.
pub fn load(project_dir: &str) -> Result<Config, LoadError> {
	let abs_dir = gopath::abs(project_dir)
		.map_err(|err| LoadError::Failed(format!("config: resolving project dir: {}", io_error_text(&err))))?;

	let config_path = gopath::join(&[&abs_dir, CONFIG_FILE_NAME]);
	if !fs::metadata(&config_path).is_ok_and(|info| !info.is_dir()) {
		return Err(LoadError::NotFound);
	}

	let data = fs::read(&config_path).map_err(|err| {
		LoadError::Failed(format!(
			"config: reading {CONFIG_FILE_NAME}: {}",
			path_error("open", &config_path, &err)
		))
	})?;

	let failed = |message: String| LoadError::Failed(format!("config: {CONFIG_FILE_NAME}: {message}"));
	let text = String::from_utf8(data).map_err(|_| failed("toml: invalid UTF-8".to_owned()))?;
	let mut decoded = decode::decode(&text).map_err(failed)?;
	if let Some(key) = decoded
		.undecoded
		.iter()
		.find(|key| key.first().is_some_and(|first| first == "flamework"))
	{
		return Err(failed(format!("flamework: unknown key {}", quote(&key_string(key)))));
	}
	decoded.config.apply_flamework_defaults();

	// Unknown keys are forward-compatible: collect them as sorted warnings.
	let mut keys: Vec<String> = decoded.undecoded.iter().map(|key| key_string(key)).collect();
	keys.sort();
	decoded.config.warnings = keys
		.iter()
		.map(|key| format!("{CONFIG_FILE_NAME}: unknown key {} (ignored)", quote(key)))
		.collect();
	Ok(decoded.config)
}
