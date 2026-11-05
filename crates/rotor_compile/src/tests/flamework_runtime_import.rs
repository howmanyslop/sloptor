//! `flamework_runtime_import_test.go`

use super::flamework_pipeline::symlink_transformer_fixture_modules;
use super::sidecar::{build, output_text};
use super::write_project;
use crate::project::ProjectOptions;

#[test]
fn native_flamework_pipeline_emits_jsx_factory_import_when_react_is_only_used_by_jsx() {
	// Given: a native Flamework project whose only React use is JSX.
	let temp = write_project("@scope/flamework-jsx-react-import", "");
	let dir = temp.dir();
	temp.write("rotor.toml", "[flamework]\nnoSemanticDiagnostics = true\n");
	temp.write(
		"tsconfig.json",
		r#"{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"jsx": "react",
		"jsxFactory": "React.createElement",
		"jsxFragmentFactory": "React.Fragment",
		"module": "CommonJS",
		"moduleResolution": "Node",
		"noLib": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"types": [],
		"typeRoots": ["node_modules/@rbxts", "node_modules/@flamework"],
		"rootDir": "src",
		"outDir": "out"
	},
	"include": ["src"]
}"#,
	);
	let mut globals = std::fs::read(temp.join("src/globals.d.ts")).expect("read globals");
	globals.extend_from_slice(b"\ndeclare namespace JSX {\n\tinterface IntrinsicElements {\n\t\tframe: {};\n\t}\n}\n");
	temp.write("src/globals.d.ts", globals);
	write_runtime_import_module(
		&dir,
		"node_modules/@rbxts/react",
		r#"{
	"name": "@rbxts/react",
	"main": "src/index.ts",
	"types": "src/index.d.ts"
}"#,
		"export function createElement(..._args: Array<unknown>): unknown;
export const Fragment: unknown;
declare const React: { createElement: typeof createElement; Fragment: unknown };
export default React;
",
	);
	temp.write(
		"src/box.tsx",
		"import React from \"@rbxts/react\";\nexport function Box() {\n\treturn <frame />;\n}\n",
	);

	// When: the full native Flamework print/overlay/reparse path emits Luau.
	let (result, diags, error) = build(&dir, &ProjectOptions::default());
	// Then: the JSX factory import survives as a runtime binding.
	assert!(
		error.is_none() && diags.is_empty(),
		"BuildProjectWithOptions: {error:?} ({diags:?})"
	);
	let got = output_text(&result.expect("a result"), "out/box.luau");
	assert!(
		got.contains("React.createElement"),
		"emitted Luau missing JSX factory call:\n{got}"
	);
	assert!(
		got.contains("local React") && got.contains("TS.import"),
		"emitted Luau dropped the React import used only by JSX:\n{got}"
	);
}

#[test]
fn native_flamework_pipeline_emits_named_import_when_used_only_as_value_after_overlay() {
	// Given: native Flamework reprints every file and skips semantic diagnostics.
	let temp = write_project("@scope/flamework-named-import-elision", "");
	let dir = temp.dir();
	temp.write("rotor.toml", "[flamework]\nnoSemanticDiagnostics = true\n");
	write_runtime_import_module(
		&dir,
		"node_modules/@rbxts/t",
		r#"{
	"name": "@rbxts/t",
	"main": "lib/t.d.ts",
	"types": "lib/t.d.ts"
}"#,
		"export const t: { interface(shape: unknown): unknown };\n",
	);
	temp.write(
		"src/guards.ts",
		"import { t } from \"@rbxts/t\";\nexport const payloadGuard = t.interface({ label: t.string });\n",
	);

	// When: the full native Flamework print/overlay/reparse path emits Luau.
	let (result, diags, error) = build(&dir, &ProjectOptions::default());
	// Then: the named import survives as a runtime binding.
	assert!(
		error.is_none() && diags.is_empty(),
		"BuildProjectWithOptions: {error:?} ({diags:?})"
	);
	let got = output_text(&result.expect("a result"), "out/guards.luau");
	assert!(
		got.contains("t.interface") || got.contains("t:interface") || got.contains(r#"t["interface"]"#),
		"emitted Luau missing t.interface use:\n{got}"
	);
	assert!(
		got.contains("local t =") && got.contains("TS.import"),
		"emitted Luau dropped the named t import:\n{got}"
	);
}

#[test]
fn native_flamework_pipeline_emits_guard_library_import_when_create_guard_expands() {
	// Given: native Flamework expands createGuard and reprints under noSemanticDiagnostics.
	let temp = write_project("@scope/flamework-create-guard-import", "");
	let dir = temp.dir();
	symlink_transformer_fixture_modules(&temp);
	temp.write("rotor.toml", "[flamework]\nnoSemanticDiagnostics = true\n");
	let tsconfig = std::fs::read_to_string(temp.join("tsconfig.json")).expect("read tsconfig");
	let updated = tsconfig.replacen(
		r#""typeRoots": ["node_modules/@rbxts"]"#,
		r#""typeRoots": ["node_modules/@rbxts", "node_modules/@flamework"]"#,
		1,
	);
	assert_ne!(updated, tsconfig, "typeRoots not found in writeProject tsconfig");
	temp.write("tsconfig.json", updated);
	temp.write(
		"src/guards.ts",
		"import { Flamework } from \"@flamework/core\";\ninterface Payload { readonly label: string; readonly count: number }\nexport const payloadGuard = Flamework.createGuard<Payload>();\n",
	);

	// When: the full native Flamework print/overlay/reparse path emits Luau.
	let (result, diags, error) = build(&dir, &ProjectOptions::default());
	// Then: the synthesized guard-library import survives as a runtime binding.
	assert!(
		error.is_none() && diags.is_empty(),
		"BuildProjectWithOptions: {error:?} ({diags:?})"
	);
	let got = output_text(&result.expect("a result"), "out/guards.luau");
	assert!(
		got.contains("t.interface") || got.contains("t:interface") || got.contains(r#"t["interface"#),
		"emitted Luau missing expanded createGuard:\n{got}"
	);
	assert!(
		got.contains("local t =") && got.contains("TS.import"),
		"emitted Luau dropped the synthesized t import:\n{got}"
	);
}

/// `writeRuntimeImportModule`
fn write_runtime_import_module(dir: &str, rel: &str, package_json: &str, types: &str) {
	let root = std::path::Path::new(dir).join(rel);
	for sub in ["src", "out", "lib"] {
		std::fs::create_dir_all(root.join(sub)).expect("mkdir");
	}
	std::fs::write(root.join("package.json"), package_json).expect("write");
	for name in [
		"src/index.ts",
		"src/index.d.ts",
		"out/index.ts",
		"out/index.d.ts",
		"lib/t.d.ts",
	] {
		std::fs::write(root.join(name), types).expect("write");
	}
}
