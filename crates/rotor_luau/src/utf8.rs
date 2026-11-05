//! Go's view of byte strings that may not be valid UTF-8.
//!
//! Rendered output is bytes, as in Go: a `$file` string literal keeps the raw bytes of a
//! non-UTF-8 text file. These helpers decode such bytes the way Go's `unicode/utf8` does, where
//! every byte that does not start a valid sequence is one `RuneError` of width 1.

/// `utf8.DecodeRune`: the first rune of `bytes` and its width in bytes. `None` is Go's
/// `RuneError` for an invalid byte, whose width is always 1. Empty input is `(None, 0)`.
#[must_use]
pub fn decode_rune(bytes: &[u8]) -> (Option<char>, usize) {
	let Some(&first) = bytes.first() else {
		return (None, 0);
	};
	if first.is_ascii() {
		return (Some(char::from(first)), 1);
	}
	let width = match first {
		0xC2..=0xDF => 2,
		0xE0..=0xEF => 3,
		0xF0..=0xF4 => 4,
		_ => return (None, 1),
	};
	match bytes.get(..width).map(core::str::from_utf8) {
		Some(Ok(text)) => (text.chars().next(), width),
		_ => (None, 1),
	}
}

/// Iterates the runes of `bytes` the way Go's `for range` over a string does: each item is the
/// rune (`None` for an invalid byte) and its width.
pub fn runes(bytes: &[u8]) -> impl Iterator<Item = (Option<char>, usize)> + '_ {
	let mut offset = 0;
	core::iter::from_fn(move || {
		let rest = bytes.get(offset..)?;
		if rest.is_empty() {
			return None;
		}
		let (rune, width) = decode_rune(rest);
		offset += width;
		Some((rune, width))
	})
}

/// `utf8.RuneCount`: invalid bytes count one each.
#[must_use]
pub fn rune_count(bytes: &[u8]) -> usize {
	if let Ok(text) = core::str::from_utf8(bytes) {
		return text.chars().count();
	}
	runes(bytes).count()
}

/// The UTF-16 length Go's encoders give a rune: 2 above U+FFFF, else 1 (`RuneError` included).
#[must_use]
pub fn utf16_units(rune: Option<char>) -> u32 {
	match rune {
		Some(character) if u32::from(character) > 0xFFFF => 2,
		_ => 1,
	}
}

/// `bytes` as a `String`, replacing invalid sequences with U+FFFD, without copying valid input.
#[must_use]
pub fn into_string_lossy(bytes: Vec<u8>) -> String {
	match String::from_utf8(bytes) {
		Ok(text) => text,
		Err(error) => String::from_utf8_lossy(error.as_bytes()).into_owned(),
	}
}

/// Whether `haystack` contains `needle` as a byte substring.
#[must_use]
pub fn contains(haystack: &[u8], needle: &[u8]) -> bool {
	needle.is_empty() || haystack.windows(needle.len()).any(|window| window == needle)
}
