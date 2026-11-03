use tsrs_core::gocompat::*;

#[test]
fn quote_styles_preserve_control_and_unicode_escapes() {
    let input = "\x07\x08\x0c\x0b\n\r\t\"\u{7f}\u{85}\u{a0}\u{200b}\u{feff}\\";
    for (style, expected) in [
        (QuoteStyle::Go, "\"\\a\\b\\f\\v\\n\\r\\t\\\"\\x7f\\u0085\\u00a0\\u200b\\ufeff\\\\\""),
        (QuoteStyle::Header, "\"\\a\\b\\f\\v\\n\\r\\t\\\"\\x7f\\u0085\\u00a0\\u200b\\ufeff\\\\\""),
        (
            QuoteStyle::Api,
            "\"\\x07\\x08\\x0c\\x0b\\n\\r\\t\\\"\\x7f\u{85}\u{a0}\u{200b}\u{feff}\\\\\"",
        ),
        (QuoteStyle::Lsp, "\"\\a\\b\\f\\v\\n\\r\\t\\\"\\x7f\u{85}\u{a0}\u{200b}\u{feff}\\\\\""),
        (
            QuoteStyle::MapperTest,
            "\"\\a\\b\\f\\v\\n\\r\\t\\\"\\x7f\\u0085\\u00a0\u{200b}\u{feff}\\\\\"",
        ),
        (
            QuoteStyle::Fourslash,
            "\"\\a\\b\\f\\v\\n\\r\\t\\\"\\x7f\\u0085\u{a0}\u{200b}\u{feff}\\\\\"",
        ),
    ] {
        assert_eq!(quote(input.as_bytes(), style), expected, "{style:?}");
        assert_eq!(quote(" space é😀".as_bytes(), style), "\" space é😀\"");
    }
}

#[test]
fn quote_invalid_utf8_uses_each_callers_style() {
    for (input, escaped, replaced) in [
        (b"\xff".as_slice(), "\"\\xff\"", "\"�\""),
        (b"\xc0\xaf".as_slice(), "\"\\xc0\\xaf\"", "\"��\""),
        (b"a\xf0\x9f".as_slice(), "\"a\\xf0\\x9f\"", "\"a�\""),
    ] {
        assert_eq!(quote(input, QuoteStyle::Lsp), replaced);
        for style in [
            QuoteStyle::Go,
            QuoteStyle::Header,
            QuoteStyle::Api,
            QuoteStyle::MapperTest,
            QuoteStyle::Fourslash,
        ] {
            assert_eq!(quote(input, style), escaped, "{style:?}");
        }
    }
}

#[test]
fn go_quote_uses_printable_unicode_categories() {
    for (input, expected) in [
        ("\u{2066}\u{2069}\u{0378}", r#""\u2066\u2069\u0378""#),
        ("\u{e000}\u{100000}", r#""\ue000\U00100000""#),
        ("\u{2028}\u{2029}\u{3000}", r#""\u2028\u2029\u3000""#),
        ("\u{0301}é😀\u{fffd}", "\"\u{0301}é😀\u{fffd}\""),
    ] {
        assert_eq!(quote(input.as_bytes(), QuoteStyle::Go), expected);
    }
}

#[test]
fn parse_int64_preserves_syntax_range_and_error_text() {
    for style in [QuoteStyle::Header, QuoteStyle::Api, QuoteStyle::Lsp] {
        for (input, value) in [
            ("+0001", 1),
            ("-0", 0),
            ("9223372036854775807", i64::MAX),
            ("-9223372036854775808", i64::MIN),
        ] {
            assert_eq!(parse_int64(input.as_bytes(), style), Ok(value));
        }
        for (input, expected) in [
            ("", "strconv.ParseInt: parsing \"\": invalid syntax"),
            ("+", "strconv.ParseInt: parsing \"+\": invalid syntax"),
            ("--1", "strconv.ParseInt: parsing \"--1\": invalid syntax"),
            (" 1", "strconv.ParseInt: parsing \" 1\": invalid syntax"),
            (
                "999999999999999999999999x",
                "strconv.ParseInt: parsing \"999999999999999999999999x\": invalid syntax",
            ),
            (
                "9223372036854775808",
                "strconv.ParseInt: parsing \"9223372036854775808\": value out of range",
            ),
            (
                "-9223372036854775809",
                "strconv.ParseInt: parsing \"-9223372036854775809\": value out of range",
            ),
        ] {
            assert_eq!(parse_int64(input.as_bytes(), style), Err(expected.to_string()));
        }
    }
    assert_eq!(
        parse_int64(b"\xff", QuoteStyle::Header),
        Err("strconv.ParseInt: parsing \"\\xff\": invalid syntax".to_string()),
    );
}
