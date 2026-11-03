use tsrs_core::pattern::*;

#[test]
fn test_pattern_overlapping_match() {
    let p = try_parse_pattern("ab*ab");
    assert!(!p.matches("ab"));
    assert!(p.matches("abXab"));
    assert_eq!(p.matched_text("abXab"), "X");
    assert!(p.matches("abab"));
    assert_eq!(p.matched_text("abab"), "");
    assert_eq!(try_parse_pattern("a*b*c"), Pattern::default());
}
