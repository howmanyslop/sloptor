//! `pnpm_test.go`: pnpm's symlinked layout, end to end. Unix symlinks only: the Windows
//! junction branch (`mklink /J`) is not ported.

use std::path::Path;

use super::{NO_LIB_GLOBAL_STUBS, Temp, expect_compiled, keys, large_stack, output};
use crate::project::compile_project;

const PNPM_TSCONFIG: &str = "{
	\"compilerOptions\": {
		\"allowSyntheticDefaultImports\": true,
		\"module\": \"CommonJS\",
		\"moduleResolution\": \"Node\",
		\"noLib\": true,
		\"moduleDetection\": \"force\",
		\"strict\": true,
		\"target\": \"ESNext\",
		\"types\": [],
		\"typeRoots\": [\"node_modules/@rbxts\"],
		\"rootDir\": \"src\",
		\"outDir\": \"out\"
	},
	\"include\": [\"src\"]
}";

const DUMMY_PKG: &str = "node_modules/.pnpm/@rbxts+dummy@1.0.0/node_modules/@rbxts/dummy";
const LINKED_PKG: &str = "node_modules/.pnpm/@rbxts+linked@1.0.0/node_modules/@rbxts/linked";

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) {
	if let Some(parent) = link.parent() {
		std::fs::create_dir_all(parent).expect("the link's parent");
	}
	std::os::unix::fs::symlink(target, link).expect("the symlink");
}

