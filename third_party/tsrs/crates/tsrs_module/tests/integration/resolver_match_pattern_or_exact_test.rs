use tsrs_core::Pattern;
use tsrs_core::PatternKind;
use tsrs_core::collections::OrderedMap;

use tsrs_module::resolver::*;

#[test]
fn matches_like_find_best_pattern_match() {
    let mut paths: OrderedMap<String, Vec<String>> = OrderedMap::default();
    for key in ["@a/*", "@a/b/*", "@a/b/*.js", "*", "@a/b/c", "@a/*/d", "x*", "@a/b/*", "@a/bc*"] {
        paths.insert(key.to_string(), vec![]);
    }
    let parsed = try_parse_patterns(Some(&paths));
    for candidate in [
        "@a/b/c",
        "@a/b/x",
        "@a/b/x.js",
        "@a/q/d",
        "@a/bcd",
        "xyz",
        "zzz",
        "@a/",
        "@a",
        "",
        "@a/b/",
    ] {
        let expected = if parsed.matchable_string_set.m.contains(candidate) {
            Pattern { text: candidate.to_string(), kind: PatternKind::Exact }
        } else {
            tsrs_core::find_best_pattern_match(&parsed.patterns, |p| p, candidate)
                .unwrap_or_default()
        };
        assert_eq!(match_pattern_or_exact(&parsed, candidate), expected, "{candidate}");
    }
}
