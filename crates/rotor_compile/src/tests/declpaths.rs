//! `declpaths_test.go`: the declaration path rewrite.

use rotor_tsrs::ast::{Kind, Node, SourceFile, SourceFileParseOptions};
use rotor_tsrs::compiler::Program;
use rotor_tsrs::tscore::ScriptKind;
use rotor_tsrs::{P, tspath};

use super::{NO_LIB_GLOBAL_STUBS, Temp, large_stack, write_project};
use crate::ProjectOptions;
use crate::declmap::declaration_column_shifts;
use crate::declpaths::{
	DeclarationPathRewriter, TextEdit, apply_text_edits, non_overlapping_text_edits, quote_specifier, relative_from,
};
use crate::project::new_project_program_with_options;

/// `newProjectProgram(dir, "")`
fn load_program(dir: &str) -> &'static Program {
	match new_project_program_with_options(dir, "", &ProjectOptions::default()) {
		Ok((_, program)) => program,
		Err(failure) => panic!("newProjectProgram: {failure} (diags: {:?})", failure.messages()),
	}
}

/// `declarationRewriterFixture`: a throwaway project, its rewriter and its program.
struct Fixture {
	_temp: Temp,
	dir: String,
	program: &'static Program,
	rewriter: DeclarationPathRewriter,
}

impl Fixture {
	fn new(options: &str, files: &[(&str, &str)]) -> Self {
		let temp = Temp::new();
		let config = format!(
			"{{
	\"compilerOptions\": {{
		\"allowSyntheticDefaultImports\": true,
		\"declaration\": true,
		\"strict\": true,
		\"rootDir\": \".\",
		\"module\": \"CommonJS\",
		\"moduleResolution\": \"Node\",
		\"noLib\": true,
		\"moduleDetection\": \"force\",
		\"target\": \"ESNext\",
		\"types\": [],
		\"typeRoots\": [\"node_modules/@rbxts\"],
		\"outDir\": \"out\",
		\"baseUrl\": \".\",
		\"paths\": {{ \"@alias/*\": [\"src/*\"] }}{options}
	}},
	\"include\": [\"src\", \"test\"]
}}"
		);
		temp.write("tsconfig.json", config);
		temp.write("package.json", r#"{"name":"@scope/declpaths-fixture"}"#);
		temp.write("src/globals.d.ts", NO_LIB_GLOBAL_STUBS);
		for (name, text) in files {
			temp.write(name, text);
		}
		let dir = temp.dir().replace('\\', "/");
		let program = load_program(&dir);
		let Some(rewriter) = DeclarationPathRewriter::new(program) else {
			panic!("newDeclarationPathRewriter returned nil for a project with paths");
		};
		Self {
			_temp: temp,
			dir,
			program,
			rewriter,
		}
	}

	fn source_file(&self, name: &str) -> P<SourceFile> {
		self.program
			.get_source_file(&format!("{}/{name}", self.dir))
			.unwrap_or_else(|| panic!("fixture source file missing from the program: {name}"))
	}

	/// `rewriteDeclaration`
	fn rewrite(&self, name: &str, decl_text: &str) -> String {
		let entry = self.source_file(name);
		let file_name = entry.file_name();
		let decl_file_name = format!("{}.d.ts", file_name.strip_suffix(".ts").unwrap_or(file_name));
		apply_text_edits(
			decl_text,
			&non_overlapping_text_edits(self.rewriter.specifier_edits(entry, &decl_file_name, decl_text)),
		)
	}
}

