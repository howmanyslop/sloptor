//! `json_syntax.go`: JavaScript-style JSON syntax errors, plus the parts of Go's `encoding/json`
//! this package observes.
//!
//! Go's scanner is ported state for state ([`Scanner`]), so syntax error messages and offsets,
//! `json.Valid`, `json.Compact` and `json.Indent` match Go byte for byte. [`Value`] is a decoded
//! document that keeps each value's raw bytes (Go's `json.RawMessage`) and object members in
//! source order.

use std::fmt::Write as _;

use crate::{Error, ErrorDetail};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Scan {
	Continue,
	BeginLiteral,
	BeginObject,
	ObjectKey,
	ObjectValue,
	EndObject,
	BeginArray,
	ArrayValue,
	EndArray,
	SkipSpace,
	End,
	Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Parse {
	ObjectKey,
	ObjectValue,
	ArrayValue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
	BeginValueOrEmpty,
	BeginValue,
	BeginStringOrEmpty,
	BeginString,
	EndValue,
	EndTop,
	InString,
	InStringEsc,
	InStringEscU,
	InStringEscU1,
	InStringEscU12,
	InStringEscU123,
	Neg,
	One,
	Zero,
	Dot,
	Dot0,
	E,
	ESign,
	E0,
	T,
	Tr,
	Tru,
	F,
	Fa,
	Fal,
	Fals,
	N,
	Nu,
	Nul,
	Error,
}

const MAX_NESTING_DEPTH: usize = 10_000;

/// Go's `encoding/json` scanner.
struct Scanner {
	step: Step,
	end_top: bool,
	parse_state: Vec<Parse>,
	err: Option<(String, usize)>,
	bytes: usize,
}

const fn is_space(c: u8) -> bool {
	matches!(c, b' ' | b'\t' | b'\r' | b'\n')
}

const fn is_hex(c: u8) -> bool {
	c.is_ascii_hexdigit()
}

/// Go's `quoteChar`.
fn quote_char(c: u8) -> String {
	if c == b'\'' {
		return r"'\''".to_owned();
	}
	if c == b'"' {
		return "'\"'".to_owned();
	}
	let quoted = rotor_support::gostr::quote(&char::from(c).to_string());
	format!("'{}'", &quoted[1..quoted.len() - 1])
}

impl Scanner {
	const fn new() -> Self {
		Self {
			step: Step::BeginValue,
			end_top: false,
			parse_state: Vec::new(),
			err: None,
			bytes: 0,
		}
	}

	fn error(&mut self, c: u8, context: &str) -> Scan {
		self.step = Step::Error;
		self.err = Some((format!("invalid character {} {context}", quote_char(c)), self.bytes));
		Scan::Error
	}

	fn eof(&mut self) -> Scan {
		if self.err.is_some() {
			return Scan::Error;
		}
		if self.end_top {
			return Scan::End;
		}
		self.run(b' ');
		if self.end_top {
			return Scan::End;
		}
		if self.err.is_none() {
			self.err = Some(("unexpected end of JSON input".to_owned(), self.bytes));
		}
		Scan::Error
	}

	fn push_parse_state(&mut self, c: u8, state: Parse, success: Scan) -> Scan {
		self.parse_state.push(state);
		if self.parse_state.len() <= MAX_NESTING_DEPTH {
			return success;
		}
		self.error(c, "exceeded max depth")
	}

	fn pop_parse_state(&mut self) {
		self.parse_state.pop();
		if self.parse_state.is_empty() {
			self.step = Step::EndTop;
			self.end_top = true;
		} else {
			self.step = Step::EndValue;
		}
	}

	fn run(&mut self, c: u8) -> Scan {
		self.dispatch(self.step, c)
	}

	#[expect(clippy::too_many_lines, reason = "one arm per state of Go's scanner")]
	fn dispatch(&mut self, step: Step, c: u8) -> Scan {
		match step {
			Step::BeginValueOrEmpty => {
				if is_space(c) {
					return Scan::SkipSpace;
				}
				if c == b']' {
					return self.dispatch(Step::EndValue, c);
				}
				self.dispatch(Step::BeginValue, c)
			}
			Step::BeginValue => {
				if is_space(c) {
					return Scan::SkipSpace;
				}
				match c {
					b'{' => {
						self.step = Step::BeginStringOrEmpty;
						self.push_parse_state(c, Parse::ObjectKey, Scan::BeginObject)
					}
					b'[' => {
						self.step = Step::BeginValueOrEmpty;
						self.push_parse_state(c, Parse::ArrayValue, Scan::BeginArray)
					}
					b'"' => self.literal(Step::InString),
					b'-' => self.literal(Step::Neg),
					b'0' => self.literal(Step::Zero),
					b't' => self.literal(Step::T),
					b'f' => self.literal(Step::F),
					b'n' => self.literal(Step::N),
					b'1'..=b'9' => self.literal(Step::One),
					_ => self.error(c, "looking for beginning of value"),
				}
			}
			Step::BeginStringOrEmpty => {
				if is_space(c) {
					return Scan::SkipSpace;
				}
				if c == b'}' {
					if let Some(last) = self.parse_state.last_mut() {
						*last = Parse::ObjectValue;
					}
					return self.dispatch(Step::EndValue, c);
				}
				self.dispatch(Step::BeginString, c)
			}
			Step::BeginString => {
				if is_space(c) {
					return Scan::SkipSpace;
				}
				if c == b'"' {
					return self.literal(Step::InString);
				}
				self.error(c, "looking for beginning of object key string")
			}
			Step::EndValue => {
				let Some(&state) = self.parse_state.last() else {
					self.step = Step::EndTop;
					self.end_top = true;
					return self.dispatch(Step::EndTop, c);
				};
				if is_space(c) {
					self.step = Step::EndValue;
					return Scan::SkipSpace;
				}
				match state {
					Parse::ObjectKey => {
						if c == b':' {
							if let Some(last) = self.parse_state.last_mut() {
								*last = Parse::ObjectValue;
							}
							self.step = Step::BeginValue;
							return Scan::ObjectKey;
						}
						self.error(c, "after object key")
					}
					Parse::ObjectValue => {
						if c == b',' {
							if let Some(last) = self.parse_state.last_mut() {
								*last = Parse::ObjectKey;
							}
							self.step = Step::BeginString;
							return Scan::ObjectValue;
						}
						if c == b'}' {
							self.pop_parse_state();
							return Scan::EndObject;
						}
						self.error(c, "after object key:value pair")
					}
					Parse::ArrayValue => {
						if c == b',' {
							self.step = Step::BeginValue;
							return Scan::ArrayValue;
						}
						if c == b']' {
							self.pop_parse_state();
							return Scan::EndArray;
						}
						self.error(c, "after array element")
					}
				}
			}
			Step::EndTop => {
				if !is_space(c) {
					self.error(c, "after top-level value");
				}
				Scan::End
			}
			Step::InString => {
				if c == b'"' {
					self.step = Step::EndValue;
					return Scan::Continue;
				}
				if c == b'\\' {
					self.step = Step::InStringEsc;
					return Scan::Continue;
				}
				if c < 0x20 {
					return self.error(c, "in string literal");
				}
				Scan::Continue
			}
			Step::InStringEsc => match c {
				b'b' | b'f' | b'n' | b'r' | b't' | b'\\' | b'/' | b'"' => self.cont(Step::InString),
				b'u' => self.cont(Step::InStringEscU),
				_ => self.error(c, "in string escape code"),
			},
			Step::InStringEscU | Step::InStringEscU1 | Step::InStringEscU12 | Step::InStringEscU123 => {
				if is_hex(c) {
					let next = match step {
						Step::InStringEscU => Step::InStringEscU1,
						Step::InStringEscU1 => Step::InStringEscU12,
						Step::InStringEscU12 => Step::InStringEscU123,
						_ => Step::InString,
					};
					return self.cont(next);
				}
				self.error(c, "in \\u hexadecimal character escape")
			}
			Step::Neg => match c {
				b'0' => self.cont(Step::Zero),
				b'1'..=b'9' => self.cont(Step::One),
				_ => self.error(c, "in numeric literal"),
			},
			Step::One => {
				if c.is_ascii_digit() {
					return self.cont(Step::One);
				}
				self.dispatch(Step::Zero, c)
			}
			Step::Zero => match c {
				b'.' => self.cont(Step::Dot),
				b'e' | b'E' => self.cont(Step::E),
				_ => self.dispatch(Step::EndValue, c),
			},
			Step::Dot => {
				if c.is_ascii_digit() {
					return self.cont(Step::Dot0);
				}
				self.error(c, "after decimal point in numeric literal")
			}
			Step::Dot0 => {
				if c.is_ascii_digit() {
					return Scan::Continue;
				}
				if c == b'e' || c == b'E' {
					return self.cont(Step::E);
				}
				self.dispatch(Step::EndValue, c)
			}
			Step::E => {
				if c == b'+' || c == b'-' {
					return self.cont(Step::ESign);
				}
				self.dispatch(Step::ESign, c)
			}
			Step::ESign => {
				if c.is_ascii_digit() {
					return self.cont(Step::E0);
				}
				self.error(c, "in exponent of numeric literal")
			}
			Step::E0 => {
				if c.is_ascii_digit() {
					return Scan::Continue;
				}
				self.dispatch(Step::EndValue, c)
			}
			Step::T => self.expect(c, b'r', Step::Tr, "in literal true (expecting 'r')"),
			Step::Tr => self.expect(c, b'u', Step::Tru, "in literal true (expecting 'u')"),
			Step::Tru => self.expect(c, b'e', Step::EndValue, "in literal true (expecting 'e')"),
			Step::F => self.expect(c, b'a', Step::Fa, "in literal false (expecting 'a')"),
			Step::Fa => self.expect(c, b'l', Step::Fal, "in literal false (expecting 'l')"),
			Step::Fal => self.expect(c, b's', Step::Fals, "in literal false (expecting 's')"),
			Step::Fals => self.expect(c, b'e', Step::EndValue, "in literal false (expecting 'e')"),
			Step::N => self.expect(c, b'u', Step::Nu, "in literal null (expecting 'u')"),
			Step::Nu => self.expect(c, b'l', Step::Nul, "in literal null (expecting 'l')"),
			Step::Nul => self.expect(c, b'l', Step::EndValue, "in literal null (expecting 'l')"),
			Step::Error => Scan::Error,
		}
	}

	const fn literal(&mut self, next: Step) -> Scan {
		self.step = next;
		Scan::BeginLiteral
	}

	const fn cont(&mut self, next: Step) -> Scan {
		self.step = next;
		Scan::Continue
	}

	fn expect(&mut self, c: u8, want: u8, next: Step, context: &str) -> Scan {
		if c == want {
			return self.cont(next);
		}
		self.error(c, context)
	}
}

/// Go's `checkValid`: the syntax error message and offset, if `data` is not one JSON value.
fn check_valid(data: &[u8]) -> Option<(String, usize)> {
	let mut scanner = Scanner::new();
	for &c in data {
		scanner.bytes += 1;
		if scanner.run(c) == Scan::Error {
			return scanner.err;
		}
	}
	if scanner.eof() == Scan::Error {
		return scanner.err;
	}
	None
}

/// `json.Valid`
#[must_use]
pub fn valid(data: &[u8]) -> bool {
	check_valid(data).is_none()
}

/// `validateJSONSyntax`: a JavaScript `JSON.parse`-style message for a syntax error.
///
/// # Errors
///
/// `data` is not one JSON value; the error carries [`ErrorDetail::JsonSyntax`].
pub fn validate_json_syntax(data: &[u8]) -> Result<(), Error> {
	let Some((mut message, position)) = check_valid(data) else {
		return Ok(());
	};
	let (line, column) = json_line_and_column(data, position);
	let trimmed = data.trim_ascii();
	if message == "unexpected end of JSON input" && trimmed.last() == Some(&b'{') {
		"Expected property name or '}'".clone_into(&mut message);
	}
	Err(Error::with_detail(
		format!("SyntaxError: {message} in JSON at position {position} (line {line} column {column})"),
		ErrorDetail::JsonSyntax,
	))
}

fn json_line_and_column(data: &[u8], position: usize) -> (usize, usize) {
	let (mut line, mut column) = (1, 1);
	for &byte in data.iter().take(position) {
		if byte == b'\n' {
			line += 1;
			column = 1;
			continue;
		}
		column += 1;
	}
	(line, column)
}

const HEX: &[u8; 16] = b"0123456789abcdef";

/// `json.Compact` (`escape` false) or the compaction `json.Marshal` applies to a `RawMessage`
/// (`escape` true). `None` if `src` is not valid JSON.
#[must_use]
pub fn compact(src: &[u8], escape: bool) -> Option<Vec<u8>> {
	let mut dst = Vec::with_capacity(src.len());
	let mut scanner = Scanner::new();
	let mut start = 0;
	for (i, &c) in src.iter().enumerate() {
		if escape && matches!(c, b'<' | b'>' | b'&') {
			if start < i {
				dst.extend_from_slice(&src[start..i]);
			}
			dst.extend_from_slice(&[
				b'\\',
				b'u',
				b'0',
				b'0',
				HEX[usize::from(c >> 4)],
				HEX[usize::from(c & 0xF)],
			]);
			start = i + 1;
		}
		if escape && c == 0xE2 && i + 2 < src.len() && src[i + 1] == 0x80 && src[i + 2] & !1 == 0xA8 {
			if start < i {
				dst.extend_from_slice(&src[start..i]);
			}
			dst.extend_from_slice(&[b'\\', b'u', b'2', b'0', b'2', HEX[usize::from(src[i + 2] & 0xF)]]);
			start = i + 3;
		}
		scanner.bytes += 1;
		let v = scanner.run(c);
		if v >= Scan::SkipSpace {
			if v == Scan::Error {
				break;
			}
			if start < i {
				dst.extend_from_slice(&src[start..i]);
			}
			start = i + 1;
		}
	}
	if scanner.eof() == Scan::Error {
		return None;
	}
	if start < src.len() {
		dst.extend_from_slice(&src[start..]);
	}
	Some(dst)
}

/// `json.Indent(dst, src, prefix, indent)`. `None` if `src` is not valid JSON.
#[must_use]
pub fn indent(src: &[u8], prefix: &str, indent: &str) -> Option<Vec<u8>> {
	fn newline(dst: &mut Vec<u8>, prefix: &str, indent: &str, depth: usize) {
		dst.push(b'\n');
		dst.extend_from_slice(prefix.as_bytes());
		for _ in 0..depth {
			dst.extend_from_slice(indent.as_bytes());
		}
	}
	let mut dst = Vec::with_capacity(src.len() * 2);
	let mut scanner = Scanner::new();
	let mut need_indent = false;
	let mut depth = 0usize;
	for &c in src {
		scanner.bytes += 1;
		let v = scanner.run(c);
		if v == Scan::SkipSpace {
			continue;
		}
		if v == Scan::Error {
			break;
		}
		if need_indent && v != Scan::EndObject && v != Scan::EndArray {
			need_indent = false;
			depth += 1;
			newline(&mut dst, prefix, indent, depth);
		}
		if v == Scan::Continue {
			dst.push(c);
			continue;
		}
		match c {
			b'{' | b'[' => {
				need_indent = true;
				dst.push(c);
			}
			b',' => {
				dst.push(c);
				newline(&mut dst, prefix, indent, depth);
			}
			b':' => dst.extend_from_slice(b": "),
			b'}' | b']' => {
				if need_indent {
					need_indent = false;
				} else {
					depth = depth.saturating_sub(1);
					newline(&mut dst, prefix, indent, depth);
				}
				dst.push(c);
			}
			_ => dst.push(c),
		}
	}
	if scanner.eof() == Scan::Error {
		return None;
	}
	Some(dst)
}

/// Appends `text` as Go's encoder writes a string, with or without HTML escaping.
pub fn write_string(out: &mut String, text: &str, escape_html: bool) {
	out.push('"');
	for c in text.chars() {
		match c {
			'"' => out.push_str("\\\""),
			'\\' => out.push_str("\\\\"),
			'\n' => out.push_str("\\n"),
			'\r' => out.push_str("\\r"),
			'\t' => out.push_str("\\t"),
			'\u{8}' => out.push_str("\\b"),
			'\u{c}' => out.push_str("\\f"),
			'<' | '>' | '&' if escape_html => {
				let _ = write!(out, "\\u{:04x}", u32::from(c));
			}
			'\u{2028}' | '\u{2029}' => {
				let _ = write!(out, "\\u{:04x}", u32::from(c));
			}
			c if u32::from(c) < 0x20 => {
				let _ = write!(out, "\\u{:04x}", u32::from(c));
			}
			c => out.push(c),
		}
	}
	out.push('"');
}

/// A string as Go's encoder writes it.
#[must_use]
pub fn string(text: &str, escape_html: bool) -> String {
	let mut out = String::with_capacity(text.len() + 2);
	write_string(&mut out, text, escape_html);
	out
}

/// A `float64` as `json.Marshal` writes it.
#[must_use]
pub fn format_float(value: f64) -> String {
	let abs = value.abs();
	if abs != 0.0 && !(1e-6..1e21).contains(&abs) {
		let text = format!("{value:e}");
		// Go writes a signed exponent of at least two digits for positive exponents.
		if let Some(index) = text.find('e') {
			let (mantissa, exponent) = text.split_at(index);
			let exponent = &exponent[1..];
			if let Some(negative) = exponent.strip_prefix('-') {
				return format!("{mantissa}e-{negative}");
			}
			return format!("{mantissa}e+{exponent:0>2}");
		}
		return text;
	}
	format!("{value}")
}

/// A decoded JSON value that keeps its raw bytes.
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
	/// An object's members in source order, duplicates kept, each with its raw value bytes.
	Object(Vec<Member>),
}