/// `writePnpmFixture`: a pnpm-shaped model project.
#[cfg(unix)]
pub fn write_pnpm_fixture() -> Temp {
	let temp = Temp::new();
	temp.write("package.json", r#"{"name":"pnpm-fixture"}"#);
	temp.write(
		"default.project.json",
		r#"{"name":"pnpm","tree":{"$path":"out","include":{"$path":"include"},"node_modules":{"$className":"Folder","@rbxts":{"$path":"node_modules/@rbxts"}}}}"#,
	);
	temp.write("tsconfig.json", PNPM_TSCONFIG);
	temp.write("src/globals.d.ts", NO_LIB_GLOBAL_STUBS);
	temp.write(
		"src/main.ts",
		"import { dummy } from \"@rbxts/dummy\";\nimport { linked } from \"@rbxts/linked\";\nprint(dummy(), linked());",
	);
	temp.write(
		&format!("{DUMMY_PKG}/package.json"),
		r#"{"name":"@rbxts/dummy","version":"1.0.0","main":"out/init.lua","types":"out/index.d.ts"}"#,
	);
	temp.write(
		&format!("{DUMMY_PKG}/out/index.d.ts"),
		"export declare function dummy(): number;",
	);
	temp.write(
		&format!("{DUMMY_PKG}/out/init.lua"),
		"return { dummy = function() return 1 end }",
	);
	temp.write(
		&format!("{LINKED_PKG}/package.json"),
		r#"{"name":"@rbxts/linked","version":"1.0.0","main":"src/init.lua","types":"src/index.d.ts"}"#,
	);
	temp.write(
		&format!("{LINKED_PKG}/default.project.json"),
		r#"{"name":"linked","tree":{"$path":"src"}}"#,
	);
	temp.write(
		&format!("{LINKED_PKG}/src/index.d.ts"),
		"export declare function linked(): number;",
	);
	temp.write(
		&format!("{LINKED_PKG}/src/init.lua"),
		"return { linked = function() return 2 end }",
	);
	for (name, real_pkg) in [("dummy", DUMMY_PKG), ("linked", LINKED_PKG)] {
		symlink(&temp.join(real_pkg), &temp.join(&format!("node_modules/@rbxts/{name}")));
	}
	temp
}

const PNPM_MAIN: &str = "-- Compiled with sloptor v3.0.0-beta.2\n\
	local TS = require(script.Parent.include.RuntimeLib)\n\
	local dummy = TS.import(script, script.Parent, \"node_modules\", \"@rbxts\", \"dummy\", \"out\").dummy\n\
	local linked = TS.import(script, script.Parent, \"node_modules\", \"@rbxts\", \"linked\").linked\n\
	print(dummy(), linked())\n\
	return nil\n";

#[test]
#[cfg(unix)]
fn compile_project_pnpm_symlinks() {
	let temp = write_pnpm_fixture();
	let files = expect_compiled(large_stack(|| compile_project(&temp.dir())));
	assert_eq!(output(&files, "out/main.luau"), PNPM_MAIN);
	assert_eq!(files.len(), 1, "produced files ({:?})", keys(&files));
}

#[test]
#[cfg(unix)]
fn compile_project_pnpm_symlinks_with_node_modules_rojo_path() {
	let temp = write_pnpm_fixture();
	temp.write(
		"default.project.json",
		r#"{"name":"pnpm","tree":{"$path":"out","include":{"$path":"include"},"node_modules":{"$path":"node_modules"}}}"#,
	);
	temp.write(
		"node_modules/default.project.json",
		r#"{"name":"node_modules","tree":{"@rbxts":{"dummy":{"$path":".pnpm/@rbxts+dummy@1.0.0/node_modules/@rbxts/dummy"},"linked":{"$path":".pnpm/@rbxts+linked@1.0.0/node_modules/@rbxts/linked"}}}}"#,
	);

	let files = expect_compiled(large_stack(|| compile_project(&temp.dir())));
	assert_eq!(output(&files, "out/main.luau"), PNPM_MAIN);
}

#[test]
#[cfg(unix)]
fn compile_project_excludes_workspace_dependency_source() {
	let workspace = Temp::new();
	let game = "e2e/default";
	let dependency = "packages/core";
	let game_files: [(&str, &str); 8] = [
		("package.json", r#"{"name":"flux-e2e"}"#),
		(
			"default.project.json",
			r#"{"name":"game","tree":{"$className":"DataModel","ReplicatedStorage":{"out":{"$path":"out"},"include":{"$path":"include"},"rbxts_include":{"$className":"Folder","node_modules":{"$className":"Folder","@flux":{"$path":"node_modules/@flux"}}}}}}"#,
		),
		(
			"tsconfig.json",
			"{
	\"compilerOptions\": {
		\"allowSyntheticDefaultImports\": true,
		\"module\": \"Preserve\",
		\"moduleResolution\": \"Bundler\",
		\"customConditions\": [\"source\"],
		\"noLib\": true,
		\"moduleDetection\": \"force\",
		\"strict\": true,
		\"target\": \"ESNext\",
		\"types\": [],
		\"typeRoots\": [\"node_modules/@flux\", \"node_modules/@rbxts\"],
		\"rootDir\": \"src\",
		\"outDir\": \"out\"
	},
	\"include\": [\"src\"]
}",
		),
		("src/globals.d.ts", NO_LIB_GLOBAL_STUBS),
		(
			"src/main.ts",
			"import { value } from \"@flux/core\";\nimport { first } from \"./first\";\nimport { second } from \"./second\";\nimport { third } from \"./third\";\nprint(value, first, second, third);",
		),
		("src/first.ts", "export const first = 1;\n"),
		("src/second.ts", "export const second = 2;\n"),
		("src/third.ts", "export const third = 3;\n"),
	];
	let dependency_files: [(&str, &str); 5] = [
		(
			"package.json",
			r#"{"name":"@flux/core","version":"1.0.0","main":"dist/init.luau","types":"dist/index.d.ts","exports":{".":{"source":"./src/index.ts","import":"./dist/init.luau","types":"./dist/index.d.ts"}}}"#,
		),
		("default.project.json", r#"{"name":"core","tree":{"$path":"dist"}}"#),
		("src/index.ts", "export const value = 42;\n"),
		("dist/index.d.ts", "export declare const value: number;\n"),
		("dist/init.luau", "return { value = 42 }\n"),
	];
	for (name, content) in game_files {
		workspace.write(&format!("{game}/{name}"), content);
	}
	for (name, content) in dependency_files {
		workspace.write(&format!("{dependency}/{name}"), content);
	}
	symlink(
		&workspace.join(dependency),
		&workspace.join(&format!("{game}/node_modules/@flux/core")),
	);

	let game_dir = workspace.join(game).to_string_lossy().into_owned();
	let files = expect_compiled(large_stack(|| compile_project(&game_dir)));
	assert_eq!(
		files.len(),
		4,
		"produced files, want the 4 game files only ({:?})",
		keys(&files)
	);
	let main = output(&files, "out/main.luau");
	assert!(
		main.contains(r#""rbxts_include", "node_modules", "@flux", "core""#),
		"workspace import did not target the package runtime:\n{main}"
	);
	for name in ["first", "second", "third"] {
		assert!(
			files.contains_key(&format!("out/{name}.luau")),
			"out/{name}.luau missing ({:?})",
			keys(&files)
		);
	}
}