/// Every syntax that can carry a module specifier into a `.d.ts`, and the shapes that survive
/// untouched: `(name, declaration, want)`.
const SPECIFIER_KINDS: &[(&str, &str, &str)] = &[
	(
		"import declaration",
		"import { Value } from \"@alias/value\";\n",
		"import { Value } from \"./value\";\n",
	),
	(
		"export declaration",
		"export { Value } from \"@alias/value\";\n",
		"export { Value } from \"./value\";\n",
	),
	(
		"export star declaration",
		"export * from \"@alias/value\";\n",
		"export * from \"./value\";\n",
	),
	(
		"external module reference",
		"import value = require(\"@alias/value\");\n",
		"import value = require(\"./value\");\n",
	),
	(
		"import type node",
		"export declare const value: import(\"@alias/value\").Value;\n",
		"export declare const value: import(\"./value\").Value;\n",
	),
	(
		"require call expression",
		"const loaded = require(\"@alias/value\");\n",
		"const loaded = require(\"./value\");\n",
	),
	(
		"dynamic import call expression",
		"const loaded = import(\"@alias/value\");\n",
		"const loaded = import(\"./value\");\n",
	),
	(
		"single quoted specifier is renormalized to double quotes",
		"import { Value } from '@alias/value';\n",
		"import { Value } from \"./value\";\n",
	),
	(
		"several specifiers on one line",
		"import { A } from \"@alias/value\"; import { B } from \"@alias/value\";\n",
		"import { A } from \"./value\"; import { B } from \"./value\";\n",
	),
	(
		"an already relative specifier resolves to itself",
		"import { Value } from \"./value\";\n",
		"import { Value } from \"./value\";\n",
	),
	(
		"external library import is left alone",
		"import { dummy } from \"@rbxts/dummy\";\n",
		"import { dummy } from \"@rbxts/dummy\";\n",
	),
	(
		"url specifier is left alone",
		"import \"https://example.com/module\";\n",
		"import \"https://example.com/module\";\n",
	),
	(
		"unresolvable specifier is left alone",
		"import { Missing } from \"@alias/missing\";\n",
		"import { Missing } from \"@alias/missing\";\n",
	),
	(
		"namespace re-export",
		"export * as shared from \"@alias/value\";\n",
		"export * as shared from \"./value\";\n",
	),
	(
		"type-only import declaration",
		"import type { Value } from \"@alias/value\";\n",
		"import type { Value } from \"./value\";\n",
	),
	(
		"type-only export declaration",
		"export type { Value } from \"@alias/value\";\n",
		"export type { Value } from \"./value\";\n",
	),
	(
		"inline type modifier on a named import",
		"import { type Value } from \"@alias/value\";\n",
		"import { type Value } from \"./value\";\n",
	),
	(
		// Every specifier in the tree is visited, type arguments included.
		"nested import type inside a type argument",
		"export declare const nested: import(\"@alias/value\").Box<import(\"@alias/value\").Value>;\n",
		"export declare const nested: import(\"./value\").Box<import(\"./value\").Value>;\n",
	),
	(
		"query specifier is left alone",
		"import \"@alias/value?raw\";\n",
		"import \"@alias/value?raw\";\n",
	),
	(
		"hash specifier is left alone",
		"import \"@alias/value#fragment\";\n",
		"import \"@alias/value#fragment\";\n",
	),
	(
		"bare url specifier is left alone",
		"import \"http://example.com\";\n",
		"import \"http://example.com\";\n",
	),
	(
		"a specifier-free declaration is returned unchanged",
		"export declare const value: number;\n",
		"export declare const value: number;\n",
	),
];

#[test]
fn declaration_path_rewrite_specifier_kinds() {
	large_stack(|| {
		let fixture = Fixture::new(
			"",
			&[
				("src/main.ts", "export const main = 1;\n"),
				(
					"src/value.ts",
					"export interface Value { label: string; }\nexport const value: Value = { label: \"v\" };\n",
				),
				(
					"node_modules/@rbxts/dummy/package.json",
					r#"{"name":"@rbxts/dummy","types":"index.d.ts"}"#,
				),
				(
					"node_modules/@rbxts/dummy/index.d.ts",
					"export declare const dummy: number;\n",
				),
			],
		);

		for &(name, decl, want) in SPECIFIER_KINDS {
			assert_eq!(fixture.rewrite("src/main.ts", decl), want, "{name}");
		}
	});
}

#[test]
fn declaration_path_rewrite_resolves_specifiers_the_file_never_imported() {
	large_stack(|| {
		let fixture = Fixture::new(
			"",
			&[
				("src/main.ts", "export const main = 1;\n"),
				("src/deep/nested.ts", "export interface Nested { id: number; }\n"),
			],
		);
		let decl = "export declare const nested: import(\"@alias/deep/nested\").Nested;\n";
		let want = "export declare const nested: import(\"./deep/nested\").Nested;\n";
		assert_eq!(fixture.rewrite("src/main.ts", decl), want);
	});
}