/// One object member.
#[derive(Clone, Debug, PartialEq)]
pub struct Member {
	/// The key.
	pub key: String,
	/// The decoded value.
	pub value: Value,
	/// The value's raw bytes (`json.RawMessage`).
	pub raw: Vec<u8>,
}

impl Value {
	/// Go's name for the JSON kind in an `UnmarshalTypeError`.
	#[must_use]
	pub const fn kind_name(&self) -> &'static str {
		match self {
			Self::Null => "null",
			Self::Bool(_) => "bool",
			Self::Number(_) => "number",
			Self::String(_) => "string",
			Self::Array(_) => "array",
			Self::Object(_) => "object",
		}
	}

	/// The members, if an object.
	#[must_use]
	pub fn as_object(&self) -> Option<&[Member]> {
		match self {
			Self::Object(members) => Some(members),
			_ => None,
		}
	}

	/// The member a Go struct field named `key` decodes: the last exact match, else the last
	/// case-insensitive match.
	#[must_use]
	pub fn field(&self, key: &str) -> Option<&Self> {
		let members = self.as_object()?;
		members
			.iter()
			.rev()
			.find(|member| member.key == key)
			.or_else(|| members.iter().rev().find(|member| member.key.eq_ignore_ascii_case(key)))
			.map(|member| &member.value)
	}

	/// The members as a Go `map[string]json.RawMessage` decodes them: the last of each key, in
	/// first-seen order.
	#[must_use]
	pub fn map_members(&self) -> Vec<&Member> {
		let Some(members) = self.as_object() else {
			return Vec::new();
		};
		let mut out: Vec<&Member> = Vec::with_capacity(members.len());
		for member in members {
			if let Some(existing) = out.iter_mut().find(|existing| existing.key == member.key) {
				*existing = member;
			} else {
				out.push(member);
			}
		}
		out
	}
}

