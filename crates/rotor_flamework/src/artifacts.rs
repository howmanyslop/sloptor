//! `artifacts.go`: writing Flamework's artifacts as one transaction under the project root.
//!
//! Go confines every file operation to the project root with `os.Root`. [`ArtifactRoot`] is its
//! counterpart: on Unix it holds the root directory's descriptor and walks each path from it with
//! `openat(O_NOFOLLOW)`, so a moved root or a symlinked directory cannot redirect a write. Go's
//! `os.Root` follows symlinks that stay inside the root; this rejects every symlinked directory.
//! Elsewhere it joins paths onto the root.

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Mutex;

use rotor_support::gopath;
use rotor_support::gostr::quote;

use crate::artifacts_commit::{cleanup_staged_artifacts, commit_artifact, rollback_artifacts};
use crate::artifacts_stage::{StagedArtifact, stage_artifact};
use crate::build_info::BuildInfo;
use crate::json_syntax;
use crate::{Error, ErrorKind};

/// `Artifact`: a file to write, or to delete when `data` is `None` (Go's nil slice).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Artifact {
	/// The root-relative path.
	pub path: String,
	/// The contents; `None` deletes the file.
	pub data: Option<Vec<u8>>,
}

static ARTIFACT_PERSISTENCE: Mutex<()> = Mutex::new(());

/// The kind and identity of a file, Go's `fs.FileInfo` as `os.SameFile` compares it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ArtifactInfo {
	kind: FileKind,
	device: i128,
	inode: i128,
	permissions: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileKind {
	Regular,
	Directory,
	Other,
}

impl ArtifactInfo {
	pub(crate) const fn is_regular(&self) -> bool {
		matches!(self.kind, FileKind::Regular)
	}

	pub(crate) const fn is_dir(&self) -> bool {
		matches!(self.kind, FileKind::Directory)
	}

	pub(crate) const fn permissions(&self) -> u32 {
		self.permissions
	}

	pub(crate) const fn same_file(&self, other: &Self) -> bool {
		self.device == other.device && self.inode == other.inode
	}
}

thread_local! {
	static RENAME_HOOK: RefCell<Option<RenameHook>> = const { RefCell::new(None) };
}

/// A test seam replacing the rename that commits an artifact (Go's `renameArtifact` variable).
pub(crate) type RenameHook = Box<dyn FnMut(&ArtifactRoot, &str, &str) -> std::io::Result<()>>;

/// Installs `hook` for this thread's renames, returning the previous one.
#[cfg(test)]
pub(crate) fn set_rename_hook(hook: Option<RenameHook>) -> Option<RenameHook> {
	RENAME_HOOK.with(|slot| std::mem::replace(&mut *slot.borrow_mut(), hook))
}

/// `renameArtifact`
pub(crate) fn rename_artifact(root: &ArtifactRoot, old_path: &str, new_path: &str) -> std::io::Result<()> {
	let hook = RENAME_HOOK.with(|slot| slot.borrow_mut().take());
	let Some(mut hook) = hook else {
		return root.rename(old_path, new_path);
	};
	let result = hook(root, old_path, new_path);
	RENAME_HOOK.with(|slot| {
		let mut slot = slot.borrow_mut();
		if slot.is_none() {
			*slot = Some(hook);
		}
	});
	result
}

#[cfg(unix)]
mod root {
	use std::fs::File;
	use std::io::Read as _;
	use std::os::fd::OwnedFd;

	use rustix::fs::{AtFlags, CWD, FileType, Mode, OFlags};

	use super::{ArtifactInfo, FileKind};

	/// Go's `os.Root`.
	#[derive(Debug)]
	pub struct ArtifactRoot {
		directory: OwnedFd,
	}

	fn info(stat: &rustix::fs::Stat) -> ArtifactInfo {
		let kind = match FileType::from_raw_mode(stat.st_mode) {
			FileType::RegularFile => FileKind::Regular,
			FileType::Directory => FileKind::Directory,
			_ => FileKind::Other,
		};
		ArtifactInfo {
			kind,
			device: i128::from(stat.st_dev),
			inode: i128::from(stat.st_ino),
			permissions: permission_bits(Mode::from_raw_mode(stat.st_mode)),
		}
	}

