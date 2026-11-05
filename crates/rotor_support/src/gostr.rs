//! Go-compatible text formatting for messages the CLI prints verbatim.

use std::fmt::Write;
use std::io;

/// Go's `%q` for a string (`strconv.Quote`): a double-quoted literal with
/// Go's escapes.
///
/// Go decides which runes are printable with its own Unicode tables. This
/// treats every control character and every non-space whitespace character as
/// unprintable, which agrees for the text rotor quotes (paths, keys, option
/// values).
#[must_use]
pub fn quote(s: &str) -> String {
	let mut out = String::with_capacity(s.len() + 2);
	out.push('"');
	for c in s.chars() {
		match c {
			'"' => out.push_str("\\\""),
			'\\' => out.push_str("\\\\"),
			'\x07' => out.push_str("\\a"),
			'\x08' => out.push_str("\\b"),
			'\x0c' => out.push_str("\\f"),
			'\n' => out.push_str("\\n"),
			'\r' => out.push_str("\\r"),
			'\t' => out.push_str("\\t"),
			'\x0b' => out.push_str("\\v"),
			' ' => out.push(' '),
			c if (c as u32) < 0x20 || c as u32 == 0x7f => {
				let _ = write!(out, "\\x{:02x}", c as u32);
			}
			c if c.is_control() || c.is_whitespace() => {
				if (c as u32) <= 0xffff {
					let _ = write!(out, "\\u{:04x}", c as u32);
				} else {
					let _ = write!(out, "\\U{:08x}", c as u32);
				}
			}
			c => out.push(c),
		}
	}
	out.push('"');
	out
}

/// The text Go's `syscall.Errno` gives for an OS error.
///
/// That is the platform's message without Rust's ` (os error N)` suffix,
/// lowercased on Unix where Go's table is lowercase (`no such file or
/// directory`) and left alone on Windows where Go uses the system message
/// as-is.
#[must_use]
pub fn io_error_text(err: &io::Error) -> String {
	let full = err.to_string();
	let text = full.rfind(" (os error ").map_or(full.as_str(), |i| &full[..i]);
	if cfg!(windows) {
		return text.to_owned();
	}
	let mut chars = text.chars();
	chars
		.next()
		.map_or_else(String::new, |first| first.to_lowercase().chain(chars).collect())
}

/// Go's `*fs.PathError` text: `<op> <path>: <error>`, as in
/// `open /x/rotor.toml: permission denied`.
#[must_use]
pub fn path_error(op: &str, path: &str, err: &io::Error) -> String {
	format!("{op} {path}: {}", io_error_text(err))
}

/// An `io::Error` of the same kind whose text is Go's `*fs.PathError` ([`path_error`]), for
/// callers that report [`io_error_text`] and would otherwise lose the path.
#[must_use]
pub fn with_path(op: &str, path: &str, err: &io::Error) -> io::Error {
	io::Error::new(err.kind(), path_error(op, path, err))
}

#[cfg(test)]
mod tests;
