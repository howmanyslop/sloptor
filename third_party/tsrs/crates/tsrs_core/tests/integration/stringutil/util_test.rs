use tsrs_core::stringutil::rangetable::Rune;

use tsrs_core::stringutil::util::*;
use tsrs_core::stringutil::{is_unicode_identifier_part, is_unicode_identifier_start};

#[test]
fn test_encode_uri() {
    assert_eq!(encode_uri("a b"), "a%20b");
    assert_eq!(encode_uri(";/?:@&=+$,#"), ";/?:@&=+$,#");
    assert_eq!(encode_uri("①Ⅻㄨㄩ U1[abc]"), "%E2%91%A0%E2%85%AB%E3%84%A8%E3%84%A9%20U1%5Babc%5D");
}

#[test]
fn test_misc() {
    for (text, expected) in [
        ("", vec![]),
        ("a\r\nb\rc\nd", vec!["a", "b", "c", "d"]),
        ("a\r\n", vec!["a"]),
        ("\r\n\r\r\n", vec!["", "", ""]),
        ("é\ra\n\n", vec!["é", "a", ""]),
    ] {
        assert_eq!(split_lines(text), expected);
    }
    assert_eq!(unquote_string("\"a\\\"b\""), "a\"b");
    assert_eq!(strip_quotes("'x'"), "x");
    assert!(is_unicode_identifier_start('é' as Rune));
    assert!(!is_unicode_identifier_start('1' as Rune));
    assert!(is_unicode_identifier_part('1' as Rune));
    assert!(is_unicode_identifier_start(0x2F800u32));
    let s = encode_js_string_rune(0xD83D) + &encode_js_string_rune(0xDE00);
    assert_eq!(combine_surrogate_pairs(&s), "😀");
    assert_eq!(decode_js_string_rune(&encode_js_string_rune(0xDC01)), (0xDC01, 3));
    assert_eq!(code_point_to_surrogate_pair('😀' as Rune), (0xD83D, 0xDE00));
    assert_eq!(truncate_by_runes("héllo", 2), "hé");
}
