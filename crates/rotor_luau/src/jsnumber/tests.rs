//! Port of `internal/luau/jsnumber_test.go`.

use crate::{js_number_parse, js_number_string};

// V8-verified vectors: each `want` is the exact output of String(number) in JS.
#[test]
fn js_number_string_matches_v8() {
	let cases: [(f64, &str); 21] = [
		(0.0, "0"),
		(-0.0, "0"),
		(5.0, "5"),
		(-3.0, "-3"),
		(1.5, "1.5"),
		(0.1, "0.1"),
		(100.0, "100"),
		(1e20, "100000000000000000000"),
		(1e21, "1e+21"),
		(2e21, "2e+21"),
		(1.5e22, "1.5e+22"),
		(0.00001, "0.00001"),
		(0.000_001, "0.000001"),
		(1e-7, "1e-7"),
		(1.234e-7, "1.234e-7"),
		(0.000_015, "0.000015"),
		(9_007_199_254_740_992.0, "9007199254740992"),
		(123.456, "123.456"),
		(f64::INFINITY, "Infinity"),
		(f64::NEG_INFINITY, "-Infinity"),
		(f64::NAN, "NaN"),
	];
	for (input, want) in cases {
		assert_eq!(js_number_string(input), want, "js_number_string({input:?})");
	}
}

#[test]
fn js_number_parse_literal_forms() {
	let cases: [(&str, f64); 8] = [
		("5", 5.0),
		("1_000", 1000.0),
		("1.5", 1.5),
		(".5", 0.5),
		("0xFF", 255.0),
		("0b101", 5.0),
		("0o17", 15.0),
		("1e3", 1000.0),
	];
	for (input, want) in cases {
		let got = js_number_parse(input);
		assert!(
			got.as_ref().is_ok_and(|got| got.to_bits() == want.to_bits()),
			"js_number_parse({input:?}) = {got:?}, want {want}"
		);
	}
	assert!(
		js_number_parse("garbage").is_err(),
		"js_number_parse(\"garbage\") must return an error"
	);
}
