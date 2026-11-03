//! `sidecar_transformer_test.go`: plugin ordering, `extends` inheritance and hooks.

use super::sidecar::{SessionsGuard, must_build, output_text, set_repo_sidecar_path, write_sidecar_plugin_fixture};
use super::write_project;
use crate::output::BuildResult;
use crate::project::ProjectOptions;
use crate::sidecar::sidecar_slot_count;

const ORDERING_TRANSFORMER_PLUGIN: &str = r#"const ts = require("typescript");

function prefixTransformer(prefix) {
	return (context) => {
		const visit = (node) => {
			if (ts.isStringLiteral(node)) {
				return ts.factory.createStringLiteral(prefix + ":" + node.text);
			}
			return ts.visitEachChild(node, visit, context);
		};
		return (sourceFile) => ts.visitNode(sourceFile, visit);
	};
}

module.exports = function (program, config, helpers) {
	if (!program.getTypeChecker() || helpers.ts !== ts) {
		throw new Error("missing program transformer inputs");
	}
	return prefixTransformer(config.prefix);
};

module.exports.named = function (config) {
	return prefixTransformer(config.prefix);
};
"#;

const HOOK_TRANSFORMER_PLUGIN: &str = r#"const ts = require("typescript");

function prefixTransformer(prefix) {
	return (context) => {
		const visit = (node) => {
			if (ts.isStringLiteral(node)) {
				return ts.factory.createStringLiteral(prefix + ":" + node.text);
			}
			return ts.visitEachChild(node, visit, context);
		};
		return (sourceFile) => ts.visitNode(sourceFile, visit);
	};
}

module.exports = function () {
	return {
		before: prefixTransformer("before"),
		after: prefixTransformer("after"),
		afterDeclarations: prefixTransformer("afterDeclarations"),
	};
};

module.exports.shouldTransformSourceFile = function (sourceFile, program, config) {
	if (!program.getTypeChecker() || config.transform !== "./plugins/hooks.js") {
		throw new Error("missing source-file hook inputs");
	}
	return sourceFile.fileName.endsWith("main.ts");
};
"#;

/// The base config every ordering fixture extends: one `inherited` plugin.
const INHERITED_BASE: &str = r#"{
	"compilerOptions": {
		"plugins": [{
			"transform": "./plugins/ordering.js",
			"import": "named",
			"type": "config",
			"prefix": "inherited"
		}]
	}
}"#;

/// Builds a fixture over the ordering plugin with `base` and `root` configs.
fn build_ordering(pkg_name: &str, base: &str, root: &str) -> BuildResult {
	set_repo_sidecar_path();
	let _sessions = SessionsGuard::new();
	let temp = write_project(pkg_name, "");
	let dir = temp.dir();
	write_sidecar_plugin_fixture(&dir, base, root);
	temp.write("plugins/ordering.js", ORDERING_TRANSFORMER_PLUGIN);
	temp.write("src/main.ts", "export const phase = \"start\";\n");
	must_build(&dir, &ProjectOptions::default(), "BuildProjectWithOptions")
}

#[test]
fn transformer_ordering() {
	// The base declares a plugin the child's list leaves out: the child's `plugins` replaces
	// it, so "inherited" must never reach the output.
	let result = build_ordering(
		"@scope/plugin-ordering-fixture",
		INHERITED_BASE,
		r#"{
	"extends": "./tsconfig.base.json",
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"module": "CommonJS",
		"moduleResolution": "Node",
		"noLib": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"types": [],
		"typeRoots": ["node_modules/@rbxts"],
		"rootDir": "src",
		"outDir": "out",
		"plugins": [
			{ "transform": "./plugins/ordering.js", "prefix": "child" },
			{ "transform": "./plugins/ordering.js", "import": "named", "type": "config", "prefix": "parent" },
			{ "transform": "./plugins/ordering.js", "prefix": "after", "after": true }
		]
	},
	"include": ["src"]
}"#,
	);
	let got = output_text(&result, "out/main.luau");
	assert!(
		got.contains(r#"local phase = "after:parent:child:start""#),
		"out/main.luau transformer order =\n{got}"
	);
	assert!(
		!got.contains("inherited:"),
		"out/main.luau ran a plugin the child's `plugins` replaced =\n{got}"
	);
}

/// A child that declares `plugins` replaces the inherited list; `"plugins": []` opts out.
#[test]
fn transformer_plugins_child_override() {
	let result = build_ordering(
		"@scope/plugin-override-fixture",
		INHERITED_BASE,
		r#"{
	"extends": "./tsconfig.base.json",
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"module": "CommonJS",
		"moduleResolution": "Node",
		"noLib": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"types": [],
		"typeRoots": ["node_modules/@rbxts"],
		"rootDir": "src",
		"outDir": "out",
		"plugins": []
	},
	"include": ["src"]
}"#,
	);
	let got = output_text(&result, "out/main.luau");
	assert!(
		got.contains(r#"local phase = "start""#),
		"out/main.luau ran an inherited transformer the child opted out of =\n{got}"
	);
	// The resolved list is empty, so the compile must not pay for a Node worker.
	assert_eq!(
		sidecar_slot_count(),
		0,
		"spawned sidecar session(s) for a project with no resolved plugins"
	);
}

/// A child that declares no `plugins` of its own still inherits the parent's list.
#[test]
fn transformer_plugins_inherited() {
	let result = build_ordering(
		"@scope/plugin-inherit-fixture",
		INHERITED_BASE,
		r#"{
	"extends": "./tsconfig.base.json",
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"module": "CommonJS",
		"moduleResolution": "Node",
		"noLib": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"types": [],
		"typeRoots": ["node_modules/@rbxts"],
		"rootDir": "src",
		"outDir": "out"
	},
	"include": ["src"]
}"#,
	);
	let got = output_text(&result, "out/main.luau");
	assert!(
		got.contains(r#"local phase = "inherited:start""#),
		"out/main.luau dropped an inherited transformer =\n{got}"
	);
}

#[test]
fn transformer_hooks() {
	set_repo_sidecar_path();
	let _sessions = SessionsGuard::new();
	let temp = write_project("@scope/plugin-hooks-fixture", "");
	let dir = temp.dir();
	write_sidecar_plugin_fixture(
		&dir,
		"",
		r#"{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"module": "CommonJS",
		"moduleResolution": "Node",
		"noLib": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"types": [],
		"typeRoots": ["node_modules/@rbxts"],
		"rootDir": "src",
		"outDir": "out",
		"plugins": [
			{ "transform": "./plugins/hooks.js" },
			{ "transform": "./plugins/no-hooks.js" }
		]
	},
	"include": ["src"]
}"#,
	);
	temp.write("plugins/hooks.js", HOOK_TRANSFORMER_PLUGIN);
	temp.write(
		"plugins/no-hooks.js",
		"module.exports = function () { return {}; };\nmodule.exports.shouldTransformSourceFile = true;\n",
	);
	temp.write("src/main.ts", "export const selected = \"selected\";\n");
	temp.write("src/skipped.ts", "export const skipped = \"skipped\";\n");
	let result = must_build(&dir, &ProjectOptions::default(), "BuildProjectWithOptions");
	let got = output_text(&result, "out/main.luau");
	assert!(
		got.contains(r#"local selected = "after:before:selected""#) && !got.contains("afterDeclarations"),
		"out/main.luau hook output =\n{got}"
	);
	let got = output_text(&result, "out/skipped.luau");
	assert!(
		got.contains(r#"local skipped = "skipped""#) && !got.contains("before:"),
		"out/skipped.luau ignored hook output =\n{got}"
	);
}
