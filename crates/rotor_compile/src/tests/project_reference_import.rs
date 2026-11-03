//! `project_reference_import_test.go`. Mirrors upstream tests/compiler/referenceFixture.ts:
//! sibling projects share a base config, output mounts, and the root runtime library.

use std::collections::HashMap;
use std::path::Path;

use rotor_transformer::ProjectType;
use serde_json::{Map, Value, json};

use super::solution::{join, write_solution_file};
use super::solution_drain::must_read_file;
use super::{NO_LIB_GLOBAL_STUBS, Temp, large_stack};
use crate::census_solution::compile_solution_diagnostics;
use crate::crossproject::populate_cross_project_metadata;
use crate::output::{BuildFailure, BuildResult};
use crate::project::{ProjectOptions, create_cross_project_import_path_map, new_project_program_with_options};
use crate::solution::{SolutionCoordinator, build_solution_graph};
use crate::solution_drain::build_solution_with_options;

/// `referenceFixture`
pub struct ReferenceFixture {
	_temps: Vec<Temp>,
	pub dir: String,
	projects: Vec<String>,
}

/// `newReferenceFixture`
pub fn new_reference_fixture() -> ReferenceFixture {
	let temp = Temp::new();
	let dir = temp.dir();
	new_reference_fixture_at(vec![temp], dir)
}

/// `newReferenceFixtureAt`
fn new_reference_fixture_at(temps: Vec<Temp>, dir: String) -> ReferenceFixture {
	let fixture = ReferenceFixture {
		_temps: temps,
		dir,
		projects: Vec::new(),
	};
	fixture.json("package.json", &json!({"name": "reference-fixture"}));
	fixture.json(
		"base.json",
		&json!({"compilerOptions": {
			"allowSyntheticDefaultImports": true, "module": "commonjs",
			"moduleResolution": "Node", "moduleDetection": "force", "noLib": true,
			"strict": true, "target": "ESNext", "incremental": true,
			"skipLibCheck": true, "typeRoots": ["node_modules/@rbxts"],
			"types": ["globals"],
		}}),
	);
	fixture.write("node_modules/@rbxts/globals/index.d.ts", NO_LIB_GLOBAL_STUBS);
	fixture.write("include/RuntimeLib.lua", "return {}\n");
	fixture
}

/// Merges `overrides`' keys into `base`.
fn merged(mut base: Value, overrides: &Value) -> Value {
	if let (Some(base), Some(overrides)) = (base.as_object_mut(), overrides.as_object()) {
		for (key, value) in overrides {
			base.insert(key.clone(), value.clone());
		}
	}
	base
}

/// A [`BuildSolutionWithOptions`] outcome as Go's triple.
pub type Built = (Option<BuildResult>, Vec<String>, Option<String>);

/// `BuildSolutionWithOptions` under a large stack, as Go's `(result, messages, err)`.
pub fn build_solution_triple(config: &str, options: &ProjectOptions) -> Built {
	large_stack(|| match build_solution_with_options(config, options) {
		Ok(result) => (Some(result), Vec::new(), None),
		Err(BuildFailure { result, failure }) => (
			result.map(|result| *result),
			failure.messages(),
			Some(failure.error.to_string()),
		),
	})
}

impl ReferenceFixture {
	pub fn write(&self, path: &str, text: &str) {
		write_solution_file(&self.dir, path, text);
	}

	pub fn json(&self, path: &str, value: &Value) {
		self.write(path, &serde_json::to_string(value).expect("JSON"));
	}

	pub fn path(&self, rel: &str) -> String {
		join(&self.dir, &[rel])
	}

	pub fn read_json(&self, rel: &str) -> Map<String, Value> {
		serde_json::from_slice(&must_read_file(&self.path(rel))).expect("JSON")
	}

	pub fn project(&mut self, name: &str, references: &[&str], overrides: &Value) {
		self.projects.push(name.to_owned());
		let options = merged(
			json!({
				"rootDir": "src", "outDir": "out",
				"tsBuildInfoFile": format!("cache/{name}.tsbuildinfo"), "composite": name != "game",
			}),
			overrides,
		);
		let refs: Vec<Value> = references
			.iter()
			.map(|reference| json!({"path": format!("../{reference}")}))
			.collect();
		self.json(
			&format!("{name}/tsconfig.json"),
			&json!({
				"extends": "../base.json", "compilerOptions": options,
				"include": ["src"], "references": refs,
			}),
		);
		self.write(&format!("{name}/src/index.ts"), "export const value = 1;\n");
		self.rojo("default.project.json", "", &json!({}));
	}

