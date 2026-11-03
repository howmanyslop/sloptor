//! Go's `path/filepath` string functions, ported for both separator flavors.
//!
//! Rust's `Path` normalizes `a//b` and `./a` while iterating, and its `join`
//! replaces the left side for absolute right-hand sides. Go's `filepath` does
//! neither, and the compiler's output (and the Rojo resolver's decisions) has
//! to match Go to the byte. So paths stay `&str`/`String` and these functions
//! reproduce `Clean`, `Join`, `Dir`, `Base`, `Rel`, `IsAbs`, `VolumeName`,
//! `FromSlash`, `ToSlash` and `Abs` exactly.
//!
//! Each function is a method of [`Flavor`], so the Windows rules are plain
//! string logic that tests exercise on every host. The free functions at the
//! bottom pass [`NATIVE`], a constant, so on macOS and Linux the Windows
//! branches are folded away at compile time and cost nothing.

use std::borrow::Cow;
use std::fmt;
use std::io;

/// Which set of `filepath` rules a function follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavor {
	/// `/` is the only separator and there are no volume names.
	Unix,
	/// `\` and `/` both separate, and paths may start with a volume name
	/// (`C:`, `\\host\share`, `\\?\C:`).
	Windows,
}

/// The flavor of the platform this crate was compiled for.
pub const NATIVE: Flavor = if cfg!(windows) { Flavor::Windows } else { Flavor::Unix };

/// Returned by [`Flavor::rel`] when no relative path exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelError {
	base: String,
	target: String,
}

impl fmt::Display for RelError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "Rel: can't make {} relative to {}", self.target, self.base)
	}
}

impl std::error::Error for RelError {}

/// A lazily constructed path buffer: it allocates only once the output
/// diverges from the input, like Go's `lazybuf`.
struct LazyBuf<'a> {
	path: &'a [u8],
	buf: Option<Vec<u8>>,
	w: usize,
	vol_and_path: &'a str,
	vol_len: usize,
}

impl<'a> LazyBuf<'a> {
	fn index(&self, i: usize) -> u8 {
		self.buf.as_ref().map_or_else(|| self.path[i], |buf| buf[i])
	}

	fn append(&mut self, c: u8) {
		let buf = if let Some(buf) = &mut self.buf {
			buf
		} else {
			if self.w < self.path.len() && self.path[self.w] == c {
				self.w += 1;
				return;
			}
			let mut buf = vec![0; self.path.len()];
			buf[..self.w].copy_from_slice(&self.path[..self.w]);
			self.buf.insert(buf)
		};
		if self.w < buf.len() {
			buf[self.w] = c;
		} else {
			buf.push(c);
		}
		self.w += 1;
	}

	fn prepend(&mut self, prefix: &[u8]) {
		let buf = self.buf.get_or_insert_with(Vec::new);
		buf.splice(0..0, prefix.iter().copied());
		self.w += prefix.len();
	}

	fn finish(self) -> Cow<'a, str> {
		match &self.buf {
			None => Cow::Borrowed(self.vol_and_path.get(..self.vol_len + self.w).unwrap_or_default()),
			Some(buf) => {
				let mut out = String::with_capacity(self.vol_len + self.w);
				out.push_str(self.vol_and_path.get(..self.vol_len).unwrap_or_default());
				out.push_str(&String::from_utf8_lossy(&buf[..self.w]));
				Cow::Owned(out)
			}
		}
	}
}

impl Flavor {
	/// The separator `Clean` writes: `/` or `\`.
	#[must_use]
	pub const fn separator(self) -> char {
		match self {
			Self::Unix => '/',
			Self::Windows => '\\',
		}
	}

	const fn separator_byte(self) -> u8 {
		match self {
			Self::Unix => b'/',
			Self::Windows => b'\\',
		}
	}

	const fn is_sep(self, c: u8) -> bool {
		match self {
			Self::Unix => c == b'/',
			Self::Windows => c == b'/' || c == b'\\',
		}
	}

