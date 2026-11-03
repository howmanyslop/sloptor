//! A two-level cache of parsed resolver states: in memory and on disk.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use rotor_support::gopath;
use sha2::{Digest, Sha256};

use crate::json::{self, JsonValue};
use crate::prefetch::read_dir;
use crate::resolver::{
	PartitionInfo, ResolverFilePathState, ResolverState, RojoResolver, entry_kind, is_rojo_project_file,
};

const RESOLVER_CACHE_FORMAT_VERSION: i64 = 4;

/// One path the resolver read. `missing` marks a partition path that did not
/// exist; creating it changes the resolver (a new directory is walked), so the
/// entry stays valid only while it is absent. `listing` is a walked
/// directory's entries that the resolver reads, so a new mtime from writing
/// other files (such as build outputs) keeps the entry. `sha256` is a project
/// file's contents: package managers extract tarballs with fixed mtimes, so an
/// upgraded package's project file can keep its mtime.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ManifestEntry {
	path: String,
	mtime_unix_nano: i64,
	missing: bool,
	listing: String,
	sha256: String,
}

struct CacheFile {
	version: i64,
	compiler_version: String,
	key: String,
	state: ResolverState,
	mtime_manifest: Vec<ManifestEntry>,
}

#[derive(Clone)]
struct CacheEntry {
	resolver: Arc<RojoResolver>,
	manifest: Vec<ManifestEntry>,
}

#[derive(Default)]
struct Entries {
	l1: HashMap<String, CacheEntry>,
	pending: HashMap<String, CacheEntry>,
}

/// Keeps parsed resolver states in memory and on disk.
pub struct RojoResolverCache {
	cache_dir: String,
	compiler_version: String,
	defer_persist: bool,
	entries: Mutex<Entries>,
}

impl RojoResolverCache {
	/// A cache whose disk entries are stored in `cache_dir`.
	#[must_use]
	pub fn new(cache_dir: &str, compiler_version: &str) -> Self {
		Self::with_deferred_persist(cache_dir, compiler_version, false)
	}

	/// A cache that never reads or writes disk.
	#[must_use]
	pub fn memory() -> Self {
		Self::with_deferred_persist("", "", false)
	}

	/// A cache that, when `defer_persist` is set, writes disk entries only
	/// when [`Self::persist`] is called and never deletes stale ones.
	#[must_use]
	pub fn with_deferred_persist(cache_dir: &str, compiler_version: &str, defer_persist: bool) -> Self {
		Self {
			cache_dir: cache_dir.to_owned(),
			compiler_version: compiler_version.to_owned(),
			defer_persist,
			entries: Mutex::new(Entries::default()),
		}
	}

	/// A resolver for `rojo_config_file_path`, preferring a valid in-memory
	/// entry, then a valid disk entry.
	#[must_use]
	#[expect(
		clippy::significant_drop_tightening,
		reason = "the lock covers the whole load, so concurrent loads of one key build the resolver once"
	)]
	pub fn load(&self, rojo_config_file_path: &str) -> Arc<RojoResolver> {
		let key = cache_key(rojo_config_file_path);
		let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
		if let Some(entry) = entries.l1.get_mut(&key)
			&& manifest_still_valid(&mut entry.manifest)
		{
			return Arc::clone(&entry.resolver);
		}
		entries.l1.remove(&key);

		if let Some((resolver, manifest)) = self.load_disk(&key) {
			entries.l1.insert(
				key,
				CacheEntry {
					resolver: Arc::clone(&resolver),
					manifest,
				},
			);
			return resolver;
		}

		let resolver = Arc::new(RojoResolver::from_path(&key));
		let state = resolver.state();
		let Some(manifest) = resolver_mtime_manifest(&state) else {
			return resolver;
		};
		let entry = CacheEntry {
			resolver: Arc::clone(&resolver),
			manifest,
		};
		entries.l1.insert(key.clone(), entry.clone());
		if self.defer_persist {
			entries.pending.insert(key, entry);
		} else {
			self.write_disk(&key, &state, &entry.manifest);
		}
		resolver
	}

	/// Writes every entry whose write was deferred.
	pub fn persist(&self) {
		let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
		for (key, entry) in &entries.pending {
			self.write_disk(key, &entry.resolver.state(), &entry.manifest);
		}
		entries.pending.clear();
	}

	fn load_disk(&self, key: &str) -> Option<(Arc<RojoResolver>, Vec<ManifestEntry>)> {
		if self.cache_dir.is_empty() {
			return None;
		}
		let path = self.cache_path(key);
		let data = fs::read(&path).ok()?;
		let discard = || {
			if !self.defer_persist {
				let _ = fs::remove_file(&path);
			}
		};

		let Some(file) = parse_cache_file(&data) else {
			discard();
			return None;
		};
		if file.version != RESOLVER_CACHE_FORMAT_VERSION
			|| file.compiler_version != self.compiler_version
			|| file.key != key
			|| !state_matches_manifest(&file.state, &file.mtime_manifest)
		{
			discard();
			return None;
		}
		let mut manifest = file.mtime_manifest;
		if !manifest_still_valid(&mut manifest) {
			return None;
		}
		let Ok(resolver) = RojoResolver::from_state(&file.state) else {
			discard();
			return None;
		};
		Some((Arc::new(resolver), manifest))
	}

	fn write_disk(&self, key: &str, state: &ResolverState, manifest: &[ManifestEntry]) {
		if self.cache_dir.is_empty() {
			return;
		}
		let mut data = cache_file_json(&self.compiler_version, key, state, manifest).to_indented_string();
		data.push('\n');
		let _ = write_file_atomic(&self.cache_path(key), data.as_bytes());
	}

	fn cache_path(&self, key: &str) -> String {
		gopath::join(&[
			&self.cache_dir,
			&format!("{}.rojocache.json", hex(&Sha256::digest(key.as_bytes()))),
		])
	}
}