	const PERMISSIONS: [(u32, Mode); 9] = [
		(0o400, Mode::RUSR),
		(0o200, Mode::WUSR),
		(0o100, Mode::XUSR),
		(0o40, Mode::RGRP),
		(0o20, Mode::WGRP),
		(0o10, Mode::XGRP),
		(0o4, Mode::ROTH),
		(0o2, Mode::WOTH),
		(0o1, Mode::XOTH),
	];

	fn mode_of(bits: u32) -> Mode {
		PERMISSIONS
			.iter()
			.filter(|(bit, _)| bits & bit != 0)
			.fold(Mode::empty(), |mode, (_, flag)| mode | *flag)
	}

	fn permission_bits(mode: Mode) -> u32 {
		PERMISSIONS
			.iter()
			.filter(|(_, flag)| mode.contains(*flag))
			.fold(0, |bits, (bit, _)| bits | bit)
	}

	fn components(path: &str) -> Vec<&str> {
		path.split('/')
			.filter(|part| !part.is_empty() && *part != ".")
			.collect()
	}

	impl ArtifactRoot {
		pub fn open(path: &str) -> std::io::Result<Self> {
			let directory = rustix::fs::openat(
				CWD,
				path,
				OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::RDONLY,
				Mode::empty(),
			)?;
			Ok(Self { directory })
		}

		/// The directory holding `path`'s last component, reached without following symlinks,
		/// and that component.
		fn parent<'p>(&self, path: &'p str) -> std::io::Result<(Option<OwnedFd>, &'p str)> {
			let parts = components(path);
			let Some((leaf, directories)) = parts.split_last() else {
				return Ok((None, "."));
			};
			let mut current: Option<OwnedFd> = None;
			for part in directories {
				let base = current.as_ref().map_or(&self.directory, |fd| fd);
				let next = rustix::fs::openat(
					base,
					*part,
					OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::RDONLY,
					Mode::empty(),
				)?;
				current = Some(next);
			}
			Ok((current, leaf))
		}

		fn with_parent<T>(
			&self,
			path: &str,
			run: impl FnOnce(&OwnedFd, &str) -> std::io::Result<T>,
		) -> std::io::Result<T> {
			let (parent, leaf) = self.parent(path)?;
			run(parent.as_ref().unwrap_or(&self.directory), leaf)
		}

		pub fn lstat(&self, path: &str) -> std::io::Result<ArtifactInfo> {
			self.with_parent(path, |parent, leaf| {
				Ok(info(&rustix::fs::statat(parent, leaf, AtFlags::SYMLINK_NOFOLLOW)?))
			})
		}

		pub fn stat(&self, path: &str) -> std::io::Result<ArtifactInfo> {
			self.with_parent(path, |parent, leaf| {
				Ok(info(&rustix::fs::statat(parent, leaf, AtFlags::empty())?))
			})
		}

		pub fn read_with_info(&self, path: &str) -> std::io::Result<(Vec<u8>, ArtifactInfo)> {
			self.with_parent(path, |parent, leaf| {
				let fd = rustix::fs::openat(
					parent,
					leaf,
					OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
					Mode::empty(),
				)?;
				let opened = info(&rustix::fs::fstat(&fd)?);
				let mut file = File::from(fd);
				let mut contents = Vec::new();
				file.read_to_end(&mut contents)?;
				Ok((contents, opened))
			})
		}

		pub fn remove(&self, path: &str) -> std::io::Result<()> {
			self.with_parent(path, |parent, leaf| {
				match rustix::fs::unlinkat(parent, leaf, AtFlags::empty()) {
					Ok(()) => Ok(()),
					Err(file_error) => rustix::fs::unlinkat(parent, leaf, AtFlags::REMOVEDIR)
						.map_err(|_| std::io::Error::from(file_error)),
				}
			})
		}

