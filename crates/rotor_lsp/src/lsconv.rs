//! `tsgo/ls/lsconv` and `lsproto.DocumentUri`: line maps, UTF-8 or UTF-16 positions, and file
//! URIs.

use crate::protocol::{Position, Range};
use std::fmt::Write as _;

/// `LSPLineMap`: the byte offset of each line's start; only `\n`, `\r` and `\r\n` break lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineMap {
	starts: Vec<usize>,
	ascii_only: bool,
}

impl LineMap {
	/// `ComputeLSPLineStarts`
	#[must_use]
	pub fn new(text: &str) -> Self {
		let bytes = text.as_bytes();
		let mut starts = Vec::with_capacity(bytecount_newlines(bytes) + 1);
		let mut ascii_only = true;
		let (mut position, mut line_start) = (0, 0);
		while position < bytes.len() {
			let byte = bytes[position];
			if byte < 0x80 {
				position += 1;
				if byte == b'\r' && bytes.get(position) == Some(&b'\n') {
					position += 1;
				}
				if byte == b'\r' || byte == b'\n' {
					starts.push(line_start);
					line_start = position;
				}
			} else {
				position += text[position..].chars().next().map_or(1, char::len_utf8);
				ascii_only = false;
			}
		}
		starts.push(line_start);
		Self { starts, ascii_only }
	}
}

#[expect(clippy::naive_bytecount, reason = "a line map is built once per published file")]
fn bytecount_newlines(bytes: &[u8]) -> usize {
	bytes.iter().filter(|&&byte| byte == b'\n').count()
}

/// `Converters.PositionToLineAndCharacter`: a byte offset as a 0-based line and character, the
/// character in UTF-8 bytes or UTF-16 units.
#[must_use]
pub fn position(text: &str, map: &LineMap, utf8: bool, offset: usize) -> Position {
	let mut offset = offset.min(text.len());
	while !text.is_char_boundary(offset) {
		offset -= 1;
	}
	let line = match map.starts.binary_search(&offset) {
		Ok(line) => line,
		Err(insertion) => insertion.saturating_sub(1),
	}
	.min(map.starts.len() - 1);
	let start = map.starts[line];
	let character = if map.ascii_only || utf8 {
		offset - start
	} else {
		text[start..offset].chars().map(char::len_utf16).sum()
	};
	Position {
		line: u32::try_from(line).unwrap_or(u32::MAX),
		character: u32::try_from(character).unwrap_or(u32::MAX),
	}
}

/// `Converters.ToLSPRange`
#[must_use]
pub fn range(text: &str, map: &LineMap, utf8: bool, start: usize, end: usize) -> Range {
	Range {
		start: position(text, map, utf8, start),
		end: position(text, map, utf8, end),
	}
}

/// `tspath.SplitVolumePath`: a leading drive letter, lowercased.
fn split_volume(path: &str) -> (String, &str) {
	let bytes = path.as_bytes();
	if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
		return (path[..2].to_ascii_lowercase(), &path[2..]);
	}
	(String::new(), path)
}

/// `url.PathEscape` followed by vscode-uri's extra escapes: every byte but `[A-Za-z0-9-_.~]`.
fn escape_component(component: &str) -> String {
	let mut out = String::with_capacity(component.len());
	for &byte in component.as_bytes() {
		if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
			out.push(char::from(byte));
		} else {
			let _ = write!(out, "%{byte:02X}");
		}
	}
	out
}

/// `FileNameToDocumentURI`: a slash-separated path as a `file://` URI.
#[must_use]
pub fn file_name_to_document_uri(file_name: &str) -> String {
	if file_name.starts_with("bundled:///") {
		return file_name.to_owned();
	}
	let (volume, rest) = split_volume(file_name);
	let volume = if volume.is_empty() {
		volume
	} else {
		format!("/{}", escape_component(&volume))
	};
	let rest = rest.strip_prefix("//").unwrap_or(rest);
	let parts: Vec<String> = rest.split('/').map(escape_component).collect();
	format!("file://{volume}{}", parts.join("/"))
}