fn hex(bytes: &[u8]) -> String {
	use std::fmt::Write;
	bytes
		.iter()
		.fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
			let _ = write!(out, "{byte:02x}");
			out
		})
}

fn cache_key(path: &str) -> String {
	gopath::abs(path).map_or_else(|_| gopath::clean(path), |abs| gopath::clean(&abs))
}

/// `info.ModTime().UnixNano()`.
fn mtime_unix_nano(info: &fs::Metadata) -> i64 {
	let Ok(modified) = info.modified() else {
		return 0;
	};
	match modified.duration_since(UNIX_EPOCH) {
		Ok(after) => i64::try_from(after.as_nanos()).unwrap_or(i64::MAX),
		Err(before) => i64::try_from(before.duration().as_nanos()).map_or(i64::MIN, |nanos| -nanos),
	}
}

fn resolver_mtime_manifest(state: &ResolverState) -> Option<Vec<ManifestEntry>> {
	let mut paths = state.walked_configs.clone();
	paths.extend(state.walked_directories.iter().cloned());
	paths.sort();
	paths.dedup();
	if paths.is_empty() {
		return None;
	}
	let mut manifest = Vec::with_capacity(paths.len());
	for path in paths {
		let info = fs::metadata(&path).ok()?;
		let mut entry = ManifestEntry {
			path,
			mtime_unix_nano: mtime_unix_nano(&info),
			missing: false,
			listing: String::new(),
			sha256: String::new(),
		};
		if info.is_dir() {
			entry.listing = resolver_directory_listing(&entry.path)?;
		} else {
			entry.sha256 = resolver_file_hash(&entry.path)?;
		}
		manifest.push(entry);
	}
	for partition in &state.partitions {
		if matches!(fs::metadata(&partition.fs_path), Err(err) if err.kind() == io::ErrorKind::NotFound) {
			manifest.push(ManifestEntry {
				path: partition.fs_path.clone(),
				mtime_unix_nano: 0,
				missing: true,
				listing: String::new(),
				sha256: String::new(),
			});
		}
	}
	Some(manifest)
}

/// Reports whether every entry still holds. A directory whose mtime changed
/// but whose listing did not gets its new mtime recorded.
fn manifest_still_valid(manifest: &mut [ManifestEntry]) -> bool {
	if manifest.is_empty() {
		return false;
	}
	for entry in manifest {
		let info = fs::metadata(&entry.path);
		if entry.missing {
			if !matches!(info, Err(err) if err.kind() == io::ErrorKind::NotFound) {
				return false;
			}
			continue;
		}
		let Ok(info) = info else {
			return false;
		};
		let mtime = mtime_unix_nano(&info);
		if mtime != entry.mtime_unix_nano {
			if entry.listing.is_empty() || !info.is_dir() {
				return false;
			}
			if resolver_directory_listing(&entry.path).is_none_or(|listing| listing != entry.listing) {
				return false;
			}
			entry.mtime_unix_nano = mtime;
		}
		if !info.is_dir() && resolver_file_hash(&entry.path).is_none_or(|hash| hash != entry.sha256) {
			return false;
		}
	}
	true
}