		pub fn mkdir_all(&self, path: &str, mode: u32) -> std::io::Result<()> {
			let mut current: Option<OwnedFd> = None;
			for part in components(path) {
				let base = current.as_ref().unwrap_or(&self.directory);
				match rustix::fs::mkdirat(base, part, mode_of(mode)) {
					Ok(()) | Err(rustix::io::Errno::EXIST) => {}
					Err(error) => return Err(error.into()),
				}
				let next = rustix::fs::openat(
					base,
					part,
					OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::RDONLY,
					Mode::empty(),
				)?;
				current = Some(next);
			}
			Ok(())
		}

		pub fn rename(&self, old_path: &str, new_path: &str) -> std::io::Result<()> {
			let (old_parent, old_leaf) = self.parent(old_path)?;
			let (new_parent, new_leaf) = self.parent(new_path)?;
			rustix::fs::renameat(
				old_parent.as_ref().unwrap_or(&self.directory),
				old_leaf,
				new_parent.as_ref().unwrap_or(&self.directory),
				new_leaf,
			)?;
			Ok(())
		}

		pub fn create_exclusive(&self, path: &str, mode: u32) -> std::io::Result<File> {
			self.with_parent(path, |parent, leaf| {
				let fd = rustix::fs::openat(
					parent,
					leaf,
					OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
					mode_of(mode),
				)?;
				Ok(File::from(fd))
			})
		}

		pub fn file_info(file: &File) -> std::io::Result<ArtifactInfo> {
			Ok(info(&rustix::fs::fstat(file)?))
		}

		pub fn chmod(file: &File, mode: u32) -> std::io::Result<()> {
			rustix::fs::fchmod(file, mode_of(mode))?;
			Ok(())
		}
	}
}

#[cfg(not(unix))]
mod root {
	use std::fs::File;

	use rotor_support::gopath;

	use super::{ArtifactInfo, FileKind};

	/// Go's `os.Root`, as paths joined onto the root.
	#[derive(Debug)]
	pub struct ArtifactRoot {
		directory: String,
	}

	fn info(metadata: &std::fs::Metadata) -> ArtifactInfo {
		let kind = if metadata.file_type().is_symlink() {
			FileKind::Other
		} else if metadata.is_file() {
			FileKind::Regular
		} else if metadata.is_dir() {
			FileKind::Directory
		} else {
			FileKind::Other
		};
		// Without a stable file index, identity is the size and modification time.
		let modified = metadata
			.modified()
			.ok()
			.and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
			.map_or(0, |duration| i128::try_from(duration.as_nanos()).unwrap_or(0));
		ArtifactInfo {
			kind,
			device: i128::from(metadata.len()),
			inode: modified,
			permissions: if metadata.permissions().readonly() {
				0o444
			} else {
				0o666
			},
		}
	}

	impl ArtifactRoot {
		fn join(&self, path: &str) -> String {
			gopath::join(&[&self.directory, path])
		}

		pub fn open(path: &str) -> std::io::Result<Self> {
			if !std::fs::metadata(path)?.is_dir() {
				return Err(std::io::Error::other("not a directory"));
			}
			Ok(Self {
				directory: path.to_owned(),
			})
		}

		pub fn lstat(&self, path: &str) -> std::io::Result<ArtifactInfo> {
			Ok(info(&std::fs::symlink_metadata(self.join(path))?))
		}

		pub fn stat(&self, path: &str) -> std::io::Result<ArtifactInfo> {
			Ok(info(&std::fs::metadata(self.join(path))?))
		}

		pub fn read_with_info(&self, path: &str) -> std::io::Result<(Vec<u8>, ArtifactInfo)> {
			let full = self.join(path);
			let opened = info(&std::fs::symlink_metadata(&full)?);
			Ok((std::fs::read(&full)?, opened))
		}

		pub fn remove(&self, path: &str) -> std::io::Result<()> {
			let full = self.join(path);
			std::fs::remove_file(&full).or_else(|error| std::fs::remove_dir(&full).map_err(|_| error))
		}

