//! `transform_addpaths_test.go`

use super::{new_transform_program, print_source_file, program_file, temp_dir, write_transform_fixture};
use crate::build_info::BuildInfoSnapshot;
use crate::project::{ProjectOptions, open_project};
use crate::transform_addpaths::{FLAMEWORK_STATIC_IMPORT_MARKER, static_add_paths_matches};
use crate::transform_state::TransformInput;

/// `writeAddPathsFixture`
pub(super) fn write_add_paths_fixture(directory: &str, main: &str) {
	write_transform_fixture(
		directory,
		"package.json",
		r#"{"name":"fixture-game","version":"1.0.0"}"#,
	);
	write_transform_fixture(
		directory,
		"tsconfig.json",
		r#"{"compilerOptions":{"moduleResolution":"node","rootDir":"src","outDir":"out"},"include":["src/**/*.ts"]}"#,
	);
	write_transform_fixture(
		directory,
		"default.project.json",
		r#"{"name":"fixture","tree":{"$className":"DataModel","ServerScriptService":{"TS":{"$path":"out/server"}},"ReplicatedStorage":{"TS":{"$path":"out/shared"}}}}"#,
	);
	write_transform_fixture(
		directory,
		"node_modules/@flamework/core/package.json",
		r#"{"name":"@flamework/core","types":"index.d.ts"}"#,
	);
	write_transform_fixture(
		directory,
		"node_modules/@flamework/core/index.d.ts",
		&[
			"export declare namespace Modding {",
			" type Intrinsic<N extends string, M extends unknown[], T = symbol> = T & { _flamework_intrinsic: [N, ...M] };",
			"}",
			"export declare namespace Flamework {",
			" function _addPaths(paths: string[][]): void;",
			" function _addPathsGlob(arg: string): void;",
			" function ignite(): void;",
			" /** @metadata macro intrinsic-arg-shift {@link _addPaths intrinsic-flamework-rewrite} */",
			r#" function addPaths<T extends string>(path: T, meta?: Modding.Intrinsic<"path", [T]>): void;"#,
			" /** @metadata macro intrinsic-arg-shift {@link _addPathsGlob intrinsic-flamework-rewrite} */",
			r#" function addPathsGlob<T extends string>(path: T, meta?: Modding.Intrinsic<"pathglob", [T]>): void;"#,
			"}",
		]
		.join("\n"),
	);
	for name in [
		"src/server/services/b.ts",
		"src/server/services/a.ts",
		"src/server/services/nested/index.ts",
		"src/server/services/runner.server.ts",
		"src/shared/components/y.ts",
		"src/shared/components/x.ts",
		"src/shared/components/types.d.ts",
		"src/shared/other.ts",
	] {
		write_transform_fixture(directory, name, "export {};\n");
	}
	write_transform_fixture(directory, "src/server/main.server.ts", main);
}

fn project_options(directory: &str) -> ProjectOptions {
	ProjectOptions {
		project_dir: directory.to_owned(),
		root_dir: "src".to_owned(),
		out_dir: "out".to_owned(),
		..ProjectOptions::default()
	}
}

/// `transformAddPathsFixture`: the main file transformed and printed, with the project's build
/// info.
fn transform_add_paths_fixture(directory: &str) -> (String, BuildInfoSnapshot) {
	rotor_tsrs::with_large_stack(|| {
		let program = new_transform_program(directory);
		let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
		let source_file = program_file(program, directory, "src/server/main.server.ts");
		let mut project = open_project(&project_options(directory)).expect("OpenProject");
		let result = crate::transform::transform(TransformInput {
			program,
			checker: &mut checker,
			files: vec![source_file],
			project: &mut project,
			analyses: Vec::new(),
			macro_runtime: None,
			print: None,
		})
		.expect("Transform");
		let diagnostics: Vec<&str> = result
			.diagnostics
			.iter()
			.map(crate::transform_diagnostics::Diagnostic::string)
			.collect();
		assert!(diagnostics.is_empty(), "diagnostics = {diagnostics:?}");
		(print_source_file(result.files[0]), project.build_info_snapshot())
	})
}

