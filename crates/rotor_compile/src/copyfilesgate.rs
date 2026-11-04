//! `copyfilesgate.go`: skips the output cleanup and the non-compiled file copy when nothing they
//! read changed since the last build, tracked in `<outDir>/rbxts.copyfiles.json`.

use std::collections::HashSet;
use std::fs;
use std::time::UNIX_EPOCH;

use rotor_rojo::PathTranslator;
use rotor_support::{fsutil, gopath};
use rotor_tsrs::P;
use rotor_tsrs::ast::SourceFile;
use rotor_tsrs::compiler::Program;

use crate::compile::Error;
use crate::gojson::{self, Value};
use crate::incremental_manifest::normalize_source_file_path;
use crate::output::is_compilable_file;

/// `copyFilesManifestVersion`
const COPY_FILES_MANIFEST_VERSION: i64 = 2;
/// `copyFilesCacheFilename`
pub(crate) const COPY_FILES_CACHE_FILENAME: &str = "rbxts.copyfiles.json";
/// `copyFilesResolverCacheFilename`
pub(crate) const COPY_FILES_RESOLVER_CACHE_FILENAME: &str = "rbxts.rojocache.json";
/// `copyFilesTypeRootsCacheFilename`
pub(crate) const COPY_FILES_TYPE_ROOTS_CACHE_FILENAME: &str = "rbxts.typeroots.rojocache.json";

/// `protectedOutputFilenames`: cleanup never removes these.
pub(crate) fn is_protected_output_filename(name: &str) -> bool {
	matches!(
		name,
		COPY_FILES_CACHE_FILENAME | COPY_FILES_RESOLVER_CACHE_FILENAME | COPY_FILES_TYPE_ROOTS_CACHE_FILENAME
	)
}

/// The compiler version the copy-files manifest and the Rojo resolver cache record. Go records
/// tsgo's `core.Version()`; sloptor records `sloptor <version>`, so a cache Go wrote is never
/// taken as current and the first sloptor build after a Go build copies and resolves in full.
pub(crate) fn compiler_version() -> String {
	format!("sloptor {}", rotor_support::version::VERSION)
}

/// `CopyFilesFileEntry`: written as `[src, dest, srcMtimeMs, destMtimeMs]`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CopyFilesFileEntry {
	/// The source file.
	pub src: String,
	/// The copy.
	pub dest: String,
	/// The source's mtime in milliseconds.
	pub src_mtime_ms: i64,
	/// The copy's mtime in milliseconds; 0 when missing.
	pub dest_mtime_ms: i64,
}

/// `CopyFilesDirEntry`: written as `[path, mtimeMs]`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CopyFilesDirEntry {
	/// The directory.
	pub path: String,
	/// Its mtime in milliseconds.
	pub mtime_ms: i64,
}

/// `CopyFilesManifest`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CopyFilesManifest {
	/// The format version.
	pub version: i64,
	/// The compiler that wrote it.
	pub compiler_version: String,
	/// `outDir|rootDirs`.
	pub key: String,
	/// Whether declarations are copied.
	pub declaration: bool,
	/// The copied files.
	pub files: Vec<CopyFilesFileEntry>,
	/// The walked directories.
	pub dirs: Vec<CopyFilesDirEntry>,
}

impl CopyFilesManifest {
	/// Go's `json.Marshal(manifest)`.
	pub(crate) fn to_json(&self) -> String {
		let files = gojson::array(self.files.iter().map(|entry| {
			gojson::array([
				gojson::string(&entry.src),
				gojson::string(&entry.dest),
				entry.src_mtime_ms.to_string(),
				entry.dest_mtime_ms.to_string(),
			])
		}));
		let dirs = gojson::array(
			self.dirs
				.iter()
				.map(|entry| gojson::array([gojson::string(&entry.path), entry.mtime_ms.to_string()])),
		);
		gojson::Object::new()
			.int("version", self.version)
			.string("compilerVersion", &self.compiler_version)
			.string("key", &self.key)
			.bool("declaration", self.declaration)
			.raw("files", &files)
			.raw("dirs", &dirs)
			.finish()
	}

