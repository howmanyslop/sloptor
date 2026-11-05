//! `declaration_emit_test.go`. The three `EmitDeclarationOnly` plugin tests run transformer
//! plugins through the sidecar (5b).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use rotor_tsrs::sourcemap::decode_mappings;

use super::declmap::declaration_map_mappings;
use super::output::{build, patch_out_dir, try_build};
use super::{Temp, copy_dir, large_stack, testdata, write_project};
use crate::ProjectOptions;
use crate::output::{declaration_emit_source_files, emit_declaration_texts};
use crate::project::new_project_program_with_options;

/// A temp copy of `testdata/declpaths_model`, a project with `paths`, `declaration` and
/// `declarationMap`.
fn copy_declaration_paths_fixture() -> Temp {
	let temp = Temp::new();
	copy_dir(Path::new(&testdata("declpaths_model")), &temp.join(""));
	temp
}

/// `buildDeclarationPathsFixture`
fn build_declaration_paths_fixture() -> Temp {
	let temp = copy_declaration_paths_fixture();
	build(&temp.dir(), &ProjectOptions::default());
	temp
}

/// `readDeclarationFixtureFile`
fn read_declaration_fixture_file(temp: &Temp, name: &str) -> String {
	fs::read_to_string(temp.join("out").join(name)).unwrap_or_else(|error| panic!("read out/{name}: {error}"))
}

