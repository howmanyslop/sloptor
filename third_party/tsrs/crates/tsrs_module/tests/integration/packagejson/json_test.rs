use tsrs_module::packagejson::json::*;

#[test]
fn parses_values_in_order() {
    let v =
        parse(r#" {"b": [1, -2.5e3, true, false, null], "a": "x\u00e9\ud83d\ude00\n", "b": {}} "#)
            .unwrap();
    let Json::Object(members) = v else { panic!() };
    assert_eq!(members.len(), 3);
    assert_eq!(members[0].0, "b");
    assert_eq!(members[1].1, Json::String("x\u{e9}\u{1F600}\n".to_string()));
    assert_eq!(
        members[0].1,
        Json::Array(vec![
            Json::Number(1.0),
            Json::Number(-2500.0),
            Json::Bool(true),
            Json::Bool(false),
            Json::Null
        ])
    );
}

#[test]
fn rejects_invalid_json() {
    #[expect(
        clippy::literal_string_with_formatting_args,
        reason = "These are deliberately malformed JSON fixtures, including an unquoted property name."
    )]
    for text in [
        "",
        "{",
        "{,}",
        "[1,]",
        "{\"a\":1,}",
        "01",
        "1.",
        "'a'",
        "{} x",
        "\"\u{1}\"",
        "{a:1}",
        "\"\\ud800\"",
        "1e999",
    ] {
        assert!(parse(text).is_err(), "{text:?}");
    }
}