	/// `filepath.FromSlash`.
	#[must_use]
	pub fn from_slash(self, path: &str) -> Cow<'_, str> {
		match self {
			Self::Unix => Cow::Borrowed(path),
			Self::Windows => replace_byte(path, '/', '\\'),
		}
	}

	/// `filepath.ToSlash`.
	#[must_use]
	pub fn to_slash(self, path: &str) -> Cow<'_, str> {
		match self {
			Self::Unix => Cow::Borrowed(path),
			Self::Windows => replace_byte(path, '\\', '/'),
		}
	}

	/// The length in bytes of the leading volume name, which is always 0 on
	/// Unix (`filepathlite.VolumeNameLen`).
	#[must_use]
	pub fn volume_name_len(self, path: &str) -> usize {
		if self == Self::Unix {
			return 0;
		}
		let b = path.as_bytes();
		if b.len() >= 2 && b[1] == b':' {
			return 2;
		}
		if b.is_empty() || !self.is_sep(b[0]) {
			return 0;
		}
		if has_prefix_fold(b, br"\\.\UNC") {
			return unc_len(b, br"\\.\UNC\".len());
		}
		if has_prefix_fold(b, br"\\.") || has_prefix_fold(b, br"\\?") || has_prefix_fold(b, br"\??") {
			if b.len() == 3 {
				return 3;
			}
			return match cut_path(&b[4..]) {
				None => b.len(),
				Some((_, rest)) => b.len() - rest.len() - 1,
			};
		}
		if b.len() >= 2 && self.is_sep(b[1]) {
			return unc_len(b, 2);
		}
		0
	}

	/// `filepath.VolumeName`.
	#[must_use]
	pub fn volume_name(self, path: &str) -> String {
		let len = self.volume_name_len(path);
		self.from_slash(path.get(..len).unwrap_or_default()).into_owned()
	}

	/// `filepath.IsAbs`.
	#[must_use]
	pub fn is_abs(self, path: &str) -> bool {
		if self == Self::Unix {
			return path.starts_with('/');
		}
		let l = self.volume_name_len(path);
		if l == 0 {
			return false;
		}
		let b = path.as_bytes();
		if self.is_sep(b[0]) && b.get(1).is_some_and(|&c| self.is_sep(c)) {
			return true;
		}
		b.get(l).is_some_and(|&c| self.is_sep(c))
	}

	/// `filepath.Clean`, returning the input unchanged (and unallocated)
	/// when it is already clean or only loses trailing separators.
	#[must_use]
	pub fn clean_cow(self, original: &str) -> Cow<'_, str> {
		let vol_len = self.volume_name_len(original);
		let bytes = &original.as_bytes()[vol_len..];
		if bytes.is_empty() {
			let o = original.as_bytes();
			if vol_len > 1 && self.is_sep(o[0]) && self.is_sep(o[1]) {
				// Should be a UNC path.
				return self.from_slash(original);
			}
			return Cow::Owned(format!("{original}."));
		}
		let rooted = self.is_sep(bytes[0]);
		let n = bytes.len();
		let mut out = LazyBuf {
			path: bytes,
			buf: None,
			w: 0,
			vol_and_path: original,
			vol_len,
		};
		let (mut r, mut dotdot) = (0, 0);
		if rooted {
			out.append(self.separator_byte());
			(r, dotdot) = (1, 1);
		}
		while r < n {
			if self.is_sep(bytes[r]) {
				// Empty path element.
				r += 1;
			} else if bytes[r] == b'.' && (r + 1 == n || self.is_sep(bytes[r + 1])) {
				// A "." element.
				r += 1;
			} else if bytes[r] == b'.' && bytes[r + 1] == b'.' && (r + 2 == n || self.is_sep(bytes[r + 2])) {
				// A ".." element: remove to the last separator.
				r += 2;
				if out.w > dotdot {
					// Can backtrack.
					out.w -= 1;
					while out.w > dotdot && !self.is_sep(out.index(out.w)) {
						out.w -= 1;
					}
				} else if !rooted {
					// Cannot backtrack, but not rooted, so append a ".." element.
					if out.w > 0 {
						out.append(self.separator_byte());
					}
					out.append(b'.');
					out.append(b'.');
					dotdot = out.w;
				}
			} else {
				// A real path element; add a separator if needed.
				if (rooted && out.w != 1) || (!rooted && out.w != 0) {
					out.append(self.separator_byte());
				}
				while r < n && !self.is_sep(bytes[r]) {
					out.append(bytes[r]);
					r += 1;
				}
			}
		}
		// Turn an empty result into ".".
		if out.w == 0 {
			out.append(b'.');
		}
		if self == Self::Windows {
			post_clean(&mut out);
		}
		match out.finish() {
			Cow::Borrowed(cleaned) => self.from_slash(cleaned),
			Cow::Owned(cleaned) => Cow::Owned(self.from_slash(&cleaned).into_owned()),
		}
	}

	/// `filepath.Clean`.
	#[must_use]
	pub fn clean(self, path: &str) -> String {
		self.clean_cow(path).into_owned()
	}

	/// `filepath.Join`.
	#[must_use]
	pub fn join(self, elems: &[&str]) -> String {
		match self {
			Self::Unix => {
				for (i, e) in elems.iter().enumerate() {
					if !e.is_empty() {
						return self.clean(&elems[i..].join("/"));
					}
				}
				String::new()
			}
			Self::Windows => join_windows(elems),
		}
	}

	/// `filepath.Dir`.
	#[must_use]
	pub fn dir(self, path: &str) -> String {
		let vol = self.volume_name(path);
		let b = path.as_bytes();
		let mut i = b.len();
		while i > vol.len() && !self.is_sep(b[i - 1]) {
			i -= 1;
		}
		let dir = self.clean(&path[vol.len()..i]);
		if dir == "." && vol.len() > 2 {
			// Must be UNC.
			return vol;
		}
		vol + &dir
	}

	/// `filepath.Base`.
	#[must_use]
	pub fn base(self, path: &str) -> String {
		if path.is_empty() {
			return ".".to_owned();
		}
		let b = path.as_bytes();
		let mut end = b.len();
		// Strip trailing separators.
		while end > 0 && self.is_sep(b[end - 1]) {
			end -= 1;
		}
		// Throw away the volume name.
		let start = self.volume_name_len(&path[..end]);
		let mut i = end;
		while i > start && !self.is_sep(b[i - 1]) {
			i -= 1;
		}
		// If nothing is left, it had only separators.
		if i == end {
			return self.separator().to_string();
		}
		path[i..end].to_owned()
	}

	/// `filepath.Rel`.
	///
	/// # Errors
	///
	/// Returns a [`RelError`] when `target` cannot be made relative to `base`
	/// lexically.
	pub fn rel(self, base_path: &str, targ_path: &str) -> Result<String, RelError> {
		let err = || RelError {
			base: base_path.to_owned(),
			target: targ_path.to_owned(),
		};
		let base_vol = self.volume_name(base_path);
		let targ_vol = self.volume_name(targ_path);
		let base_clean = self.clean_cow(base_path);
		let targ_clean = self.clean_cow(targ_path);
		if self.same_word(&targ_clean, &base_clean) {
			return Ok(".".to_owned());
		}
		let mut base = &base_clean[base_vol.len()..];
		let targ = &targ_clean[targ_vol.len()..];
		let unc_base_root;
		if base == "." {
			base = "";
		} else if base.is_empty() && self.volume_name_len(&base_vol) > 2 {
			// Treat any target path matching `\\host\share` base as absolute.
			unc_base_root = self.separator().to_string();
			base = &unc_base_root;
		}
		let sep = self.separator_byte();
		// Can't use IsAbs: `\a` and `a` are both relative on Windows.
		let base_slashed = base.as_bytes().first() == Some(&sep);
		let targ_slashed = targ.as_bytes().first() == Some(&sep);
		if base_slashed != targ_slashed || !self.same_word(&base_vol, &targ_vol) {
			return Err(err());
		}
		// Position base[b0..bi] and targ[t0..ti] at the first differing elements.
		let (bl, tl) = (base.len(), targ.len());
		let (mut b0, mut bi, mut t0, mut ti) = (0, 0, 0, 0);
		loop {
			while bi < bl && base.as_bytes()[bi] != sep {
				bi += 1;
			}
			while ti < tl && targ.as_bytes()[ti] != sep {
				ti += 1;
			}
			if !self.same_word(&targ[t0..ti], &base[b0..bi]) {
				break;
			}
			if bi < bl {
				bi += 1;
			}
			if ti < tl {
				ti += 1;
			}
			b0 = bi;
			t0 = ti;
		}
		if &base[b0..bi] == ".." {
			return Err(err());
		}
		if b0 != bl {
			// Base elements are left, so go up before going down.
			let seps = base[b0..bl].bytes().filter(|&c| c == sep).count();
			let mut buf = String::with_capacity(2 + seps * 3 + 1 + tl - t0);
			buf.push_str("..");
			for _ in 0..seps {
				buf.push(self.separator());
				buf.push_str("..");
			}
			if t0 != tl {
				buf.push(self.separator());
				buf.push_str(&targ[t0..]);
			}
			return Ok(self.clean(&buf));
		}
		Ok(targ[t0..].to_owned())
	}

	/// Go's `sameWord`: byte equality on Unix, `strings.EqualFold` on
	/// Windows.
	fn same_word(self, a: &str, b: &str) -> bool {
		match self {
			Self::Unix => a == b,
			Self::Windows => equal_fold(a, b),
		}
	}
}

