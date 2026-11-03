use tsrs_core::glob::glob::*;

fn matches(pattern: &str, input: &str) -> bool {
    parse(pattern).unwrap().match_(input)
}

#[test]
fn test_parse_round_trip() {
    for pattern in ["**/*.{ts,js}", "a/b/c", "example.[0-9]", "src/**/?.d.ts", "{a,{b,c}}/*"] {
        assert_eq!(parse(pattern).unwrap().string(), pattern);
    }
}

#[test]
fn test_parse_errors() {
    assert_eq!(parse("a**").unwrap_err(), "** may only be adjacent to '/'");
    assert_eq!(parse("**a").unwrap_err(), "** may only be adjacent to '/'");
    assert_eq!(parse("{a,b").unwrap_err(), "unmatched '{'");
    assert_eq!(parse("[a]").unwrap_err(), ERR_BAD_RANGE);
    assert_eq!(parse("[").unwrap_err(), ERR_BAD_RANGE);
    assert_eq!(parse("[a-").unwrap_err(), ERR_BAD_RANGE);
    assert!(parse("a,b}").is_ok());
}

#[test]
fn test_match() {
    // `**/*.{ts,js}` matches all TypeScript and JavaScript files
    assert!(matches("**/*.{ts,js}", "a.ts"));
    assert!(matches("**/*.{ts,js}", "src/deep/a.js"));
    assert!(!matches("**/*.{ts,js}", "src/a.tsx"));
    // `example.[0-9]` to match on `example.0`, `example.1`, …
    assert!(matches("example.[0-9]", "example.0"));
    assert!(matches("example.[0-9]", "example.9"));
    assert!(!matches("example.[0-9]", "example.a"));
    // '/' matches one or more literal slashes.
    assert!(matches("a/b", "a//b"));
    assert!(!matches("a/b", "ab"));
    // `?` matches one character in a path segment
    assert!(matches("a?c", "abc"));
    assert!(!matches("a?c", "a/c"));
    // `*` matches within a path segment only
    assert!(matches("src/*.ts", "src/a.ts"));
    assert!(!matches("src/*.ts", "src/x/a.ts"));
    assert!(matches("src/*", "src/anything"));
    // **/a matches "a"; trailing ** matches everything
    assert!(matches("**/a", "a"));
    assert!(matches("**/a", "x/y/a"));
    assert!(matches("src/**", "src/x/y"));
    assert!(matches("/dir/**/*", "/dir/a/b.ts"));
}
