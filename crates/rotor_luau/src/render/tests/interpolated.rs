//! The Go renderer escapes interpolated string parts with two regexes; the
//! port scans by hand. Brute-force short strings against the original regexes
//! to prove the outputs match.

use regex::Regex;

use crate::render::fields::escape_interpolated_part;

#[test]
fn interpolated_string_part_matches_regex_oracle() {
	let braces = Regex::new(r"(\\u\{[a-fA-F0-9]+\})|([{}])").expect("regex");
	let newlines = Regex::new("(\r\n?|\n)").expect("regex");
	let oracle = |text: &str| {
		let escaped = braces.replace_all(text, |captures: &regex::Captures<'_>| {
			let matched = &captures[0];
			if matched == "{" || matched == "}" {
				format!("\\{matched}")
			} else {
				matched.to_owned()
			}
		});
		newlines.replace_all(&escaped, "\\$1").into_owned()
	};
	let alphabet = ['\\', 'u', '{', '}', 'a', 'F', 'g', '\r', '\n', 'é'];
	let mut level = vec![String::new()];
	let mut inputs = level.clone();
	for _ in 0..4 {
		level = level
			.iter()
			.flat_map(|prefix| alphabet.iter().map(move |c| format!("{prefix}{c}")))
			.collect();
		inputs.extend(level.iter().cloned());
	}
	inputs.extend(["\\u{1F600}", "\\u{}", "\\u{12", "a\\u{FFFF}b{c}", "x\r\ny\rz\n"].map(str::to_owned));
	for input in inputs {
		assert_eq!(escape_interpolated_part(&input), oracle(&input), "part {input:?}");
	}
}
