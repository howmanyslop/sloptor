//! Loading the conformance project the way rotor does, and deriving programs from it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rotor_tsrs::ast::{Kind, Node};
use rotor_tsrs::project::{
	DERIVED_PROGRAM_CHECKERS, OverlayError, derived_program_checkers, for_each_checker_group, update_with_overlays,
};
use rotor_tsrs::{LoadError, P, Project, ProjectOptions, load_project, with_large_stack};

// An integration test is handed every dependency of its package, so the ones only the library
// uses must be acknowledged to keep `unused_crate_dependencies` quiet.
use regex as _;
use rotor_support as _;
use serde_json as _;
use tsrs_ast as _;
use tsrs_checker as _;
use tsrs_compiler as _;
use tsrs_core as _;
use tsrs_diagnostics as _;
use tsrs_module as _;
use tsrs_parser as _;
use tsrs_printer as _;
use tsrs_scanner as _;
use tsrs_sourcemap as _;
use tsrs_tsoptions as _;
use tsrs_vfs as _;

fn conformance_tsconfig() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/conformance/project/tsconfig.json")
}

fn load() -> Result<Project, LoadError> {
	load_project(&conformance_tsconfig(), &ProjectOptions::default())
}

fn identifiers(node: P<Node>, out: &mut Vec<P<Node>>) {
	if node.kind() == Kind::Identifier {
		out.push(node);
	}
	node.for_each_child(&mut |child| {
		identifiers(child, out);
		false
	});
}

#[test]
fn includes_the_synthetic_macro_declarations() {
	with_large_stack(|| {
		let project = load().expect("the conformance project loads");
		let names: Vec<&str> = project
			.program
			.source_files()
			.iter()
			.map(|file| file.file_name())
			.collect();
		assert!(
			names.iter().any(|name| name.ends_with("/__rotor_env.d.ts")),
			"{names:?}"
		);
		assert!(
			names.iter().any(|name| name.ends_with("/__rotor_macros.d.ts")),
			"{names:?}"
		);
		assert!(project.source_files().iter().all(|file| !file.is_declaration_file()));
	});
}

#[test]
fn rejects_a_config_rbxtsc_would_reject() {
	let directory = tempfile::tempdir().expect("a temporary directory");
	let tsconfig = directory.path().join("tsconfig.json");
	std::fs::write(&tsconfig, r#"{ "compilerOptions": { "strict": true }, "files": [] }"#).expect("the config writes");
	let error = with_large_stack(|| load_project(&tsconfig, &ProjectOptions::default()).err());
	let message = error.expect("validation fails").to_string();
	assert!(
		message.starts_with("Invalid \"tsconfig.json\" configuration!"),
		"{message}"
	);
	assert!(message.contains("\"noLib\" must be true"), "{message}");
}

#[test]
fn overlays_replace_a_file_in_a_derived_program() {
	with_large_stack(|| {
		let project = load().expect("the conformance project loads");
		let target = project
			.source_files()
			.into_iter()
			.find(|file| file.file_name().ends_with("/ClassWithStaticFoo.ts"))
			.expect("the helper is a project file");
		let overlays = BTreeMap::from([(
			target.file_name().to_owned(),
			"export const overlaid = 42;\n".to_owned(),
		)]);
		let (derived, _) = update_with_overlays(project.program, &overlays).expect("the overlay applies");
		let file = derived
			.get_source_file(target.file_name())
			.expect("the derived program keeps the file");
		assert_eq!(file.text(), "export const overlaid = 42;\n");

		let printed = std::sync::Mutex::new(Vec::new());
		let ran = for_each_checker_group(derived, &[file], |checker, _, file| {
			let mut found = Vec::new();
			identifiers(file.as_node(), &mut found);
			for identifier in found {
				let t = checker.get_type_at_location(identifier);
				let text = checker.type_to_string(t);
				printed.lock().expect("the lock is free").push(text);
			}
		});
		assert!(ran, "the derived program has a checker pool");
		assert_eq!(printed.into_inner().expect("the lock is free"), ["42"]);
	});
}

#[test]
fn unchanged_overlays_keep_the_program() {
	with_large_stack(|| {
		let project = load().expect("the conformance project loads");
		let file = project.source_files()[0];
		let overlays = BTreeMap::from([(file.file_name().to_owned(), file.text().to_owned())]);
		let (derived, reused) = update_with_overlays(project.program, &overlays).expect("the overlay applies");
		assert!(std::ptr::eq(derived, project.program));
		assert!(reused);
	});
}

#[test]
fn derived_programs_get_the_derived_checker_count() {
	with_large_stack(|| {
		let project = load().expect("the conformance project loads");
		assert_eq!(
			derived_program_checkers(project.program),
			Some(DERIVED_PROGRAM_CHECKERS)
		);
		let configured = load_project(
			&conformance_tsconfig(),
			&ProjectOptions {
				checkers: Some(2),
				..ProjectOptions::default()
			},
		)
		.expect("the conformance project loads");
		assert_eq!(derived_program_checkers(configured.program), None);
	});
}

#[test]
fn overlays_must_name_a_program_file() {
	with_large_stack(|| {
		let project = load().expect("the conformance project loads");
		let overlays = BTreeMap::from([("/nowhere/missing.ts".to_owned(), String::new())]);
		let error = update_with_overlays(project.program, &overlays).err();
		assert_eq!(error, Some(OverlayError::Missing("/nowhere/missing.ts".to_owned())));
	});
}