fn resolver_file_hash(path: &str) -> Option<String> {
	fs::read(path).ok().map(|data| hex(&Sha256::digest(&data)))
}

/// Lists what parsing reads from a directory: its subdirectories and its Rojo
/// project files.
fn resolver_directory_listing(dir: &str) -> Option<String> {
	let entries = read_dir(dir).ok()?;
	let mut listing = String::from("v");
	for entry in &entries {
		let (is_dir, is_regular) = entry_kind(dir, entry);
		if is_dir {
			listing.push_str("/d:");
			listing.push_str(&entry.name);
		} else if is_regular && is_rojo_project_file(&entry.name) {
			listing.push_str("/f:");
			listing.push_str(&entry.name);
		}
	}
	Some(listing)
}

fn state_matches_manifest(state: &ResolverState, manifest: &[ManifestEntry]) -> bool {
	resolver_mtime_manifest(state).is_some_and(|expected| expected == manifest)
}

/// Writes `data` to a temporary file beside `path` and renames it into place,
/// so a reader never sees a partial file.
fn write_file_atomic(path: &str, data: &[u8]) -> io::Result<()> {
	static COUNTER: AtomicU64 = AtomicU64::new(0);
	let dir = gopath::dir(path);
	fs::create_dir_all(&dir)?;
	let nanos = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map_or(0, |elapsed| elapsed.subsec_nanos());
	let (mut file, tmp_name) = loop {
		let unique = format!(
			"{}-{nanos}-{}",
			std::process::id(),
			COUNTER.fetch_add(1, Ordering::Relaxed)
		);
		let tmp_name = gopath::join(&[&dir, &format!(".rojo-cache-{unique}.tmp")]);
		let mut options = fs::OpenOptions::new();
		options.write(true).create_new(true);
		#[cfg(unix)]
		{
			use std::os::unix::fs::OpenOptionsExt;
			options.mode(0o600);
		}
		match options.open(&tmp_name) {
			Ok(file) => break (file, tmp_name),
			Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
			Err(err) => return Err(err),
		}
	};
	let written = file.write_all(data).and_then(|()| file.flush());
	drop(file);
	if let Err(err) = written.and_then(|()| fs::rename(&tmp_name, path)) {
		let _ = fs::remove_file(&tmp_name);
		return Err(err);
	}
	Ok(())
}

fn string_array(strings: &[String]) -> JsonValue {
	JsonValue::Array(strings.iter().cloned().map(JsonValue::String).collect())
}

fn cache_file_json(compiler_version: &str, key: &str, state: &ResolverState, manifest: &[ManifestEntry]) -> JsonValue {
	let partitions = state
		.partitions
		.iter()
		.map(|partition| {
			JsonValue::Object(vec![
				("RbxPath".to_owned(), string_array(&partition.rbx_path)),
				("FsPath".to_owned(), JsonValue::String(partition.fs_path.clone())),
			])
		})
		.collect();
	let file_paths = state
		.file_paths
		.iter()
		.map(|file_path| {
			JsonValue::Object(vec![
				("path".to_owned(), JsonValue::String(file_path.path.clone())),
				("rbxPath".to_owned(), string_array(&file_path.rbx_path)),
			])
		})
		.collect();
	let state_json = JsonValue::Object(vec![
		("warnings".to_owned(), string_array(&state.warnings)),
		("rbxPath".to_owned(), string_array(&state.rbx_path)),
		("partitions".to_owned(), JsonValue::Array(partitions)),
		("filePaths".to_owned(), JsonValue::Array(file_paths)),
		(
			"isolatedContainers".to_owned(),
			JsonValue::Array(state.isolated_containers.iter().map(|c| string_array(c)).collect()),
		),
		("isGame".to_owned(), JsonValue::Bool(state.is_game)),
		("walkedConfigs".to_owned(), string_array(&state.walked_configs)),
		("walkedDirectories".to_owned(), string_array(&state.walked_directories)),
	]);
	let manifest_json = manifest
		.iter()
		.map(|entry| {
			let mut members = vec![
				("path".to_owned(), JsonValue::String(entry.path.clone())),
				("mtimeUnixNano".to_owned(), JsonValue::integer(entry.mtime_unix_nano)),
			];
			if entry.missing {
				members.push(("missing".to_owned(), JsonValue::Bool(true)));
			}
			if !entry.listing.is_empty() {
				members.push(("listing".to_owned(), JsonValue::String(entry.listing.clone())));
			}
			if !entry.sha256.is_empty() {
				members.push(("sha256".to_owned(), JsonValue::String(entry.sha256.clone())));
			}
			JsonValue::Object(members)
		})
		.collect();
	JsonValue::Object(vec![
		("version".to_owned(), JsonValue::integer(RESOLVER_CACHE_FORMAT_VERSION)),
		(
			"compilerVersion".to_owned(),
			JsonValue::String(compiler_version.to_owned()),
		),
		("key".to_owned(), JsonValue::String(key.to_owned())),
		("state".to_owned(), state_json),
		("mtimeManifest".to_owned(), JsonValue::Array(manifest_json)),
	])
}

