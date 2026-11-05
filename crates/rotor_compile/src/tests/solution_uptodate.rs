//! `solution_uptodate_test.go`.

use serde_json::json;

use super::project_reference_import::new_reference_fixture;
use super::solution::{join, write_solution_file};
use super::solution_drain::cross_project_compiler_options;
use super::{NO_LIB_GLOBAL_STUBS, Temp, large_stack};
use crate::output::build_project_with_options;
use crate::project::ProjectOptions;
use crate::rbxts_options::RbxtsOptions;
use crate::solution::build_solution_graph;
use crate::solution_drain::build_solution_with_options;

/// `writeSpecLibSolution`: the issue #61 layout. A files-empty coordinator references a spec
/// project that declares its own rbxts type and Rojo config, and the spec references a lib
/// project that declares neither.
fn write_spec_lib_solution() -> (Temp, String) {
	let temp = Temp::new();
	let root = temp.dir();
	let options = cross_project_compiler_options(true).replacen(
		r#""composite":true"#,
		r#""composite":true,"incremental":true"#,
		1,
	);
	write_solution_file(&root, "package.json", r#"{"name":"pkg"}"#);
	write_solution_file(
		&root,
		"default.project.json",
		r#"{"name":"pkg","tree":{"$className":"DataModel","ReplicatedStorage":{"include":{"$path":"include"},"lib":{"$path":"out"}}}}"#,
	);
	write_solution_file(
		&root,
		"test.project.json",
		r#"{"name":"pkg-test","tree":{"$className":"DataModel","ReplicatedStorage":{"include":{"$path":"include"},"lib":{"$path":"out"},"tests":{"$path":"out-test"}}}}"#,
	);
	write_solution_file(
		&root,
		"tsconfig.test.json",
		r#"{"files":[],"references":[{"path":"./tsconfig.spec.json"}]}"#,
	);
	write_solution_file(
		&root,
		"tsconfig.lib.json",
		&format!(
			r#"{},"tsBuildInfoFile":"${{configDir}}/out/tsconfig.lib.tsbuildinfo"}},"include":["src"]}}"#,
			options.strip_suffix('}').unwrap_or(&options)
		),
	);
	let spec = options.replacen(
		r#""rootDir":"src","outDir":"out""#,
		r#""rootDir":"tests","outDir":"out-test","tsBuildInfoFile":"out-test/tsconfig.tsbuildinfo""#,
		1,
	);
	write_solution_file(
		&root,
		"tsconfig.spec.json",
		&format!(
			r#"{spec},"rbxts":{{"rojo":"./test.project.json","type":"game"}},"include":["tests"],"references":[{{"path":"./tsconfig.lib.json"}}]}}"#
		),
	);
	write_solution_file(&root, "include/RuntimeLib.lua", "return {}\n");
	write_solution_file(&root, "src/globals.d.ts", NO_LIB_GLOBAL_STUBS);
	write_solution_file(&root, "src/value.ts", "export const value = 1;\n");
	write_solution_file(&root, "tests/globals.d.ts", NO_LIB_GLOBAL_STUBS);
	write_solution_file(
		&root,
		"tests/value.spec.ts",
		"import { value } from \"../src/value\";\nexport const doubled = value * 2;\n",
	);
	(temp, root)
}

#[test]
fn solution_build_reuses_direct_referenced_project_build() {
	// Given: the lib project was built on its own.
	let (_temp, root) = write_spec_lib_solution();
	large_stack(|| {
		let options = ProjectOptions {
			ts_config_path: join(&root, &["tsconfig.lib.json"]),
			..ProjectOptions::default()
		};
		if let Err(failure) = build_project_with_options(&root, &options) {
			panic!(
				"lib build: {} ({:?})",
				failure.failure.error,
				failure.failure.messages()
			);
		}
		// When: the solution that references it builds.
		let result = build_solution_with_options(&join(&root, &["tsconfig.test.json"]), &ProjectOptions::default())
			.unwrap_or_else(|failure| {
				panic!(
					"solution build: {} ({:?})",
					failure.failure.error,
					failure.failure.messages()
				)
			});
		// Then: only the spec project compiles; the lib is already up to date.
		for path in result.outputs.keys() {
			assert!(
				rotor_support::gopath::to_slash(path).contains("/out-test/"),
				"solution build recompiled up-to-date lib output {path}"
			);
		}
	});
}

#[test]
fn solution_watch_comment_edit_skips_dependent_compile() {
	let mut f = new_reference_fixture();
	f.project("shared", &[], &json!({}));
	f.project("game", &["shared"], &json!({}));
	f.write(
		"game/src/index.ts",
		r#"import { value } from "../../shared/src"; export const result = value;"#,
	);
	let mut coordinator = f.coordinator("game");
	for text in [
		"export const value = 1;\n// comment only\n",
		"export const value = 1;\n",
	] {
		f.write("shared/src/index.ts", text);
		coordinator.invalidate(&[f.path("shared/tsconfig.json")]);
		let (result, messages, error) = coordinator.drain();
		assert!(error.is_none(), "watch drain: {error:?} ({messages:?})");
		assert_eq!(
			result.outputs.len(),
			1,
			"comment edit compiled files, want only the changed source: {:?}",
			result.outputs.keys().collect::<Vec<_>>()
		);
		for path in result.outputs.keys() {
			assert!(
				rotor_support::gopath::to_slash(path).contains("/shared/"),
				"comment edit recompiled dependent output {path}"
			);
		}
	}
}

#[test]
fn solution_watch_declaration_change_recompiles_dependent() {
	let mut f = new_reference_fixture();
	f.project("shared", &[], &json!({}));
	f.project("game", &["shared"], &json!({}));
	f.write("shared/src/index.ts", "export const enum Value { Current = 1 }\n");
	f.write(
		"game/src/index.ts",
		r#"import { Value } from "../../shared/src"; export const result = Value.Current;"#,
	);
	let mut coordinator = f.coordinator("game");
	f.write("shared/src/index.ts", "export const enum Value { Current = 2 }\n");
	coordinator.invalidate(&[f.path("shared/tsconfig.json")]);
	let (result, messages, error) = coordinator.drain();
	assert!(error.is_none(), "watch drain: {error:?} ({messages:?})");
	let output = result
		.outputs
		.get(&join(&f.dir, &["game/out/init.luau"]))
		.map(|bytes| String::from_utf8_lossy(bytes).into_owned())
		.unwrap_or_default();
	assert!(
		output.contains("local result = 2"),
		"dependent did not compile against the new declaration:\n{output}"
	);
}

#[test]
fn solution_graph_layers_referenced_rbxts_options() {
	// Given: a coordinator and a reference that each declare rbxts options, and a command line
	// that sets one of them.
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_file(
		&root,
		"tsconfig.json",
		r#"{"files":[],"references":[{"path":"./lib"}],"rbxts":{"optimizedArrayAppends":true,"optimizedLoops":false,"luau":false,"noInclude":true}}"#,
	);
	write_solution_file(
		&root,
		"lib/tsconfig.json",
		r#"{"rbxts":{"luau":true,"optimizedArrayAppends":false,"optimizedLoops":true}}"#,
	);
	let entry = ProjectOptions {
		lua_extension: true,
		no_optimized_loops: true,
		solution_argv: Some(RbxtsOptions {
			optimized_array_appends: Some(true),
			optimized_loops: Some(false),
			..RbxtsOptions::default()
		}),
		..ProjectOptions::default()
	};
	// When
	let graph =
		build_solution_graph(&join(&root, &["tsconfig.json"]), &entry).unwrap_or_else(|error| panic!("{error}"));
	// Then: defaults < coordinator rbxts < own rbxts < argv.
	let lib = &graph.projects[0].options;
	assert!(
		!lib.lua_extension,
		"reference's luau did not override the coordinator's"
	);
	assert!(
		lib.no_optimized_loops,
		"command-line optimizedLoops did not override the reference's"
	);
	assert!(
		lib.optimized_array_appends,
		"command-line optimizedArrayAppends did not override the reference's"
	);
	assert!(
		!lib.emit_include_files,
		"coordinator's noInclude did not reach the reference"
	);
}
