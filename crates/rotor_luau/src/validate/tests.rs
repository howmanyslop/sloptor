//! Port of `internal/luau/validate_test.go`.
//!
//! The hand-rolled scanners in `validate.rs` replace these exact regexes;
//! brute-force every short string over a covering alphabet to prove the
//! behavior is identical.

use regex::Regex;

use super::LUAU_RESERVED_KEYWORDS;
use crate::{
	is_metamethod, is_reserved_class_field, is_reserved_identifier, is_valid_identifier, is_valid_number_literal,
};

fn for_each_short_string(alphabet: &[char], max_len: usize, f: &mut impl FnMut(&str)) {
	fn recurse(prefix: &mut String, depth: usize, alphabet: &[char], max_len: usize, f: &mut impl FnMut(&str)) {
		f(prefix);
		if depth == max_len {
			return;
		}
		for &c in alphabet {
			prefix.push(c);
			recurse(prefix, depth + 1, alphabet, max_len, f);
			prefix.pop();
		}
	}
	recurse(&mut String::new(), 0, alphabet, max_len, f);
}

#[test]
fn is_valid_identifier_matches_regex_oracle() {
	let identifier = Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").expect("regex");
	let oracle = |s: &str| !LUAU_RESERVED_KEYWORDS.contains(&s) && identifier.is_match(s);
	// Go's alphabet ends with the lone byte 0x80; a Rust string cannot hold
	// it, so U+0080 stands in as the non-ASCII member.
	for_each_short_string(&['a', 'Z', '0', '_', ' ', '.', '\u{80}'], 4, &mut |s| {
		assert_eq!(
			is_valid_identifier(s),
			oracle(s),
			"is_valid_identifier({s:?}) disagrees with the regex oracle"
		);
	});
	for keyword in ["and", "do", "end", "nil", "while", "function"] {
		assert!(
			!is_valid_identifier(keyword),
			"reserved keyword {keyword:?} must be invalid"
		);
	}
}

#[test]
fn is_valid_number_literal_matches_regex_oracle() {
	let decimal = Regex::new(r"^(?:\d[\d_]*(?:\.[\d_]*)?|\.\d[\d_]*)(?:[eE][+-]?_*\d[\d_]*)?$").expect("regex");
	let binary = Regex::new(r"^0_*[bB]_*[01][01_]*$").expect("regex");
	let hexadecimal = Regex::new(r"^0_*[xX]_*[\da-fA-F][\da-fA-F_]*$").expect("regex");
	let oracle = |s: &str| decimal.is_match(s) || binary.is_match(s) || hexadecimal.is_match(s);
	// The alphabet covers every branch: leading zero, base prefixes (both
	// cases), binary/hex/decimal digit classes, underscores, dot, exponent
	// markers, signs, and a non-member byte.
	let alphabet: Vec<char> = "01fbBxXeE_.+-".chars().collect();
	for_each_short_string(&alphabet, 4, &mut |s| {
		assert_eq!(
			is_valid_number_literal(s),
			oracle(s),
			"is_valid_number_literal({s:?}) disagrees with the regex oracle"
		);
	});
	// Longer hand-picked cases beyond the brute-force length.
	let long_cases = [
		"0_1",
		"0__b1",
		"0b__10_1_",
		"0x_F_f0",
		"0X__",
		"0b2",
		"0xG",
		"1_000.5_5e+_1_0",
		"12.e5",
		".5e_2",
		"1e+",
		"1e_",
		"0__x__abc_def",
		"0_e1",
		"0___",
		"123_456_789",
		".0e-0_0",
		"0b1010e1",
		"0xFFp1",
	];
	for s in long_cases {
		assert_eq!(
			is_valid_number_literal(s),
			oracle(s),
			"is_valid_number_literal({s:?}) disagrees with the regex oracle"
		);
	}
}

#[test]
fn is_valid_identifier_cases() {
	for s in ["x", "_foo", "A1_b2", "_"] {
		assert!(is_valid_identifier(s), "{s:?} should be valid");
	}
	for s in [
		"and", "end", "nil", "local", "until", "then", "elseif", "repeat", "1x", "a-b", "a b", "", "héllo",
	] {
		assert!(!is_valid_identifier(s), "{s:?} should be invalid");
	}
}

#[test]
fn is_valid_number_literal_cases() {
	for s in [
		"1", "1_000", "1.5", ".5", "1e5", "1E+5", "1.2e-3", "0b1010", "0B_1010", "0xFF", "0X_ff_0",
	] {
		assert!(is_valid_number_literal(s), "{s:?} should be valid");
	}
	for s in ["", "abc", "0b", "0x", "1.2.3", "e5"] {
		assert!(!is_valid_number_literal(s), "{s:?} should be invalid");
	}
}

#[test]
fn metamethods_and_reserved() {
	assert!(is_metamethod("__index") && !is_metamethod("__banana"), "is_metamethod");
	assert!(
		is_reserved_class_field("new") && is_reserved_class_field("__index") && !is_reserved_class_field("foo"),
		"is_reserved_class_field"
	);
	assert!(
		is_reserved_identifier("TS") && is_reserved_identifier("game") && !is_reserved_identifier("myVar"),
		"is_reserved_identifier"
	);
}
