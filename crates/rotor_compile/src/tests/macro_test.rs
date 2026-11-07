//! `macro_test.go`: the `$nameof`, `$keys` and `$file` macros, end to end.

use super::{Temp, copy_dir, expect_compiled, large_stack, output, testdata};
use crate::ProjectOptions;
use crate::compile::compile_file_detailed;
use crate::output::build_project_with_options;
use crate::project::compile_project;
use crate::rotortypes::{ROTOR_TYPES_FILE_NAME, rotor_types_file_text, write_rotor_types};

const MACROS_MODEL: &str = "-- Compiled with sloptor v3.0.0-beta.4\n\
	-- $nameof: trailing property name, and a bare identifier.\n\
	print(\"Health\")\n\
	print(\"foo\")\n\
	-- $keys<T>(): inline the type's string keys as an array.\n\
	print({ \"x\", \"y\" })\n\
	-- $file: a project-relative JSON file -> a Luau table; an importer-relative\n\
	-- text file -> a Luau string.\n\
	print({\n\
	\tenabled = true,\n\
	\tlevels = { \"forest\", \"cave\", \"castle\" },\n\
	\tmeta = {\n\
	\t\tauthor = \"uproot\",\n\
	\t\tbuild = 42,\n\
	\t},\n\
	\tname = \"Adventure Quest\",\n\
	\tversion = 3,\n\
	\tvolume = 0.8,\n\
	})\n\
	print([[hello from a text file\n\
	]])\n\
	return nil\n";

#[test]
fn macros_model() {
	let files = expect_compiled(large_stack(|| compile_project(&testdata("macros_model"))));
	assert_eq!(output(&files, "out/main.luau"), MACROS_MODEL);
}

/// `assertMacroDiag`: no output and a diagnostic with `want_code`, never a hard error.
fn assert_macro_diag(rel_path: &str, want_code: &str) {
	let compiled = match large_stack(|| compile_file_detailed(&testdata("macros_diag_model"), rel_path)) {
		Ok(compiled) => compiled,
		Err(failure) => panic!("CompileFileDetailed returned hard error (want diagnostic): {failure}"),
	};
	assert!(
		compiled.text.is_empty(),
		"expected no output text, got:\n{}",
		String::from_utf8_lossy(&compiled.text)
	);
	assert!(
		compiled.diagnostics.iter().any(|info| info.code == want_code),
		"diagnostics = {:?}, want one with code {want_code}",
		compiled.diagnostics
	);
}

#[test]
fn keys_macro_bare_usage_diagnostic() {
	assert_macro_diag("src/bare.ts", "rotorKeysBadUsage");
}

#[test]
fn nameof_macro_invalid_arg_diagnostic() {
	assert_macro_diag("src/nameofcall.ts", "rotorNameofInvalid");
}

#[test]
fn nameof_macro_bare_usage_diagnostic() {
	assert_macro_diag("src/nameofbare.ts", "rotorNameofBadUsage");
}

#[test]
fn file_macro_not_found_diagnostic() {
	assert_macro_diag("src/missing.ts", "rotorFileNotFound");
}

#[test]
fn file_macro_invalid_json_diagnostic() {
	assert_macro_diag("src/badjson.ts", "rotorFileInvalidJSON");
}

fn build(dir: &str) -> crate::BuildResult {
	match large_stack(|| build_project_with_options(dir, &ProjectOptions::default())) {
		Ok(result) => result,
		Err(failure) => panic!(
			"build failed: {}\n{}",
			failure.failure,
			failure.failure.messages().join("\n")
		),
	}
}

#[test]
fn build_writes_macro_types_for_macro_projects() {
	let temp = Temp::new();
	copy_dir(std::path::Path::new(&testdata("macros_model")), &temp.join(""));

	let result = build(&temp.dir());
	assert!(result.uses_macros, "UsesMacros = false, want true");
	assert!(
		result.wrote_rotor_types,
		"WroteRotorTypes = false, want true on first build"
	);
	let data = std::fs::read_to_string(temp.join(ROTOR_TYPES_FILE_NAME)).expect("rotor.d.ts written");
	assert_eq!(
		data,
		rotor_types_file_text(),
		"rotor.d.ts content differs from RotorTypesFileText"
	);

	let result = build(&temp.dir());
	assert!(
		result.uses_macros && !result.wrote_rotor_types,
		"rebuild: UsesMacros = {}, WroteRotorTypes = {}; want true, false",
		result.uses_macros,
		result.wrote_rotor_types
	);

	let temp = Temp::new();
	copy_dir(std::path::Path::new(&testdata("macros_model")), &temp.join(""));
	temp.write("src/main.ts", "print(1);\n");
	let result = build(&temp.dir());
	assert!(
		!result.uses_macros && !result.wrote_rotor_types,
		"UsesMacros = {}, WroteRotorTypes = {}; want false, false",
		result.uses_macros,
		result.wrote_rotor_types
	);
	assert!(
		!temp.join(ROTOR_TYPES_FILE_NAME).exists(),
		"rotor.d.ts written for a project that never references a macro"
	);
}

#[test]
fn macros_coexist_with_on_disk_declaration() {
	let baseline = expect_compiled(large_stack(|| compile_project(&testdata("macros_model"))));

	let temp = Temp::new();
	copy_dir(std::path::Path::new(&testdata("macros_model")), &temp.join(""));
	write_rotor_types(&temp.dir()).expect("rotor.d.ts writes");
	let tsconfig = std::fs::read_to_string(temp.join("tsconfig.json")).expect("the config reads");
	let patched = tsconfig.replacen(r#""include": ["src"]"#, r#""include": ["src", "rotor.d.ts"]"#, 1);
	assert_ne!(patched, tsconfig, "fixture tsconfig include not found to patch");
	temp.write("tsconfig.json", patched);

	let files = expect_compiled(large_stack(|| compile_project(&temp.dir())));
	assert_eq!(
		output(&files, "out/main.luau"),
		output(&baseline, "out/main.luau"),
		"output with on-disk rotor-macros.d.ts differs from baseline"
	);
}