#[test]
fn declaration_path_rewrite_implicit_extensions_ts_and_dts_are_always_implicit() {
	large_stack(|| {
		let fixture = Fixture::new(
			"",
			&[
				("src/main.ts", "export const main = 1;\n"),
				("src/value.ts", "export interface Value { label: string; }\n"),
				("src/ambient.d.ts", "export declare const ambient: number;\n"),
			],
		);
		let decl = "import \"@alias/value\";\nimport \"@alias/ambient\";\n";
		let want = "import \"./value\";\nimport \"./ambient\";\n";
		assert_eq!(fixture.rewrite("src/main.ts", decl), want);
	});
}

#[test]
fn declaration_path_rewrite_implicit_extensions_js_is_implicit_only_under_allow_js() {
	large_stack(|| {
		let files = [
			("src/main.ts", "export const main = 1;\n"),
			("src/legacy.js", "module.exports = { legacy: 1 };\n"),
		];
		let decl = "import \"@alias/legacy\";\n";
		let fixture = Fixture::new(", \"allowJs\": true", &files);
		assert_eq!(
			fixture.rewrite("src/main.ts", decl),
			"import \"./legacy\";\n",
			"with allowJs"
		);
		// Without allowJs the extension is not implicit, so it stays in the rewritten specifier.
		let fixture = Fixture::new("", &files);
		assert_eq!(
			fixture.rewrite("src/main.ts", decl),
			"import \"./legacy.js\";\n",
			"without allowJs"
		);
	});
}

#[test]
fn declaration_path_rewrite_implicit_extensions_json_is_implicit_only_under_resolve_json_module() {
	large_stack(|| {
		let files = [
			("src/main.ts", "export const main = 1;\n"),
			("src/data.json", "{\"label\":\"data\"}\n"),
		];
		let decl = "import \"@alias/data.json\";\n";
		let fixture = Fixture::new(", \"resolveJsonModule\": true", &files);
		assert_eq!(
			fixture.rewrite("src/main.ts", decl),
			"import \"./data\";\n",
			"with resolveJsonModule"
		);
		let fixture = Fixture::new("", &files);
		assert_eq!(
			fixture.rewrite("src/main.ts", decl),
			"import \"./data.json\";\n",
			"without resolveJsonModule"
		);
	});
}

#[test]
fn declaration_path_rewrite_implicit_extensions_tsx_is_implicit_only_under_jsx() {
	large_stack(|| {
		let fixture = Fixture::new(
			", \"jsx\": \"react\"",
			&[
				("src/main.ts", "export const main = 1;\n"),
				("src/widget.tsx", "export const widget = 1;\n"),
			],
		);
		assert_eq!(
			fixture.rewrite("src/main.ts", "import \"@alias/widget\";\n"),
			"import \"./widget\";\n",
			"with jsx"
		);
	});
}

#[test]
fn declaration_path_rewrite_root_dirs() {
	large_stack(|| {
		let files = [
			("src/main.ts", "export const main = 1;\n"),
			("src/value.ts", "export interface Value { label: string; }\n"),
			("test/spec.ts", "export const spec = 1;\n"),
		];
		let decl = "import { Value } from \"@alias/value\";\n";

		let fixture = Fixture::new("", &files);
		assert_eq!(
			fixture.rewrite("test/spec.ts", decl),
			"import { Value } from \"../src/value\";\n",
			"without rootDirs"
		);

		let fixture = Fixture::new(", \"rootDirs\": [\"src\", \"test\"]", &files);
		assert_eq!(
			fixture.rewrite("test/spec.ts", decl),
			"import { Value } from \"./value\";\n",
			"with rootDirs"
		);
	});
}

