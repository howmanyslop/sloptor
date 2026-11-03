//! `glob_test.go`

use std::collections::BTreeMap;

use rotor_rojo::{PathTranslator, ResolverFilePathState, ResolverState, RojoResolver};

use super::{join, temp_dir};
use crate::ErrorKind;
use crate::glob::{GlobPaths, expand_globs, marshal_globs_artifact, resolve_glob_paths};

fn strings(values: &[&str]) -> Vec<String> {
	values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn expand_globs_matches_case_insensitively_and_translates() {
	let (_dir, root) = temp_dir();
	for rel in ["src/Alpha.TS", "src/nested/index.ts", "src/.hidden.ts", "src/data.json"] {
		let path = join(&[&root, rel]);
		std::fs::create_dir_all(rotor_support::gopath::dir(&path)).unwrap();
		std::fs::write(&path, "").unwrap();
	}
	let translator = PathTranslator::new(&join(&[&root, "src"]), &join(&[&root, "out"]), "", false, false);
	let got = expand_globs(&root, &translator, &strings(&["src/**/*.ts", "src/*.json"])).unwrap();
	let want: BTreeMap<String, Vec<String>> = [
		(
			"src/**/*.ts".to_owned(),
			strings(&["out/Alpha.TS", "out/nested/init.lua"]),
		),
		("src/*.json".to_owned(), strings(&["out/data.json"])),
	]
	.into();
	assert_eq!(got, want);
}

#[test]
fn expand_globs_rejects_pattern_escape() {
	let (_dir, root) = temp_dir();
	let translator = PathTranslator::new(&join(&[&root, "src"]), &join(&[&root, "out"]), "", false, false);
	let error = expand_globs(&root, &translator, &strings(&["../**/*.ts"])).unwrap_err();
	assert!(error.is(ErrorKind::PathEscape), "{error}");
}

#[test]
fn expand_globs_rejects_invalid_pattern_without_files() {
	let (_dir, root) = temp_dir();
	let translator = PathTranslator::new(&join(&[&root, "src"]), &join(&[&root, "out"]), "", false, false);
	let error = expand_globs(&root, &translator, &strings(&["src/[.ts"])).unwrap_err();
	assert!(error.is(ErrorKind::InvalidGlob), "{error}");
}

#[test]
fn resolve_glob_paths_preserves_empty_patterns_and_skips_unmapped_files() {
	let (_dir, root) = temp_dir();
	let mapped = join(&[&root, "out", "mapped.luau"]);
	let resolver = RojoResolver::from_state(&ResolverState {
		file_paths: vec![ResolverFilePathState {
			path: mapped,
			rbx_path: Some(strings(&["ReplicatedStorage", "mapped"])),
		}],
		..ResolverState::default()
	})
	.unwrap();
	let expanded: BTreeMap<String, Vec<String>> = [
		(
			"src/**/*.ts".to_owned(),
			strings(&["out/mapped.luau", "out/unmapped.luau"]),
		),
		("src/*.json".to_owned(), Vec::new()),
	]
	.into();
	let got = resolve_glob_paths(&root, Some(&resolver), &expanded).unwrap();
	let want: GlobPaths = [
		(
			"src/**/*.ts".to_owned(),
			vec![strings(&["ReplicatedStorage", "mapped"])],
		),
		("src/*.json".to_owned(), Vec::new()),
	]
	.into();
	assert_eq!(got, want);
}

#[test]
fn resolve_glob_paths_returns_empty_paths_without_rojo_config() {
	let (_dir, root) = temp_dir();
	let expanded: BTreeMap<String, Vec<String>> = [("src/**/*.ts".to_owned(), strings(&["out/file.luau"]))].into();
	let got = resolve_glob_paths(&root, None, &expanded).unwrap();
	let want: GlobPaths = [("src/**/*.ts".to_owned(), Vec::new())].into();
	assert_eq!(got, want);
}

#[test]
fn marshal_globs_artifact_is_deterministic() {
	let game: GlobPaths = [
		("z".to_owned(), vec![strings(&["ServerStorage", "Z"])]),
		("a".to_owned(), Vec::new()),
	]
	.into();
	let packages: BTreeMap<String, GlobPaths> = [
		(
			"pkg-b".to_owned(),
			[("q".to_owned(), vec![strings(&["ReplicatedStorage", "Q"])])].into(),
		),
		("pkg-a".to_owned(), GlobPaths::new()),
	]
	.into();
	let got = marshal_globs_artifact(Some(&game), &packages).expect("emit");
	let want = r#"{"game":{"a":[],"z":[["ServerStorage","Z"]]},"packages":{"pkg-a":{},"pkg-b":{"q":[["ReplicatedStorage","Q"]]}}}"#;
	assert_eq!(String::from_utf8(got).unwrap(), want);
}
