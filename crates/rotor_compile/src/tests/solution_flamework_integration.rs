//! `solution_flamework_integration_test.go`

use std::path::Path;

use super::solution::write_solution_file;
use super::solution_drain::{cross_project_compiler_options, must_read_file};
use super::{Temp, large_stack, repo_root};
use crate::project::ProjectOptions;
use crate::solution_drain::build_solution_with_options;

#[test]
fn solution_flamework_package_build_data_reaches_dependent_native_transform() {
	// Given: a native Flamework package linked into a dependent game with stale build data.
	let temp = Temp::new();
	let root = temp.dir();
	let package_dir = format!("{root}/package");
	let game_dir = format!("{root}/game");
	write_solution_file(
		&root,
		"tsconfig.json",
		r#"{"files":[],"references":[{"path":"./package"},{"path":"./game"}]}"#,
	);
	write_native_flamework_solution_project(&package_dir, "@scope/solution-package");
	write_native_flamework_solution_project(&game_dir, "solution-game");
	link_flamework_fixture_dependencies(&package_dir);
	link_flamework_fixture_dependencies(&game_dir);
	write_solution_file(
		&package_dir,
		"src/index.ts",
		"/** @metadata reflect */ export class PackageService {}",
	);
	write_solution_file(
		&game_dir,
		"src/index.ts",
		&[
			r#"import { OnStart, Service } from "@flamework/core";"#,
			r#"import { PackageService } from "@scope/solution-package";"#,
			"@Service() export class GameService implements OnStart { constructor(value: PackageService) {} public onStart(): void {} }",
		]
		.join("\n"),
	);
	write_solution_file(
		&package_dir,
		"flamework.build",
		r#"{"version":1,"flameworkVersion":"1.3.2","identifierPrefix":"stale-prefix","identifiers":{}}"#,
	);
	let package_link = format!("{game_dir}/node_modules/@scope/solution-package");
	std::fs::create_dir_all(Path::new(&package_link).parent().expect("parent")).expect("mkdir");
	symlink(Path::new(&package_dir), Path::new(&package_link));

	// When: the actual solution build compiles both projects in one drain.
	let result =
		large_stack(|| build_solution_with_options(&format!("{root}/tsconfig.json"), &ProjectOptions::default()));
	// Then: package metadata is published before the game transform resolves its constructor ID.
	let result = result.unwrap_or_else(|failure| {
		panic!(
			"BuildSolutionWithOptions: {} ({:?})",
			failure.failure.error,
			failure.failure.messages()
		)
	});
	let package_build_data =
		String::from_utf8_lossy(&must_read_file(&format!("{package_dir}/flamework.build"))).into_owned();
	assert!(
		package_build_data.contains(r#""identifierPrefix": "@scope/solution-package""#),
		"package build data was not refreshed:\n{package_build_data}"
	);
	let game_output = result
		.outputs
		.values()
		.map(|output| String::from_utf8_lossy(output).into_owned())
		.find(|output| output.contains("GameService"))
		.unwrap_or_else(|| panic!("solution output missing GameService: {:?}", result.outputs.keys()));
	assert!(
		!game_output.contains("stale-prefix:") && game_output.contains("@scope/solution-package:"),
		"dependent output did not use newly published package identifier:\n{game_output}"
	);
}

/// `writeNativeFlameworkSolutionProject`
pub fn write_native_flamework_solution_project(dir: &str, package_name: &str) {
	write_solution_file(
		dir,
		"package.json",
		&format!(r#"{{"name":"{package_name}","version":"1.0.0","types":"out/index.d.ts"}}"#),
	);
	write_solution_file(dir, "rotor.toml", "[flamework]\n");
	let config = cross_project_compiler_options(true).replacen(r#""types":[],"#, "", 1);
	let config = config.replacen(
		r#""typeRoots":["node_modules/@rbxts"]"#,
		r#""typeRoots":["node_modules/@rbxts","node_modules/@flamework"]"#,
		1,
	);
	let mut config =
		config.replacen(r#","rootDir""#, r#","experimentalDecorators":true,"rootDir""#, 1) + r#","include":["src"]}"#;
	if package_name == "solution-game" {
		config = config.strip_suffix('}').unwrap_or(&config).to_owned() + r#","references":[{"path":"../package"}]}"#;
		write_solution_file(
			dir,
			"default.project.json",
			r#"{"name":"game","tree":{"$className":"DataModel","ReplicatedStorage":{"out":{"$path":"out"},"include":{"$path":"include","node_modules":{"$className":"Folder","@rbxts":{"$path":"node_modules/@rbxts"},"@flamework":{"$path":"node_modules/@flamework"},"@scope":{"$path":"node_modules/@scope"}}}}}}"#,
		);
	}
	write_solution_file(dir, "tsconfig.json", &config);
}

/// `linkFlameworkFixtureDependencies`
pub fn link_flamework_fixture_dependencies(project_dir: &str) {
	let dependencies = repo_root().join("testdata/transformers/project/node_modules");
	for scope in ["@flamework", "@rbxts"] {
		let target = dependencies.join(scope);
		assert!(
			target.exists(),
			"Flamework fixture dependencies not installed: {}",
			target.display()
		);
		let link = Path::new(project_dir).join("node_modules").join(scope);
		std::fs::create_dir_all(link.parent().expect("parent")).expect("mkdir");
		symlink(&target, &link);
	}
}

pub fn symlink(target: &Path, link: &Path) {
	#[cfg(unix)]
	std::os::unix::fs::symlink(target, link).expect("symlink");
	#[cfg(windows)]
	std::os::windows::fs::symlink_dir(target, link).expect("symlink");
}
