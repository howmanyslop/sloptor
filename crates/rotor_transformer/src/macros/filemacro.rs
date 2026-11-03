//! `filemacro.go`: rotor's `$file` compile-time macro (no rbxtsc counterpart).
//!
//! ```ts
//! $file("config.json") // { Volume = 0.8, Name = "Game" } (a Luau table)
//! $file("./notes.txt") // "raw text contents" (a Luau string)
//! ```
//!
//! The string-literal path resolves from the project root, or from the importing file when it
//! begins with `./` or `../`. A `.json` file becomes a Luau value literal, anything else a string
//! literal of its text. Same interception point as `$env`.
//!
//! Go decodes the JSON with `encoding/json` (`Decoder.UseNumber`); [`json_bytes_to_luau`] is a
//! port of the parts of that decoder the macro observes: the accepted grammar, the first error
//! and its message, the trailing-value check, and the value conversion.

use std::collections::BTreeMap;

use super::{is_macro_node, string_literal_text};
use crate::call::{ChainItem, ChainItemKind};
use crate::diagnostics;
use crate::optionalchain::transform_optional_chain_inner;
use crate::prelude::*;

/// `fileMacroSymbol` (`filemacro.go`): the global `$file` symbol.
pub fn file_macro_symbol(state: &mut State<'_>) -> Option<P<Symbol>> {
	state.ambient_symbol("$file")
}

/// `hasJSONSuffix`: a case-insensitive `.json` suffix.
fn has_json_suffix(path: &str) -> bool {
	let bytes = path.as_bytes();
	bytes.len() >= 5 && bytes[bytes.len() - 5..].eq_ignore_ascii_case(b".json")
}

/// `resolveFileExpression`: reads `path` through the pass's file resolver and parses it, or adds
/// a diagnostic and returns `None`. `node` is the call expression.
fn resolve_file_expression(state: &mut State<'_>, node: P<Node>, path: &str) -> NodeId {
	let Some(files) = state.files else {
		// no resolver attached (unit tests): a clear diagnostic, never a panic
		state.diags.add(diagnostics::rotor_file_no_resolver(node));
		return state.ast.none();
	};
	let importer = state.source_file.file_name();
	let Ok(data) = files.read(importer, path) else {
		// Go reports ErrFileNotFound and every other read error alike
		state.diags.add(diagnostics::rotor_file_not_found(node, path));
		return state.ast.none();
	};

	if has_json_suffix(path) {
		return match json_bytes_to_luau(&mut state.ast, &data) {
			Ok(value) => value,
			Err(message) => {
				state
					.diags
					.add(diagnostics::rotor_file_invalid_json(node, path, &message));
				state.ast.none()
			}
		};
	}
	// Non-JSON text file: the raw contents become a single Luau string literal. Go keeps invalid
	// UTF-8 bytes verbatim; a Rust string replaces them with U+FFFD.
	state.ast.str(String::from_utf8_lossy(&data))
}

/// `interceptFileChain`: `None` when the chain's base is not `$file`. Otherwise the leading call
/// folds to the parsed file value and the rest of the chain continues from it.
pub fn intercept_file_chain(state: &mut State<'_>, chain: &[ChainItem], expression: P<Node>) -> Option<NodeId> {
	let item = chain.first()?;
	if !is_macro_node(state, expression, "$file") {
		return None;
	}

	// a bare property or element access off $file has no such surface; any other argument count
	// is unreachable after typecheck
	let (ChainItemKind::Call, &[argument]) = (item.kind, item.args) else {
		state.diags.add(diagnostics::rotor_file_bad_usage(item.node));
		return Some(state.ast.none());
	};
	let Some(path) = string_literal_text(argument) else {
		state.diags.add(diagnostics::rotor_file_non_literal_arg(argument));
		return Some(state.ast.none());
	};

	let value = resolve_file_expression(state, item.node, path);

	// Continue any remaining chain links off the inlined value, e.g. `$file("config.json").Volume`.
	Some(transform_optional_chain_inner(state, chain, value, None, 1))
}

/// A decoded JSON value (Go's `any` from `encoding/json` with `UseNumber`).
#[derive(Debug)]
enum JsonValue {
	Null,
	Bool(bool),
	/// `json.Number`: the source digits.
	Number(String),
	String(String),
	Array(Vec<Self>),
	/// Go's `map[string]any`: a repeated key keeps its last value.
	Object(BTreeMap<String, Self>),
}