/// Reads a cache file the way `json.Unmarshal` into the Go struct does:
/// missing and `null` fields are zero values, and a field of the wrong type is
/// an error.
fn parse_cache_file(data: &[u8]) -> Option<CacheFile> {
	let root = json::parse(data);
	let JsonValue::Object(_) = root else {
		return None;
	};
	Some(CacheFile {
		version: optional(root.member("version"), JsonValue::as_i64)?,
		compiler_version: optional(root.member("compilerVersion"), |v| v.as_str().map(str::to_owned))?,
		key: optional(root.member("key"), |v| v.as_str().map(str::to_owned))?,
		state: optional(root.member("state"), state_from_json)?,
		mtime_manifest: optional_list(root.member("mtimeManifest"), manifest_entry_from_json)?,
	})
}

/// Applies `convert` to a present, non-`null` value; absent and `null` are the
/// default. `None` means the value had the wrong type.
fn optional<T: Default>(value: Option<&JsonValue>, convert: impl FnOnce(&JsonValue) -> Option<T>) -> Option<T> {
	match value {
		None | Some(JsonValue::Null) => Some(T::default()),
		Some(value) => convert(value),
	}
}

fn optional_list<T>(value: Option<&JsonValue>, convert: impl Fn(&JsonValue) -> Option<T>) -> Option<Vec<T>> {
	optional(value, |v| v.as_array()?.iter().map(&convert).collect())
}

fn string_list(value: Option<&JsonValue>) -> Option<Vec<String>> {
	optional_list(value, |v| v.as_str().map(str::to_owned))
}

fn state_from_json(v: &JsonValue) -> Option<ResolverState> {
	let JsonValue::Object(_) = v else {
		return None;
	};
	Some(ResolverState {
		warnings: string_list(v.member("warnings"))?,
		rbx_path: string_list(v.member("rbxPath"))?,
		partitions: optional_list(v.member("partitions"), |p| {
			Some(PartitionInfo {
				rbx_path: string_list(p.member("RbxPath"))?,
				fs_path: optional(p.member("FsPath"), |s| s.as_str().map(str::to_owned))?,
			})
		})?,
		file_paths: optional_list(v.member("filePaths"), |p| {
			Some(ResolverFilePathState {
				path: optional(p.member("path"), |s| s.as_str().map(str::to_owned))?,
				rbx_path: string_list(p.member("rbxPath"))?,
			})
		})?,
		isolated_containers: optional_list(v.member("isolatedContainers"), |c| string_list(Some(c)))?,
		is_game: optional(v.member("isGame"), |b| match b {
			JsonValue::Bool(b) => Some(*b),
			_ => None,
		})?,
		walked_configs: string_list(v.member("walkedConfigs"))?,
		walked_directories: string_list(v.member("walkedDirectories"))?,
	})
}

fn manifest_entry_from_json(v: &JsonValue) -> Option<ManifestEntry> {
	let JsonValue::Object(_) = v else {
		return None;
	};
	Some(ManifestEntry {
		path: optional(v.member("path"), |s| s.as_str().map(str::to_owned))?,
		mtime_unix_nano: optional(v.member("mtimeUnixNano"), JsonValue::as_i64)?,
		missing: optional(v.member("missing"), |b| match b {
			JsonValue::Bool(b) => Some(*b),
			_ => None,
		})?,
		listing: optional(v.member("listing"), |s| s.as_str().map(str::to_owned))?,
		sha256: optional(v.member("sha256"), |s| s.as_str().map(str::to_owned))?,
	})
}

#[cfg(test)]
mod tests;
