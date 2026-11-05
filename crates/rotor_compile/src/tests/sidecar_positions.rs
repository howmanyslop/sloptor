//! `sidecar_positions_test.go`: a diagnostic on a transformed file points at the file on disk.

use std::collections::HashMap;

use super::sidecar::{SessionsGuard, build, set_repo_sidecar_path};
use super::{Temp, large_stack, write_project};
use crate::census::compile_project_diagnostics;
use crate::compile::DiagnosticInfo;
use crate::project::ProjectOptions;

/// `hoistingPositionPlugin`: returns a new `SourceFile` with a declaration hoisted above the
/// existing statements, so the reprint no longer matches the bytes on disk.
const HOISTING_POSITION_PLUGIN: &str = r#"const ts = require("typescript");

module.exports = function () {
	return (context) => (sourceFile) =>
		ts.factory.updateSourceFile(sourceFile, [
			ts.factory.createVariableStatement(
				undefined,
				ts.factory.createVariableDeclarationList(
					[
						ts.factory.createVariableDeclaration(
							"__hoisted_by_the_transformer",
							undefined,
							undefined,
							ts.factory.createNumericLiteral("1"),
						),
					],
					ts.NodeFlags.Const,
				),
			),
			...sourceFile.statements,
		]);
};
"#;

/// `positionFixtureSource`: the unresolved name sits on line 12, column 2.
pub const POSITION_FIXTURE_SOURCE: &str = concat!(
	"export const SEED = 1;\n",
	"\n",
	"// The import that would declare the call below is deliberately absent.\n",
	"\n",
	"/**\n",
	" * Doc comment padding.\n",
	" *\n",
	" * @returns Nothing.\n",
	" */\n",
	"export function enclosingFunction(): void {\n",
	"\tconst first = SEED;\n",
	"\tuseMountEffect(first);\n",
	"\n",
	"\tconst second = first;\n",
	"\n",
	"\tfunction nestedHelper(): void {\n",
	"\t\tconst third = second;\n",
	"\t\tprint(third);\n",
	"\t}\n",
	"\n",
	"\tnestedHelper();\n",
	"}\n",
);

const POSITION_FIXTURE_TS_CONFIG: &str = r#"{
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
		"outDir": "out",%s
		"plugins": [{ "transform": "./plugins/hoist.js" }]
	},
	"include": ["src"]
}"#;

const PLAIN_TS_CONFIG: &str = r#"{
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
}"#;

/// `writePositionFixture`
fn write_position_fixture(with_plugin: bool, source_map: bool) -> Temp {
	let temp = write_project("@scope/pos-repro-fixture", "");
	let source_map_option = if source_map { "\n\t\t\"sourceMap\": true," } else { "" };
	let tsconfig = if with_plugin {
		POSITION_FIXTURE_TS_CONFIG.replacen("%s", source_map_option, 1)
	} else {
		PLAIN_TS_CONFIG.to_owned()
	};
	temp.write("tsconfig.json", tsconfig);
	temp.write("plugins/hoist.js", HOISTING_POSITION_PLUGIN);
	temp.write("src/main.ts", POSITION_FIXTURE_SOURCE);
	temp
}

/// The census's diagnostics, files first.
fn census_diagnostics(dir: &str) -> Vec<DiagnosticInfo> {
	let (census, error) = large_stack(|| compile_project_diagnostics(dir, &ProjectOptions::default()));
	if let Some(error) = error {
		panic!("CompileProjectDiagnostics: {error} ({:?})", census.diagnostics);
	}
	census
		.files
		.into_iter()
		.flat_map(|file| file.diagnostics)
		.chain(census.diagnostics)
		.collect()
}

/// `positionOfUnresolvedName`
fn position_of_unresolved_name(with_plugin: bool, source_map: bool) -> (Temp, DiagnosticInfo) {
	set_repo_sidecar_path();
	let _sessions = SessionsGuard::new();
	let temp = write_position_fixture(with_plugin, source_map);
	let diagnostic = census_diagnostics(&temp.dir())
		.into_iter()
		.find(|diagnostic| diagnostic.code == "TS2304")
		.expect("no TS2304 diagnostic in census");
	(temp, diagnostic)
}

/// `assertAuthoredPosition`: the offset indexes the file on disk and lands on `authored`.
fn assert_authored_position(d: &DiagnosticInfo, line: usize, col: usize, authored: &str) {
	assert!(
		d.line == line && d.col == col,
		"line/col = {}/{}, want {line}/{col} ({authored:?} in main.ts on disk)",
		d.line,
		d.col
	);
	let disk =
		std::fs::read(&d.file_name).unwrap_or_else(|error| panic!("read the file the diagnostic names: {error}"));
	assert!(
		d.offset + authored.len() <= disk.len(),
		"offset {} does not index the {}-byte file on disk",
		d.offset,
		disk.len()
	);
	assert_eq!(
		String::from_utf8_lossy(&disk[d.offset..d.offset + authored.len()]),
		authored,
		"disk at offset {}",
		d.offset
	);
}

