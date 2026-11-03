//! An insertion-ordered JSON reader and a Go-compatible writer.
//!
//! Key order matters for Rojo projects because the tree walk visits
//! `Object.keys(tree)`, which drives partition ordering, so a map that
//! forgets insertion order cannot be used. The reader accepts exactly what
//! Go's `encoding/json` decoder accepts and keeps numbers as their source
//! text, since cache timestamps do not fit an `f64`.

use std::collections::HashMap;
use std::fmt::Write;

/// How deeply the reader nests before it gives up and reports invalid JSON.
/// Go's token reader has no limit; this one keeps the recursion off the stack
/// limit.
const MAX_DEPTH: usize = 1000;

/// A parsed JSON value.
#[derive(Clone, Debug, PartialEq)]
pub enum JsonValue {
	/// What `JSON.parse` leaves after throwing: upstream's `try`/`finally`
	/// still runs the validator over `undefined`, producing the "Invalid
	/// configuration!" warning.
	Invalid,
	/// `null`.
	Null,
	/// `true` or `false`.
	Bool(bool),
	/// A number, with its source text.
	Number {
		/// The value as a double.
		value: f64,
		/// The number exactly as written.
		raw: String,
	},
	/// A string.
	String(String),
	/// An array.
	Array(Vec<Self>),
	/// An object, with members in JavaScript property order.
	Object(Vec<(String, Self)>),
}

impl JsonValue {
	/// An integer value, as the writer emits it.
	#[must_use]
	#[expect(
		clippy::cast_precision_loss,
		reason = "the double is only the approximate value; `raw` is exact"
	)]
	pub fn integer(n: i64) -> Self {
		Self::Number {
			value: n as f64,
			raw: n.to_string(),
		}
	}

	/// The member named `key` when this is an object.
	#[must_use]
	pub fn member(&self, key: &str) -> Option<&Self> {
		match self {
			Self::Object(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
			_ => None,
		}
	}

	/// The text of a string value.
	#[must_use]
	pub fn as_str(&self) -> Option<&str> {
		match self {
			Self::String(s) => Some(s),
			_ => None,
		}
	}

	/// The elements of an array value.
	#[must_use]
	pub fn as_array(&self) -> Option<&[Self]> {
		match self {
			Self::Array(elements) => Some(elements),
			_ => None,
		}
	}

	/// An integer value, when this is a number written as an integer that
	/// fits an `i64`.
	#[must_use]
	pub fn as_i64(&self) -> Option<i64> {
		match self {
			Self::Number { raw, .. } => raw.parse().ok(),
			_ => None,
		}
	}

	/// Go's `json.MarshalIndent(v, "", "\t")` for this value.
	#[must_use]
	pub fn to_indented_string(&self) -> String {
		let mut out = String::new();
		self.write_indented(&mut out, 0);
		out
	}

	fn write_indented(&self, out: &mut String, depth: usize) {
		match self {
			Self::Invalid | Self::Null => out.push_str("null"),
			Self::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
			Self::Number { raw, .. } => out.push_str(raw),
			Self::String(s) => write_string(out, s),
			Self::Array(elements) if elements.is_empty() => out.push_str("[]"),
			Self::Array(elements) => {
				out.push('[');
				for (i, element) in elements.iter().enumerate() {
					if i > 0 {
						out.push(',');
					}
					out.push('\n');
					push_tabs(out, depth + 1);
					element.write_indented(out, depth + 1);
				}
				out.push('\n');
				push_tabs(out, depth);
				out.push(']');
			}
			Self::Object(members) if members.is_empty() => out.push_str("{}"),
			Self::Object(members) => {
				out.push('{');
				for (i, (key, value)) in members.iter().enumerate() {
					if i > 0 {
						out.push(',');
					}
					out.push('\n');
					push_tabs(out, depth + 1);
					write_string(out, key);
					out.push_str(": ");
					value.write_indented(out, depth + 1);
				}
				out.push('\n');
				push_tabs(out, depth);
				out.push('}');
			}
		}
	}
}

fn push_tabs(out: &mut String, depth: usize) {
	for _ in 0..depth {
		out.push('\t');
	}
}

/// Appends `s` as a JSON string the way `encoding/json` does, including its
/// HTML escaping of `<`, `>` and `&`.
fn write_string(out: &mut String, s: &str) {
	out.push('"');
	for c in s.chars() {
		match c {
			'"' => out.push_str("\\\""),
			'\\' => out.push_str("\\\\"),
			'\x08' => out.push_str("\\b"),
			'\x0c' => out.push_str("\\f"),
			'\n' => out.push_str("\\n"),
			'\r' => out.push_str("\\r"),
			'\t' => out.push_str("\\t"),
			c if (c as u32) < 0x20 || matches!(c, '<' | '>' | '&' | '\u{2028}' | '\u{2029}') => {
				let _ = write!(out, "\\u{:04x}", c as u32);
			}
			c => out.push(c),
		}
	}
	out.push('"');
}