	pub fn rojo(&self, path: &str, prefix: &str, overrides: &Value) {
		let mut mounts = json!({"$className": "ReplicatedStorage", "include": {"$path": format!("{prefix}include")}});
		for name in &self.projects {
			mounts[name] = json!({"$path": format!("{prefix}{name}/out")});
		}
		let mounts = merged(mounts, overrides);
		self.json(
			path,
			&json!({"name": "references", "tree": {"$className": "DataModel", "ReplicatedStorage": mounts}}),
		);
	}

	pub fn options(&self) -> ProjectOptions {
		ProjectOptions {
			rojo_config_path: self.path("default.project.json"),
			include_path: self.path("include"),
			..ProjectOptions::default()
		}
	}

	pub fn build(&self) -> BuildResult {
		let (result, messages, error) = build_solution_triple(&self.path("game/tsconfig.json"), &self.options());
		if let Some(error) = error {
			panic!("build: {error} ({messages:?})");
		}
		result.unwrap_or_default()
	}

	fn build_config(&self, rel: &str, what: &str) {
		let (_, messages, error) = build_solution_triple(&self.path(rel), &self.options());
		if let Some(error) = error {
			panic!("{what}: {error} ({messages:?})");
		}
	}

	fn read(&self, rel: &str) -> String {
		String::from_utf8(must_read_file(&self.path(rel))).expect("utf8")
	}

	/// `symlinkPackage`
	fn symlink_package(&self, name: &str) {
		symlink(&self.path(name), &self.path(&format!("node_modules/@rbxts/{name}")));
	}

	fn package_options(&self) -> ProjectOptions {
		ProjectOptions {
			include_path: self.path("include"),
			project_type: Some(ProjectType::Package),
			..ProjectOptions::default()
		}
	}
}

fn symlink(original: &str, link: &str) {
	#[cfg(unix)]
	std::os::unix::fs::symlink(original, link).unwrap_or_else(|error| panic!("symlinks unavailable: {error}"));
	#[cfg(windows)]
	std::os::windows::fs::symlink_dir(original, link).unwrap_or_else(|error| panic!("symlinks unavailable: {error}"));
}

fn assert_contains(output: &str, want: &str, what: &str) {
	assert!(output.contains(want), "{what}:\n{output}");
}

fn new_coordinator_error(config: &str, options: &ProjectOptions) -> Option<String> {
	large_stack(|| {
		SolutionCoordinator::new(config, options)
			.err()
			.map(|error| error.to_string())
	})
}

#[test]
fn project_reference_separate_declaration_dir() {
	let mut f = new_reference_fixture();
	f.project(
		"shared",
		&[],
		&json!({"declarationDir": "types", "declarationMap": true}),
	);
	f.project("game", &["shared"], &json!({}));
	f.write(
		"game/src/index.ts",
		r#"import { value } from "../../shared/types"; export const result = value;"#,
	);
	// Build declarations before the consumer resolves their directory.
	f.build_config("shared/tsconfig.json", "build shared");
	f.build();
	let output = f.read("game/out/init.luau");
	assert_contains(
		&output,
		r#"TS.import(script, game:GetService("ReplicatedStorage"), "shared")"#,
		"import did not use the referenced Luau mount",
	);
	assert!(Path::new(&f.path("shared/types/index.d.ts.map")).exists());
}

#[test]
fn project_reference_node_modules_symlink() {
	for preserve in [false, true] {
		test_project_reference_node_modules_symlink(new_reference_fixture(), preserve);
	}
}

/// An aliased fixture directory (a junction like an 8.3 short temp path, or a symlink) spells
/// project paths differently from their real paths. Junctions are Windows-only.
#[test]
fn project_reference_aliased_directory() {
	test_project_reference_node_modules_symlink(new_aliased_reference_fixture(), false);
}

#[test]
fn project_reference_aliased_directory_rojo_mounts() {
	let mut f = new_aliased_reference_fixture();
	f.project("shared", &[], &json!({}));
	f.project("game", &["shared"], &json!({}));
	f.write(
		"game/src/index.ts",
		r#"import { value } from "../../shared/src"; export const result = value;"#,
	);
	f.json(
		"shared/tsconfig.json",
		&json!({
			"extends": "../base.json", "compilerOptions": {
				"rootDir": "src", "outDir": "out", "composite": true, "tsBuildInfoFile": "cache/shared.tsbuildinfo",
			}, "include": ["src"], "rbxts": {"type": "game", "rojo": "owner.project.json"},
		}),
	);
	f.rojo("shared/owner.project.json", "../", &json!({}));
	f.build();
}