/// `jsonBytesToLuau`: decodes the first JSON value of `data` and converts it to a Luau literal.
/// A second complete value after it is an error; anything else after it is ignored, as in Go,
/// whose check only fails when the trailing value decodes.
pub(super) fn json_bytes_to_luau(ast: &mut Ast, data: &[u8]) -> Result<NodeId, String> {
	let mut decoder = JsonDecoder { data, pos: 0 };
	let value = decoder.decode()?;
	// Reject trailing garbage after the first JSON value so malformed files surface a diagnostic
	// rather than silently inlining a prefix.
	if decoder.more() && decoder.decode().is_ok() {
		return Err("unexpected trailing JSON value".to_owned());
	}
	Ok(json_value_to_luau(ast, value))
}

/// `jsonValueToLuau`: objects become maps (keys sorted, `null` members dropped), arrays become
/// arrays (`null` elements become `nil`), numbers keep their source digits.
fn json_value_to_luau(ast: &mut Ast, value: JsonValue) -> NodeId {
	match value {
		JsonValue::Object(members) => {
			let fields = ast.new_list();
			for (key, member) in members {
				if matches!(member, JsonValue::Null) {
					// a Luau table field set to nil is absent, so the key is dropped
					continue;
				}
				let index = ast.str(key);
				let value = json_value_to_luau(ast, member);
				let field = ast.map_field(index, value);
				ast.list_push(fields, field);
			}
			ast.map(fields)
		}
		JsonValue::Array(elements) => {
			let members = ast.new_list();
			for element in elements {
				let member = json_value_to_luau(ast, element);
				ast.list_push(members, member);
			}
			ast.array(members)
		}
		JsonValue::String(text) => ast.str(text),
		JsonValue::Number(digits) => ast.number_literal(digits),
		JsonValue::Bool(value) => ast.bool(value),
		JsonValue::Null => ast.nil(),
	}
}

/// `encoding/json`'s `maxNestingDepth`.
const MAX_NESTING_DEPTH: usize = 10000;

/// Go's `json.Decoder` over an in-memory reader: [`Self::decode`] reads one value, leaving the
/// position after it.
struct JsonDecoder<'a> {
	data: &'a [u8],
	pos: usize,
}

