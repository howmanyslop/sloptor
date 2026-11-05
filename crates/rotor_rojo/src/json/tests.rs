//! Tests for the JSON reader and writer. Go's `encoding/json` decoder defines
//! the accepted language, so the cases mirror what it accepts and rejects.

use super::{JsonValue, parse};

fn object_keys(value: &JsonValue) -> Vec<&str> {
	match value {
		JsonValue::Object(members) => members.iter().map(|(key, _)| key.as_str()).collect(),
		other => panic!("not an object: {other:?}"),
	}
}

#[test]
fn objects_keep_insertion_order_with_array_indexes_first() {
	let value = parse(br#"{"b":1,"10":2,"a":3,"2":4,"01":5}"#);
	assert_eq!(object_keys(&value), ["2", "10", "b", "a", "01"]);
}

#[test]
fn duplicate_keys_keep_their_first_position_and_last_value() {
	let value = parse(br#"{"a":1,"b":2,"a":3}"#);
	assert_eq!(object_keys(&value), ["a", "b"]);
	assert_eq!(value.member("a").and_then(JsonValue::as_i64), Some(3));
}

#[test]
#[expect(
	clippy::literal_string_with_formatting_args,
	reason = "`{1:2}` is deliberately invalid JSON, not a format string"
)]
fn rejects_what_json_parse_rejects() {
	for bad in [
		"",
		"{",
		"{not json",
		"[1,]",
		"{\"a\":1,}",
		"01",
		"1.",
		".5",
		"+1",
		"-",
		"tru",
		"truex",
		"\"\\x\"",
		"\"\t\"",
		"{} {}",
		"[1 2]",
		"{\"a\" 1}",
		r"{1:2}",
		"1e999",
		"\u{feff}{}",
	] {
		assert_eq!(parse(bad.as_bytes()), JsonValue::Invalid, "{bad:?}");
	}
}

#[test]
fn accepts_what_json_parse_accepts() {
	for good in [
		" {} ",
		"[]",
		"null",
		"true",
		"-0",
		"1e5",
		"1E+5",
		"0.5e-3",
		"\"\\u00e9\"",
		"[[],{}]",
	] {
		assert_ne!(parse(good.as_bytes()), JsonValue::Invalid, "{good:?}");
	}
}

#[test]
fn strings_decode_escapes_and_surrogates() {
	let value = parse(br#""a\n\"\\\/\u00e9\ud83d\ude00""#);
	assert_eq!(value.as_str(), Some("a\n\"\\/é😀"));
	// A lone surrogate becomes U+FFFD and consumes only its own escape.
	assert_eq!(parse(br#""\ud83dx""#).as_str(), Some("\u{fffd}x"));
	assert_eq!(parse(br#""\ude00""#).as_str(), Some("\u{fffd}"));
}

#[test]
fn invalid_utf8_becomes_one_replacement_per_byte() {
	let value = parse(b"\"a\xe2\x82b\"");
	assert_eq!(value.as_str(), Some("a\u{fffd}\u{fffd}b"));
}

#[test]
fn numbers_keep_their_source_text() {
	let value = parse(b"1700000000123456789");
	assert_eq!(value.as_i64(), Some(1_700_000_000_123_456_789));
	assert_eq!(parse(b"1.5").as_i64(), None);
}

#[test]
fn deep_nesting_is_invalid_instead_of_overflowing_the_stack() {
	let deep = format!("{}{}", "[".repeat(5000), "]".repeat(5000));
	assert_eq!(parse(deep.as_bytes()), JsonValue::Invalid);
}

#[test]
fn writer_matches_marshal_indent() {
	let value = JsonValue::Object(vec![
		("empty".to_owned(), JsonValue::Array(vec![])),
		(
			"list".to_owned(),
			JsonValue::Array(vec![JsonValue::String("<&>\u{2028}\t".to_owned()), JsonValue::Null]),
		),
		("n".to_owned(), JsonValue::integer(-5)),
		("b".to_owned(), JsonValue::Bool(true)),
	]);
	let want = "{\n\t\"empty\": [],\n\t\"list\": [\n\t\t\"\\u003c\\u0026\\u003e\\u2028\\t\",\n\t\tnull\n\t],\n\t\"n\": -5,\n\t\"b\": true\n}";
	assert_eq!(value.to_indented_string(), want);
}