/// `strings.EqualFold`, approximated with per-character case mapping. Go
/// compares simple-fold orbits; lowercasing and uppercasing both sides covers
/// every orbit that matters for file names.
fn equal_fold(a: &str, b: &str) -> bool {
	if a == b {
		return true;
	}
	let mut ac = a.chars();
	let mut bc = b.chars();
	loop {
		match (ac.next(), bc.next()) {
			(None, None) => return true,
			(Some(x), Some(y)) => {
				if x != y && !x.to_lowercase().eq(y.to_lowercase()) && !x.to_uppercase().eq(y.to_uppercase()) {
					return false;
				}
			}
			_ => return false,
		}
	}
}

fn replace_byte(s: &str, old: char, new: char) -> Cow<'_, str> {
	if s.contains(old) {
		Cow::Owned(s.replace(old, new.encode_utf8(&mut [0; 4])))
	} else {
		Cow::Borrowed(s)
	}
}

/// Whether `s` begins with `prefix`, ignoring ASCII case and treating every
/// separator as equivalent. A longer `s` must continue with a separator.
fn has_prefix_fold(s: &[u8], prefix: &[u8]) -> bool {
	if s.len() < prefix.len() {
		return false;
	}
	for (&p, &c) in prefix.iter().zip(s) {
		if p == b'\\' || p == b'/' {
			if c != b'\\' && c != b'/' {
				return false;
			}
		} else if !p.eq_ignore_ascii_case(&c) {
			return false;
		}
	}
	s.len() == prefix.len() || s[prefix.len()] == b'\\' || s[prefix.len()] == b'/'
}

