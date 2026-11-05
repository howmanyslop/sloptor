//! `path_test.go`

use rotor_rojo::PathTranslator;

use super::{join, temp_dir};
use crate::ErrorKind;
use crate::path::translate_output_path;

#[test]
fn translate_output_path_translates_project_path() {
	let (_dir, root) = temp_dir();
	let translator = PathTranslator::new(&join(&[&root, "src"]), &join(&[&root, "out"]), "", false, false);
	assert_eq!(
		translate_output_path(&root, &translator, "src/nested/index.ts").unwrap(),
		"out/nested/init.lua"
	);
}

#[test]
fn translate_output_path_rejects_input_escape() {
	let (_dir, root) = temp_dir();
	let translator = PathTranslator::new(&join(&[&root, "src"]), &join(&[&root, "out"]), "", false, false);
	let error = translate_output_path(&root, &translator, "../escape.ts").unwrap_err();
	assert!(error.is(ErrorKind::PathEscape), "{error}");
}

#[test]
fn translate_output_path_rejects_output_escape() {
	let (_dir, root) = temp_dir();
	let translator = PathTranslator::new(&join(&[&root, "src"]), &join(&[&root, "..", "out"]), "", false, false);
	let error = translate_output_path(&root, &translator, "src/file.ts").unwrap_err();
	assert!(error.is(ErrorKind::PathEscape), "{error}");
}
