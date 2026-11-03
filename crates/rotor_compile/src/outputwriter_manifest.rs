//! `outputwriter_manifest.go`: validating the previous build's output hashes and indexing which
//! manifest outputs exist.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;

use rotor_support::gopath;

use crate::outputwriter::{OutputWriter, is_local};
use crate::package_layout::hex_sha256;
use crate::parallelwrite::{Job, parallelize, write_workers};
use crate::project::lock;

impl OutputWriter {
	/// `validatedOutputHashes`: the outputs whose file on disk still has the recorded hash.
	pub(crate) fn validated_output_hashes(&self, outputs: &BTreeMap<String, String>) -> HashMap<String, String> {
		let validated = Mutex::new(HashMap::with_capacity(outputs.len()));
		let mut jobs: Vec<Job<'_>> = Vec::with_capacity(outputs.len());
		for (path, hash) in outputs {
			let Some(key) = normalize_output_presence_key(path, self.case_sensitive) else {
				continue;
			};
			let validated = &validated;
			jobs.push(Box::new(move || {
				let absolute = gopath::join(&[&self.project_dir, &gopath::from_slash(&key)]);
				let Ok(metadata) = self.lstat(&absolute) else {
					return Ok(());
				};
				if !metadata.file_type().is_file() {
					return Ok(());
				}
				let Ok(contents) = std::fs::read(&absolute) else {
					return Ok(());
				};
				if hex_sha256(&contents) == *hash {
					lock(validated).insert(path.clone(), hash.clone());
				}
				Ok(())
			}));
		}
		let _ = parallelize(write_workers(), jobs);
		validated
			.into_inner()
			.unwrap_or_else(std::sync::PoisonError::into_inner)
	}

	/// `newOutputPresenceIndex`: which of the outputs exist as regular files now, reading each
	/// parent directory once.
	pub(crate) fn new_output_presence_index<'a, I: IntoIterator<Item = &'a String>>(
		&self,
		outputs: I,
	) -> OutputPresenceIndex {
		let mut index = OutputPresenceIndex {
			case_sensitive: self.case_sensitive,
			regular: HashSet::new(),
		};
		let mut keys_by_parent: HashMap<String, Vec<String>> = HashMap::new();
		for path in outputs {
			let Some(key) = normalize_output_presence_key(path, self.case_sensitive) else {
				continue;
			};
			let parent = gopath::dir(&key);
			keys_by_parent.entry(parent).or_default().push(key);
		}
		for (parent, keys) in keys_by_parent {
			let directory = gopath::join(&[&self.project_dir, &gopath::from_slash(&parent)]);
			let Ok(entries) = std::fs::read_dir(&directory) else {
				continue;
			};
			let mut regular = HashSet::new();
			for entry in entries.filter_map(Result::ok) {
				if !entry.file_type().is_ok_and(|file_type| file_type.is_file()) {
					continue;
				}
				let mut name = entry.file_name().to_string_lossy().into_owned();
				if !self.case_sensitive {
					name = name.to_lowercase();
				}
				regular.insert(name);
			}
			for key in keys {
				let mut base = gopath::base(&key);
				if !self.case_sensitive {
					base = base.to_lowercase();
				}
				if regular.contains(&base) {
					index.regular.insert(key);
				}
			}
		}
		index
	}
}

/// `outputPresenceIndex`: a snapshot of which manifest output paths exist as regular files.
pub(crate) struct OutputPresenceIndex {
	case_sensitive: bool,
	regular: HashSet<String>,
}

/// `normalizeOutputPresenceKey`: cleaned, slash-separated, local, and lowercased when the file
/// system is case-insensitive.
pub(crate) fn normalize_output_presence_key(output_path: &str, case_sensitive: bool) -> Option<String> {
	let key = gopath::to_slash(&gopath::clean(&gopath::from_slash(output_path))).into_owned();
	if !is_local(&key) {
		return None;
	}
	Some(if case_sensitive { key } else { key.to_lowercase() })
}

impl OutputPresenceIndex {
	/// `hasRegular`
	pub(crate) fn has_regular(&self, output_path: &str) -> bool {
		normalize_output_presence_key(output_path, self.case_sensitive).is_some_and(|key| self.regular.contains(&key))
	}
}