/// `newAliasedReferenceFixture` with a symlink.
fn new_aliased_reference_fixture() -> ReferenceFixture {
	let real_temp = Temp::new();
	let alias_temp = Temp::new();
	let real = join(&real_temp.dir(), &["real"]);
	let alias = join(&alias_temp.dir(), &["alias"]);
	std::fs::create_dir(&real).expect("the directory");
	symlink(&real, &alias);
	new_reference_fixture_at(vec![real_temp, alias_temp], alias)
}

fn test_project_reference_node_modules_symlink(mut f: ReferenceFixture, preserve_symlinks: bool) {
	f.project("shared", &[], &json!({"declarationDir": "types"}));
	f.project("game", &["shared"], &json!({"preserveSymlinks": preserve_symlinks}));
	f.json(
		"shared/package.json",
		&json!({"name": "@rbxts/shared", "types": "types/index.d.ts"}),
	);
	f.json(
		"shared/tsconfig.json",
		&json!({
			"extends": "../base.json", "compilerOptions": {
				"rootDir": "src", "outDir": "out", "declarationDir": "types", "composite": true,
				"tsBuildInfoFile": "cache/shared.tsbuildinfo",
			}, "include": ["src"], "rbxts": {"type": "game", "rojo": "../default.project.json"},
		}),
	);
	f.write("shared/src/helper.ts", "export const value = 7;\n");
	f.write(
		"game/src/index.ts",
		r#"import { value } from "@rbxts/shared/types/helper"; export const result = value;"#,
	);
	symlink(&f.path("shared"), &f.path("node_modules/@rbxts/shared"));
	f.build_config("shared/tsconfig.json", "build shared");
	f.build();
	let output = f.read("game/out/init.luau");
	assert_contains(
		&output,
		r#"TS.import(script, game:GetService("ReplicatedStorage"), "shared", "helper")"#,
		"symlink import did not use the referenced project's mount",
	);
}

#[test]
fn project_reference_rojo_mounts() {
	for (name, path, mount) in [
		("matching", "", ""),
		("module", "shared/out/init.luau", "ReplicatedStorage.shared"),
		(
			"unimported module",
			"shared/out/tests/check.luau",
			"ReplicatedStorage.shared.tests.check",
		),
		(
			"runtime",
			"include/RuntimeLib.lua",
			"ReplicatedStorage.include.RuntimeLib",
		),
		("transitive module", "common/out/init.luau", "ReplicatedStorage.common"),
	] {
		let mut f = new_reference_fixture();
		f.project("common", &[], &json!({}));
		f.project("shared", &["common"], &json!({}));
		f.project("game", &["shared"], &json!({}));
		f.write("shared/src/tests/check.ts", "export const check = 1;\n");
		f.write(
			"game/src/index.ts",
			r#"import { value } from "../../shared/src"; export const result = value;"#,
		);
		f.json(
			"shared/tsconfig.json",
			&json!({
				"extends": "../base.json", "compilerOptions": {
					"rootDir": "src", "outDir": "out", "composite": true, "tsBuildInfoFile": "cache/shared.tsbuildinfo",
				}, "include": ["src"], "references": [{"path": "../common"}],
				"rbxts": {"type": "game", "rojo": "owner.project.json"},
			}),
		);
		f.rojo("shared/owner.project.json", "../", &json!({}));
		if path.is_empty() {
			f.build();
			continue;
		}
		f.rojo("default.project.json", "", &json!({"different": {"$path": path}}));
		let error = new_coordinator_error(&f.path("game/tsconfig.json"), &f.options());
		let want = format!(
			r#"Project "{}" must mount "{}" at the same Roblox path as "{}" ({mount}). Referenced projects with their own rbxts.rojo require consistent module and runtime mounts."#,
			f.path("game/tsconfig.json"),
			f.path(path),
			f.path("shared/owner.project.json")
		);
		assert!(
			error.as_deref().is_some_and(|error| error.contains(&want)),
			"{name}: error = {error:?}, want {want}"
		);
	}
}

