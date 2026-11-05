//! `packageexports_compile_test.go`

use super::packageexports::symlink_dir;
use super::{NO_LIB_GLOBAL_STUBS, Temp, expect_compiled, large_stack, output};
use crate::project::compile_project;

const EXPORT_BRANCH_TSCONFIG: &str = "{
	\"compilerOptions\": {
		\"allowSyntheticDefaultImports\": true,
		\"module\": \"Preserve\",
		\"moduleResolution\": \"Bundler\",
		\"customConditions\": [\"testing\"],
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
}";

/// A workspace package's test-only modules ship behind a custom condition, declarations and Luau
/// in different directories; the import targets the Luau the export branch names.
#[test]
fn compile_project_export_condition_branch_runtime() {
	let workspace = Temp::new();
	workspace.write("game/package.json", r#"{"name":"game"}"#);
	workspace.write(
		"game/default.project.json",
		r#"{"name":"game","tree":{"$className":"DataModel","ReplicatedStorage":{"out":{"$path":"out"},"include":{"$path":"include"},"rbxts_include":{"$className":"Folder","node_modules":{"$className":"Folder","@flux":{"$path":"node_modules/@flux"}}}}}}"#,
	);
	workspace.write("game/tsconfig.json", EXPORT_BRANCH_TSCONFIG);
	workspace.write("game/src/globals.d.ts", NO_LIB_GLOBAL_STUBS);
	workspace.write(
		"game/src/main.ts",
		"import { helper } from \"@flux/core/testing\";\nimport { styles } from \"@flux/core/test/helpers/styles\";\nprint(helper, styles);",
	);
	workspace.write(
		"packages/core/package.json",
		r#"{"name":"@flux/core","version":"1.0.0","exports":{
			"./testing":{"testing":{"source":"./test/helpers/index.ts","types":"./out-tsc/spec/test/helpers/index.d.ts","default":"./out-test/test/helpers/index.luau"}},
			"./test/*":{"testing":{"types":"./out-tsc/spec/test/*.d.ts","default":"./out-test/test/*.luau"}}
		}}"#,
	);
	workspace.write(
		"packages/core/out-tsc/spec/test/helpers/index.d.ts",
		"export declare const helper: number;\n",
	);
	workspace.write(
		"packages/core/out-tsc/spec/test/helpers/styles.d.ts",
		"export declare const styles: number;\n",
	);
	workspace.write(
		"packages/core/out-test/test/helpers/index.luau",
		"return { helper = 1 }\n",
	);
	workspace.write(
		"packages/core/out-test/test/helpers/styles.luau",
		"return { styles = 2 }\n",
	);

	let link = workspace.join("game/node_modules/@flux/core");
	std::fs::create_dir_all(link.parent().expect("a parent")).expect("the scope directory");
	symlink_dir(
		&workspace.join("packages/core").to_string_lossy(),
		&link.to_string_lossy(),
	)
	.expect("the package link");

	let game = workspace.join("game").to_string_lossy().into_owned();
	let files = expect_compiled(large_stack(|| compile_project(&game)));
	let main = output(&files, "out/main.luau");
	for want in [
		r#""@flux", "core", "out-test", "test", "helpers", "index").helper"#,
		r#""@flux", "core", "out-test", "test", "helpers", "styles").styles"#,
	] {
		assert!(main.contains(want), "out/main.luau missing {want}:\n{main}");
	}
}