	/// Go's `json.Unmarshal`; `None` for anything that does not decode.
	pub(crate) fn from_json(data: &[u8]) -> Option<(Self, bool, bool)> {
		let value = gojson::parse(data).ok()?;
		let int = |value: &Value| match value {
			Value::Number(text) => text.parse::<i64>().ok(),
			_ => None,
		};
		let string = |value: &Value| value.as_str().map(str::to_owned);
		let mut manifest = Self {
			version: value.field("version").map_or(Some(0), int)?,
			compiler_version: value.field("compilerVersion").map_or(Some(String::new()), string)?,
			key: value.field("key").map_or(Some(String::new()), string)?,
			declaration: match value.field("declaration") {
				None | Some(Value::Null) => false,
				Some(Value::Bool(flag)) => *flag,
				Some(_) => return None,
			},
			..Self::default()
		};
		let files = match value.field("files") {
			None | Some(Value::Null) => None,
			Some(files) => Some(files.as_array()?),
		};
		let dirs = match value.field("dirs") {
			None | Some(Value::Null) => None,
			Some(dirs) => Some(dirs.as_array()?),
		};
		for entry in files.unwrap_or_default() {
			let [src, dest, src_mtime, dest_mtime] = entry.as_array()? else {
				return None;
			};
			manifest.files.push(CopyFilesFileEntry {
				src: string(src)?,
				dest: string(dest)?,
				src_mtime_ms: int(src_mtime)?,
				dest_mtime_ms: int(dest_mtime)?,
			});
		}
		for entry in dirs.unwrap_or_default() {
			let [path, mtime] = entry.as_array()? else {
				return None;
			};
			manifest.dirs.push(CopyFilesDirEntry {
				path: string(path)?,
				mtime_ms: int(mtime)?,
			});
		}
		Some((manifest, files.is_some(), dirs.is_some()))
	}
}

/// `BuildStateSnapshot`: what the build considers changed.
#[derive(Clone, Debug, Default)]
pub struct BuildStateSnapshot {
	/// The changed files, normalized.
	pub changed_file_paths: HashSet<String>,
}

/// A path's mtime in Unix milliseconds (`ModTime().UnixMilli()`).
pub(crate) fn mtime_ms(metadata: &fs::Metadata) -> i64 {
	metadata
		.modified()
		.ok()
		.map_or(0, |modified| match modified.duration_since(UNIX_EPOCH) {
			Ok(after) => i64::try_from(after.as_millis()).unwrap_or(i64::MAX),
			Err(before) => -i64::try_from(before.duration().as_millis()).unwrap_or(i64::MAX),
		})
}

/// The directory walk of `buildCopyFilesManifest`.
fn walk(
	dir: &str,
	manifest: &mut CopyFilesManifest,
	normalized_out_dir: &str,
	declaration: bool,
	path_translator: &PathTranslator,
) {
	let Ok(info) = fs::metadata(dir) else {
		return;
	};
	if !info.is_dir() {
		return;
	}
	manifest.dirs.push(CopyFilesDirEntry {
		path: dir.to_owned(),
		mtime_ms: mtime_ms(&info),
	});
	let Ok(entries) = crate::packageexports::read_dir_sorted(dir) else {
		return;
	};
	for name in entries {
		if name == "node_modules" {
			continue;
		}
		let item_path = gopath::join(&[dir, &name]);
		if gopath::clean(&item_path) == normalized_out_dir {
			continue;
		}
		let Ok(info) = fs::metadata(&item_path) else {
			continue;
		};
		if info.is_dir() {
			walk(&item_path, manifest, normalized_out_dir, declaration, path_translator);
			continue;
		}
		if !info.is_file() {
			continue;
		}
		if item_path.ends_with(".d.ts") {
			if !declaration {
				continue;
			}
		} else if is_compilable_file(&item_path) {
			continue;
		}
		let dest = path_translator.get_output_path(&item_path);
		let dest_mtime = fs::metadata(&dest).map_or(0, |dest_info| mtime_ms(&dest_info));
		manifest.files.push(CopyFilesFileEntry {
			src: item_path,
			dest,
			src_mtime_ms: mtime_ms(&info),
			dest_mtime_ms: dest_mtime,
		});
	}
}

/// `buildCopyFilesManifest`: walks the root dirs for the files a build copies.
pub(crate) fn build_copy_files_manifest(
	root_dirs: &[String],
	out_dir: &str,
	declaration: bool,
	key: &str,
	path_translator: &PathTranslator,
) -> CopyFilesManifest {
	let mut manifest = CopyFilesManifest {
		version: COPY_FILES_MANIFEST_VERSION,
		compiler_version: compiler_version(),
		key: key.to_owned(),
		declaration,
		..CopyFilesManifest::default()
	};
	let normalized_out_dir = gopath::clean(out_dir);
	for root_dir in root_dirs {
		walk(
			&gopath::clean(root_dir),
			&mut manifest,
			&normalized_out_dir,
			declaration,
			path_translator,
		);
	}
	manifest
}