/// `assertLocatesUseMountEffect`
pub fn assert_locates_use_mount_effect(d: &DiagnosticInfo) {
	assert_authored_position(d, 12, 2, "useMountEffect");
	assert_eq!(d.len, "useMountEffect".len(), "len");
}

#[test]
fn diagnostic_position_without_transformer() {
	assert_locates_use_mount_effect(&position_of_unresolved_name(false, true).1);
}

#[test]
fn diagnostic_position_survives_a_transformer_reprint() {
	assert_locates_use_mount_effect(&position_of_unresolved_name(true, true).1);
}

/// The trace map is what puts a position back on disk, so the sidecar has to produce one for
/// its own sake, not the emit's.
#[test]
fn diagnostic_position_survives_a_transformer_without_source_maps() {
	assert_locates_use_mount_effect(&position_of_unresolved_name(true, false).1);
}

/// `applierShapedPositionPlugin`: the three node shapes a React-compiler-style transform
/// produces, inside one function body.
const APPLIER_SHAPED_POSITION_PLUGIN: &str = r#"const ts = require("typescript");

function findDeclaration(statements, name) {
	return statements.find(
		(statement) =>
			ts.isVariableStatement(statement) &&
			statement.declarationList.declarations.length === 1 &&
			ts.isIdentifier(statement.declarationList.declarations[0].name) &&
			statement.declarationList.declarations[0].name.text === name,
	);
}

function memoCacheSlot() {
	return ts.factory.createElementAccessExpression(
		ts.factory.createIdentifier("memoCache"),
		ts.factory.createNumericLiteral("0"),
	);
}

module.exports = function () {
	return (context) => (sourceFile) => {
		const enclosing = sourceFile.statements.find(
			(statement) =>
				ts.isFunctionDeclaration(statement) &&
				statement.name !== undefined &&
				statement.name.text === "enclosingFunction",
		);
		if (enclosing === undefined || enclosing.body === undefined) {
			throw new Error("fixture no longer declares enclosingFunction");
		}

		const dependency = findDeclaration(enclosing.body.statements, "dependency");
		const target = findDeclaration(enclosing.body.statements, "memoized");
		if (dependency === undefined || target === undefined) {
			throw new Error("fixture no longer has the statements the plugin splices around");
		}

		// No authored counterpart: the allocator is bookkeeping this transform
		// invented. The STATEMENT carries the authored range of what it
		// memoizes; the identifier inside it carries none.
		const cacheAllocation = ts.factory.createVariableStatement(
			undefined,
			ts.factory.createVariableDeclarationList(
				[
					ts.factory.createVariableDeclaration(
						"memoCache",
						undefined,
						undefined,
						ts.factory.createCallExpression(
							ts.factory.createIdentifier("bookkeepingMissingAlloc"),
							undefined,
							[ts.factory.createNumericLiteral("2")],
						),
					),
				],
				ts.NodeFlags.Const,
			),
		);
		ts.setSourceMapRange(cacheAllocation, ts.getSourceMapRange(target));

		// Synthesized, but standing in for an authored identifier — so it is
		// handed that identifier's range.
		const guardOperand = ts.factory.createIdentifier("synthesizedMissingGuard");
		ts.setSourceMapRange(
			guardOperand,
			ts.getSourceMapRange(dependency.declarationList.declarations[0].name),
		);

		const guard = ts.factory.createIfStatement(
			ts.factory.createBinaryExpression(
				memoCacheSlot(),
				ts.SyntaxKind.ExclamationEqualsEqualsToken,
				guardOperand,
			),
			ts.factory.createBlock(
				[
					ts.factory.createExpressionStatement(
						ts.factory.createAssignment(memoCacheSlot(), ts.factory.createNumericLiteral("1")),
					),
				],
				true,
			),
		);

		const body = [];
		for (const statement of enclosing.body.statements) {
			if (statement === target) {
				body.push(cacheAllocation, guard);
			}
			body.push(statement);
		}

		return ts.factory.updateSourceFile(
			sourceFile,
			sourceFile.statements.map((statement) =>
				statement === enclosing
					? ts.factory.updateFunctionDeclaration(
							enclosing,
							enclosing.modifiers,
							enclosing.asteriskToken,
							enclosing.name,
							enclosing.typeParameters,
							enclosing.parameters,
							enclosing.type,
							ts.factory.updateBlock(enclosing.body, body),
						)
					: statement,
			),
		);
	};
};
"#;

