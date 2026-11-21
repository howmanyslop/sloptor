//! `outputwriter.go`: writes build outputs, skipping a write when the file already holds the
//! text so unchanged outputs keep their mtimes, and recording each output's hash for the
//! incremental manifest.
//!
//! Go confines writes to the project directory with `os.Root`. Rust's standard library has no
//! equivalent; the writer refuses non-local paths and parents that resolve outside the project
//! directory instead.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use rotor_support::{fsutil, gopath, gostr};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::compile::Error;
use crate::output_stamps::FileStamp;
use crate::package_layout::hex_sha256;
use crate::project::lock;

/// Outcome of a single [`OutputWriter::write_prepared_keyed`] call, accumulated by parallel
/// workers and merged once via [`OutputWriter::apply_write_outcomes`].
pub(crate) struct WriteOutcome {
	pub(crate) wrote: bool,
	pub(crate) key: String,
	pub(crate) hash: String,
	pub(crate) stamp: Option<FileStamp>,
}

/// `outputWriter`
pub struct OutputWriter {
	pub(crate) case_sensitive: bool,
	pub(crate) project_dir: String,
	/// The canonical project directory, once [`OutputWriter::use_hashes`] confined the writer.
	canonical_root: Option<String>,
	pub(crate) previous: FxHashMap<String, String>,
	/// The outputs' stat fields beside their hashes (Rust only, `output_stamps`).
	pub(crate) stamps: crate::output_stamps::OutputStamps,
	current: Option<Mutex<BTreeMap<String, String>>>,
	hash_skips: AtomicUsize,
	prepared: Mutex<FxHashSet<String>>,
	/// `dirRoots`: the canonical form of each parent directory already confined, by its
	/// project-relative path (lowercased on a case-insensitive file system). Only directories
	/// that resolved inside the project are cached; one that does not exist yet is checked again.
	dir_roots: Mutex<FxHashMap<String, PathBuf>>,
	/// Output keys written or confirmed present on disk during this build.
	confirmed_this_build: Mutex<FxHashSet<String>>,
}

impl Default for OutputWriter {
	fn default() -> Self {
		Self::new()
	}
}

fn io_error(error: &io::Error) -> Error {
	Error::message(rotor_support::gostr::io_error_text(error))
}

impl OutputWriter {
	/// `newOutputWriter`
	#[must_use]
	pub fn new() -> Self {
		Self {
			case_sensitive: rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names(),
			project_dir: String::new(),
			canonical_root: None,
			previous: FxHashMap::default(),
			stamps: crate::output_stamps::OutputStamps::default(),
			current: None,
			hash_skips: AtomicUsize::new(0),
			prepared: Mutex::new(FxHashSet::default()),
			dir_roots: Mutex::new(FxHashMap::default()),
			confirmed_this_build: Mutex::new(FxHashSet::default()),
		}
	}

	/// `useHashes`: confines the writer to `project_dir`, validates the previous build's hashes
	/// against the files on disk, and records new hashes into `current`.
	pub(crate) fn use_hashes(
		&mut self,
		project_dir: &str,
		previous: &BTreeMap<String, String>,
		current: BTreeMap<String, String>,
	) -> Result<(), Error> {
		self.project_dir = gopath::clean(project_dir);
		self.current = Some(Mutex::new(current));
		let canonical = fs::canonicalize(&self.project_dir)
			.map_err(|error| Error::message(gostr::path_error("openat", &self.project_dir, &error)))?;
		self.canonical_root = Some(canonical.to_string_lossy().into_owned());
		if !previous.is_empty() {
			self.stamps = crate::output_stamps::OutputStamps::load(&self.project_dir);
		}
		self.previous = self.validated_output_hashes(previous);
		Ok(())
	}

	/// The outputs and hashes recorded so far.
	pub(crate) fn current_outputs(&self) -> BTreeMap<String, String> {
		self.current.as_ref().map_or_default(|current| lock(current).clone())
	}

	/// Replaces the recorded outputs (after pruning).
	pub(crate) fn set_current_outputs(&self, outputs: BTreeMap<String, String>) {
		if let Some(current) = &self.current {
			*lock(current) = outputs;
		}
	}

	/// `write`: writes `contents` to `path` unless it already holds them. True when it wrote.
	pub(crate) fn write(&self, path: &str, contents: &[u8]) -> Result<bool, Error> {
		let key = self.output_key(path);
		self.check_confined_parent(path)?;
		let outcome = self.write_prepared_keyed(path, &key, contents)?;
		let wrote = outcome.wrote;
		self.apply_write_outcomes(std::iter::once(outcome));
		Ok(wrote)
	}