		pub fn mkdir_all(&self, path: &str, _mode: u32) -> std::io::Result<()> {
			std::fs::create_dir_all(self.join(path))
		}

		pub fn rename(&self, old_path: &str, new_path: &str) -> std::io::Result<()> {
			std::fs::rename(self.join(old_path), self.join(new_path))
		}

		pub fn create_exclusive(&self, path: &str, _mode: u32) -> std::io::Result<File> {
			std::fs::OpenOptions::new()
				.read(true)
				.write(true)
				.create_new(true)
				.open(self.join(path))
		}

		pub fn file_info(file: &File) -> std::io::Result<ArtifactInfo> {
			Ok(info(&file.metadata()?))
		}

		pub fn chmod(file: &File, mode: u32) -> std::io::Result<()> {
			let mut permissions = file.metadata()?.permissions();
			permissions.set_readonly(mode & 0o200 == 0);
			file.set_permissions(permissions)
		}
	}
}

pub(crate) use root::ArtifactRoot;

/// `PrepareArtifacts`: `config.json` and `globs.json` under `metadata_directory`, then the build
/// info. A `None` (or empty) payload deletes its file.
///
/// # Errors
///
/// `info` is missing, a payload is invalid JSON, or the build info cannot be encoded.
pub fn prepare_artifacts(
	info: Option<&BuildInfo>,
	metadata_directory: &str,
	config_json: Option<&[u8]>,
	globs_json: Option<&[u8]>,
) -> Result<Vec<Artifact>, Error> {
	let Some(info) = info else {
		return Err(Error::sentinel(ErrorKind::InvalidBuildInfo).wrap("flamework: prepare artifacts"));
	};
	// Go ranges over a two-entry map; the order only picks which invalid payload is reported.
	for (name, data) in [("config.json", config_json), ("globs.json", globs_json)] {
		if let Some(data) = data
			&& !json_syntax::valid(data)
		{
			return Err(Error::msg(format!("flamework: prepare {name}: invalid JSON")));
		}
	}
	let build_info_json = info.marshal_ordered_json()?;
	Ok(vec![
		Artifact {
			path: gopath::join(&[metadata_directory, "config.json"]),
			data: go_clone_bytes(config_json),
		},
		Artifact {
			path: gopath::join(&[metadata_directory, "globs.json"]),
			data: go_clone_bytes(globs_json),
		},
		Artifact {
			path: info.path().to_owned(),
			data: Some(build_info_json),
		},
	])
}

/// `append([]byte(nil), data...)`: nil for empty input.
fn go_clone_bytes(data: Option<&[u8]>) -> Option<Vec<u8>> {
	data.filter(|data| !data.is_empty()).map(<[u8]>::to_vec)
}

/// `PersistArtifacts`: writes or deletes `artifacts` under `project_root` as one transaction,
/// skipping files that already hold their bytes and rolling back on a failed commit.
///
/// # Errors
///
/// A path is not local or repeats, or staging, committing or rolling back failed.
pub fn persist_artifacts(project_root: &str, artifacts: &[Artifact]) -> Result<(), Error> {
	let _guard = ARTIFACT_PERSISTENCE
		.lock()
		.unwrap_or_else(std::sync::PoisonError::into_inner);

	reject_duplicate_artifact_paths(artifacts)?;
	let root = ArtifactRoot::open(project_root).map_err(|error| {
		Error::msg(format!(
			"flamework: open artifact root: {}",
			rotor_support::gostr::path_error("open", project_root, &error)
		))
	})?;
	let changed = changed_artifacts(&root, artifacts);
	let mut staged = stage_artifacts(&root, &changed)?;

	let mut result = Ok(());
	for index in 0..staged.len() {
		if let Err(error) = commit_artifact(&root, &mut staged[index]) {
			let message = format!(
				"flamework: commit artifact {}: {}",
				quote(&staged[index].artifact.path),
				rotor_support::gostr::io_error_text(&error)
			);
			result = Err(match rollback_artifacts(&root, &staged[..index]) {
				Ok(()) => Error::msg(message),
				Err(rollback) => Error::msg(format!("{message}\n{rollback}")),
			});
			break;
		}
		staged[index].committed = true;
	}
	cleanup_staged_artifacts(&root, &staged);
	result
}

