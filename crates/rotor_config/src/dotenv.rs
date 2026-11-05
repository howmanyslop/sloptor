//! Loads the compile-time environment consumed by rotor's `$env` macro (a
//! built-in replacement for the community `rbxts-transform-env` transformer
//! plugin).
//!
//! Lookup priority, highest first:
//!
//! 1. the real process environment,
//! 2. `.env.<NODE_ENV>` in the project directory,
//! 3. `.env` in the project directory.
//!
//! `NODE_ENV` itself resolves from the process environment first, then from
//! the `.env` file. Only plain string values are supported: `KEY=VALUE` lines,
//! full-line `#` comments, optional matching single or double quotes stripped
//! from the value, no multiline values, no escapes, no variable expansion.

use std::collections::HashMap;
use std::fs;

use rotor_support::gopath;

/// One project's compile-time environment snapshot.
///
/// The file-derived values are read once per [`Env::load`] call; the process
/// environment is consulted live at [`Env::lookup`] time. Each build reloads
/// the environment. Macro values are fingerprinted by the compiler and the
/// loaded files are exposed to watch mode.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Env {
	file_vars: HashMap<String, String>,
	files: Vec<String>,
}

/// `os.LookupEnv`: the value of a process environment variable, which is
/// `Some` even when empty.
fn lookup_process_env(name: &str) -> Option<String> {
	// Names the OS cannot hold are simply unset (and `var_os` would panic).
	if name.is_empty() || name.contains(['=', '\0']) {
		return None;
	}
	std::env::var_os(name).map(|value| value.to_string_lossy().into_owned())
}

impl Env {
	/// Reads `.env` and `.env.<NODE_ENV>` from `dir` (the directory containing
	/// the project's tsconfig). Missing files are fine; an `Env` with no file
	/// values still resolves process-environment variables.
	#[must_use]
	pub fn load(dir: &str) -> Self {
		let mut file_vars = HashMap::new();

		let mut files = vec![gopath::join(&[dir, ".env"])];
		let base = read_env_file(&files[0]);
		file_vars.extend(base.iter().map(|(k, v)| (k.clone(), v.clone())));

		// NODE_ENV: process environment first, then the .env file itself.
		let node_env = lookup_process_env("NODE_ENV")
			.filter(|value| !value.is_empty())
			.or_else(|| base.get("NODE_ENV").cloned())
			.unwrap_or_default();
		if !node_env.is_empty() {
			// .env.<NODE_ENV> values override .env values.
			let overlay = gopath::join(&[dir, &format!(".env.{node_env}")]);
			file_vars.extend(read_env_file(&overlay));
			files.push(overlay);
		}

		Self { file_vars, files }
	}

	/// Resolves `name` with the documented priority: the process environment,
	/// then the merged `.env` file values. `None` only when `name` is defined
	/// nowhere.
	#[must_use]
	pub fn lookup(&self, name: &str) -> Option<String> {
		lookup_process_env(name).or_else(|| self.file_vars.get(name).cloned())
	}

	/// [`Self::lookup`] for a project that may have no env at all: with
	/// `None` only the process environment answers, so transformer states
	/// constructed without a project env still behave sensibly.
	#[must_use]
	pub fn lookup_in(env: Option<&Self>, name: &str) -> Option<String> {
		env.map_or_else(|| lookup_process_env(name), |env| env.lookup(name))
	}

	/// Lists environment inputs, including absent files that may be created.
	#[must_use]
	pub fn files(&self) -> Vec<String> {
		self.files.clone()
	}
}

/// Parses `path` with [`parse`]; a missing or unreadable file yields nothing.
fn read_env_file(path: &str) -> HashMap<String, String> {
	fs::read(path).map_or_else(|_| HashMap::new(), |data| parse(&String::from_utf8_lossy(&data)))
}

/// Parses `.env` text.
///
/// One `KEY=VALUE` per line, CRLF tolerated, blank lines and full-line `#`
/// comments skipped, surrounding whitespace trimmed from key and value, one
/// level of matching single or double quotes stripped from the value
/// (whitespace inside quotes is preserved). Lines without `=` and lines with
/// an empty key are ignored. Later duplicates win.
#[must_use]
pub fn parse(src: &str) -> HashMap<String, String> {
	let mut vars = HashMap::new();
	for line in src.split('\n') {
		let line = line.trim_end_matches('\r').trim();
		if line.is_empty() || line.starts_with('#') {
			continue;
		}
		let Some((key, value)) = line.split_once('=') else {
			continue;
		};
		let key = key.trim();
		if key.is_empty() {
			continue;
		}
		let mut value = value.trim();
		let bytes = value.as_bytes();
		if bytes.len() >= 2
			&& ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
				|| (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
		{
			value = &value[1..value.len() - 1];
		}
		vars.insert(key.to_owned(), value.to_owned());
	}
	vars
}

#[cfg(test)]
mod tests;
