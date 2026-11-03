use tsrs_core::stringutil::rangetable::*;

// Expected values are Go 1.27's unicode.ToUpper / unicode.ToLower (Unicode 17.0.0). An exhaustive comparison over
// every rune found these 27 runes as the only difference between the simple mapping and Rust's full mapping.
#[test]
fn test_unicode_to_upper_simple_mapping() {
    for (lo, hi) in [(0x1F80, 0x1F87), (0x1F90, 0x1F97), (0x1FA0, 0x1FA7)] {
        for r in lo..=hi {
            assert_eq!(unicode_to_upper(r), r + 8, "U+{r:04X}");
        }
    }
    assert_eq!(unicode_to_upper(0x1FB3), 0x1FBC);
    assert_eq!(unicode_to_upper(0x1FC3), 0x1FCC);
    assert_eq!(unicode_to_upper(0x1FF3), 0x1FFC);
    // No simple uppercase: unchanged.
    for r in [0xDF, 0x149, 0x1F0, 0x1F50, 0x1F88, 0x1FB2, 0x1FB4, 0x1FBC, 0xFB00] {
        assert_eq!(unicode_to_upper(r), r, "U+{r:04X}");
    }
    assert_eq!(unicode_to_upper(0x1C5), 0x1C4);
    assert_eq!(unicode_to_upper('a' as Rune), 'A' as Rune);
    assert_eq!(unicode_to_upper(0xFF), 0x178);
    assert_eq!(unicode_to_upper(0xD800), 0xD800);
}

// The special cases in unicode_to_upper / unicode_to_lower were derived for Unicode 17.0.0. A toolchain with
// another Unicode version needs the exhaustive comparison against Go's unicode.ToUpper / ToLower re-run.
#[test]
fn test_unicode_version_of_case_mappings() {
    assert_eq!(char::UNICODE_VERSION, (17, 0, 0));
}

// Expected values are Go 1.27's unicode.SimpleFold, which an exhaustive comparison over every rune matches.
#[test]
fn test_unicode_simple_fold() {
    let orbit = |start: Rune| {
        let mut out = vec![start];
        let mut r = unicode_simple_fold(start);
        while r != start {
            out.push(r);
            r = unicode_simple_fold(r);
        }
        out
    };
    assert_eq!(orbit('K' as Rune), ['K' as Rune, 'k' as Rune, 0x212A]);
    assert_eq!(orbit(0x398), [0x398, 0x3B8, 0x3D1, 0x3F4]);
    assert_eq!(orbit(0xFB05), [0xFB05, 0xFB06]);
    assert_eq!(orbit(0x390), [0x390, 0x1FD3]);
    assert_eq!(orbit('i' as Rune), ['i' as Rune, 'I' as Rune]);
    // U+0130 and U+0131 fold to themselves, though ToLower(U+0130) is 'i' and ToUpper(U+0131) is 'I'.
    assert_eq!(unicode_simple_fold(0x130), 0x130);
    assert_eq!(unicode_simple_fold(0x131), 0x131);
    // The ToLower / ToUpper fallback, with ToUpper's simple mapping.
    assert_eq!(unicode_simple_fold(0x1F80), 0x1F88);
    assert_eq!(unicode_simple_fold(0x1F88), 0x1F80);
    assert_eq!(unicode_simple_fold('1' as Rune), '1' as Rune);
}

#[test]
fn test_unicode_to_lower_simple_mapping() {
    assert_eq!(unicode_to_lower(0x130), 'i' as Rune);
    assert_eq!(unicode_to_lower(0x1F88), 0x1F80);
    assert_eq!(unicode_to_lower(0x1FBC), 0x1FB3);
    assert_eq!(unicode_to_lower(0x1C5), 0x1C6);
    assert_eq!(unicode_to_lower('A' as Rune), 'a' as Rune);
}