	/// Writes `contents` to `path` (whose parent directory was already confined and prepared, and
	/// whose [`OutputWriter::output_key`] is `key`) using a single `symlink_metadata` + `fstat`
	/// without locking `current`, `stamps`, or `confirmed_this_build`.
	pub(crate) fn write_prepared_keyed(&self, path: &str, key: &str, contents: &[u8]) -> Result<WriteOutcome, Error> {
		let hash = hex_sha256(contents);
		if let Ok(metadata) = fs::symlink_metadata(path) {
			let file_type = metadata.file_type();
			if file_type.is_file() {
				if self.previous.get(key) == Some(&hash) {
					self.hash_skips.fetch_add(1, Ordering::Relaxed);
					return Ok(WriteOutcome {
						wrote: false,
						key: key.to_owned(),
						hash,
						stamp: None,
					});
				}
				if metadata.len() == contents.len() as u64 && fs::read(path).is_ok_and(|existing| existing == contents)
				{
					return Ok(WriteOutcome {
						wrote: false,
						key: key.to_owned(),
						hash,
						stamp: FileStamp::of(&metadata),
					});
				}
			} else if file_type.is_symlink() {
				self.check_confined_final_symlink(path)?;
			}
		}
		let written_meta = fsutil::write_file_returning_metadata(Path::new(path), contents, 0o644)
			.map_err(|error| io_error(&error))?;
		Ok(WriteOutcome {
			wrote: true,
			key: key.to_owned(),
			hash,
			stamp: written_meta.as_ref().and_then(FileStamp::of),
		})
	}

	/// Merges a batch of [`WriteOutcome`]s into `current`, `stamps`, and `confirmed_this_build`.
	pub(crate) fn apply_write_outcomes<I>(&self, outcomes: I)
	where
		I: IntoIterator<Item = WriteOutcome>,
	{
		let outcomes: Vec<WriteOutcome> = outcomes.into_iter().collect();
		if outcomes.is_empty() {
			return;
		}
		{
			let mut confirmed = lock(&self.confirmed_this_build);
			for outcome in &outcomes {
				confirmed.insert(outcome.key.clone());
			}
		}
		if let Some(current) = &self.current {
			let mut current = lock(current);
			for outcome in &outcomes {
				current.insert(outcome.key.clone(), outcome.hash.clone());
			}
		}
		self.stamps.observe_batch(
			outcomes
				.into_iter()
				.filter_map(|outcome| outcome.stamp.map(|stamp| (outcome.key, stamp, outcome.hash))),
		);
	}

	/// Keys in `keys` not already written or confirmed present during this build.
	pub(crate) fn unconfirmed_keys<'a, I>(&self, keys: I) -> Vec<&'a String>
	where
		I: IntoIterator<Item = &'a String>,
	{
		let confirmed = lock(&self.confirmed_this_build);
		keys.into_iter().filter(|key| !confirmed.contains(*key)).collect()
	}

	/// Whether `key` was written or confirmed present during this build.
	pub(crate) fn is_confirmed(&self, key: &str) -> bool {
		lock(&self.confirmed_this_build).contains(key)
	}

	/// `lstat`: refuses paths outside the project directory.
	#[cfg(test)]
	pub(crate) fn lstat(&self, path: &str) -> Result<fs::Metadata, Error> {
		let key = self.output_key(path);
		if !gopath::is_local(&gopath::from_slash(&key)) {
			return Err(Error::message(format!(
				"compile: refusing to inspect output outside the project directory: {}",
				gostr::quote(path)
			)));
		}
		self.check_confined_parent(path)?;
		fs::symlink_metadata(path).map_err(|error| io_error(&error))
	}

	/// The `os.Root` confinement of `path`'s parent directory (`rootForKey`/`dirRoot`): resolved
	/// once per directory and cached, so a write costs one lookup rather than a realpath walk. A
	/// parent that does not exist yet resolves through its nearest existing ancestor, uncached.
	fn check_confined_parent(&self, path: &str) -> Result<(), Error> {
		let Some(root) = &self.canonical_root else {
			return Ok(());
		};
		let key = self.output_key(path);
		let relative = gopath::dir(&gopath::from_slash(&key));
		if relative == "." {
			return Ok(());
		}
		let cache_key = if self.case_sensitive {
			relative
		} else {
			relative.to_lowercase()
		};
		if lock(&self.dir_roots).contains_key(&cache_key) {
			return Ok(());
		}
		let parent = Path::new(path).parent().unwrap_or_else(|| Path::new(path));
		let (resolved, exists) = fs::canonicalize(parent).map_or_else(
			|_| (nearest_canonical(parent), false),
			|canonical| (Some(canonical), true),
		);
		let Some(resolved) = resolved else {
			return Ok(());
		};
		if !within_root(&resolved, root) {
			// `rootForKey` falls back to the project root with the whole key.
			return Err(Error::message(format!("openat {key}: path escapes from parent")));
		}
		if exists {
			lock(&self.dir_roots).insert(cache_key, resolved);
		}
		Ok(())
	}