/// `json.Unmarshal` into a generic tree. `data` must be valid JSON (checked first).
///
/// # Errors
///
/// The Go syntax error message.
pub fn parse(data: &[u8]) -> Result<Value, String> {
	if let Some((message, _)) = check_valid(data) {
		return Err(message);
	}
	let mut parser = Parser { bytes: data, at: 0 };
	Ok(parser.value())
}

/// `json.Unmarshal(data, &string)`: `None` for `null` (Go leaves the target unchanged).
///
/// # Errors
///
/// The Go error for invalid JSON or a non-string value.
pub fn unmarshal_string(data: &[u8], into: &str) -> Result<Option<String>, String> {
	match parse(data)? {
		Value::String(text) => Ok(Some(text)),
		Value::Null => Ok(None),
		other => Err(type_error(&other, into)),
	}
}

/// Go's `UnmarshalTypeError` text.
#[must_use]
pub fn type_error(value: &Value, go_type: &str) -> String {
	let what = match value {
		Value::Number(number) => format!("number {number}"),
		other => other.kind_name().to_owned(),
	};
	let what = if matches!(value, Value::Number(_)) && go_type == "string" {
		"number".to_owned()
	} else {
		what
	};
	format!("json: cannot unmarshal {what} into Go value of type {go_type}")
}

