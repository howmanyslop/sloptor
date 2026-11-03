use std::borrow::Cow;
use tsrs_core::ScriptKind;

use tsrs_core::core_go::*;

#[test]
fn index_bytes_matches_memmem() {
    let many_fails = format!("{}ab", "a".repeat(500));
    let multibyte = format!("{}éb", "éa".repeat(200));
    let cases: [(&str, &str); 14] = [
        ("hello world", "world"),
        ("hello world", "worlds"),
        ("hello", "hello"),
        ("hell", "hello"),
        ("", "a"),
        ("abc", ""),
        ("abcabc", "c"),
        (&many_fails, "ab"),
        (&many_fails, "aab"),
        (&many_fails, "ac"),
        (&multibyte, "éb"),
        (&multibyte, "aé"),
        (&multibyte, "ab"),
        ("日本語のテキスト", "テキ"),
    ];
    for (text, needle) in cases {
        for start in 0..=text.len() {
            let haystack = &text.as_bytes()[start..];
            assert_eq!(
                index_bytes(haystack, needle.as_bytes()),
                memchr::memmem::find(haystack, needle.as_bytes()),
                "{needle:?} in {text:?} from {start}"
            );
        }
    }
}

#[test]
fn helpers() {
    let v = [1, 2, 3, 4];
    assert!(matches!(filter(&v, |x| *x > 0), Cow::Borrowed(_)));
    assert_eq!(&*filter(&v, |x| x % 2 == 0), &[2, 4]);
    assert_eq!(&*splice(&v, SpliceStart::FromStart(1), 2, &[9]), &[1, 9, 4]);
    assert_eq!(&*splice(&v, SpliceStart::FromEnd(1), 1, &[]), &[1, 2, 3]);
    assert_eq!(&*deduplicate(&[1, 2, 1, 3, 2]), &[1, 2, 3]);
    assert!(same(&v[..], &v[..]));
    assert!(!same(&v[..], &[1, 2, 3, 4][..]));
    let mut s = vec![1, 3, 5];
    insert_sorted(&mut s, 4, Ord::cmp);
    assert_eq!(s, vec![1, 3, 4, 5]);
    assert_eq!(compute_ecma_line_starts("a\r\nb\nc\u{2028}d\re"), vec![0, 3, 5, 9, 11]);
    assert_eq!(position_to_line_and_byte_offset(4, &[0, 3, 5]), (1, 1));
    assert_eq!(utf16_len("a😀é"), 4);
    assert_eq!(get_script_kind_from_file_name("a.D.TS"), ScriptKind::TS);
}

#[test]
fn line_start_count() {
    for text in [
        "",
        "a",
        "\n",
        "\r",
        "\r\n",
        "\n\r",
        "a\r\nb\nc\u{2028}d\re",
        "x\r",
        "\r\r\n\n",
        "\u{2029}\u{2028}",
        "é\u{2020}\u{2027}\u{202A}\u{2029}é\r\n",
        "\u{2028}\r\n\u{E280}",
    ] {
        let mut expected = 0;
        compute_ecma_line_starts_seq(text, |_| {
            expected += 1;
            true
        });
        assert_eq!(count_ecma_line_starts(text), expected, "{text:?}");
    }
}

#[test]
fn spelling() {
    let names = ["foo", "bar", "length", "lengthy_name"];
    assert_eq!(
        get_spelling_suggestion_for_strings("lenght", names.iter().copied()),
        Some("length")
    );
    assert_eq!(get_spelling_suggestion_for_strings("xyzzy", names.iter().copied()), None);
    assert_eq!(get_spelling_suggestion_for_strings("Foo", names.iter().copied()), Some("foo"));
    assert_eq!(get_spelling_suggestion_for_strings("École", ["école"]), Some("école"));
    assert_eq!(get_spelling_suggestion_for_strings("Ecole", ["école"]), Some("école"));
}

#[test]
fn levenshtein_reuses_banded_buffers() {
    fn unbounded_distance(s1: &[u8], s2: &[u8]) -> f64 {
        let mut previous = (0..=s2.len()).map(|i| i as f64).collect::<Vec<_>>();
        let mut current = vec![0.0; s2.len() + 1];
        for (i, &c1) in s1.iter().enumerate() {
            current[0] = (i + 1) as f64;
            for (j, &c2) in s2.iter().enumerate() {
                let substitution_distance =
                    previous[j] + if c1.eq_ignore_ascii_case(&c2) { 0.1 } else { 2.0 };
                current[j + 1] = if c1 == c2 {
                    previous[j]
                } else {
                    (previous[j + 1] + 1.0).min((current[j] + 1.0).min(substitution_distance))
                };
            }
            std::mem::swap(&mut previous, &mut current);
        }
        previous[s2.len()]
    }

    let mut strings = vec![Vec::new()];
    let mut previous_length = vec![Vec::new()];
    for _ in 0..3 {
        let mut next_length = Vec::new();
        for prefix in &previous_length {
            for &byte in b"aAb" {
                let mut value = prefix.clone();
                value.push(byte);
                next_length.push(value);
            }
        }
        strings.extend(next_length.iter().cloned());
        previous_length = next_length;
    }

    let mut buffers = LevenshteinBuffers::default();
    for s1 in &strings {
        for s2 in strings.iter().rev() {
            let distance = unbounded_distance(s1, s2);
            for max_value in [0.0, 0.1, 0.9, 1.0, 1.1, 1.9, 2.0, 2.9, 4.0] {
                let expected = (distance <= max_value).then_some(distance);
                assert_eq!(
                    levenshtein_with_max(&mut buffers, s1, s2, max_value, |c| {
                        c.to_ascii_lowercase()
                    }),
                    expected,
                    "s1={s1:?}, s2={s2:?}, max={max_value}"
                );
            }
        }
    }
}
