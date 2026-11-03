use rustc_hash::FxHashSet;

use tsrs_diagnostics as diagnostics;
use tsrs_diagnostics::generated::ALL_MESSAGES;
use tsrs_diagnostics::{Category, Key, format, format_strings, key_to_message, localize};

#[test]
fn message_count_matches_go() {
    // Count of `&Message{` entries in diagnostics_generated.go at source 79.
    assert_eq!(ALL_MESSAGES.len(), 2229);
    let keys: FxHashSet<&str> = ALL_MESSAGES.iter().map(|m| m.key().as_str()).collect();
    assert_eq!(keys.len(), ALL_MESSAGES.len());
    let codes: FxHashSet<i32> = ALL_MESSAGES.iter().map(|m| m.code()).collect();
    assert_eq!(codes.len(), ALL_MESSAGES.len());
}

#[test]
fn spot_checks() {
    let m = &diagnostics::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1;
    assert_eq!(m.code(), 2322);
    assert_eq!(m.category(), Category::Error);
    assert_eq!(m.key(), Key("Type_0_is_not_assignable_to_type_1_2322"));
    assert_eq!(m.text(), "Type '{0}' is not assignable to type '{1}'.");

    assert_eq!(diagnostics::X_0_EXPECTED.code(), 1005);
    assert_eq!(diagnostics::X_0_EXPECTED.key().as_str(), "_0_expected_1005");

    assert!(
        diagnostics::CALL_SIGNATURE_RETURN_TYPES_0_AND_1_ARE_INCOMPATIBLE
            .elided_in_compatibility_pyramid()
    );
    assert!(!diagnostics::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1.reports_unnecessary());
    assert_eq!(
        diagnostics::X_K_MUST_BE_FOLLOWED_BY_A_CAPTURING_GROUP_NAME_ENCLOSED_IN_ANGLE_BRACKETS
            .text(),
        "'\\k' must be followed by a capturing group name enclosed in angle brackets."
    );
    assert_eq!(
        diagnostics::MODULE_DECLARATION_NAMES_MAY_ONLY_USE_OR_QUOTED_STRINGS.text(),
        "Module declaration names may only use ' or \" quoted strings."
    );

    assert!(ALL_MESSAGES.iter().any(|m| m.reports_unnecessary()));
    assert!(ALL_MESSAGES.iter().any(|m| m.reports_deprecated()));
    assert!(ALL_MESSAGES.iter().any(|m| m.category() == Category::Suggestion));
    assert!(ALL_MESSAGES.iter().any(|m| m.category() == Category::Message));
}

#[test]
fn category_names() {
    assert_eq!(Category::Error.name(), "error");
    assert_eq!(Category::Warning.name(), "warning");
    assert_eq!(Category::Suggestion.name(), "suggestion");
    assert_eq!(Category::Message.name(), "message");
    assert_eq!(Category::Error.to_string(), "CategoryError");
    assert_eq!(Category::Warning as i32, 0);
    assert_eq!(Category::Message as i32, 3);
}

#[test]
fn key_lookup() {
    let m = key_to_message("Identifier_expected_1003").unwrap();
    assert!(std::ptr::eq(m, &raw const diagnostics::IDENTIFIER_EXPECTED));
    assert!(key_to_message("Removed_diagnostic_99999").is_none());
    assert_eq!(localize(None, Key("_0_expected_1005"), &[")".to_string()]), "')' expected.");
    assert_eq!(localize(None, Key("Identifier_expected_1003"), &[]), "Identifier expected.");
}

#[test]
fn formatting() {
    assert_eq!(diagnostics::IDENTIFIER_EXPECTED.format(&[]), "Identifier expected.");
    assert_eq!(diagnostics::X_0_EXPECTED.format(&[&")"]), "')' expected.");
    assert_eq!(
        diagnostics::THE_PARSER_EXPECTED_TO_FIND_A_1_TO_MATCH_THE_0_TOKEN_HERE
            .format(&[&"{", &"}"]),
        "The parser expected to find a '}' to match the '{' token here."
    );
    assert_eq!(
        diagnostics::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1.localize(&[&"string", &42]),
        "Type 'string' is not assignable to type '42'."
    );
    // Substituted text is not re-scanned; repeated and out-of-order placeholders work.
    assert_eq!(format("{1}{0}{1}", &[&"{1}", &"b"]), "b{1}b");
    // Only `{digits}` is a placeholder.
    assert_eq!(format("{{0}} {x} {} {0", &[&"a"]), "{a} {x} {} {0");
    // No args: text is returned unchanged even with placeholders (Go early return).
    assert_eq!(format("'{0}'", &[]), "'{0}'");
    assert_eq!(format_strings("é{0}ü", &["ß".to_string()]), "éßü");
}

#[test]
#[should_panic(expected = "Invalid formatting placeholder")]
fn formatting_missing_arg_panics() {
    format("{0} {1}", &[&"a"]);
}

#[test]
fn ad_hoc_message() {
    let m = diagnostics::Message::ad_hoc("hello {0}");
    assert_eq!(m.code(), -1);
    assert_eq!(m.category(), Category::Error);
    assert_eq!(m.key().as_str(), "-1");
    assert_eq!(m.format(&[&"world"]), "hello world");
}