/// `writeCopyFilesManifest`: atomic, and skipped when the file already holds the bytes.
fn write_copy_files_manifest(path: &str, manifest: &CopyFilesManifest) -> Result<(), Error> {
	let data = manifest.to_json();
	if fsutil::same_contents(path, data.as_bytes()) {
		return Ok(());
	}
	let dir = gopath::dir(path);
	fsutil::mkdir_all(&dir, 0o755).map_err(|error| {
		Error::message(format!(
			"create copy-files manifest directory: {}",
			rotor_support::gostr::path_error("mkdir", &dir, &error)
		))
	})?;
	let mut temp = tempfile::Builder::new()
		.prefix(".copyfiles-")
		.suffix(".tmp")
		.tempfile_in(&dir)
		.map_err(|error| Error::message(format!("create copy-files manifest temporary file: {error}")))?;
	std::io::Write::write_all(&mut temp, data.as_bytes())
		.map_err(|error| Error::message(format!("write copy-files manifest: {error}")))?;
	// Go chmods the temp file (created 0600) to 0644 before the rename.
	fsutil::chmod(temp.as_file(), 0o644)
		.map_err(|error| Error::message(format!("set copy-files manifest permissions: {error}")))?;
	temp.persist(path)
		.map_err(|error| Error::message(format!("replace copy-files manifest: {}", error.error)))?;
	Ok(())
}

/// `tryLoadCopyFilesManifest`: the manifest when it matches this compiler, key and option.
fn try_load_copy_files_manifest(path: &str, key: &str, declaration: bool) -> Option<CopyFilesManifest> {
	let data = fs::read(path).ok()?;
	let (manifest, has_files, has_dirs) = CopyFilesManifest::from_json(&data)?;
	(manifest.version == COPY_FILES_MANIFEST_VERSION
		&& manifest.compiler_version == compiler_version()
		&& manifest.key == key
		&& manifest.declaration == declaration
		&& has_files
		&& has_dirs)
		.then_some(manifest)
}

/// `copyFilesGateInputs`
pub(crate) struct CopyFilesGateInputs<'a> {
	pub(crate) root_dirs: Vec<String>,
	pub(crate) out_dir: String,
	pub(crate) declaration: bool,
	pub(crate) path_translator: &'a PathTranslator,
	pub(crate) snapshot: BuildStateSnapshot,
}

/// `copyFilesGateOutputs`
#[derive(Default)]
pub(crate) struct CopyFilesGateOutputs {
	pub(crate) skip_cleanup: bool,
	pub(crate) skip_copy_files: bool,
	pub(crate) owned_outputs: Vec<String>,
	/// Writes the refreshed manifest after the build.
	pub(crate) persist: Option<(String, CopyFilesManifest)>,
}

impl CopyFilesGateOutputs {
	/// The gate's `Persist`: refreshes the copies' mtimes and writes the manifest.
	pub(crate) fn persist(&self) -> Result<(), Error> {
		let Some((cache_path, current)) = &self.persist else {
			return Ok(());
		};
		let mut current = current.clone();
		for entry in &mut current.files {
			entry.dest_mtime_ms = fs::metadata(&entry.dest).map_or(0, |info| mtime_ms(&info));
		}
		write_copy_files_manifest(cache_path, &current)
	}
}

/// `buildCopyFilesManifestKey`
pub(crate) fn build_copy_files_manifest_key(out_dir: &str, root_dirs: &[String]) -> String {
	let mut normalized: Vec<String> = root_dirs.iter().map(|root_dir| gopath::clean(root_dir)).collect();
	normalized.sort();
	format!("{}|{}", gopath::clean(out_dir), normalized.join(","))
}

/// `copyFilesDirsEqual`
fn copy_files_dirs_equal(a: &[CopyFilesDirEntry], b: &[CopyFilesDirEntry]) -> bool {
	if a.len() != b.len() {
		return false;
	}
	let lookup: std::collections::HashMap<&str, i64> =
		a.iter().map(|entry| (entry.path.as_str(), entry.mtime_ms)).collect();
	b.iter()
		.all(|entry| lookup.get(entry.path.as_str()).copied().unwrap_or(0) == entry.mtime_ms)
}

