//! Small filesystem helpers shared across the compiler.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::gostr;

/// Opens `path` to read its metadata and nothing else, as `fs::metadata` and `fs::symlink_metadata`
/// do: it works on directories and holds no access that could conflict with another open.
///
/// # Errors
///
/// Returns the I/O error from opening the path.
#[cfg(windows)]
pub fn open_for_metadata(path: &Path, follow_symlinks: bool) -> io::Result<fs::File> {
	use std::os::windows::fs::OpenOptionsExt as _;

	use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT};

	let link_flag = if follow_symlinks {
		0
	} else {
		FILE_FLAG_OPEN_REPARSE_POINT
	};
	fs::OpenOptions::new()
		.access_mode(0)
		.custom_flags(FILE_FLAG_BACKUP_SEMANTICS | link_flag)
		.open(path)
}

/// The volume serial number and file index of an open file, which is what `os.SameFile` compares
/// on Windows. Neither changes when the file is written or renamed.
///
/// # Errors
///
/// Returns the I/O error from querying the handle.
#[cfg(windows)]
#[expect(
	unsafe_code,
	reason = "GetFileInformationByHandle takes a handle the borrow keeps open"
)]
pub fn file_identity(file: &fs::File) -> io::Result<(u32, u64)> {
	use std::os::windows::io::AsRawHandle as _;

	use windows_sys::Win32::Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle};

	let mut information = BY_HANDLE_FILE_INFORMATION::default();
	// SAFETY: the handle is open for the duration of the borrow and `information` is a valid out
	// pointer.
	if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &raw mut information) } == 0 {
		return Err(io::Error::last_os_error());
	}
	Ok((
		information.dwVolumeSerialNumber,
		u64::from(information.nFileIndexHigh) << 32 | u64::from(information.nFileIndexLow),
	))
}

/// Whether `path` is a regular file (not a symlink to one) holding exactly
/// `data`. It compares sizes before reading, so a changed file usually costs
/// one stat.
#[must_use]
pub fn same_contents(path: impl AsRef<Path>, data: &[u8]) -> bool {
	let path = path.as_ref();
	let Ok(info) = fs::symlink_metadata(path) else {
		return false;
	};
	if !info.file_type().is_file() || info.len() != data.len() as u64 {
		return false;
	}
	fs::read(path).is_ok_and(|existing| existing == data)
}

/// Writes `data` to `path` unless `path` already holds it, and reports whether
/// it wrote. `mode` is the permission bits of a newly created file on Unix and
/// is ignored elsewhere.
///
/// # Errors
///
/// Returns the I/O error from creating or writing the file.
pub fn write_file_if_changed(path: impl AsRef<Path>, data: &[u8], mode: u32) -> io::Result<bool> {
	let path = path.as_ref();
	if same_contents(path, data) {
		return Ok(false);
	}
	write_file(path, data, mode)?;
	Ok(true)
}

/// `os.WriteFile`: create or truncate, and write. A new file gets `mode` (before the umask) on
/// unix.
///
/// # Errors
///
/// Returns the I/O error from opening or writing the file.
pub fn write_file(path: &Path, data: &[u8], mode: u32) -> io::Result<()> {
	use std::io::Write;
	let mut options = fs::OpenOptions::new();
	options.write(true).create(true).truncate(true);
	#[cfg(unix)]
	{
		use std::os::unix::fs::OpenOptionsExt;
		options.mode(mode);
	}
	#[cfg(not(unix))]
	let _ = mode;
	let name = path.to_string_lossy();
	let mut file = options
		.open(path)
		.map_err(|error| gostr::with_path("open", &name, &error))?;
	file.write_all(data)
		.map_err(|error| gostr::with_path("write", &name, &error))
}

/// `os.MkdirAll`: creates `path` and its missing parents, each with `mode` (before the umask) on
/// unix.
///
/// # Errors
///
/// Returns the I/O error from creating a directory.
pub fn mkdir_all(path: impl AsRef<Path>, mode: u32) -> io::Result<()> {
	let mut builder = fs::DirBuilder::new();
	builder.recursive(true);
	#[cfg(unix)]
	{
		use std::os::unix::fs::DirBuilderExt;
		builder.mode(mode);
	}
	#[cfg(not(unix))]
	let _ = mode;
	let path = path.as_ref();
	builder
		.create(path)
		.map_err(|error| gostr::with_path("mkdir", &path.to_string_lossy(), &error))
}

/// `(*os.File).Chmod`: sets an open file's permission bits on unix (not subject to the umask).
///
/// # Errors
///
/// Returns the I/O error from changing the mode.
#[allow(clippy::missing_const_for_fn)]
pub fn chmod(file: &fs::File, mode: u32) -> io::Result<()> {
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		file.set_permissions(fs::Permissions::from_mode(mode))
	}
	#[cfg(not(unix))]
	{
		let _ = (file, mode);
		Ok(())
	}
}

