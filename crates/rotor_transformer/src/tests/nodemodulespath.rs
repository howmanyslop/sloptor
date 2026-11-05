//! `nodemodulespath_test.go`

use std::collections::HashMap;

use rotor_rojo::canonical_file_name;

use crate::nodemodulespath::{NodeModulesPathMapping, NodeModulesPathPattern};

const PKG_DIR: &str = "/repo/node_modules/@scope/pkg";

/// `filepath.Join(filepath.FromSlash(pkgDir), parts...)`
fn join(parts: &[&str]) -> String {
	let package = rotor_support::gopath::from_slash(PKG_DIR);
	let all: Vec<&str> = std::iter::once(package.as_ref()).chain(parts.iter().copied()).collect();
	rotor_support::gopath::join(&all)
}

fn pattern(types: &str, runtime: &str, case_sensitive: bool) -> NodeModulesPathPattern {
	let canonical = canonical_file_name(types, case_sensitive);
	let (prefix, suffix) = canonical.split_once('*').unwrap_or((canonical.as_str(), ""));
	NodeModulesPathPattern {
		types_prefix: prefix.to_owned(),
		types_suffix: suffix.to_owned(),
		runtime: runtime.to_owned(),
	}
}

#[test]
fn prefers_an_exact_key_over_a_pattern() {
	let mapping = NodeModulesPathMapping {
		exact: HashMap::from([(
			canonical_file_name(&join(&["out", "a.d.ts"]), true),
			join(&["exact", "a.luau"]),
		)]),
		patterns: vec![pattern(&join(&["out", "*.d.ts"]), &join(&["out", "*.luau"]), true)],
	};
	assert_eq!(
		mapping.lookup(&join(&["out", "a.d.ts"]), true),
		Some(join(&["exact", "a.luau"]))
	);
}

#[test]
fn substitutes_every_star_in_the_runtime_target() {
	let mapping = NodeModulesPathMapping {
		exact: HashMap::new(),
		patterns: vec![pattern(
			&join(&["types", "*.d.ts"]),
			&join(&["out", "*", "*.luau"]),
			true,
		)],
	};
	assert_eq!(
		mapping.lookup(&join(&["types", "a.d.ts"]), true),
		Some(join(&["out", "a", "a.luau"]))
	);
}

#[test]
fn keeps_the_import_casing_on_case_insensitive_file_systems() {
	let mapping = NodeModulesPathMapping {
		exact: HashMap::new(),
		patterns: vec![pattern(&join(&["Test", "*.ts"]), &join(&["out-test", "*.luau"]), false)],
	};
	assert_eq!(
		mapping.lookup(&join(&["TEST", "Helpers", "Styles.ts"]), false),
		Some(join(&["out-test", "Helpers", "Styles.luau"]))
	);
}

#[test]
fn does_not_match_when_prefix_and_suffix_overlap() {
	let mapping = NodeModulesPathMapping {
		exact: HashMap::new(),
		patterns: vec![pattern(&join(&["a.d*.d.ts"]), &join(&["*.luau"]), true)],
	};
	assert_eq!(mapping.lookup(&join(&["a.d.ts"]), true), None);
}