#[test]
fn project_reference_validation() {
	for (name, options, message) in [
		(
			"composite",
			json!({"composite": false}),
			r#"must have setting "composite": true"#,
		),
		("noEmit", json!({"noEmit": true}), "may not disable emit"),
		("declaration", json!({"declaration": false}), "may not disable emit"),
		(
			"shared build info",
			json!({"tsBuildInfoFile": "../game/cache/game.tsbuildinfo"}),
			"unique tsBuildInfoFile",
		),
		(
			"build info in another output",
			json!({"tsBuildInfoFile": "../game/out/shared.tsbuildinfo"}),
			"another project's output",
		),
		(
			"output containing source",
			json!({"outDir": "../game"}),
			"contains source directory",
		),
		(
			"overlapping outputs",
			json!({"outDir": "../game/out/shared"}),
			"overlapping output directories",
		),
		(
			"overlapping declarations",
			json!({"declarationDir": "../game/out/types"}),
			"overlapping output directories",
		),
	] {
		let mut f = new_reference_fixture();
		f.project("shared", &[], &options);
		f.project("game", &["shared"], &json!({}));
		f.write("game/out/keep.luau", "return 7\n");
		let (result, _, error) = build_solution_triple(&f.path("game/tsconfig.json"), &f.options());
		assert!(
			error.as_deref().is_some_and(|error| error.contains(message)),
			"{name}: error = {error:?}, want {message}"
		);
		let code = HashMap::from([("composite", "TS6306"), ("noEmit", "TS6310"), ("declaration", "TS6310")]);
		if let Some(code) = code.get(name) {
			assert!(
				result
					.as_ref()
					.is_some_and(|result| result.diagnostics.len() == 1 && result.diagnostics[0].code == *code),
				"{name}: result = {result:?}, want diagnostic {code}"
			);
		}
		assert_eq!(
			f.read("game/out/keep.luau"),
			"return 7\n",
			"{name}: existing output changed"
		);
	}
}

#[test]
fn project_reference_incremental_import_paths() {
	let mut f = new_reference_fixture();
	f.project("shared", &[], &json!({"declarationDir": "types"}));
	f.project("game", &["shared"], &json!({}));
	f.write("shared/src/left.ts", "export const left = 1;\n");
	f.write("shared/src/right.ts", "export const right = 1;\n");
	f.write(
		"game/src/index.ts",
		r#"import { value } from "../../shared/types"; export const result = value;"#,
	);
	f.rojo("default.project.json", "", &json!({"moved": {"$path": "shared/next"}}));
	f.build();
	for _ in 0..5 {
		let result = f.build();
		assert!(
			result.outputs.is_empty(),
			"unchanged reference paths caused a rebuild: {:?}",
			result.outputs.keys().collect::<Vec<_>>()
		);
	}
	let declaration = f.read("shared/types/index.d.ts");
	let mut config = f.read_json("shared/tsconfig.json");
	config["compilerOptions"]["outDir"] = json!("next");
	f.json("shared/tsconfig.json", &Value::Object(config));
	f.build();
	let output = f.read("game/out/init.luau");
	assert_contains(
		&output,
		r#"TS.import(script, game:GetService("ReplicatedStorage"), "moved")"#,
		"cached import did not follow the referenced output directory",
	);
	assert_eq!(
		f.read("shared/types/index.d.ts"),
		declaration,
		"declaration unexpectedly changed"
	);
}

/// Sets `shared/tsconfig.json`'s `rbxts` to its own Rojo config.
fn own_shared_rojo(f: &ReferenceFixture) {
	let mut config = f.read_json("shared/tsconfig.json");
	config.insert(
		"rbxts".to_owned(),
		json!({"rojo": "owner.project.json", "type": "game"}),
	);
	f.json("shared/tsconfig.json", &Value::Object(config));
	f.rojo("shared/owner.project.json", "../", &json!({}));
}

#[test]
fn project_reference_rojo_mounts_refresh() {
	let mut f = new_reference_fixture();
	f.project("shared", &[], &json!({}));
	f.project("game", &["shared"], &json!({}));
	f.write(
		"game/src/index.ts",
		r#"import { value } from "../../shared/src"; export const result = value;"#,
	);
	let config_path = f.path("shared/tsconfig.json");
	own_shared_rojo(&f);
	large_stack(|| {
		let mut coordinator = SolutionCoordinator::new(&f.path("game/tsconfig.json"), &f.options())
			.unwrap_or_else(|error| panic!("{error}"));
		let (_, messages, error) = coordinator.drain();
		assert!(error.is_none(), "first build: {error:?} ({messages:?})");
		let before = f.read("game/out/init.luau");
		f.rojo(
			"shared/owner.project.json",
			"../",
			&json!({"changed": {"$path": "../shared/out/init.luau"}}),
		);
		coordinator.invalidate(std::slice::from_ref(&config_path));
		let (_, messages, error) = coordinator.drain();
		assert!(
			error.is_some() && messages.join("\n").contains("must mount"),
			"changed reference mount: {error:?} ({messages:?})"
		);
		assert_eq!(
			f.read("game/out/init.luau"),
			before,
			"output changed after inconsistent mounts"
		);
	});
}

