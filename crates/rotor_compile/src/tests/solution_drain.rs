//! `solution_drain_test.go`, and the cross-project solution the other solution tests share.

use std::path::Path;

use super::solution::{join, write_solution_file};
use super::{NO_LIB_GLOBAL_STUBS, Temp, large_stack};
use crate::copyfilesgate::COPY_FILES_CACHE_FILENAME;
use crate::project::ProjectOptions;
use crate::solution::SolutionCoordinator;
use crate::solution_drain::build_solution_with_options;

/// `crossProjectCompilerOptions`: the config through `compilerOptions`, left open.
pub fn cross_project_compiler_options(declaration: bool) -> String {
	format!(
		r#"{{"compilerOptions":{{"allowSyntheticDefaultImports":true,"composite":true,"declaration":{declaration},"module":"CommonJS","moduleResolution":"Node","noLib":true,"moduleDetection":"force","strict":true,"target":"ESNext","types":[],"typeRoots":["node_modules/@rbxts"],"rootDir":"src","outDir":"out"}}"#
	)
}

/// `writeCrossProjectSolution`: the root, the library and the game.
pub fn write_cross_project_solution() -> (Temp, String, String, String) {
	let temp = Temp::new();
	let root = temp.dir();
	let lib_dir = join(&root, &["lib"]);
	let game_dir = join(&root, &["game"]);
	for dir in [
		join(&lib_dir, &["src", "handauth"]),
		join(&game_dir, &["src"]),
		join(&game_dir, &["include"]),
		join(&lib_dir, &["node_modules", "@rbxts"]),
		join(&game_dir, &["node_modules", "@rbxts"]),
	] {
		std::fs::create_dir_all(dir).expect("the directory");
	}
	write_solution_file(
		&root,
		"tsconfig.json",
		r#"{"files":[],"references":[{"path":"./lib"},{"path":"./game"}]}"#,
	);
	write_solution_file(&lib_dir, "package.json", r#"{"name":"@scope/lib"}"#);
	write_solution_file(
		&lib_dir,
		"default.project.json",
		r#"{"name":"lib","tree":{"$path":"out"}}"#,
	);
	write_solution_file(
		&lib_dir,
		"tsconfig.json",
		&format!(r#"{},"include":["src"]}}"#, cross_project_compiler_options(true)),
	);
	write_solution_file(&lib_dir, "src/globals.d.ts", NO_LIB_GLOBAL_STUBS);
	write_solution_file(
		&lib_dir,
		"src/regular.ts",
		"export function regular(): string { return \"regular\"; }\n",
	);
	write_solution_file(
		&lib_dir,
		"src/handauth/index.d.ts",
		"export declare function handauth(): string;\n",
	);
	write_solution_file(
		&lib_dir,
		"src/handauth/init.luau",
		"return { handauth = function() return \"handauth\" end }\n",
	);
	write_solution_file(&game_dir, "package.json", r#"{"name":"game"}"#);
	write_solution_file(
		&game_dir,
		"tsconfig.json",
		&format!(
			r#"{},"rbxts":{{"rojo":"./game.project.json","type":"game"}},"include":["src"],"references":[{{"path":"../lib"}}]}}"#,
			cross_project_compiler_options(true)
		),
	);
	write_solution_file(&game_dir, "src/globals.d.ts", NO_LIB_GLOBAL_STUBS);
	write_solution_file(
		&game_dir,
		"src/index.ts",
		"import { handauth } from \"../../lib/src/handauth\";\nimport { regular } from \"../../lib/src/regular\";\nexport const value = regular() + handauth();\n",
	);
	write_solution_file(&game_dir, "include/RuntimeLib.lua", "return {}\n");
	write_solution_file(
		&game_dir,
		"game.project.json",
		r#"{"name":"game","tree":{"$className":"DataModel","ReplicatedStorage":{"include":{"$path":"include"},"shared":{"$path":"../lib/out"},"game":{"$path":"out"}}}}"#,
	);
	(temp, root, lib_dir, game_dir)
}

/// `mustReadFile`
pub fn must_read_file(path: &str) -> Vec<u8> {
	std::fs::read(path).unwrap_or_else(|error| panic!("read {path}: {error}"))
}

/// `onlyCacheFile`
pub fn only_cache_file(dir: &str) -> String {
	let entries: Vec<_> = std::fs::read_dir(dir)
		.unwrap_or_else(|error| panic!("read {dir}: {error}"))
		.flatten()
		.collect();
	assert_eq!(entries.len(), 1, "cache files = {entries:?}, want exactly one");
	entries[0].path().to_string_lossy().into_owned()
}

/// A solution build that must succeed.
pub fn build_solution(config: &str, options: &ProjectOptions) {
	large_stack(|| {
		if let Err(failure) = build_solution_with_options(config, options) {
			panic!("BuildSolutionWithOptions: {failure} ({:?})", failure.failure.messages());
		}
	});
}

#[test]
fn cross_project_import_map() {
	let (_temp, root, lib_dir, game_dir) = write_cross_project_solution();
	build_solution(&join(&root, &["tsconfig.json"]), &ProjectOptions::default());
	let game_output = String::from_utf8(must_read_file(&join(&game_dir, &["out", "init.luau"]))).expect("utf8");
	for module in ["regular", "handauth"] {
		assert!(
			game_output.contains(&format!(r#""shared", "{module}""#)),
			"game import for {module} did not use the library mount:\n{game_output}"
		);
		assert!(
			!game_output.contains(&format!(r#""game", "{module}""#)),
			"game import for {module} used the game mount:\n{game_output}"
		);
	}
	assert!(
		Path::new(&join(&lib_dir, &["out", "regular.d.ts"])).exists(),
		"library declaration"
	);
}

#[test]
fn drain_invalidated_project() {
	let (_temp, root, lib_dir, game_dir) = write_cross_project_solution();
	large_stack(|| {
		let mut coordinator = SolutionCoordinator::new(&join(&root, &["tsconfig.json"]), &ProjectOptions::default())
			.unwrap_or_else(|error| panic!("NewSolutionCoordinator: {error}"));
		let (_, messages, error) = coordinator.drain();
		assert!(error.is_none(), "Drain: {error:?} ({messages:?})");
	});
	for path in [
		join(&lib_dir, &["out", "regular.luau"]),
		join(&lib_dir, &["out", "regular.d.ts"]),
		join(&game_dir, &["out", "init.luau"]),
		join(&game_dir, &["out", "index.d.ts"]),
	] {
		assert!(Path::new(&path).exists(), "drain did not emit {path}");
	}
}

#[test]
fn inherited_reference_only_coordinator_preserves_leaf_declarations() {
	let temp = Temp::new();
	let root = temp.dir();
	let package_dir = join(&root, &["package"]);
	write_solution_file(
		&root,
		"tsconfig.json",
		r#"{"files":[],"references":[{"path":"./package"},{"path":"./package/tsconfig.lib.json"}]}"#,
	);
	write_solution_file(
		&package_dir,
		"tsconfig.base.json",
		r#"{
		"compilerOptions": {
			"allowSyntheticDefaultImports": true,
			"composite": true,
			"module": "CommonJS",
			"moduleResolution": "Node",
			"noLib": true,
			"moduleDetection": "force",
			"strict": true,
			"target": "ESNext",
			"types": ["globals"],
			"typeRoots": ["node_modules/@rbxts"],
			"rootDir": "src",
			"outDir": "out"
		},
		"files": [],
		"include": []
	}"#,
	);
	write_solution_file(
		&package_dir,
		"tsconfig.json",
		r#"{"extends":"./tsconfig.base.json","references":[{"path":"./tsconfig.lib.json"}]}"#,
	);
	write_solution_file(
		&package_dir,
		"tsconfig.lib.json",
		r#"{"extends":"./tsconfig.base.json","compilerOptions":{"declaration":true},"include":["src"]}"#,
	);
	write_solution_file(&package_dir, "package.json", r#"{"name":"@scope/package"}"#);
	write_solution_file(
		&package_dir,
		"node_modules/@rbxts/globals/index.d.ts",
		NO_LIB_GLOBAL_STUBS,
	);
	write_solution_file(&package_dir, "src/index.ts", "export const value = 1;\n");
	build_solution(&join(&root, &["tsconfig.json"]), &ProjectOptions::default());
	assert!(
		Path::new(&join(&package_dir, &["out", "index.d.ts"])).exists(),
		"leaf declaration was removed"
	);
}

#[test]
fn cross_project_declarations() {
	let (_temp, root, lib_dir, game_dir) = write_cross_project_solution();
	build_solution(&join(&root, &["tsconfig.json"]), &ProjectOptions::default());
	build_solution(&join(&game_dir, &["tsconfig.json"]), &ProjectOptions::default());
	let game_output = String::from_utf8(must_read_file(&join(&game_dir, &["out", "init.luau"]))).expect("utf8");
	assert!(
		game_output.contains(r#""shared", "handauth""#),
		"handwritten declaration import did not use library output:\n{game_output}"
	);
	assert!(
		Path::new(&join(&lib_dir, &["out", "handauth", "index.d.ts"])).exists(),
		"handwritten declaration was not copied"
	);
}

#[test]
fn solution_failure_rollback() {
	let (_temp, root, lib_dir, game_dir) = write_cross_project_solution();
	let lib_config_path = join(&lib_dir, &["tsconfig.json"]);
	let lib_config = String::from_utf8(must_read_file(&lib_config_path))
		.expect("utf8")
		.replacen(
			r#""outDir":"out""#,
			r#""outDir":"out","incremental":true,"tsBuildInfoFile":"out/cache.tsbuildinfo""#,
			1,
		);
	std::fs::write(&lib_config_path, lib_config).expect("write");
	build_solution(&join(&root, &["tsconfig.json"]), &ProjectOptions::default());

	let build_info_path = join(&lib_dir, &["out", "cache.rbxtsc.tsbuildinfo"]);
	let copy_cache_path = join(&lib_dir, &["out", COPY_FILES_CACHE_FILENAME]);
	let rojo_cache_path = only_cache_file(&join(&lib_dir, &[".rotor", "cache", "rojo"]));
	let prior_build_info = must_read_file(&build_info_path);
	let prior_copy_cache = must_read_file(&copy_cache_path);
	let prior_rojo_cache = must_read_file(&rojo_cache_path);
	let prior_game_output = must_read_file(&join(&game_dir, &["out", "init.luau"]));

	std::fs::write(
		join(&lib_dir, &["src", "regular.ts"]),
		"export function regular(): string { return \"changed\"; }\n",
	)
	.expect("write");
	std::fs::remove_file(join(&lib_dir, &["out", "regular.luau"])).expect("remove");
	std::fs::create_dir(join(&lib_dir, &["out", "regular.luau"])).expect("mkdir");
	let project_json = std::fs::File::options()
		.write(true)
		.open(join(&lib_dir, &["default.project.json"]))
		.expect("open");
	project_json
		.set_modified(std::time::SystemTime::now())
		.expect("chtimes");

	let result =
		large_stack(|| build_solution_with_options(&join(&root, &["tsconfig.json"]), &ProjectOptions::default()));
	assert!(
		result.is_err(),
		"BuildSolutionWithOptions succeeded after the library output became a directory"
	);
	assert_eq!(
		must_read_file(&build_info_path),
		prior_build_info,
		"build info changed after failed library emit"
	);
	assert_eq!(
		must_read_file(&copy_cache_path),
		prior_copy_cache,
		"copy cache changed after failed library emit"
	);
	assert_eq!(
		must_read_file(&rojo_cache_path),
		prior_rojo_cache,
		"Rojo cache changed after failed library emit"
	);
	assert_eq!(
		must_read_file(&join(&game_dir, &["out", "init.luau"])),
		prior_game_output,
		"dependent output changed after failed library emit"
	);
}
