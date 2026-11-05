use tsrs_core::json::json::*;

#[test]
fn round_trip() {
    let v = unmarshal(r#"{"a": 1, "b": "two", "c": { "d": [4.5, true, null] }, "e": "é😀<>"}"#)
        .unwrap();
    assert_eq!(marshal(&v).unwrap(), r#"{"a":1,"b":"two","c":{"d":[4.5,true,null]},"e":"é😀<>"}"#);
    assert_eq!(
        marshal_indent(&unmarshal(r#"{"a":[1,2],"b":{}}"#).unwrap(), "", "  ").unwrap(),
        "{\n  \"a\": [\n    1,\n    2\n  ],\n  \"b\": {}\n}"
    );
    assert_eq!(marshal_string("a\"\n\u{1}"), "\"a\\\"\\n\\u0001\"");
    assert_eq!(marshal_f64(1e21).unwrap(), "1e+21");
    assert_eq!(marshal_f64(123.456).unwrap(), "123.456");
    assert_eq!(marshal_f64(-0.000_001).unwrap(), "-0.000001");
    assert_eq!(marshal_f64(1.5e-7).unwrap(), "1.5e-7");
    assert_eq!(marshal_f64(1e20).unwrap(), "100000000000000000000");
    // The integer fast path agrees with the general ES6 formatting.
    for n in [
        0.0,
        -0.0,
        1.0,
        -1.0,
        9.0,
        10.0,
        12345.0,
        -987_654_321.0,
        9_007_199_254_740_991.0,
        -9_007_199_254_740_991.0,
        9_007_199_254_740_992.0,
        1e20,
    ] {
        assert_eq!(marshal(&Value::Number(n)).unwrap(), marshal_f64(n).unwrap(), "{n}");
    }
    assert_eq!(marshal(&Value::Integer(i64::MIN)).unwrap(), i64::MIN.to_string());
    assert_eq!(
        marshal_string("plain/ascii run é \"q\" \\ tab\t end"),
        "\"plain/ascii run é \\\"q\\\" \\\\ tab\\t end\""
    );
    assert!(marshal_f64(f64::NAN).is_err());
    assert!(unmarshal("{\"a\":1,\"a\":2}").is_err());
    assert!(unmarshal("[1,]").is_err());
    // The integer fast path agrees with the general parse, including the sign of zero.
    for text in [
        "0",
        "-0",
        "7",
        "-42",
        "123456789012345",
        "-999999999999999",
        "1234567890123456",
        "1.5",
        "2e3",
    ] {
        let Value::Number(n) = unmarshal(text).unwrap() else { panic!() };
        let expected: f64 = text.parse().unwrap();
        assert_eq!(n.to_bits(), expected.to_bits(), "{text}");
    }
}

#[test]
fn unicode_escape_needs_four_hex_digits() {
    assert_eq!(unmarshal(r#""\u0041\u00e9\u00C9""#).unwrap(), Value::String("AéÉ".to_string()));
    for text in [r#""\u+041""#, r#""\u-041""#, r#""\u 041""#, r#""\u004""#, r#""\ud83d\u+e00""#] {
        assert!(
            unmarshal(text).unwrap_err().starts_with("jsontext: invalid escape sequence"),
            "{text}"
        );
    }
}

#[test]
fn max_nesting_depth() {
    // Recursion to the limit needs more than the default test thread stack in an unoptimized build.
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(|| {
            let nested = |open: &str, close: &str, depth: usize| {
                format!("{}{}", open.repeat(depth), close.repeat(depth))
            };
            assert!(unmarshal(&nested("[", "]", MAX_NESTING_DEPTH)).is_ok());
            assert!(
                unmarshal(&format!("{{\"a\":{}}}", nested("[", "]", MAX_NESTING_DEPTH - 1)))
                    .is_ok()
            );
            assert_eq!(
                unmarshal(&nested("[", "]", MAX_NESTING_DEPTH + 1)).unwrap_err(),
                "jsontext: exceeded max depth at offset 10000"
            );
            let deep_object = format!("{{\"a\":{}", nested("[", "]", MAX_NESTING_DEPTH));
            assert_eq!(
                unmarshal(&deep_object).unwrap_err(),
                "jsontext: exceeded max depth at offset 10004"
            );
            // Far deeper than the stack could recurse.
            assert!(unmarshal(&nested("[", "]", 2_000_000)).is_err());
            // Scalars at the limit are fine.
            assert!(
                unmarshal(&format!(
                    "{}1{}",
                    "[".repeat(MAX_NESTING_DEPTH),
                    "]".repeat(MAX_NESTING_DEPTH)
                ))
                .is_ok()
            );
        })
        .unwrap()
        .join()
        .unwrap();
}
