//! `sidecar_declaration_test.go` and `sidecar_roots_build_test.go`.

use std::sync::Arc;

use rotor_support::logservice;

use super::sidecar::{
	SessionsGuard, build, must_build, output_text, set_repo_sidecar_path, write_sidecar_plugin_fixture,
};
use super::write_project;
use crate::afterdeclarations::{take_after_declarations_scan, warn_unsupported_after_declarations};
use crate::project::ProjectOptions;
use crate::sidecar::close_sidecar_sessions;
use crate::timings::BuildTimings;

const DECLARATION_MARKER_PLUGIN: &str = r#"const ts = require("typescript");

module.exports = function () {
	return (context) => (sourceFile) => {
		const marker = context.factory.createVariableStatement(undefined, context.factory.createVariableDeclarationList([
			context.factory.createVariableDeclaration("__DECLARATION_MARKER__", undefined, undefined, context.factory.createStringLiteral("after-declarations")),
		], ts.NodeFlags.Const));
		return context.factory.updateSourceFile(sourceFile, sourceFile.statements.concat([marker]));
	};
};
"#;

const INSERT_STATEMENT_PLUGIN: &str = r#"const ts = require("typescript");

module.exports = function () {
	return (context) => (sourceFile) => {
		const marker = context.factory.createVariableStatement(undefined, context.factory.createVariableDeclarationList([
			context.factory.createVariableDeclaration("__INJECTED__", undefined, undefined, context.factory.createStringLiteral("transformer-was-here")),
		], ts.NodeFlags.Const));
		return context.factory.updateSourceFile(sourceFile, [marker].concat(sourceFile.statements));
	};
};
"#;

/// `sidecarDeclarationConfig`
pub fn sidecar_declaration_config(plugins: &str) -> String {
	format!(
		"{{\n\t\"compilerOptions\": {{\n\t\t\"allowSyntheticDefaultImports\": true,\n\t\t\"declaration\": true,\n\t\t\"module\": \"CommonJS\",\n\t\t\"moduleResolution\": \"Node\",\n\t\t\"noLib\": true,\n\t\t\"moduleDetection\": \"force\",\n\t\t\"strict\": true,\n\t\t\"target\": \"ESNext\",\n\t\t\"types\": [],\n\t\t\"typeRoots\": [\"node_modules/@rbxts\"],\n\t\t\"rootDir\": \"src\",\n\t\t\"outDir\": \"out\",\n\t\t\"plugins\": {plugins}\n\t}},\n\t\"include\": [\"src\"]\n}}"
	)
}

/// afterDeclarations transformers have nowhere to run now that declarations are emitted
/// natively, so the build must say so out loud rather than dropping the transform silently.
#[test]
fn after_declarations_transformer_warns() {
	set_repo_sidecar_path();
	let _sessions = SessionsGuard::new();
	let temp = write_project("@scope/after-declarations-fixture", "");
	let dir = temp.dir();
	write_sidecar_plugin_fixture(
		&dir,
		"",
		&sidecar_declaration_config(
			r#"[{ "transform": "./plugins/declaration-marker.js", "afterDeclarations": true }]"#,
		),
	);
	temp.write("plugins/declaration-marker.js", DECLARATION_MARKER_PLUGIN);
	logservice::capture();
	let result = must_build(&dir, &ProjectOptions::default(), "build");
	let log = logservice::take_capture();
	assert!(
		log.contains("afterDeclarations transformers are not supported"),
		"no afterDeclarations warning in compiler log:\n{log}"
	);
	let declaration = std::fs::read_to_string(temp.join("out/main.d.ts")).expect("the declaration");
	assert!(
		!declaration.contains("__DECLARATION_MARKER__"),
		"afterDeclarations transformer ran:\n{declaration}"
	);
	let luau = output_text(&result, "out/main.luau");
	assert!(
		!luau.contains("__DECLARATION_MARKER__"),
		"declaration marker leaked into Luau:\n{luau}"
	);
}

