//! `compile_test.go`
//!
//! The Go tests that stub the statement dispatch (`withStubDispatch`, and the synthetic panic of
//! `TestCompileFilePanicBecomesError`) swap a package-level function variable. The Rust
//! transformer's dispatch is a plain function, so those are ignored; the real dispatch's outputs
//! are covered by the diff and conformance gates.

use super::{Temp, fixture_project_dir, large_stack, testdata, write_project};
use crate::compile::{compile_file, compile_file_detailed};

/// With statements stubbed out, `01_literals.ts` is the header and `return nil`.
#[test]
#[ignore = "needs a swappable statement dispatch (Go's transformer.TransformStatement variable)"]
fn compile_file_pipeline_no_exports() {}

/// With statements stubbed out, `08_exports.ts`'s mutable export forces the exports table.
#[test]
#[ignore = "needs a swappable statement dispatch (Go's transformer.TransformStatement variable)"]
fn compile_file_pipeline_export_shapes() {}

#[test]
fn compile_file_return_map_shape() {
	let temp = Temp::new();
	let type_roots = format!("{}/node_modules/@rbxts", fixture_project_dir()).replace('\\', "/");
	let tsconfig = format!(
		"{{
	\"compilerOptions\": {{
		\"allowSyntheticDefaultImports\": true,
		\"module\": \"commonjs\",
		\"moduleResolution\": \"Node\",
		\"noLib\": true,
		\"forceConsistentCasingInFileNames\": true,
		\"moduleDetection\": \"force\",
		\"strict\": true,
		\"target\": \"ESNext\",
		\"typeRoots\": [\"node_modules/@rbxts\", {type_roots:?}],
		\"rootDir\": \"src\",
		\"outDir\": \"out\"
	}},
	\"include\": [\"src\"]
}}"
	);
	temp.write("tsconfig.json", tsconfig);
	temp.write("package.json", r#"{"name":"returnmap-fixture"}"#);
	temp.write(
		"default.project.json",
		r#"{"name":"returnmap","tree":{"$path":"out","include":{"$path":"include"}}}"#,
	);
	temp.write("src/returnmap.ts", "export const x = 1;\n");

	let (got, diags) = match large_stack(|| compile_file(&temp.dir(), "src/returnmap.ts")) {
		Ok(compiled) => compiled,
		Err(failure) => panic!("CompileFile: {failure} (diags: {:?})", failure.messages()),
	};
	assert!(diags.is_empty(), "diagnostics: {diags:?}");
	let want = "-- Compiled with sloptor v3.0.0-beta.2\n\
		local x = 1\n\
		return {\n\
		\tx = x,\n\
		}\n";
	assert_eq!(String::from_utf8_lossy(&got), want);
}

#[test]
#[ignore = "needs a swappable statement dispatch to inject a synthetic transformer panic"]
fn compile_file_panic_becomes_error() {}

#[test]
fn compile_file_syntactic_error_fails() {
	let temp = write_project("@syntax/error-fixture", "");
	temp.write("src/main.ts", "export const x = 5;\n}\n");

	let failure = match large_stack(|| compile_file(&temp.dir(), "src/main.ts")) {
		Ok((text, _)) => panic!("expected a hard error for the parse error, got {text:?}"),
		Err(failure) => failure,
	};
	let diags = failure.messages();
	assert!(!diags.is_empty(), "expected diagnostics for the parse error");
	assert!(
		diags[0].contains("Declaration or statement expected"),
		"diags[0] = {:?}, want the TS1128 'Declaration or statement expected.' diagnostic (all: {diags:?})",
		diags[0]
	);
}

#[test]
fn compile_file_missing_source() {
	let result = large_stack(|| compile_file(&fixture_project_dir(), "src/does_not_exist.ts"));
	assert!(result.is_err(), "expected error for missing source file");
}

#[test]
fn transformer_diagnostic_carries_location() {
	let infos = match large_stack(|| compile_file_detailed(&testdata("env_diag_model"), "src/main.ts")) {
		Ok(output) => output.diagnostics,
		Err(failure) => failure.diagnostics,
	};
	assert!(!infos.is_empty(), "expected at least one diagnostic");
	assert!(
		infos.iter().any(|info| !info.file_name.is_empty() && info.len > 0),
		"no diagnostic carried a location: {infos:?}"
	);
}

#[test]
fn compile_file_uses_file_affined_checker_for_imports() {
	let (got, diags) = match large_stack(|| compile_file(&testdata("imports_model"), "src/_scratch_once.ts")) {
		Ok(compiled) => compiled,
		Err(failure) => panic!("CompileFile: {failure} (diags: {:?})", failure.messages()),
	};
	assert!(diags.is_empty(), "diagnostics: {diags:?}");
	let want = "-- Compiled with sloptor v3.0.0-beta.2\n\
		local TS = require(script.Parent.include.RuntimeLib)\n\
		local g = TS.import(script, script.Parent, \"_scratch_util\").greet\n\
		print(g(\"once\"))\n\
		return nil\n";
	assert_eq!(String::from_utf8_lossy(&got), want);
}
