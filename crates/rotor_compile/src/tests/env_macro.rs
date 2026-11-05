//! `env_macro_test.go`. `TestEnvMacroModel` sets process variables (`t.Setenv`), which is
//! `unsafe` in Rust 2024 and leaks into the tests running beside it, so it is ignored;
//! `TestEnvMacroCoexistsWithOnDiskDeclaration` compares two compiles under the same environment,
//! so it ports without the variables.

use std::path::Path;

use super::{Temp, copy_dir, expect_compiled, large_stack, output, testdata};
use crate::ProjectOptions;
use crate::compile::compile_file_detailed;
use crate::output::build_project_with_options;
use crate::project::compile_project;
use crate::rotortypes::{ROTOR_TYPES_FILE_NAME, rotor_types_file_text, write_rotor_types};

#[test]
#[ignore = "sets process environment variables (ROTOR_TEST_ENV_PROC, NODE_ENV), which the workspace's unsafe_code deny forbids"]
fn env_macro_model() {
	let files = expect_compiled(large_stack(|| compile_project(&testdata("env_model"))));
	let want = concat!(
		"-- Compiled with sloptor v3.0.0-beta.2\n",
		"print(\"Adventure Quest\")\n",
		"print(\"from-process\")\n",
		"print(\"fallback-value\")\n",
		"print(nil)\n",
		"print(\"Adventure Quest\")\n",
		"print(\"Adventure Quest\")\n",
		"print(\"Adventure Quest\")\n",
		"print(\"from-studio\")\n",
		"print(\"hello world\")\n",
		"print(\"single quoted\")\n",
		"return nil\n",
	);
	assert_eq!(output(&files, "out/main.luau"), want);
}

fn diagnostic_codes(rel_path: &str) -> Vec<crate::DiagnosticInfo> {
	let compiled = match large_stack(|| compile_file_detailed(&testdata("env_diag_model"), rel_path)) {
		Ok(compiled) => compiled,
		Err(failure) => panic!("CompileFileDetailed returned hard error (want diagnostic): {failure}"),
	};
	assert!(
		compiled.text.is_empty(),
		"expected no output text, got:\n{}",
		String::from_utf8_lossy(&compiled.text)
	);
	compiled.diagnostics
}

#[test]
fn env_macro_non_literal_arg_diagnostic() {
	let diags = diagnostic_codes("src/main.ts");
	let found: Vec<_> = diags
		.iter()
		.filter(|info| info.code == "rotorEnvNonLiteralArg")
		.collect();
	assert!(
		!found.is_empty(),
		"diagnostics = {diags:?}, want one with code rotorEnvNonLiteralArg"
	);
	for info in found {
		assert!(
			info.message.contains("string literal"),
			"diagnostic message not descriptive: {:?}",
			info.message
		);
	}
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
fn build_writes_env_types_for_env_projects() {
	let temp = Temp::new();
	copy_dir(Path::new(&testdata("env_model")), &temp.join(""));
	let result = build(&temp.dir());
	assert!(result.uses_env_macro, "UsesEnvMacro = false, want true");
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
		result.uses_env_macro && !result.wrote_rotor_types,
		"rebuild: UsesEnvMacro = {}, WroteRotorTypes = {}; want true, false",
		result.uses_env_macro,
		result.wrote_rotor_types
	);

	let temp = Temp::new();
	copy_dir(Path::new(&testdata("env_model")), &temp.join(""));
	temp.write("src/main.ts", "print(1);\n");
	let result = build(&temp.dir());
	assert!(
		!result.uses_env_macro && !result.wrote_rotor_types,
		"UsesEnvMacro = {}, WroteRotorTypes = {}; want false, false",
		result.uses_env_macro,
		result.wrote_rotor_types
	);
	assert!(
		!temp.join(ROTOR_TYPES_FILE_NAME).exists(),
		"rotor.d.ts written for a project that never references a macro"
	);
}

#[test]
fn env_macro_coexists_with_on_disk_declaration() {
	let baseline = expect_compiled(large_stack(|| compile_project(&testdata("env_model"))));

	let temp = Temp::new();
	copy_dir(Path::new(&testdata("env_model")), &temp.join(""));
	write_rotor_types(&temp.dir()).expect("rotor.d.ts writes");
	let tsconfig = std::fs::read_to_string(temp.join("tsconfig.json")).expect("the config reads");
	let patched = tsconfig.replacen(r#""include": ["src"]"#, r#""include": ["src", "rotor.d.ts"]"#, 1);
	assert_ne!(patched, tsconfig, "fixture tsconfig include not found to patch");
	temp.write("tsconfig.json", patched);

	let files = expect_compiled(large_stack(|| compile_project(&temp.dir())));
	assert_eq!(
		output(&files, "out/main.luau"),
		output(&baseline, "out/main.luau"),
		"output with on-disk rotor-env.d.ts differs from baseline"
	);
}

#[test]
fn env_macro_bare_usage_diagnostic() {
	let diags = diagnostic_codes("src/bare.ts");
	assert!(
		diags.iter().any(|info| info.code == "rotorEnvBadUsage"),
		"diagnostics = {diags:?}, want one with code rotorEnvBadUsage"
	);
}
