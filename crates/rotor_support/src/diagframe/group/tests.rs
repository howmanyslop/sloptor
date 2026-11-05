//! Port of `internal/diagframe/group_test.go`.

use super::{Group, render_groups};
use crate::diagframe::{Language, Options, Severity, Spot};

fn spot(severity: Severity, message: &str) -> Spot {
	Spot {
		offset: 0,
		len: 1,
		severity,
		message: message.to_owned(),
		..Spot::default()
	}
}

fn group(path: &str, source: &str, spots: Vec<Spot>) -> Group {
	Group {
		path: path.to_owned(),
		source: source.to_owned(),
		lang: Language::Luau,
		spots,
	}
}

#[test]
fn render_groups_summary_and_order() {
	let groups = [
		group("b.luau", "y = 2\n", vec![spot(Severity::Error, "b err")]),
		group(
			"a.luau",
			"x = 1\n",
			vec![spot(Severity::Error, "a err"), spot(Severity::Warning, "a warn")],
		),
	];
	let out = render_groups(&groups, Options::default(), 0);
	assert!(
		out.find("a.luau") <= out.find("b.luau"),
		"files not sorted (a.luau should come before b.luau)"
	);
	assert!(
		out.contains("2 errors") && out.contains("1 warning"),
		"summary counts wrong:\n{out}"
	);
	assert!(out.contains("in 2 files"), "file count wrong:\n{out}");
}

#[test]
fn render_groups_truncation() {
	let spots = (0..5).map(|_| spot(Severity::Error, "e")).collect();
	let groups = [group("a.luau", "x = 1\n", spots)];
	let out = render_groups(&groups, Options::default(), 2);
	assert!(out.contains("and 3 more"), "expected truncation note:\n{out}");
}

#[test]
fn render_groups_no_color_is_ascii() {
	let groups = [group(
		"a.luau",
		"x = 1\n",
		vec![spot(Severity::Error, "e"), spot(Severity::Warning, "w")],
	)];
	let out = render_groups(&groups, Options::default(), 1); // forces a truncation note too
	assert!(out.is_ascii(), "non-ASCII byte in no-color output:\n{out:?}");
}