const fn hex(byte: u8) -> Option<u8> {
	match byte {
		b'0'..=b'9' => Some(byte - b'0'),
		b'a'..=b'f' => Some(byte - b'a' + 10),
		b'A'..=b'F' => Some(byte - b'A' + 10),
		_ => None,
	}
}

/// `url.PathUnescape`; `None` for a malformed escape.
fn unescape(text: &str) -> Option<String> {
	let bytes = text.as_bytes();
	let mut out = Vec::with_capacity(bytes.len());
	let mut index = 0;
	while index < bytes.len() {
		if bytes[index] == b'%' {
			let high = hex(*bytes.get(index + 1)?)?;
			let low = hex(*bytes.get(index + 2)?)?;
			out.push(high << 4 | low);
			index += 3;
		} else {
			out.push(bytes[index]);
			index += 1;
		}
	}
	String::from_utf8(out).ok()
}

/// `DocumentUri.FileName` of a `file://` URI (`url.Parse` it, then the host and the unescaped
/// path, with a Windows drive's leading slash dropped); `None` when it does not parse.
#[must_use]
pub fn document_uri_to_file_name(uri: &str) -> Option<String> {
	let rest = uri.strip_prefix("file://")?;
	// The fragment, then the query, are not part of the path.
	let rest = rest.split_once('#').map_or(rest, |(path, _)| path);
	let rest = rest.split_once('?').map_or(rest, |(path, _)| path);
	let (host, path) = rest
		.find('/')
		.map_or((rest, ""), |slash| (&rest[..slash], &rest[slash..]));
	let host = unescape(host)?;
	let path = unescape(path)?;
	if !host.is_empty() {
		return Some(format!("//{host}{path}"));
	}
	if let Some(after) = path.strip_prefix('/') {
		let (volume, rest) = split_volume(after);
		if !volume.is_empty() {
			return Some(format!("{volume}{rest}"));
		}
	}
	Some(path)
}

#[cfg(test)]
mod tests {
	use super::{LineMap, document_uri_to_file_name, file_name_to_document_uri, position};

	#[test]
	fn positions_count_utf16_units_or_bytes() {
		let text = "// héllo\nconst s = \"\u{1F600}\"; let x: any;";
		let map = LineMap::new(text);
		let offset = text.find("any").expect("any");
		let utf16 = position(text, &map, false, offset);
		assert_eq!((utf16.line, utf16.character), (1, 23));
		let utf8 = position(text, &map, true, offset);
		assert_eq!((utf8.line, utf8.character), (1, 25));
		assert_eq!(position(text, &map, false, 10_000).line, 1);
	}

	#[test]
	fn crlf_and_cr_break_lines() {
		let map = LineMap::new("a\r\nb\rc\nd");
		let text = "a\r\nb\rc\nd";
		assert_eq!(position(text, &map, false, 3).line, 1);
		assert_eq!(position(text, &map, false, 5).line, 2);
		assert_eq!(position(text, &map, false, 7).line, 3);
	}

	#[test]
	fn uris_escape_like_vscode_uri() {
		assert_eq!(
			file_name_to_document_uri("/tmp/a b/c#d+é.ts"),
			"file:///tmp/a%20b/c%23d%2B%C3%A9.ts"
		);
		assert_eq!(file_name_to_document_uri("C:/x/y.ts"), "file:///c%3A/x/y.ts");
		assert_eq!(
			document_uri_to_file_name("file:///tmp/a%20b/c%23d%2B%C3%A9.ts").as_deref(),
			Some("/tmp/a b/c#d+é.ts")
		);
		assert_eq!(
			document_uri_to_file_name("file:///c%3A/x/y.ts").as_deref(),
			Some("c:/x/y.ts")
		);
		assert_eq!(
			document_uri_to_file_name("file://server/share/a.ts").as_deref(),
			Some("//server/share/a.ts")
		);
		assert_eq!(document_uri_to_file_name("file:///bad%zz.ts"), None);
	}
}