#[test]
fn project_reference_transitive_imports() {
	let mut f = new_reference_fixture();
	f.project("common", &[], &json!({"declarationDir": "types"}));
	f.project("left", &["common"], &json!({}));
	f.project("right", &["common"], &json!({}));
	f.project("game", &["left", "right"], &json!({}));
	f.write(
		"game/src/index.ts",
		r#"import { value } from "../../common/types"; export const result = value;"#,
	);
	f.build();
	assert_contains(
		&f.read("game/out/init.luau"),
		r#"TS.import(script, game:GetService("ReplicatedStorage"), "common")"#,
		"transitive declaration import did not use the common project's mount",
	);
}

#[test]
fn project_reference_added_source() {
	let mut f = new_reference_fixture();
	f.project("shared", &[], &json!({"declarationDir": "types"}));
	f.project("game", &["shared"], &json!({}));
	f.write(
		"game/src/index.ts",
		r#"import { value } from "../../shared/types"; export const result = value;"#,
	);
	large_stack(|| {
		let mut coordinator = SolutionCoordinator::new(&f.path("game/tsconfig.json"), &f.options())
			.unwrap_or_else(|error| panic!("{error}"));
		let (_, messages, error) = coordinator.drain();
		assert!(error.is_none(), "first build: {error:?} ({messages:?})");
		f.write("shared/src/added.ts", "export const added = 8;\n");
		f.write(
			"game/src/index.ts",
			r#"import { added } from "../../shared/types/added"; export const result = added;"#,
		);
		coordinator.invalidate(&[f.path("shared/tsconfig.json")]);
		let (_, messages, error) = coordinator.drain();
		assert!(error.is_none(), "added reference source: {error:?} ({messages:?})");
	});
	assert_contains(
		&f.read("game/out/init.luau"),
		r#"TS.import(script, game:GetService("ReplicatedStorage"), "shared", "added")"#,
		"added declaration import did not use the referenced project's mount",
	);
}

#[test]
fn project_reference_rojo_mounts_after_dependency_copy() {
	let mut f = new_reference_fixture();
	f.project("shared", &[], &json!({}));
	f.project("game", &["shared"], &json!({}));
	f.write(
		"game/src/index.ts",
		r#"import { value } from "../../shared/src"; export const result = value;"#,
	);
	own_shared_rojo(&f);
	f.json(
		"shared/src/default.project.json",
		&json!({"name": "nested", "tree": {"current": {"$path": "init.luau"}}}),
	);
	f.rojo(
		"default.project.json",
		"",
		&json!({"shared": {"$path": "shared/out/init.luau"}}),
	);
	let (_, messages, error) = build_solution_triple(&f.path("game/tsconfig.json"), &f.options());
	assert!(
		error
			.as_deref()
			.is_some_and(|error| format!("{error}{}", messages.join("\n")).contains("must mount")),
		"inconsistent copied reference mount: {error:?} ({messages:?})"
	);
	assert!(
		!Path::new(&f.path("game/out/init.luau")).exists(),
		"consumer emitted with inconsistent mounts"
	);
}

