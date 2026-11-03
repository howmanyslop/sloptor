use tsrs_core::stringutil::compare::*;

// Expected values are Go 1.27's strings.EqualFold. On these pairs a lowercase/uppercase comparison disagrees with
// Go's SimpleFold orbit walk; an exhaustive comparison over all case-related rune pairs finds no others.
#[test]
fn test_equal_fold_simple_fold_orbits() {
    for (a, b) in [("I", "\u{130}"), ("i", "\u{130}"), ("I", "\u{131}"), ("i", "\u{131}")] {
        assert!(!equal_fold(a, b), "{a} {b}");
        assert!(!equal_fold(b, a), "{b} {a}");
    }
    for (a, b) in [
        ("\u{390}", "\u{1FD3}"),
        ("\u{3B0}", "\u{1FE3}"),
        ("\u{3D1}", "\u{3F4}"),
        ("\u{FB05}", "\u{FB06}"),
    ] {
        assert!(equal_fold(a, b), "{a} {b}");
        assert!(equal_fold(b, a), "{b} {a}");
        assert!(equal_fold(&format!("ab{a}c"), &format!("AB{b}C")), "{a} {b}");
    }
    assert!(equal_fold("k", "\u{212A}"));
    assert!(equal_fold("\u{1F80}", "\u{1F88}"));
    assert!(equal_fold("Stra\u{DF}e", "STRA\u{1E9E}E"));
    assert!(!equal_fold("abc", "ab"));
}

// has_prefix / has_suffix compare byte slices that can end mid-rune, as Go's do.
#[test]
fn test_has_suffix_ignore_case_cut_rune() {
    assert!(has_suffix("x\u{3F4}.TS", "\u{3D1}.ts", false));
    assert!(!has_suffix("x\u{131}.TS", "i.ts", false));
    assert!(has_prefix("\u{3B8}", "\u{3D1}", false));
    // The 2-byte prefix of U+1FD3 is invalid UTF-8, which never folds to U+0390.
    assert!(!has_prefix("\u{1FD3}x", "\u{390}", false));
}
