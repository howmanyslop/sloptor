//! `state.go`: a project's identifiers, string hashes, classes and artifacts.

use std::collections::BTreeMap;

use rotor_support::gopath;

use crate::artifacts::{Artifact, local_artifact_path, persist_artifacts, prepare_artifacts};
use crate::build_info::{BuildClass, BuildInfo, BuildInfoSnapshot};
use crate::id::{DEFAULT_HASH_CONTEXT, IdentifierRequest, generate_identifier, new_salt};
use crate::project::{IdMode, Project};
use crate::{Error, ErrorKind};

/// `FlameworkVersion`: the Flamework release this port reproduces.
pub const FLAMEWORK_VERSION: &str = "1.3.2";

/// `DeclarationIdentity`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeclarationIdentity {
	/// The declaration's internal ID.
	pub internal_id: String,
	/// The declaration's name.
	pub declaration_name: String,
	/// The declaring file's Lua module name.
	pub lua_file_name: String,
	/// A package declaration's build prefix.
	pub package_prefix: String,
	/// Whether the declaration belongs to a package.
	pub is_package: bool,
}

impl Project {
	/// The identifier request for `declaration`, numbered after the build info's identifiers.
	pub(crate) fn identifier_request(&self, declaration: &DeclarationIdentity, salt: String) -> IdentifierRequest {
		IdentifierRequest {
			mode: self.id_mode.clone(),
			salt,
			hash_prefix: self.hash_prefix.clone(),
			package_name: self.package_name.clone(),
			internal_id: declaration.internal_id.clone(),
			declaration_name: declaration.declaration_name.clone(),
			lua_file_name: declaration.lua_file_name.clone(),
			package_prefix: declaration.package_prefix.clone(),
			next_id: self.build_info.latest_id() as u64,
			is_game: self.is_game,
			is_package: declaration.is_package,
		}
	}

	/// `Identifier`: the persisted identifier of a declaration, or a new one.
	///
	/// # Errors
	///
	/// Generating the identifier or the salt failed.
	pub fn identifier(&mut self, declaration: &DeclarationIdentity) -> Result<String, Error> {
		if let Some(existing) = self.build_info.identifier(&declaration.internal_id) {
			return Ok(existing);
		}
		let mut salt = self.config.salt.clone();
		if !declaration.is_package && self.id_mode != IdMode::Full && salt.is_empty() {
			salt = self.identifier_salt()?;
		}
		let id = generate_identifier(&self.identifier_request(declaration, salt)).map_err(|error| {
			error.wrap(&format!(
				"generate identifier {}",
				rotor_support::gostr::quote(&declaration.internal_id)
			))
		})?;
		if declaration.is_package {
			return Ok(id);
		}
		self.build_info_mut().add_identifier(&declaration.internal_id, &id)?;
		Ok(id)
	}

	/// `PreloadIdentifiers`: identifiers for `declarations`, generated in internal-ID order.
	///
	/// # Errors
	///
	/// A declaration repeats, or generating an identifier failed.
	pub fn preload_identifiers(
		&mut self,
		declarations: &[DeclarationIdentity],
	) -> Result<BTreeMap<String, String>, Error> {
		let mut ordered = declarations.to_vec();
		ordered.sort_by(|left, right| left.internal_id.cmp(&right.internal_id));
		let mut identifiers = BTreeMap::new();
		for declaration in &ordered {
			if identifiers.contains_key(&declaration.internal_id) {
				return Err(Error::kinded(
					ErrorKind::DuplicateIdentifier,
					&format!(": {}", declaration.internal_id),
				));
			}
			let identifier = self.identifier(declaration)?;
			identifiers.insert(declaration.internal_id.clone(), identifier);
		}
		Ok(identifiers)
	}

	/// `HashString`: the persisted UUID for `context:text` (`@` when `context` is empty).
	///
	/// # Errors
	///
	/// The system's randomness failed.
	pub fn hash_string(&mut self, text: &str, context: &str) -> Result<String, Error> {
		let context = if context.is_empty() {
			DEFAULT_HASH_CONTEXT
		} else {
			context
		};
		self.build_info_mut().hash_string(text, context)
	}

	/// `AddPackageBuildInfo`
	///
	/// # Errors
	///
	/// The package's build info cannot be loaded.
	pub fn add_package_build_info(&mut self, path: &str) -> Result<(), Error> {
		let package_info = BuildInfo::load(path, crate::state::FLAMEWORK_VERSION).map_err(|error| {
			error.wrap(&format!(
				"load package Flamework build info {}",
				rotor_support::gostr::quote(path)
			))
		})?;
		self.build_info_mut().add_package(package_info);
		Ok(())
	}

	/// `AddClass`
	///
	/// # Errors
	///
	/// The class is already recorded.
	pub fn add_class(&mut self, class: &BuildClass) -> Result<(), Error> {
		self.build_info_mut().add_class(class)
	}

	/// `BuildInfoSnapshot`
	#[must_use]
	pub fn build_info_snapshot(&self) -> BuildInfoSnapshot {
		self.build_info.snapshot()
	}

	/// `PackageBuildInfoSnapshots`
	#[must_use]
	pub fn package_build_info_snapshots(&self) -> Vec<BuildInfoSnapshot> {
		self.build_info.package_snapshots()
	}

	/// The project's build info.
	#[must_use]
	pub fn build_info(&self) -> &BuildInfo {
		&self.build_info
	}

	/// `PrepareArtifacts`: the artifacts to write, with root-relative paths.
	///
	/// # Errors
	///
	/// An artifact is invalid or lies outside the project root.
	pub fn prepare_artifacts(
		&self,
		config_json: Option<&[u8]>,
		globs_json: Option<&[u8]>,
	) -> Result<Vec<Artifact>, Error> {
		let mut artifacts = if !self.is_game || self.no_include {
			let build_json = self.build_info.marshal_ordered_json()?;
			vec![Artifact {
				path: self.build_info.path().to_owned(),
				data: Some(build_json),
			}]
		} else {
			prepare_artifacts(
				Some(&self.build_info),
				&gopath::join(&[&self.include_directory, "flamework"]),
				config_json,
				globs_json,
			)?
		};
		for artifact in &mut artifacts {
			let relative = gopath::rel(&self.root_directory, &artifact.path).map_err(|error| {
				Error::msg(format!(
					"flamework: resolve artifact path {}: {error}",
					rotor_support::gostr::quote(&artifact.path)
				))
			})?;
			artifact.path = local_artifact_path(&relative)?;
		}
		Ok(artifacts)
	}

	/// `PersistArtifacts`
	///
	/// # Errors
	///
	/// Preparing or writing the artifacts failed.
	pub fn persist_artifacts(&self, config_json: Option<&[u8]>, globs_json: Option<&[u8]>) -> Result<(), Error> {
		let artifacts = self.prepare_artifacts(config_json, globs_json)?;
		persist_artifacts(&self.root_directory, &artifacts)
	}

	/// `identifierSalt`
	fn identifier_salt(&mut self) -> Result<String, Error> {
		if !self.config.salt.is_empty() {
			return Ok(self.config.salt.clone());
		}
		if let Some(salt) = self.build_info.salt() {
			return Ok(salt.to_owned());
		}
		let salt = new_salt()?;
		self.build_info_mut().set_salt(Some(&salt));
		Ok(salt)
	}
}
