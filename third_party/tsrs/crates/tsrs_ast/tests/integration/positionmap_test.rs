use tsrs_ast::positionmap::*;

#[test]
fn position_map_ascii() {
    let text = "const x = 1;";
    let pm = compute_position_map(text);
    assert!(pm.is_ascii_only());
    for i in 0..=u32::try_from(text.len()).unwrap() {
        assert_eq!(pm.utf8_to_utf16(i), i);
        assert_eq!(pm.utf16_to_utf8(i), i);
    }
}

#[test]
fn position_map_two_byte() {
    let text = "const café = 1;\nconst x = 2;";
    let pm = compute_position_map(text);
    assert!(!pm.is_ascii_only());
    for i in 0..10 {
        assert_eq!(pm.utf8_to_utf16(i), i);
    }
    assert_eq!(pm.utf8_to_utf16(9), 9);
    assert_eq!(pm.utf8_to_utf16(11), 10);
    let x_utf8 = u32::try_from(text.rfind('x').unwrap()).unwrap();
    assert_eq!(pm.utf8_to_utf16(x_utf8), x_utf8 - 1);
    assert_eq!(pm.utf16_to_utf8(x_utf8 - 1), x_utf8);
}

#[test]
fn position_map_four_byte() {
    let text = "const a = \"🎉\";\nconst b = 2;";
    let pm = compute_position_map(text);
    assert!(!pm.is_ascii_only());
    let b_utf8 = u32::try_from(text.rfind('b').unwrap()).unwrap();
    let b_utf16 = b_utf8 - 2;
    assert_eq!(pm.utf8_to_utf16(b_utf8), b_utf16);
    assert_eq!(pm.utf16_to_utf8(b_utf16), b_utf8);
}

#[test]
fn position_map_multiple_non_ascii() {
    let pm = compute_position_map("à🎉x");
    for (utf8, utf16) in [(0, 0), (2, 1), (6, 3), (7, 4)] {
        assert_eq!(pm.utf8_to_utf16(utf8), utf16);
        assert_eq!(pm.utf16_to_utf8(utf16), utf8);
    }
}

#[test]
fn position_map_roundtrip() {
    let text = "let café = \"🎉\"; // naïve";
    let pm = compute_position_map(text);
    let utf16_len = pm.utf8_to_utf16(u32::try_from(text.len()).unwrap());
    for i in 0..=utf16_len {
        let utf8_pos = pm.utf16_to_utf8(i);
        assert_eq!(pm.utf8_to_utf16(utf8_pos), i);
    }
}
