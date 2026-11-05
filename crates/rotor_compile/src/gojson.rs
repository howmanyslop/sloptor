//! Go's `encoding/json`, the parts `internal/compile` observes.
//!
//! Not a Go file of its own: the reader keeps object members in source order with duplicates (Go's
//! token stream), and the writer escapes strings and orders struct fields the way `json.Marshal`
//! does, so files and hashes rotor writes match Go's bytes.

use std::fmt::Write as _;

/// A decoded JSON value, members in source order, duplicates kept.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
	/// `null`
	Null,
	/// `true` or `false`
	Bool(bool),
	/// A number, as written.
	Number(String),
	/// A string.
	String(String),
	/// An array.
	Array(Vec<Self>),
	/// An object.
	Object(Vec<(String, Self)>),
}

impl Value {
	/// The string, if this is one.
	#[must_use]
	pub fn as_str(&self) -> Option<&str> {
		match self {
			Self::String(text) => Some(text),
			_ => None,
		}
	}

	/// The members, if this is an object.
	#[must_use]
	pub fn as_object(&self) -> Option<&[(String, Self)]> {
		match self {
			Self::Object(members) => Some(members),
			_ => None,
		}
	}

	/// The elements, if this is an array.
	#[must_use]
	pub fn as_array(&self) -> Option<&[Self]> {
		match self {
			Self::Array(elements) => Some(elements),
			_ => None,
		}
	}

	/// The member a Go struct field named `key` decodes: the last member whose key matches
	/// exactly, else the last that matches case-insensitively (`json.Unmarshal`'s rule).
	#[must_use]
	pub fn field(&self, key: &str) -> Option<&Self> {
		let members = self.as_object()?;
		members
			.iter()
			.rev()
			.find(|(name, _)| name == key)
			.or_else(|| members.iter().rev().find(|(name, _)| name.eq_ignore_ascii_case(key)))
			.map(|(_, value)| value)
	}

	/// The member named `key` exactly (a Go `map[string]any` lookup: the last duplicate wins).
	#[must_use]
	pub fn get(&self, key: &str) -> Option<&Self> {
		self.as_object()?
			.iter()
			.rev()
			.find(|(name, _)| name == key)
			.map(|(_, value)| value)
	}
}

/// Parses one JSON document. Trailing non-space text is an error, as in `json.Unmarshal`.
///
/// # Errors
///
/// A syntax error, described.
pub fn parse(data: &[u8]) -> Result<Value, String> {
	let mut parser = Parser { bytes: data, at: 0 };
	let value = parser.value(0)?;
	parser.skip_space();
	if parser.at != data.len() {
		return Err(format!(
			"invalid character {:?} after top-level value",
			char::from(data[parser.at])
		));
	}
	Ok(value)
}

