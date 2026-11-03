//! Cases that pin the decoder to the `BurntSushi` behavior the Go `Load` has.
//! The expected text was captured from the Go implementation.

use super::{decode, key_string};

fn undecoded(source: &str) -> Vec<String> {
	decode(source)
		.expect("decodes")
		.undecoded
		.iter()
		.map(|key| key_string(key))
		.collect()
}

fn error(source: &str) -> String {
	decode(source).err().expect("fails")
}

#[test]
fn undecoded_keys_follow_document_order_and_header_shape() {
	assert_eq!(undecoded("[assets.sub]\nmode='x'\n"), ["assets.sub", "assets.sub.mode"]);
	assert_eq!(undecoded("a = {b = 1, c = {d = 2}}\n"), ["a", "a.b", "a.c", "a.c.d"]);
	assert_eq!(
		undecoded("x = [{a=1},{a=2}]\ny = [[1],[2]]\n"),
		["x", "x.a", "x.a", "y"]
	);
	assert_eq!(undecoded("[a.b]\nc=1\n[a]\nd=2\n"), ["a.b", "a.b.c", "a", "a.d"]);
	assert_eq!(
		undecoded("[[z]]\nq=1\n[z.sub]\nk=1\n[[z]]\nq=2\n"),
		["z", "z.q", "z.sub", "z.sub.k", "z", "z.q"]
	);
	assert_eq!(undecoded("a.b = 1\na.c = 2\n"), ["a.b", "a.c"]);
	assert_eq!(undecoded("[t]\na.b.c = 1\n"), ["t", "t.a.b.c"]);
	assert_eq!(
		undecoded("[ 'q.r' ]\nk=1\n[\"é\".\"a b\"]\nz=1\n"),
		["\"q.r\"", "\"q.r\".k", "\"é\".\"a b\"", "\"é\".\"a b\".z"]
	);
	assert_eq!(undecoded("d = 1979-05-27\n"), ["d"]);
	assert_eq!(undecoded("Warnings = [\"x\"]\n"), ["Warnings"]);
}

#[test]
fn keys_match_fields_case_insensitively() {
	let decoded = decode("nativetypes = true\n").expect("decodes");
	assert_eq!(decoded.config.native_types, Some(true));
	assert_eq!(decoded.undecoded, Vec::<Vec<String>>::new());
}

#[test]
fn type_mismatches_use_burntsushi_wording() {
	assert_eq!(
		error("flamework = 5\n"),
		"toml: line 1 (last key \"flamework\"): type mismatch for config.FlameworkConfig: expected table but found int64"
	);
	assert_eq!(
		error("[parallelTransformers]\nworkers = \"x\"\n"),
		"toml: line 2 (last key \"parallelTransformers.workers\"): incompatible types: TOML value has type string; destination has type integer"
	);
	assert_eq!(
		error("[parallelTransformers]\nplugins = 3\n"),
		"toml: line 2 (last key \"parallelTransformers.plugins\"): incompatible types: TOML value has type int64; destination has type slice"
	);
	assert_eq!(
		error("[parallelTransformers]\nplugins = [1]\n"),
		"toml: line 2 (last key \"parallelTransformers.plugins\"): type mismatch for config.ParallelTransformerPlugin: expected table but found int64"
	);
	assert_eq!(
		error("nativeTypes = 07:32:00\n"),
		"toml: line 1 (last key \"nativeTypes\"): incompatible types: TOML value has type time.Time; destination has type boolean"
	);
	assert_eq!(
		error("nativeTypes = [[1]]\n"),
		"toml: line 1 (last key \"nativeTypes\"): incompatible types: TOML value has type []any; destination has type boolean"
	);
	assert_eq!(
		error("[flamework.profiles.\"a.json\"]\nafter = 4\n"),
		"toml: line 2 (last key \"flamework.profiles.\\\"a.json\\\".after\"): incompatible types: TOML value has type int64; destination has type string"
	);
}

#[test]
fn a_non_table_profiles_value_is_ignored() {
	// BurntSushi's `unifyMap` returns without error for a value that is not a table.
	let decoded = decode("[flamework]\nprofiles = 3\n").expect("decodes");
	assert!(
		decoded
			.config
			.flamework
			.is_some_and(|flamework| flamework.profiles.is_empty())
	);
	assert_eq!(decoded.undecoded, Vec::<Vec<String>>::new());
}

#[test]
fn syntax_errors_report_the_line() {
	assert!(error("x = \"unterminated\n").starts_with("toml: line 1: "));
	assert!(error("[a]\n[a]\n").starts_with("toml: line 2: "));
}

#[test]
fn a_byte_order_mark_is_skipped() {
	let decoded = decode("\u{feff}nativeTypes = true\n").expect("decodes");
	assert_eq!(decoded.config.native_types, Some(true));
}
