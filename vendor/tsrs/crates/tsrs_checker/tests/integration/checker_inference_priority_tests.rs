use tsrs_checker::checker::InferencePriority;

#[test]
fn reachable_priorities_match_integer_semantics() {
    // Circularity keeps its sentinel behavior; finite flags follow the integer operations below.
    let circularity = InferencePriority::CIRCULARITY;
    assert_eq!(InferencePriority::default().memo_bits(), 0);
    assert_eq!(circularity.memo_bits(), u32::MAX);
    assert_eq!(circularity.cmp(&circularity), std::cmp::Ordering::Equal);
    assert_eq!(circularity.min(circularity), circularity);
    assert_eq!(circularity | circularity, circularity);
    assert!(!circularity.intersects(circularity));
    assert_eq!(InferencePriority::from_bits(0x7fff).memo_bits(), 0x7fff);
    for bits in [0x8000, u16::MAX] {
        assert!(std::panic::catch_unwind(|| InferencePriority::from_bits(bits)).is_err());
    }
    // All OR combinations of the twelve public flag bits, including MaxValue.
    for left_bits in 0..4096u16 {
        let left = InferencePriority::from_bits(left_bits);
        assert_eq!(left.memo_bits(), u32::from(left_bits));
        assert_eq!(circularity.cmp(&left), (-1i32).cmp(&i32::from(left_bits)));
        assert_eq!(left.cmp(&circularity), i32::from(left_bits).cmp(&-1));
        assert_ne!(left, circularity);
        assert_eq!(left.min(circularity), circularity);
        assert_eq!(circularity.min(left), circularity);
        assert_eq!(left | circularity, circularity);
        assert_eq!(circularity | left, circularity);
        assert!(!left.intersects(circularity));
        assert!(!circularity.intersects(left));
        for right_bits in 0..4096u16 {
            let right = InferencePriority::from_bits(right_bits);
            assert_eq!((left | right).memo_bits(), u32::from(left_bits | right_bits));
            assert_eq!(left.cmp(&right), i32::from(left_bits).cmp(&i32::from(right_bits)));
            assert_eq!(left == right, left_bits == right_bits);
            assert_eq!(left.min(right).memo_bits(), u32::from(left_bits.min(right_bits)));
            assert_eq!(left.intersects(right), left_bits & right_bits != 0);
        }
    }
}
