//! `project_artifacts.go`: a project's runtime artifacts (`config.json`, `globs.json`).

use std::collections::BTreeMap;

use rotor_support::gopath;

use crate::build_info::{BuildMetadata, GoStrings};
use crate::build_info_clone::clone_runtime_config;
use crate::glob::{GlobPaths, expand_globs, marshal_globs_artifact, resolve_glob_paths};
use crate::project::Project;
use crate::project_discovery::{find_package_json, package_identity};
use crate::runtime_config::{RuntimeConfig, marshal_runtime_config_artifact};
use crate::{Error, ErrorKind};

/// The runtime artifacts: `config.json` and `globs.json` bytes, each `None` when not written.
pub type RuntimeArtifacts = (Option<Vec<u8>>, Option<Vec<u8>>);

impl Project {
	/// `AddGlob`
	pub fn add_glob(&mut self, pattern: &str, origin: &str) {
		self.build_info.add_glob(pattern, origin);
	}

	/// `InvalidateGlobs`
	pub fn invalidate_globs(&mut self, origin: &str) {
		self.build_info.invalidate_globs(origin);
	}

	/// `RuntimeArtifacts`: expands the recorded globs, then builds a game's runtime artifacts.
	///
	/// # Errors
	///
	/// Expanding or resolving a glob failed, or a package's root cannot be found.
	pub fn runtime_artifacts(&mut self) -> Result<RuntimeArtifacts, Error> {
		let mut local = self.build_info.snapshot();
		if let Some(paths) = local
			.metadata
			.as_ref()
			.and_then(|metadata| metadata.globs.as_ref()?.paths.as_ref())
		{
			let patterns: Vec<String> = paths.keys().cloned().collect();
			let expanded = expand_globs(&self.root_directory, &self.translator, &patterns)?;
			let expanded: BTreeMap<String, GoStrings> = expanded
				.into_iter()
				.map(|(pattern, matches)| (pattern, Some(matches)))
				.collect();
			self.build_info.set_glob_paths(&expanded);
			local = self.build_info.snapshot();
		}
		if !self.is_game {
			return Ok((None, None));
		}
		let packages = self.build_info.package_snapshots();
		let mut config_packages: BTreeMap<String, RuntimeConfig> = BTreeMap::new();
		let mut glob_packages: BTreeMap<String, GlobPaths> = BTreeMap::new();

		for package in &packages {
			let Some(prefix) = package.identifier_prefix() else {
				continue;
			};
			if let Some(config) = package.metadata.as_ref().and_then(|metadata| metadata.config.as_ref())
				&& let Some(cloned) = clone_runtime_config(Some(config))
			{
				config_packages.insert(prefix.to_owned(), cloned);
			}
			let Some(paths) = package
				.metadata
				.as_ref()
				.and_then(|metadata| metadata.globs.as_ref()?.paths.as_ref())
			else {
				continue;
			};
			let root = self.package_artifact_root(&package.path)?;
			let resolved = resolve_glob_paths(&root, Some(&self.resolver), &clone_glob_matches(paths))?;
			glob_packages.insert(prefix.to_owned(), resolved);
		}

		let game_config = runtime_config_from_metadata(local.metadata.as_ref());
		let config_json = marshal_runtime_config_artifact(game_config.as_ref(), &config_packages);

		let game_globs = match local.metadata.as_ref().and_then(|metadata| metadata.globs.as_ref()) {
			Some(globs) => {
				let expanded = globs.paths.as_ref().map(clone_glob_matches).unwrap_or_default();
				let mut resolved = resolve_glob_paths(&self.root_directory, Some(&self.resolver), &expanded)?;
				if self.config.obfuscation {
					resolved = self.obfuscate_glob_patterns(&resolved)?;
				}
				Some(resolved)
			}
			None => None,
		};
		let globs_json = marshal_globs_artifact(game_globs.as_ref(), &glob_packages);
		Ok((config_json, globs_json))
	}

	fn obfuscate_glob_patterns(&mut self, paths: &GlobPaths) -> Result<GlobPaths, Error> {
		let mut obfuscated = GlobPaths::new();
		for (pattern, matches) in paths {
			let hash = self.hash_string(pattern, "addPaths")?;
			obfuscated.insert(hash, matches.clone());
		}
		Ok(obfuscated)
	}

	/// `Persist`: builds the runtime artifacts and writes them with the build info.
	///
	/// # Errors
	///
	/// Building or writing the artifacts failed.
	pub fn persist(&mut self) -> Result<(), Error> {
		let (config_json, globs_json) = self.runtime_artifacts()?;
		self.persist_artifacts(config_json.as_deref(), globs_json.as_deref())
	}

	fn package_artifact_root(&self, build_info_path: &str) -> Result<String, Error> {
		let root = gopath::dir(build_info_path);
		if let Ok(relative) = gopath::rel(&self.root_directory, &root)
			&& relative != ".."
			&& !relative.starts_with(&format!("..{}", std::path::MAIN_SEPARATOR))
		{
			return Ok(root);
		}
		let package_json_path = find_package_json(&root);
		if package_json_path.is_empty() {
			return Err(Error::kinded(
				ErrorKind::PackageNotFound,
				&format!(" for package build info {build_info_path}"),
			));
		}
		let data = std::fs::read(&package_json_path).map_err(|error| {
			Error::io("open", &package_json_path, &error).wrap("read package identity for build info")
		})?;
		let Some((name, _)) = package_identity(&data).filter(|(name, _)| !name.is_empty()) else {
			return Err(Error::kinded(
				ErrorKind::InvalidPackage,
				&format!(" at {package_json_path}"),
			));
		};
		Ok(gopath::join(&[
			&self.root_directory,
			"node_modules",
			&gopath::from_slash(&name),
		]))
	}
}

fn runtime_config_from_metadata(metadata: Option<&BuildMetadata>) -> Option<RuntimeConfig> {
	clone_runtime_config(metadata?.config.as_ref())
}

fn clone_glob_matches(paths: &BTreeMap<String, GoStrings>) -> BTreeMap<String, Vec<String>> {
	paths
		.iter()
		.map(|(pattern, matches)| (pattern.clone(), matches.clone().unwrap_or_default()))
		.collect()
}