#[test]
fn build_reports_declaration_emit_diagnostic() {
	let temp = write_project("@scope/declaration-diagnostic", "");
	patch_out_dir(&temp, r#", "declaration": true"#);
	temp.write(
		"node_modules/@rbxts/dependency/index.d.ts",
		"declare const tag: unique symbol;\nexport declare function create(): { readonly [tag]: string };\n",
	);
	temp.write(
		"src/main.ts",
		"import { create } from '@rbxts/dependency';\nexport const merchantRecoveryStore = create();\n",
	);

	let Err(failure) = try_build(&temp.dir(), &ProjectOptions::default()) else {
		panic!("build succeeded despite an inaccessible exported type");
	};
	let Some(result) = failure.result.as_ref().filter(|result| !result.diagnostics.is_empty()) else {
		panic!(
			"declaration emit error lost its diagnostic: messages={:?} err={}",
			failure.failure.messages(),
			failure.failure
		);
	};
	let main = temp.join("src/main.ts").to_string_lossy().replace('\\', "/");
	assert!(
		result.diagnostics.iter().any(|diagnostic| diagnostic.code == "TS4023"
			&& diagnostic.message.contains("merchantRecoveryStore")
			&& diagnostic.file_name == main
			&& diagnostic.line == 2),
		"missing located TS4023: {:?}",
		result.diagnostics
	);
}

/// A project with `paths` publishes declarations whose specifiers the Luau runtime can resolve.
#[test]
fn declaration_emit_rewrites_path_aliases() {
	let temp = build_declaration_paths_fixture();
	let declaration = read_declaration_fixture_file(&temp, "main.d.ts");

	assert!(
		declaration.contains(r#"from "./shared/mod""#),
		"path alias was not rewritten in an import declaration:\n{declaration}"
	);
	// The emitter synthesizes this `import("...")` type for the inferred return type, so its
	// specifier is not in the file's import list.
	assert!(
		declaration.contains(r#"import("./shared/mod")"#),
		"synthesized import type specifier was not rewritten:\n{declaration}"
	);
	assert!(
		!declaration.contains("@alias/"),
		"an alias spelling survived into the declaration:\n{declaration}"
	);
	// An external-library import keeps its package spelling: the runtime resolves that one.
	assert!(
		declaration.contains(r#""@rbxts/dummy""#),
		"external library import was rewritten:\n{declaration}"
	);
}

/// The `.d.ts.map` is written from the text before the specifier splice, so every generated
/// column past a rewritten specifier has to move with it.
#[test]
fn declaration_map_columns_follow_the_rewritten_specifier() {
	let temp = build_declaration_paths_fixture();
	let declaration = read_declaration_fixture_file(&temp, "main.d.ts");
	let map_text = read_declaration_fixture_file(&temp, "main.d.ts.map");
	let source = fs::read_to_string(temp.join("src/main.ts")).expect("src/main.ts reads");

	let declaration = declaration.replace("\r\n", "\n");
	let generated_lines: Vec<&str> = declaration.split('\n').collect();
	let source = source.replace("\r\n", "\n");
	let source_lines: Vec<&str> = source.split('\n').collect();

	let mut checked_specifier = false;
	let mut decoder = decode_mappings(declaration_map_mappings(&map_text));
	for mapping in decoder.values() {
		let generated_column = mapping.generated_character as usize;
		let generated_line = mapping.generated_line as usize;
		assert!(
			generated_line < generated_lines.len(),
			"mapping generated line {generated_line} is past the end of the declaration"
		);
		let line = generated_lines[generated_line];
		assert!(
			generated_column <= line.len(),
			"mapping generated column {generated_column} is past the end of line {generated_line} ({line:?})"
		);
		if !mapping.is_source_mapping() {
			continue;
		}
		let (Some(source_line), Some(source_character)) = (mapping.source_line, mapping.source_character) else {
			continue;
		};
		let (source_line, source_character) = (source_line as usize, source_character as usize);
		if source_line >= source_lines.len() || source_character > source_lines[source_line].len() {
			continue;
		}
		// Where the source has the aliased specifier, the generated text must have the rewritten one.
		if !source_lines[source_line][source_character..].starts_with(r#""@alias/shared/mod""#) {
			continue;
		}
		assert!(
			line[generated_column..].starts_with(r#""./shared/mod""#),
			"specifier mapping points at {:?}, want the rewritten specifier",
			&line[generated_column..]
		);
		checked_specifier = true;
	}
	assert!(
		decoder.error().is_none(),
		"decode declaration map mappings: {:?}",
		decoder.error()
	);
	assert!(checked_specifier, "no mapping covered the rewritten specifier");
}

/// The rewrite resolves against the text the build compiles: an import that exists only in a
/// caller overlay still has to be rewritten.
#[test]
fn declaration_emit_rewrites_overlaid_imports() {
	let temp = copy_declaration_paths_fixture();
	let overlaid = temp.join("src/main.ts").to_string_lossy().into_owned();
	let overlay = "import type { Shared } from \"@alias/shared/mod\";\nexport type Overlaid = Shared;\n";

	build(
		&temp.dir(),
		&ProjectOptions {
			overlays: BTreeMap::from([(overlaid, overlay.to_owned())]),
			..ProjectOptions::default()
		},
	);

	let declaration = read_declaration_fixture_file(&temp, "main.d.ts");
	assert!(
		declaration.contains(r#"from "./shared/mod""#),
		"overlaid import was not rewritten:\n{declaration}"
	);
	assert!(
		!declaration.contains("@alias/"),
		"an alias spelling survived into the declaration:\n{declaration}"
	);
	assert!(
		declaration.contains("Overlaid"),
		"declaration was emitted from disk rather than from the overlay:\n{declaration}"
	);
}

/// A declaration-only plugin fixture over `plugins`.
fn declaration_plugin_project(pkg_name: &str, plugins: &str) -> (Temp, super::sidecar::SessionsGuard) {
	super::sidecar::set_repo_sidecar_path();
	let sessions = super::sidecar::SessionsGuard::new();
	let temp = write_project(pkg_name, "");
	super::sidecar::write_sidecar_plugin_fixture(
		&temp.dir(),
		"",
		&super::sidecar_declaration::sidecar_declaration_config(plugins),
	);
	(temp, sessions)
}

fn declaration_only() -> ProjectOptions {
	ProjectOptions {
		emit_declaration_only: true,
		..ProjectOptions::default()
	}
}

#[test]
fn emit_declaration_only_still_reports_a_failing_plugin() {
	let (temp, _sessions) = declaration_plugin_project(
		"@scope/emit-declaration-only-plugin",
		r#"[{ "transform": "./plugins/does-not-exist.js" }]"#,
	);
	let (_, diags, error) = super::sidecar::build(&temp.dir(), &declaration_only());
	assert!(error.is_some(), "build succeeded with an unloadable transformer plugin");
	assert!(
		diags.join("\n").contains("does-not-exist.js"),
		"diagnostics do not name the failing plugin: {diags:?}"
	);
}

/// Catches declaration-only builds executing transformer factories even though no transformed
/// source participates in declaration emit.
#[test]
fn emit_declaration_only_validates_plugins_without_invoking_factories() {
	let (temp, _sessions) = declaration_plugin_project(
		"@scope/declaration-plugin-validation",
		"[\n\t\t{ \"transform\": \"./plugins/program.js\" },\n\t\t{ \"transform\": \"./plugins/raw.js\", \"type\": \"raw\" }\n\t]",
	);
	let marker = temp.join("factory-ran").to_string_lossy().into_owned();
	let plugin = format!(
		"const fs = require(\"node:fs\");\nmodule.exports = () => {{\n\tfs.writeFileSync({}, \"called\");\n\tthrow new Error(\"declaration validation must not invoke factories\");\n}};\n",
		rotor_support::gostr::quote(&marker)
	);
	for name in ["program.js", "raw.js"] {
		temp.write(&format!("plugins/{name}"), &plugin);
	}
	let (_, diags, error) = super::sidecar::build(&temp.dir(), &declaration_only());
	assert!(error.is_none(), "declaration-only build: {error:?} (diags: {diags:?})");
	assert!(
		!Path::new(&marker).exists(),
		"plugin factory marker exists after declaration validation"
	);
	assert!(
		temp.join("out/main.d.ts").exists(),
		"declaration-only build did not emit main.d.ts"
	);
}

/// Catches a declaration-only build accepting a configured module that does not export the
/// requested transformer factory.
#[test]
fn emit_declaration_only_rejects_missing_plugin_export() {
	let (temp, _sessions) = declaration_plugin_project(
		"@scope/declaration-plugin-export",
		r#"[{ "transform": "./plugins/invalid.js" }]"#,
	);
	temp.write("plugins/invalid.js", "module.exports = {};\n");
	let (_, diags, error) = super::sidecar::build(&temp.dir(), &declaration_only());
	assert!(
		error.is_some(),
		"declaration-only build accepted a plugin without a factory export"
	);
	assert!(
		diags.join("\n").contains("factory not a function"),
		"diagnostics do not explain the missing factory export: {diags:?}"
	);
}

/// Declaration text must not depend on the order in which files reach their checker.
///
/// Go sets `ROTOR_WRITE_WORKERS=1`, which only sizes the output writer; this test writes no
/// outputs, and setting a process variable is `unsafe` in Rust 2024, so it is left unset.
#[test]
fn emit_declaration_texts_ignores_file_order() {
	let temp = write_project("@scope/declaration-order", "");
	patch_out_dir(&temp, r#", "declaration": true, "checkers": 1"#);
	temp.write("src/a.ts", "export const a = [{ x: 1 }, { y: 1, z: 1 }];\n");
	temp.write("src/b.ts", "export const b = [{ z: 1, y: 1 }, { w: 1 }];\n");

	let emit_b = |reverse: bool| {
		large_stack(|| {
			let program = match new_project_program_with_options(&temp.dir(), "", &ProjectOptions::default()) {
				Ok((_, program)) => program,
				Err(failure) => panic!("newProjectProgram: {failure} (diags: {:?})", failure.messages()),
			};
			let mut files = declaration_emit_source_files(program, None);
			if reverse {
				files.reverse();
			}
			let emitted =
				emit_declaration_texts(program, &files).unwrap_or_else(|error| panic!("emitDeclarationTexts: {error}"));
			emitted
				.into_iter()
				.find(|file| file.file_name.ends_with("b.d.ts"))
				.map_or_else(|| panic!("b.d.ts not emitted"), |file| file.text)
		})
	};

	let forward = emit_b(false);
	let reversed = emit_b(true);
	assert_eq!(forward, reversed, "b.d.ts depends on emit order");
}