struct Parser<'a> {
	bytes: &'a [u8],
	at: usize,
}

impl Parser<'_> {
	const fn skip_space(&mut self) {
		while self.at < self.bytes.len() && is_space(self.bytes[self.at]) {
			self.at += 1;
		}
	}

	fn peek(&self) -> u8 {
		self.bytes.get(self.at).copied().unwrap_or(0)
	}

	fn value(&mut self) -> Value {
		self.skip_space();
		match self.peek() {
			b'{' => {
				self.at += 1;
				let mut members = Vec::new();
				loop {
					self.skip_space();
					if self.peek() == b'}' {
						self.at += 1;
						break;
					}
					if self.peek() == b',' {
						self.at += 1;
						continue;
					}
					let key = self.string();
					self.skip_space();
					// The colon.
					self.at += 1;
					self.skip_space();
					let start = self.at;
					let value = self.value();
					let raw = self.bytes[start..self.at].to_vec();
					members.push(Member { key, value, raw });
				}
				Value::Object(members)
			}
			b'[' => {
				self.at += 1;
				let mut elements = Vec::new();
				loop {
					self.skip_space();
					if self.peek() == b']' {
						self.at += 1;
						break;
					}
					if self.peek() == b',' {
						self.at += 1;
						continue;
					}
					elements.push(self.value());
				}
				Value::Array(elements)
			}
			b'"' => Value::String(self.string()),
			b't' => {
				self.at += 4;
				Value::Bool(true)
			}
			b'f' => {
				self.at += 5;
				Value::Bool(false)
			}
			b'n' => {
				self.at += 4;
				Value::Null
			}
			_ => {
				let start = self.at;
				while self.at < self.bytes.len()
					&& matches!(self.bytes[self.at], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
				{
					self.at += 1;
				}
				Value::Number(String::from_utf8_lossy(&self.bytes[start..self.at]).into_owned())
			}
		}
	}

	fn string(&mut self) -> String {
		// The opening quote.
		self.at += 1;
		let mut out: Vec<u8> = Vec::new();
		while self.at < self.bytes.len() {
			let byte = self.bytes[self.at];
			self.at += 1;
			match byte {
				b'"' => break,
				b'\\' => {
					let escape = self.peek();
					self.at += 1;
					match escape {
						b'b' => out.push(0x08),
						b'f' => out.push(0x0c),
						b'n' => out.push(b'\n'),
						b'r' => out.push(b'\r'),
						b't' => out.push(b'\t'),
						b'u' => {
							let first = self.hex4();
							let character =
								if (0xD800..0xDC00).contains(&first) && self.bytes[self.at..].starts_with(b"\\u") {
									let save = self.at;
									self.at += 2;
									let second = self.hex4();
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
						other => out.push(other),
					}
				}
				_ => out.push(byte),
			}
		}
		String::from_utf8_lossy(&out).into_owned()
	}

	fn hex4(&mut self) -> u32 {
		let digits = self.bytes.get(self.at..self.at + 4).unwrap_or_default();
		self.at += 4;
		std::str::from_utf8(digits)
			.ok()
			.and_then(|text| u32::from_str_radix(text, 16).ok())
			.unwrap_or(0xFFFD)
	}
}
