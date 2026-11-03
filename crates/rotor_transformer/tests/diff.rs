//! The differential gate (Go's `internal/diff`): each enabled fixture of `testdata/diff/project`
//! must compile byte-identically to `testdata/diff/golden/<name>.luau`.
//!
//! The enabled fixtures are listed in `tests/fixtures/*.txt`, one list per stage or slice.
//! `differential/<name>` rows of `testdata/forkparity/divergence-ledger.json` classified
//! `upstream-corrected` are exempt from byte equality in Go (their runtime behavior is verified
//! instead); none of Stage 4a's fixtures is such a row, and listing one here fails the gate until
//! that verification is ported.

mod harness;

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;

use rotor_config as _;
use rotor_luau as _;
use rotor_rojo as _;
use rotor_support as _;
use rotor_transformer as _;
use rotor_tsrs as _;
use tempfile as _;

#[expect(clippy::expect_used, reason = "a test harness fails loudly")]
fn enabled_fixtures() -> BTreeSet<String> {
	let directory = harness::repo_root().join("crates/rotor_transformer/tests/fixtures");
	let mut names = BTreeSet::new();
	for entry in fs::read_dir(&directory).expect("the fixture lists exist") {
		let path = entry.expect("a directory entry").path();
		if path.extension().is_some_and(|extension| extension == "txt") {
			let text = fs::read_to_string(&path).expect("the fixture list reads");
			for line in text.lines().map(str::trim) {
				if !line.is_empty() && !line.starts_with('#') {
					names.insert(line.to_owned());
				}
			}
		}
	}
	names
}

#[expect(clippy::expect_used, reason = "a test harness fails loudly")]
fn upstream_corrected_rows() -> BTreeSet<String> {
	let ledger = fs::read_to_string(harness::repo_root().join("testdata/forkparity/divergence-ledger.json"))
		.expect("the divergence ledger reads");
	let mut rows = BTreeSet::new();
	let mut id = None;
	for line in ledger.lines().map(str::trim) {
		if let Some(rest) = line.strip_prefix("\"id\": \"differential/") {
			id = rest.split('"').next().map(str::to_owned);
		} else if line.starts_with("\"classification\": \"upstream-corrected\"")
			&& let Some(name) = id.take()
		{
			rows.insert(name);
		}
	}
	rows
}

fn first_difference(got: &str, want: &str) -> String {
	let (got_lines, want_lines): (Vec<_>, Vec<_>) = (got.split('\n').collect(), want.split('\n').collect());
	for (index, (got_line, want_line)) in got_lines.iter().zip(&want_lines).enumerate() {
		if got_line != want_line {
			return format!(
				"first diff at line {}:\n  got:  {got_line:?}\n  want: {want_line:?}",
				index + 1
			);
		}
	}
	format!(
		"length mismatch: got {} lines, want {}",
		got_lines.len(),
		want_lines.len()
	)
}

#[test]
fn differential_fixtures_match_the_goldens() {
	rotor_tsrs::with_large_stack(|| {
		let root = harness::repo_root();
		let enabled = enabled_fixtures();
		let corrected = upstream_corrected_rows();
		for name in &enabled {
			assert!(
				!corrected.contains(name),
				"{name} is an upstream-corrected ledger row; port diff_test.go's behavioral verification first"
			);
		}

		// One project-wide compile, as Go's diff test: multi-file fixtures import each other.
		let outputs = harness::compile_project(&root.join("testdata/diff/project"));
		let mut failures = String::new();
		let mut seen = BTreeSet::new();
		for name in &enabled {
			let Some(got) = outputs.get(&format!("out/{name}.luau")) else {
				continue;
			};
			seen.insert(name.clone());
			let golden = fs::read(root.join(format!("testdata/diff/golden/{name}.luau"))).expect("the golden reads");
			if *got != golden {
				let _ = writeln!(
					failures,
					"{name}: {}",
					first_difference(&String::from_utf8_lossy(got), &String::from_utf8_lossy(&golden))
				);
			}
		}
		for name in enabled.difference(&seen) {
			let _ = writeln!(failures, "out/{name}.luau missing from CompileProject output");
		}
		assert!(failures.is_empty(), "differential failures:\n{failures}");
	});
}