/// The length of a UNC volume prefix; `prefix_len` is how much precedes the
/// host (2 for `//host/share`).
fn unc_len(path: &[u8], prefix_len: usize) -> usize {
	let mut count = 0;
	for (i, &c) in path.iter().enumerate().skip(prefix_len) {
		if c == b'\\' || c == b'/' {
			count += 1;
			if count == 2 {
				return i;
			}
		}
	}
	path.len()
}

/// Splits around the first separator, returning the part after it.
fn cut_path(path: &[u8]) -> Option<(&[u8], &[u8])> {
	let i = path.iter().position(|&c| c == b'\\' || c == b'/')?;
	Some((&path[..i], &path[i + 1..]))
}

/// Keeps `Clean` from turning a relative Windows path into an absolute or
/// rooted one (`a/../c:` must not become `c:`).
fn post_clean(out: &mut LazyBuf<'_>) {
	if out.vol_len != 0 {
		return;
	}
	let Some(buf) = &out.buf else {
		return;
	};
	for &c in buf {
		if c == b'\\' || c == b'/' {
			break;
		}
		if c == b':' {
			out.prepend(b".\\");
			return;
		}
	}
	// A path beginning `\??\` would become a Root Local Device path.
	if buf.len() >= 3 && (buf[0] == b'\\' || buf[0] == b'/') && buf[1] == b'?' && buf[2] == b'?' {
		out.prepend(b"\\.");
	}
}