#[test]
fn declaration_path_rewrite_relative_is_case_aware() {
	let insensitive = tspath::ComparePathsOptions {
		use_case_sensitive_file_names: false,
		current_directory: String::new(),
	};
	assert_eq!(
		relative_from(&insensitive, "D:/Project/Src", "d:/project/src/value.ts"),
		"value.ts",
		"case-insensitive relative"
	);
	let sensitive = tspath::ComparePathsOptions {
		use_case_sensitive_file_names: true,
		current_directory: String::new(),
	};
	assert_eq!(
		relative_from(&sensitive, "/project/Src", "/project/src/value.ts"),
		"../src/value.ts",
		"case-sensitive relative"
	);
	assert_eq!(
		relative_from(&insensitive, "D:/project/src", "D:/project/src"),
		"",
		"relative to itself"
	);
}

#[test]
fn declaration_path_rewriter_is_nil_without_path_mappings() {
	large_stack(|| {
		let temp = write_project("@scope/no-path-mappings", "");
		let program = load_program(&temp.dir());
		assert!(
			DeclarationPathRewriter::new(program).is_none(),
			"newDeclarationPathRewriter returned a rewriter for a project with no baseUrl or paths"
		);
	});
}

#[test]
fn declaration_path_rewrite_declaration_file_wins_over_source() {
	large_stack(|| {
		let fixture = Fixture::new(
			"",
			&[
				("src/main.ts", "export const main = 1;\n"),
				("src/pair.ts", "export const fromSource = 1;\n"),
				("src/pair.d.ts", "export declare const fromDeclaration: number;\n"),
			],
		);
		assert_eq!(
			fixture.rewrite("src/main.ts", "import \"@alias/pair\";\n"),
			"import \"./pair\";\n"
		);
	});
}

#[test]
fn declaration_path_rewrite_extensionless_json_specifier() {
	large_stack(|| {
		let fixture = Fixture::new(
			", \"resolveJsonModule\": true",
			&[
				("src/main.ts", "export const main = 1;\n"),
				("src/data.json", "{\"label\":\"data\"}\n"),
			],
		);
		let decl = "import \"@alias/data\";\n";
		assert_eq!(fixture.rewrite("src/main.ts", decl), decl, "want it left alone");
		// The same module with its extension resolves, and `.json` comes off.
		assert_eq!(
			fixture.rewrite("src/main.ts", "import \"@alias/data.json\";\n"),
			"import \"./data\";\n"
		);
	});
}

#[test]
fn declaration_path_rewrite_is_deterministic_under_conditional_exports() {
	large_stack(|| {
		let fixture = Fixture::new(
			", \"module\": \"preserve\", \"moduleResolution\": \"bundler\"",
			&[
				(
					"src/main.ts",
					"import \"@alias/value\";\nimport \"conditional\";\nexport const main = 1;\n",
				),
				("src/value.ts", "export interface Value { label: string; }\n"),
				(
					"node_modules/conditional/package.json",
					r#"{"name":"conditional","exports":{".":{"types":"./types/index.d.ts","import":"./esm/index.js","require":"./cjs/index.js"}}}"#,
				),
				(
					"node_modules/conditional/types/index.d.ts",
					"export declare const conditional: number;\n",
				),
				(
					"node_modules/conditional/esm/index.js",
					"export const conditional = 1;\n",
				),
				(
					"node_modules/conditional/cjs/index.js",
					"module.exports = { conditional: 1 };\n",
				),
			],
		);
		let decl = "import \"@alias/value\";\nimport \"conditional\";\n";

		let first = fixture.rewrite("src/main.ts", decl);
		for run in 0..50 {
			assert_eq!(
				fixture.rewrite("src/main.ts", decl),
				first,
				"run {run}, want run 0's text"
			);
		}
		assert!(first.contains("\"./value\""), "path alias was not rewritten: {first:?}");
	});
}

