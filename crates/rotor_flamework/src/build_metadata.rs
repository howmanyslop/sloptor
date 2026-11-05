//! `build_metadata.go`: the runtime config and glob parts of a build info.

use std::collections::BTreeMap;

use crate::build_info::{BuildGlobs, BuildInfo, BuildMetadata, GoStrings};
use crate::build_info_clone::{clone_runtime_config, go_append_nil};
use crate::runtime_config::RuntimeConfig;

impl BuildInfo {
	fn metadata_mut(&mut self) -> &mut BuildMetadata {
		if self.metadata.is_none() {
			self.ensure_field("metadata");
		}
		self.metadata.get_or_insert_with(BuildMetadata::default)
	}

	/// `SetRuntimeConfig`
	pub fn set_runtime_config(&mut self, config: Option<&RuntimeConfig>) {
		let cloned = clone_runtime_config(config);
		self.metadata_mut().config = cloned;
	}

	/// `AddGlob`: records `glob` with no matches yet, used by `origin`.
	pub fn add_glob(&mut self, glob: &str, origin: &str) {
		let globs = self.metadata_mut().globs.get_or_insert_with(BuildGlobs::default);
		globs
			.paths
			.get_or_insert_with(BTreeMap::new)
			.insert(glob.to_owned(), Some(Vec::new()));
		globs
			.origins
			.get_or_insert_with(BTreeMap::new)
			.entry(origin.to_owned())
			.or_insert(None)
			.get_or_insert_with(Vec::new)
			.push(glob.to_owned());
	}

	/// `SetGlobPaths`
	pub fn set_glob_paths(&mut self, paths: &BTreeMap<String, GoStrings>) {
		let globs = self.metadata_mut().globs.get_or_insert_with(BuildGlobs::default);
		globs.paths = Some(
			paths
				.iter()
				.map(|(pattern, matches)| (pattern.clone(), go_append_nil(matches.as_deref().unwrap_or_default())))
				.collect(),
		);
	}

	/// `InvalidateGlobs`: forgets `origin`'s globs and every glob no origin still uses.
	pub fn invalidate_globs(&mut self, origin: &str) {
		let Some(BuildMetadata {
			globs: Some(BuildGlobs {
				paths: Some(paths),
				origins: Some(origins),
			}),
			..
		}) = &mut self.metadata
		else {
			return;
		};
		origins.remove(origin);
		paths.retain(|glob, _| {
			origins
				.values()
				.any(|globs| globs.iter().flatten().any(|candidate| candidate == glob))
		});
	}
}