/// Package output stays relocatable: a package reaches a referenced package through getModule,
/// and the reference map only supplies the Luau path.
#[test]
fn project_reference_package_import() {
	type Case<'a> = (&'a str, Value, &'a str, Value, &'a [(&'a str, &'a str)]);
	let cases: [Case<'_>; 2] = [
		(
			"luau next to declarations",
			json!({}),
			"src/**/*.d.ts",
			json!({"types": "src/helper.d.ts", "main": "out/helper.luau"}),
			&[
				("lib/src/helper.d.ts", "export declare const value: number;\n"),
				("lib/src/helper.luau", "return { value = 7 }\n"),
			],
		),
		(
			"declaration directory",
			json!({"declarationDir": "types"}),
			"src",
			json!({"types": "types/helper.d.ts"}),
			&[("lib/src/helper.ts", "export const value = 7;\n")],
		),
	];
	for (name, options, include, manifest, files) in cases {
		let f = new_reference_fixture();
		let options = merged(
			json!({"rootDir": "src", "outDir": "out", "composite": true, "tsBuildInfoFile": "cache/lib.tsbuildinfo"}),
			&options,
		);
		f.json(
			"lib/tsconfig.json",
			&json!({
				"extends": "../base.json", "compilerOptions": options, "include": [include],
				"rbxts": {"type": "package"},
			}),
		);
		f.json("lib/package.json", &merged(manifest, &json!({"name": "@rbxts/lib"})));
		for (path, text) in files {
			f.write(path, text);
		}
		f.json(
			"pkg/tsconfig.json",
			&json!({
				"extends": "../base.json", "compilerOptions": {
					"rootDir": "src", "outDir": "out", "composite": true, "tsBuildInfoFile": "cache/pkg.tsbuildinfo",
				}, "include": ["src"], "references": [{"path": "../lib"}],
				"rbxts": {"type": "package"},
			}),
		);
		f.write(
			"pkg/src/index.ts",
			r#"import { value } from "@rbxts/lib"; export const result = value;"#,
		);
		f.symlink_package("lib");
		let (result, messages, error) = build_solution_triple(&f.path("pkg/tsconfig.json"), &f.package_options());
		assert!(error.is_none(), "{name}: build: {error:?} ({messages:?})");
		let result = result.unwrap_or_default();
		assert!(
			result.diagnostics.is_empty(),
			"{name}: diagnostics: {:?}",
			result.diagnostics
		);
		assert_contains(
			&f.read("pkg/out/init.luau"),
			r#"TS.getModule(script, "@rbxts", "lib").out.helper"#,
			"package import did not use getModule with the referenced Luau path",
		);
	}
}

/// A game reaches a referenced game project imported by package name through the project's
/// Rojo mount, even when that mount is outside `node_modules`.
#[test]
fn project_reference_game_package_name_import() {
	let mut f = new_reference_fixture();
	f.project("shared", &[], &json!({}));
	f.project("game", &["shared"], &json!({}));
	f.json("shared/package.json", &json!({"name": "@rbxts/shared"}));
	f.write("shared/src/test/helpers.ts", "export const value = 7;\n");
	f.write(
		"game/src/index.ts",
		r#"import { value } from "@rbxts/shared/src/test/helpers"; export const result = value;"#,
	);
	f.json(
		"default.project.json",
		&json!({"name": "references", "tree": {
			"$className": "DataModel", "ReplicatedStorage": {
				"$className": "ReplicatedStorage", "include": {"$path": "include"},
				"game": {"$path": "game/out"}, "shared-tests": {"$path": "shared/out"},
			},
		}}),
	);
	f.symlink_package("shared");
	let result = f.build();
	assert!(result.diagnostics.is_empty(), "diagnostics: {:?}", result.diagnostics);
	assert_contains(
		&f.read("game/out/init.luau"),
		r#"TS.import(script, game:GetService("ReplicatedStorage"), "shared-tests", "test", "helpers")"#,
		"game import did not use the referenced project's mount",
	);
}

/// `solutionLibFixture`: a lib whose tsconfig.json is a solution config over a package lib and
/// a game spec with its own rbxts.rojo.
fn solution_lib_fixture(plugin_reference: &str) -> ReferenceFixture {
	let f = new_reference_fixture();
	f.json(
		"lib/tsconfig.json",
		&json!({
			"compilerOptions": {"composite": true}, "files": [], "include": [],
			"references": [{"path": "./tsconfig.lib.json"}, {"path": "./tsconfig.spec.json"}],
		}),
	);
	f.json(
		"lib/tsconfig.lib.json",
		&json!({
			"extends": "../base.json", "compilerOptions": {
				"rootDir": "src", "outDir": "out", "declarationDir": "types", "composite": true,
				"tsBuildInfoFile": "cache/lib.tsbuildinfo",
			}, "include": ["src"], "rbxts": {"type": "package"},
		}),
	);
	f.json(
		"lib/tsconfig.spec.json",
		&json!({
			"extends": "../base.json", "compilerOptions": {
				"rootDir": "spec", "outDir": "out-test", "composite": true,
				"tsBuildInfoFile": "cache/spec.tsbuildinfo",
			}, "include": ["spec"], "rbxts": {"type": "game", "rojo": "./test.project.json"},
		}),
	);
	f.json(
		"lib/package.json",
		&json!({"name": "@rbxts/lib", "types": "types/helper.d.ts"}),
	);
	f.write("lib/src/helper.ts", "export const value = 7;\n");
	f.write("lib/spec/helper.test.ts", "export const checked = true;\n");
	f.json(
		"lib/test.project.json",
		&json!({"name": "lib-tests", "tree": {
			"$className": "DataModel", "ReplicatedStorage": {
				"$className": "ReplicatedStorage", "include": {"$path": "../include"},
				"lib-tests": {"$path": "out-test"},
			},
		}}),
	);
	f.json(
		"plugin/tsconfig.json",
		&json!({
			"extends": "../base.json", "compilerOptions": {
				"rootDir": "src", "outDir": "out", "composite": true, "tsBuildInfoFile": "cache/plugin.tsbuildinfo",
			}, "include": ["src"], "references": [{"path": plugin_reference}],
			"rbxts": {"type": "package"},
		}),
	);
	f.write(
		"plugin/src/index.ts",
		r#"import { value } from "@rbxts/lib"; export const result = value;"#,
	);
	f.symlink_package("lib");
	f
}