/// Parses `data`, preserving object key order. Any syntax error yields
/// [`JsonValue::Invalid`], as `JSON.parse` would have thrown.
#[must_use]
pub fn parse(data: &[u8]) -> JsonValue {
	let mut parser = Parser { bytes: data, at: 0 };
	let Some(value) = parser.value(0) else {
		return JsonValue::Invalid;
	};
	parser.skip_space();
	if parser.at == data.len() {
		value
	} else {
		JsonValue::Invalid // trailing garbage
	}
}

struct Parser<'a> {
	bytes: &'a [u8],
	at: usize,
}

impl Parser<'_> {
	fn peek(&self) -> Option<u8> {
		self.bytes.get(self.at).copied()
	}

	fn skip_space(&mut self) {
		while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
			self.at += 1;
		}
	}

	fn eat(&mut self, c: u8) -> bool {
		if self.peek() == Some(c) {
			self.at += 1;
			true
		} else {
			false
		}
	}

	fn literal(&mut self, text: &str) -> Option<()> {
		let end = self.at + text.len();
		if self.bytes.get(self.at..end)? == text.as_bytes() {
			self.at = end;
			Some(())
		} else {
			None
		}
	}

	fn value(&mut self, depth: usize) -> Option<JsonValue> {
		if depth > MAX_DEPTH {
			return None;
		}
		self.skip_space();
		match self.peek()? {
			b'{' => {
				self.at += 1;
				self.object(depth)
			}
			b'[' => {
				self.at += 1;
				self.array(depth)
			}
			b'"' => self.string().map(JsonValue::String),
			b't' => self.literal("true").map(|()| JsonValue::Bool(true)),
			b'f' => self.literal("false").map(|()| JsonValue::Bool(false)),
			b'n' => self.literal("null").map(|()| JsonValue::Null),
			b'-' | b'0'..=b'9' => self.number(),
			_ => None,
		}
	}

	fn object(&mut self, depth: usize) -> Option<JsonValue> {
		let mut members: Vec<(String, JsonValue)> = Vec::new();
		let mut index: HashMap<String, usize> = HashMap::new();
		self.skip_space();
		if self.eat(b'}') {
			return Some(JsonValue::Object(members));
		}
		loop {
			self.skip_space();
			if self.peek()? != b'"' {
				return None;
			}
			let key = self.string()?;
			self.skip_space();
			if !self.eat(b':') {
				return None;
			}
			let value = self.value(depth + 1)?;
			// Duplicate keys: the later value wins and the original position
			// is kept, as JavaScript property assignment does.
			if let Some(&i) = index.get(&key) {
				members[i].1 = value;
			} else {
				index.insert(key.clone(), members.len());
				members.push((key, value));
			}
			self.skip_space();
			if self.eat(b',') {
				continue;
			}
			if self.eat(b'}') {
				return Some(JsonValue::Object(order_like_js(members)));
			}
			return None;
		}
	}

	fn array(&mut self, depth: usize) -> Option<JsonValue> {
		let mut elements = Vec::new();
		self.skip_space();
		if self.eat(b']') {
			return Some(JsonValue::Array(elements));
		}
		loop {
			elements.push(self.value(depth + 1)?);
			self.skip_space();
			if self.eat(b',') {
				continue;
			}
			if self.eat(b']') {
				return Some(JsonValue::Array(elements));
			}
			return None;
		}
	}

	/// Reads the number grammar `-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?`.
	fn number(&mut self) -> Option<JsonValue> {
		let start = self.at;
		self.eat(b'-');
		match self.peek()? {
			b'0' => self.at += 1,
			b'1'..=b'9' => self.digits(),
			_ => return None,
		}
		if self.eat(b'.') {
			if !self.peek()?.is_ascii_digit() {
				return None;
			}
			self.digits();
		}
		if matches!(self.peek(), Some(b'e' | b'E')) {
			self.at += 1;
			if matches!(self.peek(), Some(b'+' | b'-')) {
				self.at += 1;
			}
			if !self.peek()?.is_ascii_digit() {
				return None;
			}
			self.digits();
		}
		let raw = std::str::from_utf8(&self.bytes[start..self.at]).ok()?;
		let value: f64 = raw.parse().ok()?;
		// strconv.ParseFloat reports a range error for an overflowing literal.
		if value.is_infinite() {
			return None;
		}
		Some(JsonValue::Number {
			value,
			raw: raw.to_owned(),
		})
	}

	fn digits(&mut self) {
		while self.peek().is_some_and(|c| c.is_ascii_digit()) {
			self.at += 1;
		}
	}

	fn hex4(&self, at: usize) -> Option<u32> {
		let digits = self.bytes.get(at..at + 4)?;
		let mut n = 0;
		for &d in digits {
			n = n * 16 + char::from(d).to_digit(16)?;
		}
		Some(n)
	}

	/// Reads a string literal starting at its opening quote, decoding escapes
	/// and replacing each invalid UTF-8 byte with U+FFFD, as Go does.
	fn string(&mut self) -> Option<String> {
		self.at += 1; // opening quote
		let mut out = Vec::new();
		loop {
			let c = self.peek()?;
			match c {
				b'"' => {
					self.at += 1;
					return Some(decode_utf8_lossy_per_byte(&out));
				}
				b'\\' => {
					self.at += 1;
					self.escape(&mut out)?;
				}
				0..=0x1f => return None,
				_ => {
					out.push(c);
					self.at += 1;
				}
			}
		}
	}

	fn escape(&mut self, out: &mut Vec<u8>) -> Option<()> {
		let c = self.peek()?;
		self.at += 1;
		let simple = match c {
			b'"' => b'"',
			b'\\' => b'\\',
			b'/' => b'/',
			b'b' => 0x08,
			b'f' => 0x0c,
			b'n' => b'\n',
			b'r' => b'\r',
			b't' => b'\t',
			b'u' => {
				let first = self.hex4(self.at)?;
				self.at += 4;
				let decoded = if (0xd800..0xdc00).contains(&first) {
					// A high surrogate pairs with an immediately following
					// `\u` low surrogate; otherwise it becomes U+FFFD.
					let low = (self.bytes.get(self.at..self.at + 2) == Some(b"\\u"))
						.then(|| self.hex4(self.at + 2))
						.flatten()
						.filter(|low| (0xdc00..0xe000).contains(low));
					match low {
						Some(low) => {
							self.at += 6;
							char::from_u32(0x1_0000 + ((first - 0xd800) << 10) + (low - 0xdc00))
						}
						None => None,
					}
				} else {
					char::from_u32(first)
				};
				let mut buf = [0; 4];
				out.extend_from_slice(decoded.unwrap_or('\u{fffd}').encode_utf8(&mut buf).as_bytes());
				return Some(());
			}
			_ => return None,
		};
		out.push(simple);
		Some(())
	}
}

