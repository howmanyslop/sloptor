use tsrs_core::stringutil::rangetable::Rune;
use tsrs_core::stringutil::util::encode_js_string_rune;

use tsrs_core::stringutil::js_case::*;

#[test]
fn test_js_casing() {
    let e = |r: Rune| encode_js_string_rune(r);
    let tests: Vec<(&str, String, String)> = vec![
        ("ascii lowercase", to_lower_js("HELLO"), "hello".into()),
        ("ascii uppercase", to_upper_js("hello"), "HELLO".into()),
        ("lowercase dotted i", to_lower_js("İSPANYOL"), "i̇spanyol".into()),
        ("lowercase lone sigma", to_lower_js("Σ"), "σ".into()),
        ("lowercase final sigma", to_lower_js("ΟΣ"), "ος".into()),
        ("lowercase non-sigma greek", to_lower_js("Ω"), "ω".into()),
        ("uppercase sharp s", to_upper_js("ßfoo"), "SSFOO".into()),
        ("uppercase non-ascii simple mapping", to_upper_js("ω"), "Ω".into()),
        ("uppercase ligature", to_upper_js("ﬁoo"), "FIOO".into()),
        ("capitalize-style uppercase", to_upper_js("ß") + "foo", "SSfoo".into()),
        ("uncapitalize-style lowercase", to_lower_js("İ") + "foo", "i̇foo".into()),
        (
            "final sigma after lowercase letter without uppercase mapping",
            to_lower_js("ʕΣ"),
            "ʕς".into(),
        ),
        ("sigma after modifier letter", to_lower_js("ʰΣ"), "ʰσ".into()),
        ("sigma after case ignorable ypogegrammeni", to_lower_js("ͅΣ"), "ͅσ".into()),
        ("final sigma after feminine ordinal indicator", to_lower_js("ªΣ"), "ªς".into()),
        ("final sigma after masculine ordinal indicator", to_lower_js("ºΣ"), "ºς".into()),
        ("final sigma after roman numeral", to_lower_js("ⅠΣ"), "ⅰς".into()),
        (
            "sigma after uppercase property added after unicode 15",
            to_lower_js("\u{1C89}Σ"),
            "\u{1C89}σ".into(),
        ),
        ("sigma after uppercase property skewed", to_lower_js("\u{A7CB}Σ"), "\u{A7CB}σ".into()),
        ("sigma before immediate latin letter", to_lower_js("ΣA"), "σa".into()),
        ("sigma before immediate roman numeral letter", to_lower_js("ΣⅠ"), "σⅰ".into()),
        ("sigma before case ignorable then latin letter", to_lower_js("ΣͅA"), "σͅa".into()),
        ("uppercase lone surrogate", to_upper_js(&e(0xD800)), e(0xD800)),
        (
            "lowercase lone surrogate",
            to_lower_js(&("A".to_string() + &e(0xD800) + "B")),
            "a".to_string() + &e(0xD800) + "b",
        ),
        (
            "uppercase lone low surrogate with text",
            to_upper_js(&(e(0xDC00) + "x")),
            e(0xDC00) + "X",
        ),
        ("lowercase lone surrogate before sigma", to_lower_js(&(e(0xD800) + "Σ")), e(0xD800) + "σ"),
    ];
    for (name, got, want) in tests {
        assert!(
            got.as_bytes() == want.as_bytes(),
            "{name}: got {:?}, want {:?}",
            got.as_bytes(),
            want.as_bytes()
        );
    }
}