const MAX_DEPTH: usize = 10_000;

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

	fn error<T>(&self) -> Result<T, String> {
		Err(self.peek().map_or_else(
			|| "unexpected end of JSON input".to_owned(),
			|byte| {
				format!(
					"invalid character {:?} looking for beginning of value",
					char::from(byte)
				)
			},
		))
	}

	fn eat(&mut self, byte: u8) -> bool {
		if self.peek() == Some(byte) {
			self.at += 1;
			return true;
		}
		false
	}

	fn literal(&mut self, text: &[u8]) -> bool {
		if self.bytes[self.at..].starts_with(text) {
			self.at += text.len();
			return true;
		}
		false
	}

	fn value(&mut self, depth: usize) -> Result<Value, String> {
		if depth > MAX_DEPTH {
			return Err("exceeded max depth".to_owned());
		}
		self.skip_space();
		match self.peek() {
			Some(b'{') => {
				self.at += 1;
				self.object(depth)
			}
			Some(b'[') => {
				self.at += 1;
				self.array(depth)
			}
			Some(b'"') => self.string().map(Value::String),
			Some(b't') if self.literal(b"true") => Ok(Value::Bool(true)),
			Some(b'f') if self.literal(b"false") => Ok(Value::Bool(false)),
			Some(b'n') if self.literal(b"null") => Ok(Value::Null),
			Some(b'-' | b'0'..=b'9') => self.number(),
			_ => self.error(),
		}
	}

	fn object(&mut self, depth: usize) -> Result<Value, String> {
		let mut members = Vec::new();
		self.skip_space();
		if self.eat(b'}') {
			return Ok(Value::Object(members));
		}
		loop {
			self.skip_space();
			if self.peek() != Some(b'"') {
				return self.error();
			}
			let key = self.string()?;
			self.skip_space();
			if !self.eat(b':') {
				return self.error();
			}
			let value = self.value(depth + 1)?;
			members.push((key, value));
			self.skip_space();
			if self.eat(b',') {
				continue;
			}
			if self.eat(b'}') {
				return Ok(Value::Object(members));
			}
			return self.error();
		}
	}

	fn array(&mut self, depth: usize) -> Result<Value, String> {
		let mut elements = Vec::new();
		self.skip_space();
		if self.eat(b']') {
			return Ok(Value::Array(elements));
		}
		loop {
			elements.push(self.value(depth + 1)?);
			self.skip_space();
			if self.eat(b',') {
				continue;
			}
			if self.eat(b']') {
				return Ok(Value::Array(elements));
			}
			return self.error();
		}
	}

	fn number(&mut self) -> Result<Value, String> {
		let start = self.at;
		self.eat(b'-');
		match self.peek() {
			Some(b'0') => self.at += 1,
			Some(b'1'..=b'9') => {
				while matches!(self.peek(), Some(b'0'..=b'9')) {
					self.at += 1;
				}
			}
			_ => return self.error(),
		}
		if self.eat(b'.') {
			if !matches!(self.peek(), Some(b'0'..=b'9')) {
				return self.error();
			}
			while matches!(self.peek(), Some(b'0'..=b'9')) {
				self.at += 1;
			}
		}
		if matches!(self.peek(), Some(b'e' | b'E')) {
			self.at += 1;
			if matches!(self.peek(), Some(b'+' | b'-')) {
				self.at += 1;
			}
			if !matches!(self.peek(), Some(b'0'..=b'9')) {
				return self.error();
			}
			while matches!(self.peek(), Some(b'0'..=b'9')) {
				self.at += 1;
			}
		}
		Ok(Value::Number(
			String::from_utf8_lossy(&self.bytes[start..self.at]).into_owned(),
		))
	}

	fn string(&mut self) -> Result<String, String> {
		// The opening quote.
		self.at += 1;
		let mut out: Vec<u8> = Vec::new();
		loop {
			let Some(byte) = self.peek() else {
				return Err("unexpected end of JSON input".to_owned());
			};
			self.at += 1;
			match byte {
				b'"' => return Ok(String::from_utf8_lossy(&out).into_owned()),
				b'\\' => {
					let Some(escape) = self.peek() else {
						return Err("unexpected end of JSON input".to_owned());
					};
					self.at += 1;
					match escape {
						b'"' => out.push(b'"'),
						b'\\' => out.push(b'\\'),
						b'/' => out.push(b'/'),
						b'b' => out.push(0x08),
						b'f' => out.push(0x0c),
						b'n' => out.push(b'\n'),
						b'r' => out.push(b'\r'),
						b't' => out.push(b'\t'),
						b'u' => {
							let first = self.hex4()?;
							let character =
								if (0xD800..0xDC00).contains(&first) && self.bytes[self.at..].starts_with(b"\\u") {
									let save = self.at;
									self.at += 2;
									let second = self.hex4()?;
									if (0xDC00..0xE000).contains(&second) {
										char::from_u32(0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00))
									} else {
										self.at = save;
										None
									}
								} else {
									char::from_u32(first)
								};
							let mut buffer = [0; 4];
							out.extend_from_slice(
								character
									.unwrap_or(char::REPLACEMENT_CHARACTER)
									.encode_utf8(&mut buffer)
									.as_bytes(),
							);
						}
						_ => {
							return Err(format!(
								"invalid character {:?} in string escape code",
								char::from(escape)
							));
						}
					}
				}
				0x00..=0x1F => {
					return Err(format!("invalid character {:?} in string literal", char::from(byte)));
				}
				_ => out.push(byte),
			}
		}
	}

	fn hex4(&mut self) -> Result<u32, String> {
		let digits = self
			.bytes
			.get(self.at..self.at + 4)
			.ok_or_else(|| "unexpected end of JSON input".to_owned())?;
		let text = std::str::from_utf8(digits).map_err(|error| error.to_string())?;
		let value = u32::from_str_radix(text, 16)
			.map_err(|_| "invalid character in \\u hexadecimal character escape".to_owned())?;
		self.at += 4;
		Ok(value)
	}
}

/// Appends `text` as `json.Marshal` writes a string: HTML-safe, with U+2028 and U+2029
/// escaped and invalid UTF-8 already replaced.
pub fn write_string(out: &mut String, text: &str) {
	write_string_escaping(out, text, true);
}

/// Appends `text` as a `json.Encoder` writes a string; `escape_html` is its `SetEscapeHTML`.
pub fn write_string_escaping(out: &mut String, text: &str, escape_html: bool) {
	out.push('"');
	for character in text.chars() {
		match character {
			'"' => out.push_str("\\\""),
			'\\' => out.push_str("\\\\"),
			'\n' => out.push_str("\\n"),
			'\r' => out.push_str("\\r"),
			'\t' => out.push_str("\\t"),
			'\x08' => out.push_str("\\b"),
			'\x0c' => out.push_str("\\f"),
			character
				if u32::from(character) < 0x20
					|| matches!(character, '\u{2028}' | '\u{2029}')
					|| (escape_html && matches!(character, '<' | '>' | '&')) =>
			{
				let _ = write!(out, "\\u{:04x}", u32::from(character));
			}
			character => out.push(character),
		}
	}
	out.push('"');
}

