//! `artifacts.go`: writing Flamework's artifacts as one transaction under the project root.
//!
//! Go confines every file operation to the project root with `os.Root`. [`ArtifactRoot`] is its
//! counterpart: on Unix it holds the root directory's descriptor and resolves each path from it
//! as `os.Root` does, one `openat(O_NOFOLLOW)` at a time, following symlinks that stay inside the
//! root and rejecting the rest, with `os.Root`'s error text. Elsewhere it joins paths onto the
//! root.

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
	Symlink,
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
	use std::os::fd::{AsFd as _, BorrowedFd, OwnedFd};

	use rotor_support::gostr::with_path;
	use rustix::fs::{AtFlags, CWD, FileType, Mode, OFlags};
	use rustix::io::Errno;

	use super::{ArtifactInfo, FileKind};

	/// Go's `os.Root`.
	#[derive(Debug)]
	pub struct ArtifactRoot {
		directory: OwnedFd,
	}

	/// `rootMaxSymlinks`
	const MAX_SYMLINKS: usize = 8;

	/// Why one step of [`ArtifactRoot::do_in_root`] stopped.
	enum Fail {
		/// A plain error, which the operation names with its path.
		Io(std::io::Error),
		/// The component is a symlink to this target, which resolution follows (`errSymlink`).
		Symlink(String),
		/// A `*PathError` without a path: resolution names it with the components so far.
		Op(&'static str, std::io::Error),
	}

	impl From<Errno> for Fail {
		fn from(error: Errno) -> Self {
			Self::Io(error.into())
		}
	}

	/// What [`ArtifactRoot::do_in_root`] returns on failure.
	enum Failure {
		Plain(std::io::Error),
		Named(&'static str, String, std::io::Error),
	}

	impl Failure {
		/// The error as Go's `*PathError{Op: op, Path: name}` wraps a plain failure.
		fn named(self, op: &str, name: &str) -> std::io::Error {
			match self {
				Self::Plain(error) => with_path(op, name, &error),
				Self::Named(op, path, error) => with_path(op, &path, &error),
			}
		}
	}

	/// `doInRoot`'s flags.
	#[derive(Clone, Copy, Default)]
	struct Flags {
		creating_directory: bool,
		no_handle_terminal_slash: bool,
	}

	/// `errPathEscapes`
	fn path_escapes() -> std::io::Error {
		std::io::Error::other("path escapes from parent")
	}

	fn info(stat: &rustix::fs::Stat) -> ArtifactInfo {
		let kind = match FileType::from_raw_mode(stat.st_mode) {
			FileType::RegularFile => FileKind::Regular,
			FileType::Directory => FileKind::Directory,
			FileType::Symlink => FileKind::Symlink,
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

	/// `splitPathInRoot`: `path`'s components between `prefix` and `suffix`, and whether it ends
	/// in a slash. `.` components go, except a last one.
	fn split_path_in_root(path: &str, prefix: &[String], suffix: &[String]) -> std::io::Result<(Vec<String>, bool)> {
		if path.is_empty() {
			return Err(std::io::Error::other("empty path"));
		}
		if path.starts_with('/') {
			return Err(path_escapes());
		}
		let mut parts = prefix.to_vec();
		let pieces: Vec<&str> = path.split('/').collect();
		let ends_in_slash = pieces.len() > 1 && pieces.last().is_some_and(|last| last.is_empty());
		let mut components = pieces.into_iter().filter(|piece| !piece.is_empty()).peekable();
		while let Some(component) = components.next() {
			if component != "." || components.peek().is_none() {
				parts.push(component.to_owned());
			}
		}
		if !suffix.is_empty() && parts.last().is_some_and(|last| last == ".") {
			parts.pop();
		}
		parts.extend_from_slice(suffix);
		Ok((parts, ends_in_slash))
	}

	fn lstatat(parent: BorrowedFd<'_>, name: &str) -> Result<rustix::fs::Stat, Errno> {
		rustix::fs::statat(parent, name, AtFlags::SYMLINK_NOFOLLOW)
	}

	fn is_link(stat: &rustix::fs::Stat) -> bool {
		FileType::from_raw_mode(stat.st_mode) == FileType::Symlink
	}

	/// `isNoFollowErr`
	fn is_no_follow_error(error: Errno) -> bool {
		error == Errno::LOOP || error == Errno::MLINK
	}

	/// `checkSymlink`: the link's target to follow, or `original` when `name` is no symlink.
	fn check_symlink(parent: BorrowedFd<'_>, name: &str, original: Errno) -> Fail {
		rustix::fs::readlinkat(parent, name, Vec::new()).map_or_else(
			|_| original.into(),
			|target| Fail::Symlink(target.to_string_lossy().into_owned()),
		)
	}

	/// `rootOpenDir`
	fn root_open_dir(parent: BorrowedFd<'_>, name: &str) -> Result<OwnedFd, Fail> {
		match rustix::fs::openat(
			parent,
			name,
			OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::DIRECTORY | OFlags::RDONLY,
			Mode::empty(),
		) {
			Ok(fd) => Ok(fd),
			Err(error) if is_no_follow_error(error) || error == Errno::NOTDIR => {
				Err(check_symlink(parent, name, error))
			}
			Err(error) if error == Errno::NOTSUP || error == Errno::OPNOTSUPP => Err(Errno::NOTDIR.into()),
			Err(error) => Err(error.into()),
		}
	}

	/// `removeat`: a file, else an empty directory.
	fn removeat(parent: BorrowedFd<'_>, name: &str) -> Result<(), Errno> {
		let Err(file_error) = rustix::fs::unlinkat(parent, name, AtFlags::empty()) else {
			return Ok(());
		};
		match rustix::fs::unlinkat(parent, name, AtFlags::REMOVEDIR) {
			Ok(()) => Ok(()),
			Err(directory_error) if directory_error != Errno::NOTDIR => Err(directory_error),
			Err(_) => Err(file_error),
		}
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

		/// `doInRoot`: resolves `name` from the root's descriptor one component at a time, so a
		/// moved root or a swapped directory cannot redirect it, following symlinks whose
		/// targets stay inside the root, then runs `last` on the final component.
		fn do_in_root<T>(
			&self,
			name: &str,
			flags: Flags,
			open_dir: &dyn Fn(BorrowedFd<'_>, &str) -> Result<OwnedFd, Fail>,
			mut last: impl FnMut(BorrowedFd<'_>, &str, bool) -> Result<T, Fail>,
		) -> Result<T, Failure> {
			// Go stops only once both limits are exceeded.
			const MAX_STEPS: usize = 255;
			const MAX_RESTARTS: usize = 8;
			let (mut parts, mut ends_in_slash) = split_path_in_root(name, &[], &[]).map_err(Failure::Plain)?;
			let mut directory: Option<OwnedFd> = None;
			let (mut index, mut steps, mut restarts, mut symlinks) = (0, 0, 0, 0);
			loop {
				steps += 1;
				if steps > MAX_STEPS && restarts > MAX_RESTARTS {
					return Err(Failure::Plain(Errno::NAMETOOLONG.into()));
				}
				if parts[index] == ".." {
					// Resolution restarts from the root without the components `..` removes.
					restarts += 1;
					let mut end = index + 1;
					while end < parts.len() && parts[end] == ".." {
						end += 1;
					}
					let count = end - index;
					if count > index {
						return Err(Failure::Plain(path_escapes()));
					}
					parts.drain(index - count..end);
					if parts.is_empty() {
						parts.push(".".to_owned());
					}
					index = 0;
					directory = None;
					continue;
				}
				let base = directory
					.as_ref()
					.map_or_else(|| self.directory.as_fd(), |fd| fd.as_fd());
				let step = if index == parts.len() - 1 {
					let mut checked = Ok(());
					if ends_in_slash && !flags.no_handle_terminal_slash {
						match lstatat(base, &parts[index]) {
							Err(error) if error == Errno::NOENT && flags.creating_directory => {}
							Err(error) => return Err(Failure::Plain(error.into())),
							Ok(stat) if FileType::from_raw_mode(stat.st_mode) == FileType::Directory => {}
							Ok(stat) if is_link(&stat) => {
								checked = Err(check_symlink(base, &parts[index], Errno::NOTDIR));
							}
							Ok(_) => return Err(Failure::Plain(Errno::NOTDIR.into())),
						}
					}
					match checked.and_then(|()| last(base, &parts[index], ends_in_slash)) {
						Ok(value) => return Ok(value),
						Err(error) => Err(error),
					}
				} else {
					open_dir(base, &parts[index]).map(Some)
				};
				match step {
					Ok(opened) => directory = opened,
					Err(Fail::Symlink(target)) => {
						symlinks += 1;
						if symlinks > MAX_SYMLINKS {
							return Err(Failure::Plain(Errno::LOOP.into()));
						}
						let at_last = index == parts.len() - 1;
						let (new_parts, new_ends_in_slash) =
							split_path_in_root(&target, &parts[..index], &parts[index + 1..])
								.map_err(Failure::Plain)?;
						if at_last && new_ends_in_slash {
							ends_in_slash = true;
						}
						if new_parts.len() < index || parts[..index] != new_parts[..index] {
							// A component already walked changed: start over from the root.
							index = 0;
							directory = None;
						}
						parts = new_parts;
						continue;
					}
					Err(Fail::Op(op, error)) => return Err(Failure::Named(op, parts[..=index].join("/"), error)),
					Err(Fail::Io(error)) => return Err(Failure::Plain(error)),
				}
				index += 1;
			}
		}

		/// `rootStat`
		fn root_stat(&self, path: &str, lstat: bool) -> std::io::Result<ArtifactInfo> {
			self.do_in_root(path, Flags::default(), &root_open_dir, |parent, name, _| {
				let stat = lstatat(parent, name)?;
				if !lstat && is_link(&stat) {
					return Err(check_symlink(parent, name, Errno::LOOP));
				}
				Ok(info(&stat))
			})
			.map_err(|failure| failure.named("statat", path))
		}

		pub fn lstat(&self, path: &str) -> std::io::Result<ArtifactInfo> {
			self.root_stat(path, true)
		}

		pub fn stat(&self, path: &str) -> std::io::Result<ArtifactInfo> {
			self.root_stat(path, false)
		}

		/// `rootOpenFileNolog`
		fn open_file(&self, path: &str, flags: OFlags, mode: u32) -> std::io::Result<OwnedFd> {
			let create_exclusive = flags.contains(OFlags::CREATE | OFlags::EXCL);
			self.do_in_root(path, Flags::default(), &root_open_dir, |parent, name, _| {
				match rustix::fs::openat(parent, name, OFlags::NOFOLLOW | OFlags::CLOEXEC | flags, mode_of(mode)) {
					Ok(fd) => Ok(fd),
					// Never follow symlinks when creating exclusively.
					Err(error) if !create_exclusive && (is_no_follow_error(error) || error == Errno::NOTDIR) => {
						Err(check_symlink(parent, name, error))
					}
					Err(error) => Err(error.into()),
				}
			})
			.map_err(|failure| failure.named("openat", path))
		}

		pub fn read_with_info(&self, path: &str) -> std::io::Result<(Vec<u8>, ArtifactInfo)> {
			let fd = self.open_file(path, OFlags::RDONLY, 0)?;
			let opened = info(&rustix::fs::fstat(&fd)?);
			let mut file = File::from(fd);
			let mut contents = Vec::new();
			file.read_to_end(&mut contents)?;
			Ok((contents, opened))
		}

		/// `rootRemove`
		pub fn remove(&self, path: &str) -> std::io::Result<()> {
			self.do_in_root(path, Flags::default(), &root_open_dir, |parent, name, _| {
				Ok(removeat(parent, name)?)
			})
			.map_err(|failure| failure.named("removeat", path))
		}

		/// `rootMkdirAll`
		pub fn mkdir_all(&self, path: &str, mode: u32) -> std::io::Result<()> {
			let open_dir = |parent: BorrowedFd<'_>, name: &str| -> Result<OwnedFd, Fail> {
				for attempt in 0..2 {
					match root_open_dir(parent, name) {
						Err(Fail::Io(error)) if attempt == 0 && error.kind() == std::io::ErrorKind::NotFound => {}
						Err(Fail::Io(error)) => return Err(Fail::Op("openat", error)),
						result => return result,
					}
					// Created by another process meanwhile is fine.
					match rustix::fs::mkdirat(parent, name, mode_of(mode)) {
						Ok(()) | Err(Errno::EXIST) => {}
						Err(error) => return Err(Fail::Op("mkdirat", error.into())),
					}
				}
				Err(Fail::Op("openat", Errno::NOENT.into()))
			};
			let flags = Flags {
				creating_directory: true,
				no_handle_terminal_slash: cfg!(any(target_os = "linux", target_os = "openbsd")),
			};
			self.do_in_root(path, flags, &open_dir, |parent, name, _| {
				let mut result = rustix::fs::mkdirat(parent, name, mode_of(mode)).map_err(std::io::Error::from);
				if matches!(&result, Err(error) if error.raw_os_error() == Some(Errno::EXIST.raw_os_error()))
					&& let Ok(stat) = lstatat(parent, name)
				{
					if FileType::from_raw_mode(stat.st_mode) == FileType::Directory {
						result = Ok(());
					} else if is_link(&stat) {
						// A link to a directory is one, as for `os.MkdirAll`.
						match self.stat(path) {
							Ok(target) if target.is_dir() => result = Ok(()),
							Ok(_) => result = Err(Errno::NOTDIR.into()),
							Err(error) if error.kind() != std::io::ErrorKind::NotFound => result = Err(error),
							Err(_) => {}
						}
					}
				}
				result.map_err(|error| Fail::Op("mkdirat", error))
			})
			.map_err(|failure| failure.named("mkdirat", path))
		}

		/// `rootRename`
		pub fn rename(&self, old_path: &str, new_path: &str) -> std::io::Result<()> {
			let new_flags = Flags {
				creating_directory: true,
				no_handle_terminal_slash: false,
			};
			self.do_in_root(old_path, Flags::default(), &root_open_dir, |old_parent, old_name, _| {
				self.do_in_root(
					new_path,
					new_flags,
					&root_open_dir,
					|new_parent, new_name, new_ends_in_slash| {
						if new_ends_in_slash
							&& FileType::from_raw_mode(lstatat(old_parent, old_name)?.st_mode) != FileType::Directory
						{
							return Err(Errno::NOTDIR.into());
						}
						if let Ok(existing) = lstatat(new_parent, new_name)
							&& FileType::from_raw_mode(existing.st_mode) == FileType::Directory
						{
							let old = lstatat(old_parent, old_name)?;
							if new_name == old_name || existing.st_dev != old.st_dev || existing.st_ino != old.st_ino {
								return Err(Errno::EXIST.into());
							}
						}
						Ok(rustix::fs::renameat(old_parent, old_name, new_parent, new_name)?)
					},
				)
				.map_err(|failure| match failure {
					Failure::Plain(error) => Fail::Io(error),
					Failure::Named(op, path, error) => Fail::Io(with_path(op, &path, &error)),
				})
			})
			.map_err(|failure| {
				// Go's `*LinkError`: `renameat <old> <new>: <error>`.
				let error = match failure {
					Failure::Plain(error) => error,
					Failure::Named(op, path, error) => with_path(op, &path, &error),
				};
				with_path("renameat", &format!("{old_path} {new_path}"), &error)
			})
		}

		pub fn create_exclusive(&self, path: &str, mode: u32) -> std::io::Result<File> {
			let fd = self.open_file(path, OFlags::RDWR | OFlags::CREATE | OFlags::EXCL, mode)?;
			Ok(File::from(fd))
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
			FileKind::Symlink
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