/// The real path of `path`. Unlike [`fs::canonicalize`], it never adds Windows's `\\?\\` prefix
/// when the path can be spelled without one, because the compiler cannot read a prefixed path.
///
/// # Errors
///
/// Returns the I/O error from looking the path up.
pub fn real_path(path: impl AsRef<Path>) -> io::Result<PathBuf> {
	dunce::canonicalize(path)
}

/// `filepath.EvalSymlinks`, which also resolves junctions on Windows.
///
/// Windows additionally resolves directory junctions, `subst` drives and 8.3
/// short names, as Node's `fs.realpathSync.native` does. Since Go 1.23
/// `filepath.EvalSymlinks` fails on any path through a junction, so Go and the
/// Node sidecar spelled the same file differently. Like
/// `filepath.EvalSymlinks`, it fails when the path does not exist.
///
/// # Errors
///
/// Returns the I/O error from looking the path up.
#[cfg(not(windows))]
pub fn eval_symlinks(path: &str) -> io::Result<String> {
	walk_symlinks(path)
}

/// `filepath.EvalSymlinks`, which also resolves junctions on Windows.
///
/// Windows additionally resolves directory junctions, `subst` drives and 8.3
/// short names, as Node's `fs.realpathSync.native` does. Like
/// `filepath.EvalSymlinks`, it fails when the path does not exist.
///
/// # Errors
///
/// Returns the I/O error from looking the path up.
#[cfg(windows)]
pub fn eval_symlinks(path: &str) -> io::Result<String> {
	use crate::gopath;

	let absolute = gopath::abs(path)?;
	fs::metadata(&absolute)?;
	let real = tsrs_vfs::osvfs::fs().realpath(&gopath::to_slash(&absolute));
	Ok(gopath::from_slash(&real).into_owned())
}

/// Go's `filepath.walkSymlinks` for Unix: resolves every symlink in `path`
/// one component at a time, keeping a relative path relative.
#[cfg(not(windows))]
fn walk_symlinks(path: &str) -> io::Result<String> {
	use crate::gopath;

	const MAX_LINKS: u32 = 255;
	let mut path = path.to_owned();
	let mut vol_len = usize::from(path.starts_with('/'));
	let mut dest = path[..vol_len].to_owned();
	let mut links_walked = 0;
	let mut start = vol_len;
	while start < path.len() {
		let bytes = path.as_bytes();
		while start < bytes.len() && bytes[start] == b'/' {
			start += 1;
		}
		let mut end = start;
		while end < bytes.len() && bytes[end] != b'/' {
			end += 1;
		}
		let component = &path[start..end];
		if end == start {
			// No more path components.
			break;
		}
		if component == "." {
			start = end;
			continue;
		}
		if component == ".." {
			// Back up to the previous component if possible.
			let last_slash = dest.as_bytes()[vol_len..]
				.iter()
				.rposition(|&c| c == b'/')
				.map(|r| r + vol_len);
			match last_slash {
				Some(r) if &dest[r + 1..] != ".." => dest.truncate(r),
				_ => {
					// No slashes, or the path ends in a ".." that had to stay.
					if dest.len() > vol_len {
						dest.push('/');
					}
					dest.push_str("..");
				}
			}
			start = end;
			continue;
		}

		// An ordinary path component.
		if !dest.is_empty() && !dest.ends_with('/') {
			dest.push('/');
		}
		dest.push_str(component);

		let info = fs::symlink_metadata(&dest)?;
		if !info.file_type().is_symlink() {
			if !info.is_dir() && end < path.len() {
				return Err(io::ErrorKind::NotADirectory.into());
			}
			start = end;
			continue;
		}

		// Found a symlink.
		links_walked += 1;
		if links_walked > MAX_LINKS {
			return Err(io::Error::other("EvalSymlinks: too many links"));
		}
		let link = fs::read_link(&dest)?.to_string_lossy().into_owned();
		path = format!("{link}{}", &path[end..]);
		if link.starts_with('/') {
			// A symlink to an absolute path.
			"/".clone_into(&mut dest);
			end = 1;
			vol_len = 1;
		} else {
			// A symlink to a relative path replaces the last component of dest.
			match dest.as_bytes()[vol_len..].iter().rposition(|&c| c == b'/') {
				None => dest.truncate(vol_len),
				Some(r) => dest.truncate(r + vol_len),
			}
			end = 0;
		}
		start = end;
	}
	Ok(gopath::clean(&dest))
}

#[cfg(test)]
mod tests;
