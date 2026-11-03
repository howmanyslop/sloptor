//! A build-local snapshot of project-file contents.

use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex, PoisonError};

use rotor_support::gopath;

use crate::config::validate_config;
use crate::json::{self, JsonValue};
use crate::resolver::realpath_or;

/// A build-local snapshot of project contents.
///
/// Package resolver discovery and declaration-to-runtime remapping must read
/// the same contents. Construct a new snapshot for each build, including
/// incremental rebuilds.
#[derive(Debug, Default)]
pub struct ProjectFiles {
	configs: Mutex<HashMap<String, Arc<JsonValue>>>,
}

impl ProjectFiles {
	/// An empty snapshot.
	#[must_use]
	pub fn new() -> Self {
		Self::default()
	}

	/// The parsed contents of the project file at `path`, reading it on first
	/// use. An unreadable or malformed file is [`JsonValue::Invalid`].
	pub(crate) fn read(&self, path: &str) -> Arc<JsonValue> {
		let path = gopath::clean(&realpath_or(path));
		if let Some(config) = self.configs.lock().unwrap_or_else(PoisonError::into_inner).get(&path) {
			return Arc::clone(config);
		}
		let config = Arc::new(fs::read(&path).map_or(JsonValue::Invalid, |contents| json::parse(&contents)));
		let mut configs = self.configs.lock().unwrap_or_else(PoisonError::into_inner);
		Arc::clone(configs.entry(path).or_insert(config))
	}

	/// A validated project's string `tree.$path`. The optional object form
	/// deliberately does not participate in runtime-path remapping.
	#[must_use]
	pub fn root_path(&self, path: &str) -> Option<String> {
		let config = self.read(path);
		validate_config(&config).ok()?;
		let root_path = config.member("tree")?.member("$path")?;
		root_path.as_str().map(str::to_owned)
	}

	/// Every project path read so far, sorted. It includes absent candidates
	/// so watch mode can observe their creation.
	#[must_use]
	pub fn paths(&self) -> Vec<String> {
		let mut paths: Vec<String> = self
			.configs
			.lock()
			.unwrap_or_else(PoisonError::into_inner)
			.keys()
			.cloned()
			.collect();
		paths.sort();
		paths
	}
}