fn add_paths(main: &[&str]) -> (String, BuildInfoSnapshot) {
	let (_dir, directory) = temp_dir();
	write_add_paths_fixture(&directory, &main.join("\n"));
	transform_add_paths_fixture(&directory)
}

fn dynamic_import(module: &str) -> String {
	format!(r#"import("{module}" + "{FLAMEWORK_STATIC_IMPORT_MARKER}")"#)
}

// A bundler cannot follow Flamework's runtime requires of Instances, so top-level addPaths calls
// expand into static side-effect imports in place.
#[test]
fn transform_expands_add_paths_to_side_effect_imports_when_call_is_top_level() {
	let (printed, snapshot) = add_paths(&[
		r#"import { Flamework } from "@flamework/core";"#,
		r#"Flamework.addPaths("src/server/services");"#,
		r#"Flamework.addPathsGlob("./../shared/components/*.ts");"#,
		"Flamework.ignite();",
	]);

	let want = [
		r#"import { Flamework } from "@flamework/core";"#,
		r#"import "./services/a";"#,
		r#"import "./services/b";"#,
		r#"import "./services/nested/index";"#,
		r#"import "../shared/components/x";"#,
		r#"import "../shared/components/y";"#,
		"Flamework.ignite();",
		"",
	]
	.join("\n");
	assert_eq!(printed, want);
	let origins = snapshot
		.metadata
		.and_then(|metadata| metadata.globs)
		.and_then(|globs| globs.origins)
		.expect("glob registration, want origins for watch invalidation");
	assert_eq!(
		origins.get("src/server/main.server.ts").cloned().flatten(),
		Some(vec!["src/shared/components/*.ts".to_owned()]),
		"origin globs, want only the addPathsGlob pattern"
	);
}

#[test]
fn transform_expands_add_paths_at_nested_call_site() {
	let (printed, _) = add_paths(&[
		r#"import { Flamework } from "@flamework/core";"#,
		"function load() {",
		"    const before = 1;",
		r#"    Flamework.addPathsGlob("src/shared/components/*.ts");"#,
		r#"    Flamework.addPaths("src/server/services");"#,
		"    const after = 2;",
		"}",
		"load();",
	]);

	assert!(
		!printed.contains("_addPaths"),
		"transformed source =\n{printed}\nwant static imports"
	);
	for module in [
		"../shared/components/x",
		"../shared/components/y",
		"./services/a",
		"./services/b",
		"./services/nested/index",
	] {
		assert!(
			printed.contains(&dynamic_import(module)),
			"transformed source =\n{printed}\nmissing import {module:?}"
		);
	}
	assert!(
		printed.contains("const before = 1;") && printed.contains("const after = 2;"),
		"transformed source =\n{printed}\nlost statements around call"
	);
}

#[test]
fn transform_ignores_tsconfig_excluded_matches() {
	// Test files exist in the Rojo source directory but are not compiled.
	let (_dir, directory) = temp_dir();
	write_add_paths_fixture(
		&directory,
		&[
			r#"import { Flamework } from "@flamework/core";"#,
			r#"function load() { Flamework.addPaths("src/server/services"); }"#,
		]
		.join("\n"),
	);
	write_transform_fixture(&directory, "src/server/services/player.test.ts", "export {};\n");
	write_transform_fixture(
		&directory,
		"tsconfig.json",
		r#"{"compilerOptions":{"moduleResolution":"node","rootDir":"src","outDir":"out"},"include":["src/**/*.ts"],"exclude":["src/**/*.test.ts"]}"#,
	);

	let (printed, _) = transform_add_paths_fixture(&directory);

	assert!(
		!printed.contains("_addPaths")
			&& !printed.contains("player.test")
			&& printed.contains(&dynamic_import("./services/a")),
		"transformed source =\n{printed}\nwant only compiled modules as imports"
	);
}

#[test]
fn transform_keeps_unbraced_conditional_around_expanded_call() {
	let (printed, _) = add_paths(&[
		r#"import { Flamework } from "@flamework/core";"#,
		"function load(enabled: boolean) {",
		r#"    if (enabled) Flamework.addPaths("src/server/services");"#,
		"}",
	]);

	assert!(
		printed.contains("if (enabled) {")
			&& printed.contains(&dynamic_import("./services/a"))
			&& !printed.contains("_addPaths"),
		"transformed source =\n{printed}\nwant conditional static imports"
	);
}

#[test]
fn static_add_paths_matches_tracks_compiled_files_without_changing_runtime_globs() {
	let (_dir, directory) = temp_dir();
	write_add_paths_fixture(
		&directory,
		&[
			r#"import { Flamework } from "@flamework/core";"#,
			r#"function load() { Flamework.addPaths("src/server/services"); }"#,
		]
		.join("\n"),
	);
	write_transform_fixture(
		&directory,
		"tsconfig.json",
		r#"{"compilerOptions":{"moduleResolution":"node","rootDir":"src","outDir":"out"},"include":["src/**/*.ts"],"exclude":["src/**/*.test.ts"]}"#,
	);
	let (before, after, globs) = rotor_tsrs::with_large_stack(|| {
		let mut project = open_project(&project_options(&directory)).expect("OpenProject");
		let matches = |project: &mut crate::project::Project| {
			let program = new_transform_program(&directory);
			let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
			static_add_paths_matches(program, &mut checker, project).expect("StaticAddPathsMatches")
		};
		let before = matches(&mut project);
		write_transform_fixture(&directory, "src/server/services/new.ts", "export {};\n");
		write_transform_fixture(&directory, "src/server/services/excluded.test.ts", "export {};\n");
		let after = matches(&mut project);
		let globs = project
			.build_info_snapshot()
			.metadata
			.and_then(|metadata| metadata.globs);
		(before, after, globs)
	});

	let pattern = "src/server/services/**";
	let contains = |matches: &std::collections::BTreeMap<String, Vec<String>>, module: &str| {
		matches
			.get(pattern)
			.is_some_and(|modules| modules.iter().any(|candidate| candidate == module))
	};
	assert!(
		!contains(&before, "./services/new")
			&& contains(&after, "./services/new")
			&& !contains(&after, "./services/excluded.test"),
		"plain addPaths matches before={:?} after={:?}",
		before.get(pattern),
		after.get(pattern)
	);
	assert!(
		globs.is_none(),
		"plain addPaths changed flamework.build runtime glob metadata"
	);
}

#[test]
fn transform_removes_add_paths_when_first_statement_matches_nothing() {
	// The call precedes the (hoisted) import and matches no module.
	let (printed, _) = add_paths(&[
		r#"Flamework.addPathsGlob("src/missing/*.ts");"#,
		r#"import { Flamework } from "@flamework/core";"#,
	]);

	assert_eq!(printed, "import { Flamework } from \"@flamework/core\";\n");
}

// A hand-written Lua module is not a program source, so a static import cannot load it; the
// runtime call must stay.
#[test]
fn transform_keeps_runtime_add_paths_when_match_includes_lua_module() {
	let (_dir, directory) = temp_dir();
	write_add_paths_fixture(
		&directory,
		&[
			r#"import { Flamework } from "@flamework/core";"#,
			r#"Flamework.addPaths("src/server/services");"#,
		]
		.join("\n"),
	);
	write_transform_fixture(&directory, "src/server/services/legacy.lua", "return {}\n");

	let (printed, _) = transform_add_paths_fixture(&directory);

	assert!(
		printed.contains(r#"Flamework["_addPaths"]("#) && !printed.contains(r#"import ""#),
		"transformed source =\n{printed}\nwant runtime _addPaths call"
	);
}
