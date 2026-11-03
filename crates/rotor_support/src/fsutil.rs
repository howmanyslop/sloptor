//! Small filesystem helpers shared across the compiler.

use std::fs;
use std::io;
use std::path::Path;

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

/// `os.WriteFile`: create or truncate, and write.
fn write_file(path: &Path, data: &[u8], mode: u32) -> io::Result<()> {
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
	options.open(path)?.write_all(data)
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
