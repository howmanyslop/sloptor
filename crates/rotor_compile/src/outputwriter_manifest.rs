//! `outputwriter_manifest.go`: validating the previous build's output hashes and indexing which
//! manifest outputs exist.

use std::collections::{BTreeMap, HashMap, HashSet};

use rotor_support::gopath;
use rustc_hash::FxHashMap;

use crate::output_stamps::FileStamp;
use crate::outputwriter::OutputWriter;
use crate::package_layout::hex_sha256;
use crate::parallelwrite::{WRITE_SYNC_CUTOFF, parallel_fold_with_cutoff, write_workers};

impl OutputWriter {
	/// `validatedOutputHashes`: the outputs whose file on disk still has the recorded hash.
	pub(crate) fn validated_output_hashes(&self, outputs: &BTreeMap<String, String>) -> FxHashMap<String, String> {
		if outputs.is_empty() {
			return FxHashMap::default();
		}
		let mut items: Vec<(&String, &String, String)> = Vec::with_capacity(outputs.len());
		let mut confined_parents: HashSet<String> = HashSet::new();
		for (path, hash) in outputs {
			let Some(key) = normalize_output_presence_key(path, self.case_sensitive) else {
				continue;
			};
			if !gopath::is_local(&gopath::from_slash(&key)) {
				continue;
			}
			let absolute = gopath::join(&[&self.project_dir, &gopath::from_slash(&key)]);
			let parent = gopath::dir(&key);
			if confined_parents.insert(parent) && self.confine_parent(&absolute).is_err() {
				continue;
			}
			items.push((path, hash, absolute));
		}

		let chunks = parallel_fold_with_cutoff(
			write_workers(),
			WRITE_SYNC_CUTOFF,
			&items,
			|acc: &mut Vec<(String, String, FileStamp)>, (path, hash, absolute)| {
				let Ok(metadata) = std::fs::symlink_metadata(absolute) else {
					return Ok(());
				};
				if !metadata.file_type().is_file() {
					return Ok(());
				}
				let stamp = FileStamp::of(&metadata);
				if self.stamps.holds(path, stamp, hash) {
					if let Some(stamp) = stamp {
						acc.push(((*path).clone(), (*hash).clone(), stamp));
					}
					return Ok(());
				}
				let Ok(contents) = std::fs::read(absolute) else {
					return Ok(());
				};
				if hex_sha256(&contents) == **hash
					&& let Some(stamp) = stamp
				{
					acc.push(((*path).clone(), (*hash).clone(), stamp));
				}
				Ok(())
			},
		);

		let mut validated = FxHashMap::with_capacity_and_hasher(items.len(), rustc_hash::FxBuildHasher);
		let mut stamp_entries = Vec::with_capacity(items.len());
		for chunk in chunks.unwrap_or_default() {
			for (path, hash, stamp) in chunk {
				stamp_entries.push((path.clone(), stamp, hash.clone()));
				validated.insert(path, hash);
			}
		}
		self.stamps.observe_batch(stamp_entries);
		validated
	}

	/// `newOutputPresenceIndex(previous)` for the validated previous outputs, which validation
	/// just found to be regular files, without reading their directories again.
	pub(crate) fn validated_presence_index(&self) -> OutputPresenceIndex {
		OutputPresenceIndex {
			case_sensitive: self.case_sensitive,
			regular: self
				.previous
				.keys()
				.filter_map(|path| normalize_output_presence_key(path, self.case_sensitive))
				.collect(),
		}
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
	if !gopath::is_local(&key) {
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
