//! `macro_inputs_test.go`. The `t.Setenv` step of the first test (a process environment
//! override) is left out: setting the environment is `unsafe` in Rust 2024, which the workspace
//! denies, and the variable would leak into the tests running beside it.

use std::path::Path;

use super::{Temp, copy_dir, expect_failed, large_stack, set_compiler_options, testdata};
use crate::ProjectOptions;
use crate::macro_inputs::macro_watch_files;
use crate::output::build_project_with_options;
use crate::project::compile_project;
use crate::rotortypes::rotor_types_file_text;

fn build(dir: &str, opts: &ProjectOptions) {
	if let Err(failure) = large_stack(|| build_project_with_options(dir, opts)) {
		panic!("build: {}\n{:?}", failure.failure, failure.failure.messages());
	}
}

#[test]
fn incremental_macro_inputs_rebuild_without_source_edits() {
	let temp = Temp::new();
	copy_dir(Path::new(&testdata("macros_model")), &temp.join(""));
	set_compiler_options(
		&temp.join("tsconfig.json"),
		&serde_json::json!({"incremental": true, "tsBuildInfoFile": "cache/build.rbxtsc.tsbuildinfo"}),
	);
	temp.write(
		"src/main.ts",
		"print($env.SLOPTOR_MACRO_INPUT); print($file('./../data.txt'));\n",
	);
	temp.write(".env", "SLOPTOR_MACRO_INPUT=first\n");
	temp.write("data.txt", "first data");
	let read = || std::fs::read_to_string(temp.join("out/main.luau")).expect("out/main.luau reads");

	build(&temp.dir(), &ProjectOptions::default());
	let first = read();
	temp.write(".env", "SLOPTOR_MACRO_INPUT=second\n");
	temp.write("data.txt", "second data");
	build(&temp.dir(), &ProjectOptions::default());
	let second = read();
	assert!(
		first != second && second.contains("second") && second.contains("second data"),
		"macro edits left stale output:\n{second}"
	);
	build(&temp.dir(), &ProjectOptions::default());
	let got = read();
	assert!(got.contains("second data"), "no-change build lost file macro: {got}");

	let files = macro_watch_files(&temp.join("tsconfig.json").to_string_lossy());
	for name in [".env", "data.txt"] {
		let want = temp.join(name).to_string_lossy().into_owned();
		assert!(files.contains(&want), "watch files {files:?} omit {name}");
	}
}

#[test]
fn removed_macros_are_ordinary_undeclared_identifiers() {
	for name in ["$asset", "$git", "$buildTime"] {
		let temp = Temp::new();
		copy_dir(Path::new(&testdata("macros_model")), &temp.join(""));
		temp.write("src/main.ts", format!("print({name}('value'));\n"));

		let diags = expect_failed(large_stack(|| compile_project(&temp.dir())));
		assert!(
			diags.join("\n").contains(&format!("Cannot find name '{name}'")),
			"removed macro compiled: {diags:?}"
		);
		assert!(
			!rotor_types_file_text().contains(name),
			"editor companion advertises {name}"
		);
	}
}

#[test]
fn incremental_macro_inputs_follow_custom_config_directory() {
	let root = Temp::new();
	let dir = root.join("project");
	copy_dir(Path::new(&testdata("macros_model")), &dir);
	set_compiler_options(
		&dir.join("tsconfig.json"),
		&serde_json::json!({"incremental": true, "tsBuildInfoFile": "cache/build.rbxtsc.tsbuildinfo", "typeRoots": ["../node_modules/@rbxts"]}),
	);
	root.write("package.json", r#"{"name":"macros-fixture"}"#);
	root.write("project/src/main.ts", "print($file('config.json'));\n");
	let path = |rel: &str| dir.join(rel).to_string_lossy().into_owned();
	let opts = ProjectOptions {
		ts_config_path: path("tsconfig.json"),
		rojo_config_path: path("default.project.json"),
		include_path: path("include"),
		..ProjectOptions::default()
	};
	for value in ["first", "second"] {
		root.write("project/config.json", format!(r#"{{"value":"{value}"}}"#));
		build(&root.dir(), &opts);
		let output = std::fs::read_to_string(dir.join("out/main.luau")).unwrap_or_default();
		assert!(output.contains(value), "custom config left stale output: {output}");
	}
}
