//! `jsxtext_test.go`: the recorded fixup shapes plus the worked examples from TS's
//! `fixupWhitespaceAndDecodeEntities.ts` comments.

use crate::extras::jsxtext::{decode_entities, fixup_whitespace_and_decode_entities};

#[test]
fn fixup_whitespace_and_decode_entities_cases() {
	let tests: &[(&str, &str, &str)] = &[
		// leading/trailing newline+indent trimmed
		("case4 hello world", "\n\t\t\thello world\n\t\t\t", "hello world"),
		// &amp; -> &, &nbsp; -> U+00A0, lines joined with " "
		(
			"case9 entities",
			"\n\t\t\tone &amp; two&nbsp;three\n\t\t\tline2\n\t\t",
			"one & two\u{a0}three line2",
		),
		// backslashes pass through the fixup untouched (doubling happens at the
		// transformJsxChildren call site)
		("caseE backslash", "back\\slash", "back\\slash"),
		// single line: returned as-is (including surrounding spaces)
		("single line kept", "  one line  ", "  one line  "),
		// whitespace-only with a line break: "" (TS returns undefined; the port returns "",
		// matching upstream's `?? ""` consumption)
		("all whitespace multiline", "  \n  ", ""),
		// trimRight first line, trim middles, trimLeft last line, drop empty
		("three lines", "a  \n  b  \n  c", "a b c"),
		// empty lines removed before joining
		("empty middle line", "a\n   \nb", "a b"),
		// decimal and hex numeric entities
		("numeric entities", "&#123;&#x41;", "{A"),
		// unknown entity passes through verbatim
		("unknown entity", "&bogus;", "&bogus;"),
		// missing semicolon: not an entity
		("no semicolon", "a &amp b", "a &amp b"),
	];
	for &(name, input, want) in tests {
		let got = fixup_whitespace_and_decode_entities(input);
		assert_eq!(got, want, "{name}: fixupWhitespaceAndDecodeEntities({input:?})");
	}
}

#[test]
fn decode_entities_cases() {
	let tests: &[(&str, &str)] = &[
		("no entities", "no entities"),
		("&quot;&amp;&apos;&lt;&gt;", "\"&'<>"),
		("&nbsp;", "\u{a0}"),
		("&hellip;", "…"),
		("&#65;", "A"),
		("&#x2603;", "☃"),
		("&#xZZ;", "&#xZZ;"), // invalid hex digits -> literal
		("&#;", "&#;"),       // empty numeric -> literal
		("&;", "&;"),         // empty name -> literal
		("tail &", "tail &"), // trailing ampersand, no semicolon
	];
	for &(input, want) in tests {
		assert_eq!(decode_entities(input), want, "decodeEntities({input:?})");
	}
}

/// Source text `back\slash` becomes the Luau string value `back\\slash`, which renders raw.
#[test]
fn jsx_text_backslash_doubling() {
	let fixed = fixup_whitespace_and_decode_entities("back\\slash");
	let doubled = fixed.replace('\\', "\\\\");
	assert_eq!(doubled, r"back\\slash", "backslash doubling");
}