#[test]
fn transformer_parent_fix() {
	set_repo_sidecar_path();
	let _sessions = SessionsGuard::new();
	let temp = write_project("@scope/parent-fix-fixture", "");
	let dir = temp.dir();
	let plugin = r#"const ts = require("typescript");
function inject(context) {
	return (sourceFile) => ts.visitNode(sourceFile, function visit(node) {
		if (ts.isStringLiteral(node) && node.text === "start") return ts.factory.createStringLiteral("synthetic");
		return ts.visitEachChild(node, visit, context);
	});
}
function requireParent(context) {
	return (sourceFile) => ts.visitNode(sourceFile, function visit(node) {
		if (ts.isStringLiteral(node) && node.text === "synthetic" && !ts.isVariableDeclaration(node.parent)) throw new Error("missing synthetic parent");
		return ts.visitEachChild(node, visit, context);
	});
}
module.exports = function () { return { before: inject, after: requireParent }; };"#;
	write_sidecar_plugin_fixture(
		&dir,
		"",
		&sidecar_declaration_config(r#"[{ "transform": "./plugins/parent-fix.js" }]"#),
	);
	temp.write("plugins/parent-fix.js", plugin);
	temp.write("src/main.ts", "export const value = \"start\";\n");
	let result = must_build(&dir, &ProjectOptions::default(), "build");
	let luau = output_text(&result, "out/main.luau");
	assert!(luau.contains(r#""synthetic""#), "parent-fix transform missing:\n{luau}");
}

#[test]
fn declaration_path_rewrite() {
	set_repo_sidecar_path();
	let _sessions = SessionsGuard::new();
	let temp = write_project("@scope/declaration-path-fixture", "");
	let dir = temp.dir();
	let config = sidecar_declaration_config("[]").replacen(
		r#""outDir": "out","#,
		r#""outDir": "out", "baseUrl": ".", "paths": { "@alias/*": ["src/*"] },"#,
		1,
	);
	write_sidecar_plugin_fixture(&dir, "", &config);
	temp.write("src/value.ts", "export interface Value { label: string; }\n");
	temp.write(
		"src/main.ts",
		"import type { Value } from \"@alias/value\";\nexport type Output = Value;\n",
	);
	let (_, diags, error) = build(&dir, &ProjectOptions::default());
	assert!(error.is_none(), "build: {error:?} (diags: {diags:?})");
	let declaration = std::fs::read_to_string(temp.join("out/main.d.ts")).expect("the declaration");
	assert!(
		declaration.contains(r#"from "./value""#),
		"declaration alias was not rewritten:\n{declaration}"
	);
}

#[test]
fn transformer_source_map_original_content() {
	set_repo_sidecar_path();
	let _sessions = SessionsGuard::new();
	let temp = write_project("@scope/transformer-source-map-fixture", "");
	let dir = temp.dir();
	let config = sidecar_declaration_config(r#"[{ "transform": "./plugins/insert.js" }]"#).replacen(
		r#""declaration": true,"#,
		r#""declaration": true, "sourceMap": true,"#,
		1,
	);
	write_sidecar_plugin_fixture(&dir, "", &config);
	temp.write("plugins/insert.js", INSERT_STATEMENT_PLUGIN);
	let original = "export const value = \"start\";\n";
	temp.write("src/main.ts", original);
	let (_, diags, error) = build(&dir, &ProjectOptions::default());
	assert!(error.is_none(), "build: {error:?} (diags: {diags:?})");
	let map: serde_json::Value =
		serde_json::from_slice(&std::fs::read(temp.join("out/main.luau.map")).expect("the map")).expect("JSON");
	let strings = |key: &str| -> Vec<String> {
		map[key]
			.as_array()
			.map(|values| {
				values
					.iter()
					.filter_map(|value| value.as_str().map(str::to_owned))
					.collect()
			})
			.unwrap_or_default()
	};
	assert_eq!(
		strings("sourcesContent"),
		[original],
		"sourcesContent, want original source"
	);
	assert_eq!(map["file"], "main.luau", "file");
	assert_eq!(strings("sources"), ["../src/main.ts"], "sources");
	assert!(
		map["mappings"].as_str().is_some_and(|mappings| !mappings.is_empty()),
		"transformed source map has no mappings"
	);
}

/// The tsconfig-chain scan is claimed once per project. Each nextest test runs in its own
/// process, so the once-per-process sentinels start clear.
#[test]
fn after_declarations_scan_is_claimed_once() {
	const CONFIG_PATH: &str = "C:/project/tsconfig.json";
	assert!(
		take_after_declarations_scan(CONFIG_PATH),
		"the first caller must own the scan"
	);
	for attempt in 0..3 {
		assert!(
			!take_after_declarations_scan(CONFIG_PATH),
			"attempt {attempt} re-ran the scan"
		);
	}
	assert!(
		take_after_declarations_scan("C:/other/tsconfig.json"),
		"a different project must get its own scan"
	);
}

/// Once the warning is out there is nothing left for the scan to discover.
#[test]
fn after_declarations_scan_is_skipped_after_the_warning() {
	const CONFIG_PATH: &str = "C:/project/tsconfig.json";
	logservice::capture();
	warn_unsupported_after_declarations(CONFIG_PATH, 1);
	let log = logservice::take_capture();
	assert!(
		log.contains("afterDeclarations transformers are not supported"),
		"no warning in the compiler log:\n{log}"
	);
	assert!(
		!take_after_declarations_scan(CONFIG_PATH),
		"the scan ran after the warning was already out"
	);
	// And the warning itself is one line per project, not one per round trip.
	logservice::capture();
	warn_unsupported_after_declarations(CONFIG_PATH, 2);
	let repeated = logservice::take_capture();
	assert!(repeated.is_empty(), "the warning repeated:\n{log}{repeated}");
}

/// `ambientProbePlugin`: inlines what the checker thinks `ambient` is typed as.
const AMBIENT_PROBE_PLUGIN: &str = r#"const ts = require("typescript");

module.exports = function ambientProbe(program) {
	const checker = program.getTypeChecker();
	return (context) => (sourceFile) => {
		const answerFor = (file) => {
			for (const statement of file.statements) {
				if (!ts.isVariableStatement(statement)) {
					continue;
				}
				for (const declaration of statement.declarationList.declarations) {
					if (ts.isIdentifier(declaration.name) && declaration.name.text === "ambient") {
						// The members, not the type NAME: TypeScript keeps the
						// written name for an unresolved type reference, so
						// typeToString would answer the same either way.
						const members = checker.getPropertiesOfType(checker.getTypeAtLocation(declaration.name));
						return members.length === 0 ? "unresolved" : members.map((symbol) => symbol.name).sort().join("+");
					}
				}
			}
			return "no-declaration";
		};
		const visit = (node) => {
			if (ts.isStringLiteral(node) && node.text === "AMBIENT_TYPE") {
				return ts.factory.createStringLiteral(answerFor(sourceFile));
			}
			return ts.visitEachChild(node, visit, context);
		};
		return ts.visitNode(sourceFile, visit);
	};
};
"#;

/// A `declare global` in a module nothing imports only reaches the worker's checker if the
/// narrowed root set keeps that module.
#[test]
fn narrowed_sidecar_roots_preserve_global_augmentations_for_transformers() {
	set_repo_sidecar_path();
	let _sessions = SessionsGuard::new();
	let temp = write_project("@scope/narrow-roots-ambient", "");
	let dir = temp.dir();
	write_sidecar_plugin_fixture(
		&dir,
		"",
		&sidecar_declaration_config(r#"[{"transform":"./plugins/ambient-probe.js"}]"#),
	);
	// `enableDeclarationIncrementalBuilds`
	let config = std::fs::read_to_string(temp.join("tsconfig.json")).expect("read").replacen(
		r#""outDir": "out""#,
		"\"outDir\": \"out\",\n\t\t\"declaration\": true,\n\t\t\"incremental\": true,\n\t\t\"tsBuildInfoFile\": \"out/cache.tsbuildinfo\"",
		1,
	);
	temp.write("tsconfig.json", config);
	temp.write("plugins/ambient-probe.js", AMBIENT_PROBE_PLUGIN);
	for (rel, text) in [
		// Imported by nobody, yet its `declare global` types `ambient` below.
		(
			"src/augment.ts",
			"export {};\ndeclare global {\n\tinterface RotorAmbient {\n\t\tname: string;\n\t}\n}\n",
		),
		(
			"src/main.ts",
			"declare const ambient: RotorAmbient;\nexport const probe = \"AMBIENT_TYPE\";\nexport const named = ambient.name;\n",
		),
		("src/filler.ts", "export const filler = 1;\n"),
		("src/filler2.ts", "export const filler2 = 2;\n"),
	] {
		temp.write(rel, text);
	}
	let probe = |stage: &str| -> String {
		let timings = Arc::new(BuildTimings::new());
		let opts = ProjectOptions {
			timings: Some(Arc::clone(&timings)),
			..ProjectOptions::default()
		};
		let (_, diags, error) = build(&dir, &opts);
		assert!(error.is_none(), "{stage} build: {error:?} (diags: {diags:?})");
		let out =
			std::fs::read_to_string(temp.join("out/main.luau")).unwrap_or_else(|error| panic!("{stage}: {error}"));
		out.lines()
			.map(str::trim)
			.find(|line| line.starts_with("local probe ="))
			.unwrap_or_else(|| panic!("{stage} build emitted no probe line:\n{out}"))
			.to_owned()
	};
	let full = probe("full");
	assert_eq!(full, r#"local probe = "name""#, "full build resolved the ambient type");
	// A warm worker keeps whatever root set the full build left it, so the narrowed request
	// only reaches a session that did not serve that full build.
	close_sidecar_sessions();
	// A comment-only edit selects main.ts alone, which is what narrows the worker's root set.
	temp.write(
		"src/main.ts",
		"declare const ambient: RotorAmbient;\nexport const probe = \"AMBIENT_TYPE\";\nexport const named = ambient.name;\n// edited\n",
	);
	let incremental = probe("incremental");
	assert_eq!(
		incremental, full,
		"the narrowed rebuild changed the transformer's checker answer"
	);
}
