use tsrs_core::text::*;

#[test]
fn positions_are_compact_and_preserve_order() {
    assert_eq!(std::mem::size_of::<TextPos>(), 4);
    assert_eq!(std::mem::size_of::<TextRange>(), 8);
    assert!(TextPos::Synthetic < TextPos::source(0));
    assert!(TextPos::source(2) < TextPos::source(10));
    assert_eq!(TextPos::source(SourcePos::MAX).source_offset(), Some(SourcePos::MAX));

    let ordered = [
        TextPos::Synthetic,
        TextPos::source(0),
        TextPos::source(1),
        TextPos::source(i32::MAX as u32),
        TextPos::source(SourcePos::MAX),
    ];
    for (i, a) in ordered.iter().enumerate() {
        for (j, b) in ordered.iter().enumerate() {
            assert_eq!(a.cmp(b), i.cmp(&j));
            assert_eq!(a.partial_cmp(b), Some(i.cmp(&j)));
            assert_eq!(a == b, i == j);
            assert_eq!(a < b, i < j);
        }
    }
}

#[test]
fn synthetic_ranges_have_no_source_offsets() {
    let range = TextRange::synthetic();
    assert_eq!(range.source_pos(), None);
    assert_eq!(range.source_end(), None);
    assert_eq!(range.len(), None);
    assert!(!range.is_valid());
}

#[test]
fn lengths_cover_source_boundaries_and_mixed_ranges() {
    assert_eq!(TextRange::new(0, SourcePos::MAX).len(), Some(SourcePos::MAX));
    assert_eq!(TextRange::new(SourcePos::MAX, SourcePos::MAX).len(), Some(0));
    assert_eq!(TextRange::from_text_pos(TextPos::Synthetic, TextPos::source(1)).len(), None);
    assert_eq!(TextRange::from_text_pos(TextPos::source(1), TextPos::Synthetic).len(), None);
}

#[test]
#[should_panic(expected = "source position exceeds the supported range")]
fn source_position_rejects_unrepresentable_offset() {
    SourcePos::new(u32::MAX);
}

#[test]
#[should_panic(expected = "text range ends before it starts")]
fn range_length_rejects_reversed_source_range() {
    TextRange::new(2, 1).len();
}
