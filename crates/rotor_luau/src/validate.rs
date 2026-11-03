//! Identifier, number literal, and reserved name checks.

use crate::globals::RESERVED_GLOBAL_NAMES;

pub const LUAU_RESERVED_KEYWORDS: &[&str] = &[
	"and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in", "local", "nil", "not", "or",
	"repeat", "return", "then", "true", "until", "while",
];

const LUAU_METAMETHODS: &[&str] = &[
	"__index",
	"__newindex",
	"__call",
	"__concat",
	"__unm",
	"__add",
	"__sub",
	"__mul",
	"__div",
	"__mod",
	"__pow",
	"__tostring",
	"__metatable",
	"__eq",
	"__lt",
	"__le",
	"__mode",
	"__gc",
	"__len",
];

const LUAU_RESERVED_CLASS_FIELDS: &[&str] = &["__index", "new"];

/// Whether `id` is a Luau reserved keyword (the set behind
/// [`is_valid_identifier`]).
#[must_use]
pub fn is_reserved_keyword(id: &str) -> bool {
	LUAU_RESERVED_KEYWORDS.contains(&id)
}

const fn is_identifier_start(c: u8) -> bool {
	c == b'_' || c.is_ascii_alphabetic()
}

const fn is_identifier_part(c: u8) -> bool {
	is_identifier_start(c) || c.is_ascii_digit()
}

/// Whether `id` matches `^[A-Za-z_][A-Za-z0-9_]*$` and is not a reserved
/// keyword. Any non-ASCII byte fails the classes, exactly like the regex.
#[must_use]
pub fn is_valid_identifier(id: &str) -> bool {
	let bytes = id.as_bytes();
	let Some((&first, rest)) = bytes.split_first() else {
		return false;
	};
	is_identifier_start(first) && rest.iter().all(|&c| is_identifier_part(c)) && !is_reserved_keyword(id)
}

const fn is_binary_digit(c: u8) -> bool {
	c == b'0' || c == b'1'
}

/// Whether `text` is a decimal, binary, or hexadecimal Luau number literal: a
/// hand-rolled equivalent of the anchored regexes
///
/// ```text
/// decimal:     ^(?:\d[\d_]*(?:\.[\d_]*)?|\.\d[\d_]*)(?:[eE][+-]?_*\d[\d_]*)?$
/// binary:      ^0_*[bB]_*[01][01_]*$
/// hexadecimal: ^0_*[xX]_*[\da-fA-F][\da-fA-F_]*$
/// ```
///
/// Greedy scanning is safe: at every branch point the follow sets are
/// disjoint, so no backtracking can change the outcome.
#[must_use]
pub fn is_valid_number_literal(text: &str) -> bool {
	let bytes = text.as_bytes();
	if bytes.is_empty() {
		return false;
	}
	if bytes[0] == b'0' {
		let mut i = 1;
		while i < bytes.len() && bytes[i] == b'_' {
			i += 1;
		}
		if i < bytes.len() {
			match bytes[i] {
				b'b' | b'B' => return match_base_digits(&bytes[i + 1..], is_binary_digit),
				b'x' | b'X' => return match_base_digits(&bytes[i + 1..], |c| c.is_ascii_hexdigit()),
				_ => {}
			}
		}
	}
	is_decimal_literal(bytes)
}

/// Matches `^_*D[D_]*$` for the digit class `D`.
fn match_base_digits(s: &[u8], is_digit: impl Fn(u8) -> bool) -> bool {
	let mut i = 0;
	while i < s.len() && s[i] == b'_' {
		i += 1;
	}
	if i >= s.len() || !is_digit(s[i]) {
		return false;
	}
	s[i + 1..].iter().all(|&c| c == b'_' || is_digit(c))
}

const fn skip_decimal_digits(s: &[u8], mut i: usize) -> usize {
	while i < s.len() && (s[i].is_ascii_digit() || s[i] == b'_') {
		i += 1;
	}
	i
}

/// Matches the anchored decimal pattern documented on
/// [`is_valid_number_literal`].
const fn is_decimal_literal(s: &[u8]) -> bool {
	let n = s.len();
	let mut i;
	if s[0].is_ascii_digit() {
		i = skip_decimal_digits(s, 1);
		if i < n && s[i] == b'.' {
			i = skip_decimal_digits(s, i + 1);
		}
	} else if s[0] == b'.' {
		if n < 2 || !s[1].is_ascii_digit() {
			return false;
		}
		i = skip_decimal_digits(s, 2);
	} else {
		return false;
	}
	if i == n {
		return true;
	}
	if s[i] != b'e' && s[i] != b'E' {
		return false;
	}
	i += 1;
	if i < n && (s[i] == b'+' || s[i] == b'-') {
		i += 1;
	}
	while i < n && s[i] == b'_' {
		i += 1;
	}
	if i >= n || !s[i].is_ascii_digit() {
		return false;
	}
	skip_decimal_digits(s, i + 1) == n
}

/// Whether `id` is a Luau metamethod name.
#[must_use]
pub fn is_metamethod(id: &str) -> bool {
	LUAU_METAMETHODS.contains(&id)
}

/// Whether `id` is a field name that roblox-ts classes reserve.
#[must_use]
pub fn is_reserved_class_field(id: &str) -> bool {
	LUAU_RESERVED_CLASS_FIELDS.contains(&id)
}

/// Whether `id` is a top-level key of upstream `globals.ts`.
#[must_use]
pub fn is_reserved_identifier(id: &str) -> bool {
	RESERVED_GLOBAL_NAMES.contains(&id)
}

#[cfg(test)]
mod tests;