fn stage_artifacts(root: &ArtifactRoot, artifacts: &[Artifact]) -> Result<Vec<StagedArtifact>, Error> {
	let mut staged: Vec<StagedArtifact> = Vec::with_capacity(artifacts.len());
	let mut seen = HashSet::with_capacity(artifacts.len());
	for artifact in artifacts {
		let path = match local_artifact_path(&artifact.path) {
			Ok(path) => path,
			Err(error) => {
				cleanup_staged_artifacts(root, &staged);
				return Err(error);
			}
		};
		if !seen.insert(path.clone()) {
			cleanup_staged_artifacts(root, &staged);
			return Err(Error::msg(format!(
				"flamework: duplicate artifact path {}",
				quote(&path)
			)));
		}
		match stage_artifact(
			root,
			Artifact {
				path,
				data: artifact.data.clone(),
			},
		) {
			Ok(entry) => staged.push(entry),
			Err(error) => {
				cleanup_staged_artifacts(root, &staged);
				return Err(error);
			}
		}
	}
	Ok(staged)
}

fn reject_duplicate_artifact_paths(artifacts: &[Artifact]) -> Result<(), Error> {
	let mut seen = HashSet::with_capacity(artifacts.len());
	for artifact in artifacts {
		let path = local_artifact_path(&artifact.path)?;
		if !seen.insert(path.clone()) {
			return Err(Error::msg(format!(
				"flamework: duplicate artifact path {}",
				quote(&path)
			)));
		}
	}
	Ok(())
}

/// `changedArtifacts`: drops writes whose destination already holds the same bytes, so unchanged
/// artifacts keep their modification times.
fn changed_artifacts(root: &ArtifactRoot, artifacts: &[Artifact]) -> Vec<Artifact> {
	artifacts
		.iter()
		.filter(|artifact| artifact.data.is_none() || !artifact_holds(root, artifact))
		.cloned()
		.collect()
}

fn artifact_holds(root: &ArtifactRoot, artifact: &Artifact) -> bool {
	let Ok(path) = local_artifact_path(&artifact.path) else {
		return false;
	};
	let data = artifact.data.as_deref().unwrap_or_default();
	let Ok(info) = root.lstat(&path) else {
		return false;
	};
	if !info.is_regular() {
		return false;
	}
	root.read_with_info(&path).is_ok_and(|(existing, _)| existing == data)
}

/// `localArtifactPath`: `path` cleaned, when it is local to the project root.
pub(crate) fn local_artifact_path(path: &str) -> Result<String, Error> {
	let cleaned = gopath::clean(path);
	if path.is_empty() || cleaned == "." || !is_local(path) || gopath::is_abs(path) {
		return Err(Error::msg(format!(
			"flamework: artifact path must be local to the project root: {}",
			quote(path)
		)));
	}
	Ok(cleaned)
}

/// Go's `filepath.IsLocal`.
fn is_local(path: &str) -> bool {
	if gopath::is_abs(path) || path.is_empty() {
		return false;
	}
	if cfg!(windows) && !gopath::volume_name(path).is_empty() {
		return false;
	}
	let mut has_dots = false;
	for part in path.split(|c| c == '/' || (cfg!(windows) && c == '\\')) {
		if part == "." || part == ".." {
			has_dots = true;
			break;
		}
	}
	if has_dots {
		let cleaned = gopath::clean(path);
		let sep = std::path::MAIN_SEPARATOR;
		if cleaned == ".." || cleaned.starts_with(&format!("..{sep}")) {
			return false;
		}
	}
	true
}

/// Opens the artifact root, for tests that stage and commit by hand.
#[cfg(test)]
pub(crate) fn open_artifact_root(path: &str) -> std::io::Result<ArtifactRoot> {
	ArtifactRoot::open(path)
}