/// Upstream gives a solution config no path translator and checks Rojo mounts only for direct
/// references, so a game spec reached through a solution reference is not checked.
#[test]
fn project_reference_solution_rojo_mounts() {
	let f = solution_lib_fixture("../lib");
	let (result, messages, error) = build_solution_triple(&f.path("plugin/tsconfig.json"), &f.package_options());
	assert!(error.is_none(), "build: {error:?} ({messages:?})");
	let result = result.unwrap_or_default();
	assert!(result.diagnostics.is_empty(), "diagnostics: {:?}", result.diagnostics);
	assert_contains(
		&f.read("plugin/out/init.luau"),
		r#"TS.getModule(script, "@rbxts", "lib").out.helper"#,
		"import through the solution reference did not resolve",
	);
}

#[test]
fn project_reference_direct_spec_rojo_mounts() {
	let f = solution_lib_fixture("../lib/tsconfig.spec.json");
	let error = new_coordinator_error(&f.path("plugin/tsconfig.json"), &f.package_options());
	let want = format!(
		r#"Project "{}" must mount "{}" at the same Roblox path as "{}" (ReplicatedStorage.lib-tests.helper.test). Referenced projects with their own rbxts.rojo require consistent module and runtime mounts."#,
		f.path("plugin/tsconfig.json"),
		f.path("lib/out-test/helper.test.luau"),
		f.path("lib/test.project.json")
	);
	assert!(
		error.as_deref().is_some_and(|error| error.contains(&want)),
		"error = {error:?}, want {want}"
	);
}

/// Hand-written declarations and JSON outside rootDir describe files mapped directly by Rojo;
/// index declarations reach the init module beside them.
#[test]
fn project_import_outside_root_dir() {
	let mut f = new_reference_fixture();
	f.project("game", &[], &json!({"resolveJsonModule": true}));
	f.write("lib/index.d.ts", "export declare const value: number;\n");
	f.write("lib/init.luau", "return { value = 1 }\n");
	f.write("lib/pure.d.ts", "export declare const pure: number;\n");
	f.write("lib/pure.luau", "return { pure = 2 }\n");
	f.json("lib/data.json", &json!({"count": 3}));
	f.rojo("default.project.json", "", &json!({"lib": {"$path": "lib"}}));
	f.write(
		"game/src/index.ts",
		"import { value } from \"../../lib\";\nimport { pure } from \"../../lib/pure\";\nimport data from \"../../lib/data.json\";\nexport const result = value + pure + data.count;",
	);
	let result = f.build();
	assert!(result.diagnostics.is_empty(), "diagnostics: {:?}", result.diagnostics);
	let output = f.read("game/out/init.luau");
	for want in [
		r#"TS.import(script, game:GetService("ReplicatedStorage"), "lib")"#,
		r#"TS.import(script, game:GetService("ReplicatedStorage"), "lib", "pure")"#,
		r#"TS.import(script, game:GetService("ReplicatedStorage"), "lib", "data")"#,
	] {
		assert_contains(&output, want, &format!("missing {want}"));
	}
}

/// TypeScript reports a source file outside rootDir, but a census still transforms the
/// importer; the import must not fail the transform.
#[test]
fn project_import_source_outside_root_dir_census() {
	let mut f = new_reference_fixture();
	f.project("game", &[], &json!({}));
	f.write("lib/helper.ts", "export const value = 1;\n");
	f.write(
		"game/src/index.ts",
		r#"import { value } from "../../lib/helper"; export const result = value;"#,
	);
	let (solution, error) = large_stack(|| compile_solution_diagnostics(&f.path("game/tsconfig.json"), &f.options()));
	assert!(error.is_none(), "{error:?}");
	for project in &solution.projects {
		for file in &project.files {
			assert!(
				file.internal_error.is_none(),
				"{}: {:?}",
				file.file_name,
				file.internal_error
			);
		}
	}
}