/// `copyFilesFilesEqual`
fn copy_files_files_equal(a: &[CopyFilesFileEntry], b: &[CopyFilesFileEntry]) -> bool {
	if a.len() != b.len() {
		return false;
	}
	let lookup: std::collections::HashMap<&str, i64> =
		a.iter().map(|entry| (entry.src.as_str(), entry.src_mtime_ms)).collect();
	b.iter()
		.all(|entry| lookup.get(entry.src.as_str()).copied().unwrap_or(0) == entry.src_mtime_ms)
}

/// `copyFilesDestsValid`
fn copy_files_dests_valid(files: &[CopyFilesFileEntry]) -> bool {
	files
		.iter()
		.all(|entry| fs::metadata(&entry.dest).is_ok_and(|info| mtime_ms(&info) == entry.dest_mtime_ms))
}

/// `copyFilesFastValidate`
fn copy_files_fast_validate(manifest: &CopyFilesManifest, snapshot: &BuildStateSnapshot) -> bool {
	if !snapshot.changed_file_paths.is_empty() {
		return false;
	}
	manifest
		.dirs
		.iter()
		.all(|entry| fs::metadata(&entry.path).is_ok_and(|info| mtime_ms(&info) == entry.mtime_ms))
		&& manifest.files.iter().all(|entry| {
			fs::metadata(&entry.src).is_ok_and(|info| mtime_ms(&info) == entry.src_mtime_ms)
				&& fs::metadata(&entry.dest).is_ok_and(|info| mtime_ms(&info) == entry.dest_mtime_ms)
		})
}

/// `loadCopyFilesGatePreBuild`
pub(crate) fn load_copy_files_gate_pre_build(inputs: &CopyFilesGateInputs<'_>) -> CopyFilesGateOutputs {
	let cache_path = gopath::join(&[&inputs.out_dir, COPY_FILES_CACHE_FILENAME]);
	let key = build_copy_files_manifest_key(&inputs.out_dir, &inputs.root_dirs);
	let cached = try_load_copy_files_manifest(&cache_path, &key, inputs.declaration);
	if let Some(cached) = &cached
		&& copy_files_fast_validate(cached, &inputs.snapshot)
	{
		return CopyFilesGateOutputs {
			skip_cleanup: true,
			skip_copy_files: true,
			owned_outputs: copy_files_destinations(&cached.files),
			persist: None,
		};
	}
	let current = build_copy_files_manifest(
		&inputs.root_dirs,
		&inputs.out_dir,
		inputs.declaration,
		&key,
		inputs.path_translator,
	);
	let owned_outputs = copy_files_destinations(&current.files);
	let Some(cached) = cached else {
		return CopyFilesGateOutputs {
			owned_outputs,
			persist: Some((cache_path, current)),
			..CopyFilesGateOutputs::default()
		};
	};
	let dirs_match = copy_files_dirs_equal(&current.dirs, &cached.dirs);
	let files_match = copy_files_files_equal(&current.files, &cached.files);
	let dests_valid = copy_files_dests_valid(&cached.files);
	CopyFilesGateOutputs {
		skip_cleanup: dirs_match && files_match && dests_valid && inputs.snapshot.changed_file_paths.is_empty(),
		skip_copy_files: files_match && dests_valid,
		owned_outputs,
		persist: Some((cache_path, current)),
	}
}

/// `copyFilesDestinations`
fn copy_files_destinations(files: &[CopyFilesFileEntry]) -> Vec<String> {
	files.iter().map(|file| file.dest.clone()).collect()
}

/// `copyFilesChangedSnapshot`: a non-incremental build reports the whole program, so cleanup is
/// only skipped when incremental state proves nothing changed.
pub(crate) fn copy_files_changed_snapshot(program: &Program, selected_files: &[P<SourceFile>]) -> BuildStateSnapshot {
	let all;
	let changed_files: &[P<SourceFile>] = if program.options().is_incremental() {
		selected_files
	} else {
		all = program.source_files().to_vec();
		&all
	};
	BuildStateSnapshot {
		changed_file_paths: changed_files
			.iter()
			.map(|file| normalize_source_file_path(file.file_name()))
			.collect(),
	}
}