/// `applierPositionFixtureSource`
const APPLIER_POSITION_FIXTURE_SOURCE: &str = concat!(
	"export const SEED = 1;\n",
	"\n",
	"// The imports that would declare the missing names below are deliberately absent.\n",
	"\n",
	"/**\n",
	" * Doc comment padding.\n",
	" *\n",
	" * @returns Nothing.\n",
	" */\n",
	"export function enclosingFunction(): void {\n",
	"\tconst dependency = SEED;\n",
	"\tconst memoized = authoredMissingCall(dependency);\n",
	"\n",
	"\tprint(memoized);\n",
	"}\n",
);

const APPLIER_POSITION_FIXTURE_TS_CONFIG: &str = r#"{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"module": "CommonJS",
		"moduleResolution": "Node",
		"noLib": true,
		"moduleDetection": "force",
		"sourceMap": true,
		"strict": true,
		"target": "ESNext",
		"types": [],
		"typeRoots": ["node_modules/@rbxts"],
		"rootDir": "src",
		"outDir": "out",
		"plugins": [{ "transform": "./plugins/applier.js" }]
	},
	"include": ["src"]
}"#;

/// `unresolvedName`: the identifier in "Cannot find name 'x'.".
fn unresolved_name(message: &str) -> String {
	message
		.split_once('\'')
		.and_then(|(_, rest)| rest.split_once('\''))
		.map(|(name, _)| name.to_owned())
		.unwrap_or_default()
}

/// `unresolvedNamePositions`
fn unresolved_name_positions() -> (Temp, HashMap<String, DiagnosticInfo>) {
	set_repo_sidecar_path();
	let _sessions = SessionsGuard::new();
	let temp = write_project("@scope/applier-pos-fixture", "");
	temp.write("tsconfig.json", APPLIER_POSITION_FIXTURE_TS_CONFIG);
	temp.write("plugins/applier.js", APPLIER_SHAPED_POSITION_PLUGIN);
	temp.write("src/main.ts", APPLIER_POSITION_FIXTURE_SOURCE);
	let (census, error) = large_stack(|| compile_project_diagnostics(&temp.dir(), &ProjectOptions::default()));
	if let Some(error) = error {
		panic!("CompileProjectDiagnostics: {error} ({:?})", census.diagnostics);
	}
	let by_name = census
		.files
		.into_iter()
		.flat_map(|file| file.diagnostics)
		.filter(|diagnostic| diagnostic.code == "TS2304")
		.map(|diagnostic| (unresolved_name(&diagnostic.message), diagnostic))
		.collect();
	(temp, by_name)
}

/// The round trip for the node shapes a React-compiler-style transform actually emits.
#[test]
fn diagnostic_position_survives_an_applier_shaped_transform() {
	let (_temp, by_name) = unresolved_name_positions();
	for name in [
		"authoredMissingCall",
		"synthesizedMissingGuard",
		"bookkeepingMissingAlloc",
	] {
		assert!(
			by_name.contains_key(name),
			"no TS2304 for {name}; the plugin did not splice what the test assumes (got {by_name:?})"
		);
	}
	// Authored, and pushed forward by the statements spliced above it.
	let d = &by_name["authoredMissingCall"];
	assert_authored_position(d, 12, 19, "authoredMissingCall");
	assert_eq!(d.len, "authoredMissingCall".len(), "len");
	// Synthesized, carrying the authored range of the identifier it stands in for.
	assert_authored_position(&by_name["synthesizedMissingGuard"], 11, 8, "dependency");
	// No authored counterpart at all: it resolves coarsely to the enclosing statement.
	assert_authored_position(&by_name["bookkeepingMissingAlloc"], 12, 2, "const memoized");
}

/// The BUILD path rather than the census path: each builds its own project context.
#[test]
fn build_diagnostic_position_survives_a_transformer_reprint() {
	set_repo_sidecar_path();
	let _sessions = SessionsGuard::new();
	let temp = write_position_fixture(true, true);
	let (result, _, error) = build(&temp.dir(), &ProjectOptions::default());
	assert!(error.is_some(), "build of a project with an unresolved name succeeded");
	let result = result.expect("failed build carries no BuildResult, so no diagnostic to locate");
	let d = result
		.diagnostics
		.iter()
		.find(|diagnostic| diagnostic.code == "TS2304")
		.unwrap_or_else(|| panic!("no TS2304 diagnostic in the build result: {:?}", result.diagnostics));
	assert_locates_use_mount_effect(d);
}
