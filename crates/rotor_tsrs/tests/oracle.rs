//! The Go-vs-Rust differential: `rotor_tsrs::dump` against `tools/tsrs-oracle`.
//!
//! The conformance test compares with the committed Go dump in `testdata/tsrs-oracle`, so it needs
//! no Go; regenerate that with `mise run tsrs:oracle`. The ignored test compares the project at
//! `ROTOR_BENCH_PROJECT` (a tsconfig) with the Go dump at `ROTOR_BENCH_ORACLE`; `mise run
//! tsrs:oracle-bench` writes the dump and runs it.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use rotor_tsrs::dump::{Comparison, Deviation, compare, dump_project};
use rotor_tsrs::{ProjectOptions, load_project, with_large_stack};

// An integration test is handed every dependency of its package, so the ones only the library
// uses must be acknowledged to keep `unused_crate_dependencies` quiet.
use regex as _;
use rotor_support as _;
use serde_json as _;
use tempfile as _;
use tsrs_ast as _;
use tsrs_checker as _;
use tsrs_compiler as _;
use tsrs_contentmapper as _;
use tsrs_core as _;
use tsrs_diagnostics as _;
use tsrs_module as _;
use tsrs_parser as _;
use tsrs_printer as _;
use tsrs_scanner as _;
use tsrs_sourcemap as _;
use tsrs_tsoptions as _;
use tsrs_vfs as _;

fn repository() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn compare_project(tsconfig: &Path, go_dump: &Path) -> Result<Comparison, String> {
	let go = std::fs::read_to_string(go_dump).map_err(|error| format!("{}: {error}", go_dump.display()))?;
	let options = ProjectOptions {
		checkers: Some(1),
		..ProjectOptions::default()
	};
	let rust =
		with_large_stack(|| load_project(tsconfig, &options).map(dump_project)).map_err(|error| error.to_string())?;
	Ok(compare(&go, &rust))
}

fn describe(comparison: &Comparison) -> String {
	let mut text = format!(
		"go lines {}, rust lines {}, matched {}, deviating {}, deviations {:?}, mismatches {}\n",
		comparison.go_lines,
		comparison.rust_lines,
		comparison.matched,
		comparison.deviating,
		comparison.deviations,
		comparison.mismatches.len()
	);
	for mismatch in comparison.mismatches.iter().take(20) {
		let _ = writeln!(
			text,
			"{}\n  go:   {}\n  rust: {}",
			mismatch.key,
			mismatch.go.as_deref().unwrap_or("(none)"),
			mismatch.rust.as_deref().unwrap_or("(none)")
		);
	}
	text
}

#[test]
fn conformance_project_matches_the_go_oracle() {
	let root = repository();
	let comparison = compare_project(
		&root.join("testdata/conformance/project/tsconfig.json"),
		&root.join("testdata/tsrs-oracle/conformance.tsv"),
	)
	.expect("the conformance project compares");
	assert!(comparison.mismatches.is_empty(), "{}", describe(&comparison));
	// The documented TypeScript-version differences, counted in crates/rotor_tsrs/ORACLE.md.
	let expected = BTreeMap::from([
		(Deviation::JsxSpreadAttributeDeclaration, 12),
		(Deviation::TypeToStringTruncation, 36),
		(Deviation::EnumDynamicMemberName, 3),
		(Deviation::UnionPropertyDeclarations, 6),
		(Deviation::InstantiationExpressionDeclarations, 4),
		(Deviation::DeclaredConditionalConstraint, 2),
	]);
	assert_eq!(comparison.deviations, expected, "{}", describe(&comparison));
	assert_eq!(
		(comparison.matched, comparison.deviating),
		(29_628, 63),
		"{}",
		describe(&comparison)
	);
}

#[test]
#[ignore = "needs ROTOR_BENCH_PROJECT (a tsconfig) and ROTOR_BENCH_ORACLE (its Go dump)"]
fn benchmark_project_matches_the_go_oracle() {
	let tsconfig = std::env::var_os("ROTOR_BENCH_PROJECT").expect("set ROTOR_BENCH_PROJECT to a tsconfig path");
	let go_dump = std::env::var_os("ROTOR_BENCH_ORACLE").expect("set ROTOR_BENCH_ORACLE to the Go dump");
	let comparison =
		compare_project(Path::new(&tsconfig), Path::new(&go_dump)).expect("the benchmark project compares");
	println!("{}", describe(&comparison));
	assert!(comparison.mismatches.is_empty(), "{}", describe(&comparison));
}