#[test]
fn quote_specifier_cases() {
	for (name, input, want) in [
		("ordinary path", "./value", r#""./value""#),
		("quote", r#"./va"lue"#, r#""./va\"lue""#),
		("backslash", r"./va\lue", r#""./va\\lue""#),
		("both", r#"./a"b\c"#, r#""./a\"b\\c""#),
	] {
		assert_eq!(quote_specifier(input), want, "{name}: quoteSpecifier({input:?})");
	}
}

/// The import declaration's module specifier text, searched depth-first.
fn find_import_specifier(node: P<Node>, found: &mut String) -> bool {
	if node.kind() == Kind::ImportDeclaration
		&& let Some(specifier) = node.module_specifier()
	{
		*found = specifier.text().to_owned();
	}
	node.for_each_child(&mut |child| find_import_specifier(child, found));
	false
}

#[test]
fn quote_specifier_survives_a_reparse() {
	let specifier = r#"./awkward"dir\name/value"#;
	let decl_text = "import { Value } from \"@alias/value\";\n";
	let start = decl_text.find("\"@alias/value\"").expect("the specifier");
	let spliced = apply_text_edits(
		decl_text,
		&[TextEdit {
			start,
			end: start + "\"@alias/value\"".len(),
			text: quote_specifier(specifier),
		}],
	);

	let parsed = rotor_tsrs::parser::parse_source_file(
		SourceFileParseOptions {
			file_name: "/spliced.d.ts".to_owned(),
			path: tspath::to_path("/spliced.d.ts", "/", true),
			..SourceFileParseOptions::default()
		},
		&spliced,
		ScriptKind::TS,
	);
	assert!(
		parsed.diagnostics().is_empty(),
		"spliced declaration has parse diagnostics {:?}:\n{spliced}",
		parsed.diagnostics().len()
	);
	let mut found = String::new();
	parsed
		.as_node()
		.for_each_child(&mut |child| find_import_specifier(child, &mut found));
	assert_eq!(found, specifier, "reparsed specifier (text: {spliced})");
}

#[test]
#[cfg_attr(windows, ignore = "a Windows path cannot contain a quote")]
fn declaration_path_rewrite_quote_in_directory_name() {
	large_stack(|| {
		let fixture = Fixture::new(
			"",
			&[
				("src/main.ts", "export const main = 1;\n"),
				("src/od\"d/value.ts", "export interface Value { label: string; }\n"),
			],
		);
		let got = fixture.rewrite("src/main.ts", "import \"@alias/od\\\"d/value\";\n");
		assert!(
			got.contains("\\\""),
			"quote in the resolved path was not escaped: {got}"
		);
	});
}

#[test]
fn non_overlapping_text_edits_cases() {
	let edit = |start, end, text: &str| TextEdit {
		start,
		end,
		text: text.to_owned(),
	};
	let kept = non_overlapping_text_edits(vec![
		edit(10, 20, "aaa"),
		edit(0, 5, "b"),
		edit(12, 16, "dropped"),
		edit(20, 25, "c"),
	]);
	assert_eq!(kept.len(), 3, "kept edits, want 3: {kept:?}");
	let starts: Vec<usize> = kept.iter().map(|edit| edit.start).collect();
	assert_eq!(starts, [0, 10, 20], "kept starts");
}

#[test]
fn overlapping_edits_keep_text_and_map_agreed() {
	// A specifier whose text happens to contain the type-reference marker.
	let decl_text = "import \"@alias/types=\\\"types\\\"/value\";\n";
	let specifier_start = decl_text.find("\"@alias/").expect("the specifier");
	let specifier_end = decl_text.find(';').expect("the semicolon");
	// Go searches for the unescaped `types="types"`, which the literal does not contain, so its
	// `strings.Index` returns -1 and the marker edit is one `applyTextEdits` skips. A `usize`
	// cannot hold -1; the escaped spelling makes the overlap the comment above describes real.
	let marker = "types=\\\"types\\\"";
	let marker_start = decl_text.find(marker).expect("the marker");
	let edits = non_overlapping_text_edits(vec![
		TextEdit {
			start: marker_start,
			end: marker_start + marker.len(),
			text: "types=\"@rbxts/types\"".to_owned(),
		},
		TextEdit {
			start: specifier_start,
			end: specifier_end,
			text: "\"./value\"".to_owned(),
		},
	]);
	assert_eq!(edits.len(), 1, "overlapping edits were not collapsed: {edits:?}");

	let rewritten = apply_text_edits(decl_text, &edits);
	let shifts = declaration_column_shifts(decl_text, &edits, 0);
	let total: i64 = shifts.values().flatten().map(|shift| shift.delta).sum();
	let got = i64::try_from(rewritten.len()).expect("a length") - i64::try_from(decl_text.len()).expect("a length");
	assert_eq!(got, total, "text changed by {got} but the map was shifted by {total}");
}