/// A file named "..x" sits inside rootDir, so its import uses the outDir path. Rojo itself
/// treats such a name as outside its mount.
#[test]
fn project_import_dot_dot_name_inside_root_dir() {
	let mut f = new_reference_fixture();
	f.project("game", &[], &json!({}));
	f.write("game/src/..types.d.ts", "export declare const value: number;\n");
	f.write(
		"game/src/index.ts",
		r#"import { value } from "./..types"; export const result = value;"#,
	);
	let (_, messages, _) = build_solution_triple(&f.path("game/tsconfig.json"), &f.options());
	let want = rotor_support::gopath::join(&["out", "..types.luau"]);
	assert!(
		messages.join("\n").contains(&want),
		"messages = {messages:?}, want path {want}"
	);
}

/// A direct build maps a reference with its own luau option, like a solution build, so both
/// agree on the incremental salt.
#[test]
fn project_reference_paths_use_reference_luau() {
	let mut f = new_reference_fixture();
	f.project("shared", &[], &json!({"declarationDir": "types"}));
	f.project("game", &["shared"], &json!({}));
	let mut config = f.read_json("shared/tsconfig.json");
	config.insert("rbxts".to_owned(), json!({"luau": false}));
	f.json("shared/tsconfig.json", &Value::Object(config));
	let config_path = f.path("game/tsconfig.json");
	large_stack(|| {
		let mut graph = build_solution_graph(&config_path, &f.options()).unwrap_or_else(|error| panic!("{error}"));
		populate_cross_project_metadata(&mut graph);
		let solution = graph
			.projects
			.iter()
			.find(|project| project.config_path == config_path)
			.and_then(|project| project.options.project_reference_paths.clone())
			.map(|paths| (*paths).clone())
			.unwrap_or_default();
		let (_, program) = new_project_program_with_options(
			&rotor_support::gopath::dir(&config_path),
			&config_path,
			&ProjectOptions::default(),
		)
		.unwrap_or_else(|failure| panic!("{}", failure.error));
		let direct = create_cross_project_import_path_map(program, &f.options());
		assert!(
			!solution.is_empty() && direct == solution,
			"direct = {direct:?}, solution = {solution:?}"
		);
	});
}

/// A census reports every project even when solution validation fails.
#[test]
fn project_reference_validation_census() {
	let mut f = new_reference_fixture();
	f.project("shared", &[], &json!({"composite": false}));
	f.project("game", &["shared"], &json!({}));
	let (solution, error) = large_stack(|| compile_solution_diagnostics(&f.path("game/tsconfig.json"), &f.options()));
	let error = error.map(|error| error.to_string());
	assert!(
		error
			.as_deref()
			.is_some_and(|error| error.contains(r#"must have setting "composite": true"#)),
		"error = {error:?}"
	);
	assert_eq!(solution.projects.len(), 2, "projects");
}

/// A project builds when only an unrelated project's reference mounts break.
#[test]
fn project_reference_rojo_mounts_unrelated_project() {
	let mut f = new_reference_fixture();
	f.project("shared", &[], &json!({}));
	f.project("game", &["shared"], &json!({}));
	f.project("other", &[], &json!({}));
	f.write(
		"game/src/index.ts",
		r#"import { value } from "../../shared/src"; export const result = value;"#,
	);
	own_shared_rojo(&f);
	f.json(
		"shared/src/default.project.json",
		&json!({"name": "nested", "tree": {"current": {"$path": "init.luau"}}}),
	);
	f.rojo(
		"default.project.json",
		"",
		&json!({"shared": {"$path": "shared/out/init.luau"}}),
	);
	let options = ProjectOptions {
		single_threaded: Some(true),
		..f.options()
	};
	large_stack(|| {
		let mut coordinator =
			SolutionCoordinator::for_roots(&[f.path("game/tsconfig.json"), f.path("other/tsconfig.json")], &options)
				.unwrap_or_else(|error| panic!("{error}"));
		let _ = coordinator.drain();
	});
	assert!(
		Path::new(&f.path("other/out/init.luau")).exists(),
		"unrelated project did not build"
	);
	assert!(
		!Path::new(&f.path("game/out/init.luau")).exists(),
		"consumer emitted with inconsistent mounts"
	);
}
