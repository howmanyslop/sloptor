//! Port of `internal/diagframe/diagframe_test.go` and
//! `internal/diagframe/keywords_drift_test.go`.

use super::{
	LUAU_KEYWORDS, Language, Options, Severity, Spot, highlight_keywords, is_keyword, line_col_at, line_text, render,
	styler_for,
};

#[test]
fn line_col_at_maps_offsets() {
	let src = "ab\ncde\nf";
	let cases = [
		(0, 1, 1),  // 'a'
		(1, 1, 2),  // 'b'
		(3, 2, 1),  // 'c'
		(6, 2, 4),  // newline after "cde" -> col past 'e'
		(7, 3, 1),  // 'f'
		(99, 3, 2), // clamp past end
	];
	for (offset, want_line, want_col) in cases {
		let (line, col, _) = line_col_at(src, offset);
		assert_eq!((line, col), (want_line, want_col), "offset {offset}");
	}
}

#[test]
fn line_text_strips_cr() {
	let src = "x = 1\r\ny = 2\r\n";
	assert_eq!(line_text(src, 1), "x = 1");
}

#[test]
fn highlight_keywords_luau() {
	let got = highlight_keywords("local function f()", Language::Luau, styler_for(true));
	assert!(got.contains("\x1b["), "expected ANSI codes, got {got:?}");
	assert_eq!(strip_ansi(&got), "local function f()");
}

#[test]
fn highlight_keywords_no_color_is_identity() {
	let input = "const x = 1";
	assert_eq!(
		highlight_keywords(input, Language::TypeScript, styler_for(false)),
		input
	);
}

#[test]
fn keyword_sets() {
	assert!(
		is_keyword("function", Language::Luau) && is_keyword("function", Language::TypeScript),
		"`function` is a keyword in both"
	);
	assert!(
		!is_keyword("print", Language::Luau),
		"`print` is a global, not a reserved keyword"
	);
	assert!(
		is_keyword("const", Language::TypeScript) && !is_keyword("const", Language::Luau),
		"`const` is TS-only"
	);
}

fn strip_ansi(s: &str) -> String {
	let mut out = String::new();
	let mut in_esc = false;
	for c in s.chars() {
		if in_esc {
			if c == 'm' {
				in_esc = false;
			}
		} else if c == '\x1b' {
			in_esc = true;
		} else {
			out.push(c);
		}
	}
	out
}

fn spot(offset: isize, len: isize, message: &str) -> Spot {
	Spot {
		offset,
		len,
		severity: Severity::Error,
		message: message.to_owned(),
		..Spot::default()
	}
}

#[test]
fn render_basic_no_color() {
	let got = render(
		"a.luau",
		"x = 1\n",
		Language::Luau,
		&[spot(0, 1, "bad")],
		Options::default(),
	);
	let want = "error: bad\n  --> a.luau:1:1\n  |\n1 | x = 1\n  | ^ bad\n";
	assert_eq!(got, want, "frame mismatch");
}

#[test]
fn render_help_lines() {
	let sp = Spot {
		help: vec!["do this instead".to_owned()],
		..spot(0, 1, "bad")
	};
	let got = render("a.luau", "x = 1\n", Language::Luau, &[sp], Options::default());
	assert!(got.contains("help: do this instead"), "missing help line:\n{got}");
}

#[test]
fn render_empty_source_falls_back() {
	let got = render("a.luau", "", Language::Luau, &[spot(0, 0, "boom")], Options::default());
	assert_eq!(got, "a.luau:1:1: boom\n");
}

#[test]
fn render_tab_expansion_caret_aligns() {
	let src = "\tbad\n"; // one leading tab, then "bad"
	let got = render("a.luau", src, Language::Luau, &[spot(1, 3, "x")], Options::default());
	// The tab expands to 4 spaces; the caret sits under "bad" (visual col 5).
	assert!(
		got.contains("  |     ^^^ x"),
		"caret not aligned after tab expansion:\n{got:?}"
	);
}

#[test]
fn render_caret_at_eof_without_trailing_newline() {
	let src = "abc"; // no trailing newline; the span starts at EOF
	let got = render("a.luau", src, Language::Luau, &[spot(3, 3, "x")], Options::default());
	let carets = got.matches('^').count();
	assert_eq!(carets, 1, "want a single caret at EOF:\n{got:?}");
}

#[test]
fn luau_keyword_set_matches_canonical() {
	// Every diagframe Luau keyword must be canonical.
	for w in LUAU_KEYWORDS {
		assert!(
			rotor_luau::is_reserved_keyword(w),
			"diagframe lists {w:?} but rotor_luau does not"
		);
	}
	// Every canonical Luau keyword must be in diagframe's set.
	for w in [
		"and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in", "local", "nil", "not",
		"or", "repeat", "return", "then", "true", "until", "while",
	] {
		assert!(
			LUAU_KEYWORDS.contains(&w),
			"canonical keyword {w:?} missing from diagframe"
		);
		assert!(
			rotor_luau::is_reserved_keyword(w),
			"test list has {w:?} but rotor_luau rejects it"
		);
	}
	assert_eq!(LUAU_KEYWORDS.len(), 21, "expected 21 Luau keywords");
}
