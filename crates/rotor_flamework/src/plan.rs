//! `plan.go`: per-file Flamework class plans.

use std::collections::HashMap;

use rotor_support::gopath;

use crate::metadata::Metadata;
use crate::{Error, ErrorKind};

/// `DecoratorPlan`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DecoratorPlan {
	/// The decorator's name.
	pub name: String,
	/// The decorator's internal ID.
	pub internal_id: String,
}

/// `ClassPlan`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClassPlan {
	/// The declaring file's ID.
	pub file_path: String,
	/// The class's internal ID.
	pub internal_id: String,
	/// The class's Flamework decorators.
	pub decorators: Vec<DecoratorPlan>,
	pub(crate) contains_legacy_decorator: bool,
}

/// `FileAnalysis`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileAnalysis {
	/// The project-relative file ID.
	pub file_id: String,
	/// The file's metadata.
	pub metadata: Metadata,
	/// The file's classes.
	pub classes: Vec<ClassPlan>,
}

/// `FilePlan`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilePlan {
	pub(crate) file_id: String,
	pub(crate) metadata: Metadata,
	pub(crate) classes: Vec<ClassPlan>,
}

impl FilePlan {
	/// `FileID`
	#[must_use]
	pub fn file_id(&self) -> &str {
		&self.file_id
	}

	/// `Metadata`
	#[must_use]
	pub fn metadata(&self) -> Metadata {
		self.metadata.clone()
	}

	/// `Classes`
	#[must_use]
	pub fn classes(&self) -> Vec<ClassPlan> {
		self.classes.clone()
	}
}

/// `AnalysisState`
#[derive(Clone, Debug, Default)]
pub struct AnalysisState {
	plans: Vec<FilePlan>,
	class_cache: HashMap<String, ClassPlan>,
	macro_cache: HashMap<String, bool>,
}

impl AnalysisState {
	/// `NewAnalysisState`
	#[must_use]
	pub fn new() -> Self {
		Self::default()
	}

	/// `Analyze`: the plans, ordered by file ID.
	///
	/// # Errors
	///
	/// A file ID is invalid or repeated, or a class appears twice.
	pub fn analyze(&mut self, inputs: &[FileAnalysis]) -> Result<Vec<FilePlan>, Error> {
		let mut ordered = inputs.to_vec();
		for input in &mut ordered {
			input.file_id = normalize_file_id(&input.file_id)?;
		}
		// sort.Slice is not stable; equal IDs are rejected below, so the order is total.
		ordered.sort_by(|left, right| left.file_id.cmp(&right.file_id));

		let mut plans = Vec::with_capacity(ordered.len());
		let mut class_cache = HashMap::new();
		for (index, input) in ordered.iter().enumerate() {
			if index > 0 && input.file_id == ordered[index - 1].file_id {
				return Err(Error::kinded(ErrorKind::DuplicateFile, &format!(": {}", input.file_id)));
			}
			let mut classes = input.classes.clone();
			for class in &mut classes {
				class.file_path.clone_from(&input.file_id);
				if class_cache.contains_key(&class.internal_id) {
					return Err(Error::kinded(
						ErrorKind::DuplicateClass,
						&format!(": {}", class.internal_id),
					));
				}
				class_cache.insert(class.internal_id.clone(), class.clone());
			}
			plans.push(FilePlan {
				file_id: input.file_id.clone(),
				metadata: input.metadata.clone(),
				classes,
			});
		}
		self.plans.clone_from(&plans);
		self.class_cache = class_cache;
		Ok(plans)
	}

	/// `Plans`
	#[must_use]
	pub fn plans(&self) -> Vec<FilePlan> {
		self.plans.clone()
	}

	/// `Class`
	#[must_use]
	pub fn class(&self, internal_id: &str) -> Option<ClassPlan> {
		self.class_cache.get(internal_id).cloned()
	}

	/// `CacheUserMacro`
	pub fn cache_user_macro(&mut self, symbol: &str, is_macro: bool) {
		self.macro_cache.insert(symbol.to_owned(), is_macro);
	}

	/// `UserMacro`
	#[must_use]
	pub fn user_macro(&self, symbol: &str) -> Option<bool> {
		self.macro_cache.get(symbol).copied()
	}
}

/// `normalizeFileID`
pub(crate) fn normalize_file_id(file_id: &str) -> Result<String, Error> {
	if gopath::is_abs(file_id) {
		return Err(Error::kinded(ErrorKind::InvalidFileId, &format!(": {file_id}")));
	}
	let cleaned = gopath::to_slash(&gopath::clean(&gopath::from_slash(file_id))).into_owned();
	if cleaned == "." || cleaned == ".." || cleaned.starts_with("../") {
		return Err(Error::kinded(ErrorKind::InvalidFileId, &format!(": {file_id}")));
	}
	Ok(cleaned)
}
