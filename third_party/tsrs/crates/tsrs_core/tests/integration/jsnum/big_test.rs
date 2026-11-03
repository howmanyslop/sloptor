use tsrs_core::jsnum::big::*;

#[test]
fn parsing_preserves_go_prefixes_and_separator_rules() {
    for (text, base, expected) in [
        ("-0", 0, "0"),
        ("+0x_Ff", 0, "255"),
        ("0_17", 0, "15"),
        ("-0b_101", 0, "-5"),
        ("0o_77", 0, "63"),
        ("010", 10, "10"),
        ("FF", 16, "255"),
    ] {
        assert_eq!(Int::set_string(text, base).unwrap().string(), expected);
    }
    for (text, base) in [
        ("", 0),
        ("+", 0),
        ("0x", 0),
        ("08", 0),
        ("_1", 0),
        ("1_", 0),
        ("1__2", 0),
        ("0x__1", 0),
        ("1_2", 10),
        ("0x1", 16),
        ("--1", 0),
        ("１２", 0),
    ] {
        assert!(Int::set_string(text, base).is_none(), "{text:?}, base {base}");
    }
}

#[test]
fn powers_preserve_non_positive_exponents_and_signs() {
    assert_eq!(Int::new(-3).exp(&Int::new(3)).string(), "-27");
    assert_eq!(Int::new(-3).exp(&Int::new(4)).string(), "81");
    assert_eq!(Int::new(0).exp(&Int::new(0)).string(), "1");
    assert_eq!(Int::new(3).exp(&Int::new(-1)).string(), "1");
}

#[test]
fn intermediate_rounding_preserves_ties_even_and_signs() {
    for (value, expected) in [(9, 8), (11, 12), (13, 12), (15, 16), (-9, -8), (-11, -12)] {
        assert_eq!(Int::new(value).round_to_prec(3), Int::new(expected));
    }
    assert_eq!(Int::new(35).round_to_prec(3), Int::new(32));
    assert_eq!(Int::new(37).round_to_prec(3), Int::new(40));
}

#[test]
fn float_conversion_preserves_rounding_and_overflow() {
    for (text, expected) in [
        ("9007199254740993", 9_007_199_254_740_992.0f64),
        ("9007199254740995", 9_007_199_254_740_996.0),
        ("-9007199254740993", -9_007_199_254_740_992.0),
    ] {
        assert_eq!(Int::set_string(text, 10).unwrap().float64().to_bits(), expected.to_bits());
    }
    assert_eq!(Int::new(2).exp(&Int::new(1024)).float64(), f64::INFINITY);
    assert_eq!(Int::new(-2).exp(&Int::new(1025)).float64(), f64::NEG_INFINITY);
    assert_eq!(Int::new(0).float64().to_bits(), 0);
}