/// `json.Marshal` of a string.
#[must_use]
pub fn string(text: &str) -> String {
	let mut out = String::with_capacity(text.len() + 2);
	write_string(&mut out, text);
	out
}

/// Builds a JSON object member by member, in the order written (a Go struct's field order).
#[derive(Debug, Default)]
pub struct Object {
	out: String,
	first: bool,
}

impl Object {
	/// An empty object.
	#[must_use]
	pub fn new() -> Self {
		Self {
			out: String::from("{"),
			first: true,
		}
	}

	/// Adds a member whose value is already JSON.
	#[must_use]
	pub fn raw(mut self, key: &str, value: &str) -> Self {
		if !self.first {
			self.out.push(',');
		}
		self.first = false;
		write_string(&mut self.out, key);
		self.out.push(':');
		self.out.push_str(value);
		self
	}

	/// Adds a string member.
	#[must_use]
	pub fn string(self, key: &str, value: &str) -> Self {
		let value = string(value);
		self.raw(key, &value)
	}

	/// Adds a string member unless it is empty (`omitempty`).
	#[must_use]
	pub fn string_omitempty(self, key: &str, value: &str) -> Self {
		if value.is_empty() {
			return self;
		}
		self.string(key, value)
	}

	/// Adds a boolean member.
	#[must_use]
	pub fn bool(self, key: &str, value: bool) -> Self {
		self.raw(key, if value { "true" } else { "false" })
	}

	/// Adds a boolean member unless it is false (`omitempty`).
	#[must_use]
	pub fn bool_omitempty(self, key: &str, value: bool) -> Self {
		if !value {
			return self;
		}
		self.bool(key, value)
	}

	/// Adds an integer member.
	#[must_use]
	pub fn int(self, key: &str, value: i64) -> Self {
		self.raw(key, &value.to_string())
	}

	/// The object's JSON.
	#[must_use]
	pub fn finish(mut self) -> String {
		self.out.push('}');
		self.out
	}
}

/// A JSON array of already-encoded values.
#[must_use]
pub fn array<I: IntoIterator<Item = String>>(values: I) -> String {
	let mut out = String::from("[");
	for (index, value) in values.into_iter().enumerate() {
		if index > 0 {
			out.push(',');
		}
		out.push_str(&value);
	}
	out.push(']');
	out
}

/// A JSON array of strings; `null` for Go's nil slice when `nil` is true.
#[must_use]
pub fn string_array(values: &[String]) -> String {
	array(values.iter().map(|value| string(value)))
}

/// A Go `map[string]string` as `json.Marshal` writes it: keys sorted.
#[must_use]
pub fn string_map<'a, I: IntoIterator<Item = (&'a String, &'a String)>>(entries: I) -> String {
	let mut entries: Vec<(&String, &String)> = entries.into_iter().collect();
	entries.sort_by(|left, right| left.0.cmp(right.0));
	let mut object = Object::new();
	for (key, value) in entries {
		object = object.string(key, value);
	}
	object.finish()
}

/// Re-indents compact JSON the way `json.MarshalIndent(value, prefix, indent)` lays it out.
#[must_use]
pub fn indent(compact: &str, prefix: &str, indent: &str) -> String {
	let mut out = String::with_capacity(compact.len() * 2);
	let mut depth = 0usize;
	let mut in_string = false;
	let mut escaped = false;
	let bytes = compact.as_bytes();
	let newline = |out: &mut String, depth: usize| {
		out.push('\n');
		out.push_str(prefix);
		for _ in 0..depth {
			out.push_str(indent);
		}
	};
	let mut index = 0;
	while index < bytes.len() {
		let character = compact[index..].chars().next().unwrap_or('\0');
		let width = character.len_utf8();
		if in_string {
			out.push(character);
			if escaped {
				escaped = false;
			} else if character == '\\' {
				escaped = true;
			} else if character == '"' {
				in_string = false;
			}
			index += width;
			continue;
		}
		match character {
			'"' => {
				in_string = true;
				out.push('"');
			}
			'{' | '[' => {
				let closing = if character == '{' { b'}' } else { b']' };
				out.push(character);
				if bytes.get(index + 1) == Some(&closing) {
					out.push(char::from(closing));
					index += 2;
					continue;
				}
				depth += 1;
				newline(&mut out, depth);
			}
			'}' | ']' => {
				depth = depth.saturating_sub(1);
				newline(&mut out, depth);
				out.push(character);
			}
			',' => {
				out.push(',');
				newline(&mut out, depth);
			}
			':' => out.push_str(": "),
			character => out.push(character),
		}
		index += width;
	}
	out
}