/// `isSpace` (`encoding/json/scanner.go`).
const fn is_space(byte: u8) -> bool {
	matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

/// `quoteChar` (`scanner.go`): the byte as Go's error messages quote it.
fn quote_char(byte: u8) -> String {
	match byte {
		b'\'' => r"'\''".to_owned(),
		b'"' => "'\"'".to_owned(),
		_ => format!("'{}'", go_quote_rune(char::from(byte))),
	}
}

/// The body of `strconv.Quote(string(rune))` for a rune at most U+00FF.
fn go_quote_rune(character: char) -> String {
	match character {
		'\u{7}' => r"\a".to_owned(),
		'\u{8}' => r"\b".to_owned(),
		'\u{c}' => r"\f".to_owned(),
		'\n' => r"\n".to_owned(),
		'\r' => r"\r".to_owned(),
		'\t' => r"\t".to_owned(),
		'\u{b}' => r"\v".to_owned(),
		'\\' => r"\\".to_owned(),
		'"' => "\\\"".to_owned(),
		// strconv.IsPrint: the Latin-1 controls, NBSP and the soft hyphen are not printable
		'\0'..='\u{1f}' | '\u{7f}' => format!(r"\x{:02x}", u32::from(character)),
		'\u{80}'..='\u{a0}' | '\u{ad}' => format!(r"\u{:04x}", u32::from(character)),
		_ => character.to_string(),
	}
}

/// A `*json.SyntaxError` message.
fn syntax_error(byte: u8, context: &str) -> String {
	format!("invalid character {} {context}", quote_char(byte))
}

impl JsonDecoder<'_> {
	fn peek(&self) -> Option<u8> {
		self.data.get(self.pos).copied()
	}

	fn skip_space(&mut self) {
		while self.peek().is_some_and(is_space) {
			self.pos += 1;
		}
	}

	/// The error for the end of input inside a value: `io.ErrUnexpectedEOF`.
	fn unexpected_eof() -> String {
		"unexpected EOF".to_owned()
	}

	/// `Decoder.Decode`: `EOF` at the end of input, else one value.
	fn decode(&mut self) -> Result<JsonValue, String> {
		self.skip_space();
		if self.peek().is_none() {
			return Err("EOF".to_owned());
		}
		self.value(0)
	}

	/// `Decoder.More`: another value follows (the next non-space byte is not `]` or `}`).
	fn more(&mut self) -> bool {
		self.skip_space();
		self.peek().is_some_and(|byte| byte != b']' && byte != b'}')
	}

	/// `stateBeginValue`, then the value. `depth` counts the enclosing objects and arrays.
	fn value(&mut self, depth: usize) -> Result<JsonValue, String> {
		self.skip_space();
		let Some(byte) = self.peek() else {
			return Err(Self::unexpected_eof());
		};
		match byte {
			b'{' | b'[' => {
				if depth + 1 > MAX_NESTING_DEPTH {
					return Err(syntax_error(byte, "exceeded max depth"));
				}
				self.pos += 1;
				if byte == b'{' {
					self.object(depth + 1)
				} else {
					self.array(depth + 1)
				}
			}
			b'"' => {
				self.pos += 1;
				self.string().map(JsonValue::String)
			}
			b'-' | b'0'..=b'9' => self.number().map(JsonValue::Number),
			b't' => self.literal("true").map(|()| JsonValue::Bool(true)),
			b'f' => self.literal("false").map(|()| JsonValue::Bool(false)),
			b'n' => self.literal("null").map(|()| JsonValue::Null),
			_ => Err(syntax_error(byte, "looking for beginning of value")),
		}
	}

	/// After `{`.
	fn object(&mut self, depth: usize) -> Result<JsonValue, String> {
		let mut members = BTreeMap::new();
		self.skip_space();
		if self.peek() == Some(b'}') {
			self.pos += 1;
			return Ok(JsonValue::Object(members));
		}
		loop {
			// stateBeginString
			self.skip_space();
			match self.peek() {
				None => return Err(Self::unexpected_eof()),
				Some(b'"') => self.pos += 1,
				Some(byte) => return Err(syntax_error(byte, "looking for beginning of object key string")),
			}
			let key = self.string()?;
			self.skip_space();
			match self.peek() {
				None => return Err(Self::unexpected_eof()),
				Some(b':') => self.pos += 1,
				Some(byte) => return Err(syntax_error(byte, "after object key")),
			}
			let value = self.value(depth)?;
			members.insert(key, value);
			self.skip_space();
			match self.peek() {
				None => return Err(Self::unexpected_eof()),
				Some(b',') => self.pos += 1,
				Some(b'}') => {
					self.pos += 1;
					return Ok(JsonValue::Object(members));
				}
				Some(byte) => return Err(syntax_error(byte, "after object key:value pair")),
			}
		}
	}

	/// After `[`.
	fn array(&mut self, depth: usize) -> Result<JsonValue, String> {
		let mut elements = Vec::new();
		self.skip_space();
		if self.peek() == Some(b']') {
			self.pos += 1;
			return Ok(JsonValue::Array(elements));
		}
		loop {
			elements.push(self.value(depth)?);
			self.skip_space();
			match self.peek() {
				None => return Err(Self::unexpected_eof()),
				Some(b',') => self.pos += 1,
				Some(b']') => {
					self.pos += 1;
					return Ok(JsonValue::Array(elements));
				}
				Some(byte) => return Err(syntax_error(byte, "after array element")),
			}
		}
	}

	/// `stateT`/`stateF`/`stateN` and their successors: the rest of `word` after its first byte.
	fn literal(&mut self, word: &str) -> Result<(), String> {
		self.pos += 1;
		for expected in word.bytes().skip(1) {
			match self.peek() {
				None => return Err(Self::unexpected_eof()),
				Some(byte) if byte == expected => self.pos += 1,
				Some(byte) => {
					let context = format!("in literal {word} (expecting {})", quote_char(expected));
					return Err(syntax_error(byte, &context));
				}
			}
		}
		Ok(())
	}

	/// A run of digits.
	fn digits(&mut self) {
		while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
			self.pos += 1;
		}
	}

	/// One digit, or `context`'s error.
	fn digit(&mut self, context: &str) -> Result<(), String> {
		match self.peek() {
			None => Err(Self::unexpected_eof()),
			Some(byte) if byte.is_ascii_digit() => {
				self.pos += 1;
				Ok(())
			}
			Some(byte) => Err(syntax_error(byte, context)),
		}
	}

	/// `stateNeg` through `stateE0`: the number's source text. A number ends at the first byte
	/// that cannot continue it.
	fn number(&mut self) -> Result<String, String> {
		let start = self.pos;
		if self.peek() == Some(b'-') {
			self.pos += 1;
		}
		match self.peek() {
			None => return Err(Self::unexpected_eof()),
			Some(b'0') => self.pos += 1,
			Some(b'1'..=b'9') => {
				self.pos += 1;
				self.digits();
			}
			Some(byte) => return Err(syntax_error(byte, "in numeric literal")),
		}
		if self.peek() == Some(b'.') {
			self.pos += 1;
			self.digit("after decimal point in numeric literal")?;
			self.digits();
		}
		if matches!(self.peek(), Some(b'e' | b'E')) {
			self.pos += 1;
			if matches!(self.peek(), Some(b'+' | b'-')) {
				self.pos += 1;
			}
			self.digit("in exponent of numeric literal")?;
			self.digits();
		}
		Ok(String::from_utf8_lossy(&self.data[start..self.pos]).into_owned())
	}

	/// Four hex digits of a `\u` escape.
	fn hex4(&mut self) -> Result<u32, String> {
		let mut value = 0;
		for _ in 0..4 {
			let Some(byte) = self.peek() else {
				return Err(Self::unexpected_eof());
			};
			let Some(digit) = char::from(byte).to_digit(16) else {
				return Err(syntax_error(byte, "in \\u hexadecimal character escape"));
			};
			value = value * 16 + digit;
			self.pos += 1;
		}
		Ok(value)
	}

	/// After the opening quote: the unquoted string (`unquoteBytes`). Invalid UTF-8 bytes and
	/// unpaired surrogate escapes become U+FFFD, one per byte or escape.
	fn string(&mut self) -> Result<String, String> {
		let mut out = String::new();
		loop {
			let Some(byte) = self.peek() else {
				return Err(Self::unexpected_eof());
			};
			match byte {
				b'"' => {
					self.pos += 1;
					return Ok(out);
				}
				b'\\' => {
					self.pos += 1;
					self.escape(&mut out)?;
				}
				0..=0x1f => return Err(syntax_error(byte, "in string literal")),
				0x20..=0x7f => {
					out.push(char::from(byte));
					self.pos += 1;
				}
				_ => {
					let width = utf8_width(byte);
					let decoded = self
						.data
						.get(self.pos..self.pos + width)
						.and_then(|bytes| std::str::from_utf8(bytes).ok());
					if let Some(text) = decoded {
						out.push_str(text);
						self.pos += width;
					} else {
						out.push(char::REPLACEMENT_CHARACTER);
						self.pos += 1;
					}
				}
			}
		}
	}

	/// After a backslash (`stateInStringEsc`).
	fn escape(&mut self, out: &mut String) -> Result<(), String> {
		let Some(byte) = self.peek() else {
			return Err(Self::unexpected_eof());
		};
		self.pos += 1;
		let character = match byte {
			b'"' => '"',
			b'\\' => '\\',
			b'/' => '/',
			b'b' => '\u{8}',
			b'f' => '\u{c}',
			b'n' => '\n',
			b'r' => '\r',
			b't' => '\t',
			b'u' => {
				let code = self.hex4()?;
				if (0xd800..0xe000).contains(&code) {
					// utf16.IsSurrogate: a following low-surrogate escape completes the pair;
					// otherwise the escape decodes alone, to U+FFFD
					self.low_surrogate(code).unwrap_or(char::REPLACEMENT_CHARACTER)
				} else {
					char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER)
				}
			}
			_ => return Err(syntax_error(byte, "in string escape code")),
		};
		out.push(character);
		Ok(())
	}

	/// `utf16.DecodeRune(high, <the next \uXXXX>)`: consumes the next escape only when the pair is
	/// valid. The scanner has already accepted the whole string, so a malformed next escape here
	/// only means "no pair".
	fn low_surrogate(&mut self, high: u32) -> Option<char> {
		if !(0xd800..0xdc00).contains(&high) {
			return None;
		}
		let [b'\\', b'u', hex @ ..] = self.data.get(self.pos..self.pos + 6)? else {
			return None;
		};
		// getu4: exactly four hex digits
		let low = hex
			.iter()
			.try_fold(0, |value, &byte| Some(value * 16 + char::from(byte).to_digit(16)?))?;
		if !(0xdc00..0xe000).contains(&low) {
			return None;
		}
		self.pos += 6;
		char::from_u32(0x10000 + ((high - 0xd800) << 10) + (low - 0xdc00))
	}
}

/// The length of the UTF-8 sequence a lead byte announces (1 for an invalid lead byte).
const fn utf8_width(byte: u8) -> usize {
	match byte {
		0xc2..=0xdf => 2,
		0xe0..=0xef => 3,
		0xf0..=0xf4 => 4,
		_ => 1,
	}
}