/// Decodes UTF-8, turning every invalid byte into one U+FFFD, which is what
/// Go's string decoding does (Rust's lossy decoding merges a truncated
/// sequence into one replacement).
fn decode_utf8_lossy_per_byte(bytes: &[u8]) -> String {
	let mut out = String::with_capacity(bytes.len());
	let mut rest = bytes;
	loop {
		match std::str::from_utf8(rest) {
			Ok(valid) => {
				out.push_str(valid);
				return out;
			}
			Err(error) => {
				let (valid, after) = rest.split_at(error.valid_up_to());
				out.push_str(&String::from_utf8_lossy(valid));
				out.push('\u{fffd}');
				rest = &after[1..];
			}
		}
	}
}

/// Reorders object members to JavaScript own-property order: array-index-like
/// keys first in ascending numeric order, then the remaining string keys in
/// insertion order. The tree walk observes this order, which feeds partition
/// ordering.
fn order_like_js(members: Vec<(String, JsonValue)>) -> Vec<(String, JsonValue)> {
	if !members.iter().any(|(key, _)| array_index(key).is_some()) {
		return members;
	}
	let (mut indexed, strings): (Vec<_>, Vec<_>) = members.into_iter().partition(|(key, _)| array_index(key).is_some());
	indexed.sort_by_key(|(key, _)| array_index(key));
	indexed.extend(strings);
	indexed
}

/// The value of `key` when it is a canonical ECMAScript array index (an
/// integer in `0..=2^32-2` in canonical decimal form).
fn array_index(key: &str) -> Option<u64> {
	if !key.bytes().all(|b| b.is_ascii_digit()) {
		return None;
	}
	let n: u64 = key.parse().ok()?;
	(n <= 4_294_967_294 && n.to_string() == key).then_some(n)
}

#[cfg(test)]
mod tests;