	/// The final component of `path`, which a write follows: a symlink must resolve inside the
	/// project. Only the final component is inspected (its parent is
	/// [`OutputWriter::check_confined_parent`]'s).
	fn check_confined_final_symlink(&self, path: &str) -> Result<(), Error> {
		let Some(root) = &self.canonical_root else {
			return Ok(());
		};
		let path = Path::new(path);
		// A dangling symlink resolves through its target, which a write creates.
		let resolved = fs::canonicalize(path).ok().or_else(|| {
			fs::read_link(path)
				.ok()
				.and_then(|target| nearest_canonical(&path.parent().unwrap_or(path).join(target)))
		});
		match resolved {
			Some(resolved) if !within_root(&resolved, root) => {
				// The parent root opened, so Go names the base name.
				let name = path.file_name().map_or_default(|name| name.to_string_lossy());
				Err(Error::message(format!("openat {name}: path escapes from parent")))
			}
			_ => Ok(()),
		}
	}

	/// `prepare`: creates the parent directories of `paths`.
	pub(crate) fn prepare(&self, paths: &[String]) -> Result<(), Error> {
		for path in paths {
			self.prepare_parent(path)?;
		}
		Ok(())
	}

	/// `outputKey`: the project-relative slash path, empty before [`OutputWriter::use_hashes`].
	pub(crate) fn output_key(&self, path: &str) -> String {
		if self.project_dir.is_empty() {
			return String::new();
		}
		gopath::rel(&self.project_dir, &gopath::clean(path)).map_or_else(
			|_| gopath::to_slash(&gopath::clean(path)).into_owned(),
			|relative| gopath::to_slash(&relative).into_owned(),
		)
	}

	/// Writes the stamps of this build's outputs (`output_stamps`).
	pub(crate) fn persist_stamps(&self) {
		if self.project_dir.is_empty() {
			return;
		}
		self.stamps.persist(&self.project_dir, &self.current_outputs());
	}

	/// `hashSkipCount`
	pub(crate) fn hash_skip_count(&self) -> usize {
		self.hash_skips.load(Ordering::Relaxed)
	}

	/// `check_confined_parent` exposed for pre-confining parent directories before parallel work.
	pub(crate) fn confine_parent(&self, path: &str) -> Result<(), Error> {
		self.check_confined_parent(path)
	}

	/// `prepareParent`: creates the parent of `path` once per directory.
	fn prepare_parent(&self, path: &str) -> Result<(), Error> {
		if !self.project_dir.is_empty() {
			let local =
				gopath::rel(&self.project_dir, &gopath::clean(path)).is_ok_and(|relative| gopath::is_local(&relative));
			if !local {
				return Err(Error::message(format!(
					"compile: refusing to prepare output outside the project directory: {}",
					gostr::quote(path)
				)));
			}
		}
		let parent = gopath::abs(&gopath::dir(path)).map_err(|error| io_error(&error))?;
		let parent = gopath::clean(&parent);
		let key = if self.case_sensitive {
			parent.clone()
		} else {
			parent.to_lowercase()
		};
		if lock(&self.prepared).contains(&key) {
			return Ok(());
		}
		// Checked before creating, so a symlinked ancestor never gets directories made outside.
		let probe = gopath::join(&[&parent, "_"]);
		self.check_confined_parent(&probe)?;
		fsutil::mkdir_all(&parent, 0o755).map_err(|error| io_error(&error))?;
		// `cacheDirRoot`: the created directory is confined once more and cached.
		self.check_confined_parent(&probe)?;
		lock(&self.prepared).insert(key);
		Ok(())
	}
}

/// Whether the canonical `resolved` is `root` or inside it.
fn within_root(resolved: &Path, root: &str) -> bool {
	let resolved = resolved.to_string_lossy();
	resolved == root || crate::project::is_path_descendant_of(&resolved, root)
}

/// The canonical form of `path`, or of its nearest existing ancestor joined with the rest.
fn nearest_canonical(path: &Path) -> Option<PathBuf> {
	let mut rest: Vec<&std::ffi::OsStr> = Vec::new();
	let mut probe = path;
	loop {
		if let Ok(canonical) = fs::canonicalize(probe) {
			return Some(rest.iter().rev().fold(canonical, |joined, name| joined.join(name)));
		}
		rest.push(probe.file_name()?);
		probe = probe.parent()?;
	}
}