fn join_windows(elems: &[&str]) -> String {
	let mut b = String::new();
	let mut last_char = 0_u8;
	for &element in elems {
		let mut e = element;
		if b.is_empty() {
			// Add the first non-empty element unchanged.
		} else if last_char == b'\\' || last_char == b'/' {
			// After a separator, strip leading separators from the next
			// element so non-UNC elements never create a UNC path.
			e = e.trim_start_matches(['\\', '/']);
			// `\` then `??` must stay `\.\??`, not `\??\`.
			if b.len() == 1 && e.starts_with("??") && (e.len() == 2 || matches!(e.as_bytes()[2], b'\\' | b'/')) {
				b.push_str(".\\");
			}
		} else if last_char == b':' {
			// `C:` then `f` stays `C:f`: relative to the drive's directory.
		} else {
			b.push('\\');
			last_char = b'\\';
		}
		if let Some(&last) = e.as_bytes().last() {
			b.push_str(e);
			last_char = last;
		}
	}
	if b.is_empty() {
		return String::new();
	}
	Flavor::Windows.clean(&b)
}

/// `filepath.FromSlash` for the native platform.
#[must_use]
pub fn from_slash(path: &str) -> Cow<'_, str> {
	NATIVE.from_slash(path)
}

/// `filepath.ToSlash` for the native platform.
#[must_use]
pub fn to_slash(path: &str) -> Cow<'_, str> {
	NATIVE.to_slash(path)
}

/// `filepath.IsAbs` for the native platform.
#[must_use]
pub fn is_abs(path: &str) -> bool {
	NATIVE.is_abs(path)
}

/// `filepath.Clean` for the native platform.
#[must_use]
pub fn clean(path: &str) -> String {
	NATIVE.clean(path)
}

/// `filepath.Clean` for the native platform, borrowing when nothing changes.
#[must_use]
pub fn clean_cow(path: &str) -> Cow<'_, str> {
	NATIVE.clean_cow(path)
}

/// `filepath.Join` for the native platform.
#[must_use]
pub fn join(elems: &[&str]) -> String {
	NATIVE.join(elems)
}

/// `filepath.Dir` for the native platform.
#[must_use]
pub fn dir(path: &str) -> String {
	NATIVE.dir(path)
}

/// `filepath.Base` for the native platform.
#[must_use]
pub fn base(path: &str) -> String {
	NATIVE.base(path)
}

/// `filepath.VolumeName` for the native platform.
#[must_use]
pub fn volume_name(path: &str) -> String {
	NATIVE.volume_name(path)
}

/// `filepath.Rel` for the native platform.
///
/// # Errors
///
/// Returns a [`RelError`] when `target` cannot be made relative to `base`.
pub fn rel(base_path: &str, target_path: &str) -> Result<String, RelError> {
	NATIVE.rel(base_path, target_path)
}

/// `filepath.Abs`: the path unchanged if absolute, else joined to the working
/// directory and cleaned.
///
/// # Errors
///
/// Returns the I/O error from looking up the working directory.
pub fn abs(path: &str) -> io::Result<String> {
	#[cfg(windows)]
	{
		// syscall.FullPath is GetFullPathNameW, which is also what the
		// standard library's `absolute` calls. An empty path means ".".
		let full = std::path::absolute(if path.is_empty() { "." } else { path })?;
		Ok(clean(&full.to_string_lossy()))
	}
	#[cfg(not(windows))]
	{
		if is_abs(path) {
			return Ok(clean(path));
		}
		Ok(join(&[&getwd()?, path]))
	}
}

/// `os.Getwd` on Unix: `$PWD` when it names the current directory (so a
/// symlinked working directory keeps its logical spelling), else the
/// kernel's answer.
#[cfg(not(windows))]
fn getwd() -> io::Result<String> {
	use std::os::unix::fs::MetadataExt;
	if let Some(pwd) = std::env::var_os("PWD").and_then(|pwd| pwd.into_string().ok())
		&& pwd.starts_with('/')
	{
		let dot = std::fs::metadata(".")?;
		if let Ok(d) = std::fs::metadata(&pwd)
			&& d.dev() == dot.dev()
			&& d.ino() == dot.ino()
		{
			return Ok(pwd);
		}
	}
	Ok(std::env::current_dir()?.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests;
