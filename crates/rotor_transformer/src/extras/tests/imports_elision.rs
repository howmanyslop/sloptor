//! `imports_elision_test.go`: the import elision predicate.

use std::path::Path;

use rotor_tsrs::ast::{self as tsast};
use rotor_tsrs::{Context, ProjectOptions, load_project, with_large_stack};

use super::write_fixture;
use crate::extras::imports::is_referenced_alias_value;
use crate::prelude::*;
use crate::state::{MultiState, StateOptions};

const PACKAGE_JSON: &str = r#"{"name":"fixture","version":"1.0.0"}"#;
const TSCONFIG: &str = r#"{"compilerOptions":{"rootDir":"src","outDir":"out","moduleResolution":"node","strict":true},"include":["src/**/*.ts"]}"#;

/// Go's `buildElisionState`: a state for `rel_path` in `directory`, with no diagnostics check.
fn with_elision_state<R: Send>(directory: &Path, rel_path: &str, test: impl FnOnce(&mut State<'_>) -> R + Send) -> R {
	let directory = directory.to_path_buf();
	let rel_path = rel_path.to_owned();
	with_large_stack(move || {
		let options = ProjectOptions {
			checkers: Some(1),
			single_threaded: Some(true),
			skip_validation: true,
		};
		let loaded = load_project(&directory.join("tsconfig.json"), &options).expect("the project loads");
		let program = loaded.program;
		let wanted = format!("{}/{rel_path}", loaded.directory());
		let file = program
			.source_files()
			.iter()
			.copied()
			.find(|file| file.file_name() == wanted)
			.unwrap_or_else(|| panic!("source file not in program: {wanted}"));
		let context = Context::default();
		let mut checker = program.get_type_checker_for_file(&context, file);
		let mut multi = MultiState::new();
		let mut state = State::new(program, &mut checker, file, &mut multi, StateOptions::default());
		let result = test(&mut state);
		drop(state);
		drop(checker);
		result
	})
}

/// Go's `findNamedImport`: the import clause and bound name of the named import `name`.
fn find_named_import(state: &State<'_>, name: &str) -> Option<(P<Node>, P<Node>)> {
	for &statement in state.source_file.statements().nodes() {
		if !tsast::is_import_declaration(statement) {
			continue;
		}
		let Some(import_clause) = statement.as_import_declaration().import_clause else {
			continue;
		};
		let Some(named_bindings) = import_clause.as_import_clause().named_bindings else {
			continue;
		};
		if !tsast::is_named_imports(named_bindings) {
			continue;
		}
		for &element in named_bindings.elements() {
			if let Some(element_name) = element.name()
				&& element_name.text() == name
			{
				return Some((import_clause, element_name));
			}
		}
	}
	None
}

fn elision_project(files: &[(&str, &str)]) -> tempfile::TempDir {
	let directory = tempfile::tempdir().expect("a temporary directory");
	write_fixture(directory.path(), "package.json", PACKAGE_JSON);
	write_fixture(directory.path(), "tsconfig.json", TSCONFIG);
	for &(path, contents) in files {
		write_fixture(directory.path(), path, contents);
	}
	directory
}

/// The only use of `t` is a property access `t.interface(...)`, as in Flamework's
/// macro-generated guard under noSemanticDiagnostics.
#[test]
fn is_referenced_alias_value_keeps_import_when_checker_cannot_resolve_reference() {
	let directory = elision_project(&[
		(
			"node_modules/@rbxts/t/index.d.ts",
			"export declare const t: { interface(x: unknown): (v: unknown) => boolean; string: unknown; };",
		),
		(
			"src/main.ts",
			r#"import { t } from "@rbxts/t";
declare const value: unknown;
const check = t.interface({ animationId: t.string })(value);
"#,
		),
	]);
	let root = directory.path().canonicalize().expect("the directory resolves");
	let kept = with_elision_state(&root, "src/main.ts", |state| {
		let (import_clause, import_name) = find_named_import(state, "t").expect("the import exists");
		is_referenced_alias_value(state, import_clause, import_name)
	});
	assert!(kept, "t import elided even though it is referenced as a value");
}

/// `t` is re-exported through a prelude module and the guard code uses it as a value.
#[test]
fn is_referenced_alias_value_keeps_reexported_t_alias() {
	let directory = elision_project(&[
		(
			"node_modules/@rbxts/t/index.d.ts",
			"export declare const t: { interface(x: unknown): (v: unknown) => boolean; string: unknown; };",
		),
		(
			"node_modules/@flamework/core/out/prelude.d.ts",
			"import { t } from \"@rbxts/t\";\nexport { t };\n",
		),
		(
			"src/main.ts",
			r#"import { t } from "@flamework/core/out/prelude";
declare const value: unknown;
const check = t.interface({ animationId: t.string })(value);
"#,
		),
	]);
	let root = directory.path().canonicalize().expect("the directory resolves");
	let kept = with_elision_state(&root, "src/main.ts", |state| {
		let (import_clause, import_name) = find_named_import(state, "t").expect("the import exists");
		is_referenced_alias_value(state, import_clause, import_name)
	});
	assert!(kept, "re-exported t import elided even though t is used as a value");
}

#[test]
#[expect(clippy::too_many_lines, reason = "the case table is verbatim")]
fn is_referenced_alias_value_qualified_type_names() {
	struct Case {
		name: &'static str,
		files: &'static [(&'static str, &'static str)],
		importee: &'static str,
		want: bool,
	}
	let cases = [
		Case {
			name: "type-only declared namespace used as A.T",
			files: &[
				(
					"src/webhook.d.ts",
					"export declare namespace Webhook { export interface Post { content: string } }",
				),
				(
					"src/main.ts",
					r#"import { Webhook } from "./webhook";
export function send(body: Webhook.Post) {}
export type P = Webhook.Post;
export function many(body: Array<Webhook.Post>, other: { inner: Webhook.Post }) {}
"#,
				),
			],
			importee: "Webhook",
			want: false,
		},
		Case {
			name: "implicitly exported namespace in a .d.ts module",
			files: &[
				(
					"src/webhook.d.ts",
					"declare namespace Webhook { interface Post { content: string } }
export { Webhook };",
				),
				(
					"src/main.ts",
					r#"import { Webhook } from "./webhook";
export function send(body: Webhook.Post) {}
"#,
				),
			],
			importee: "Webhook",
			want: false,
		},
		Case {
			name: "nested type-only namespace used as A.B.T",
			files: &[
				(
					"src/a.ts",
					"export namespace A { export namespace B { export interface Post { content: string } } }",
				),
				(
					"src/main.ts",
					r#"import { A } from "./a";
export function send(body: A.B.Post) {}
"#,
				),
			],
			importee: "A",
			want: false,
		},
		Case {
			name: "instantiated namespace used only as a value",
			files: &[
				("src/n.ts", "export namespace N { export const v = 1; }"),
				(
					"src/main.ts",
					r#"import { N } from "./n";
export const x = N.v;
"#,
				),
			],
			importee: "N",
			want: true,
		},
		Case {
			name: "instantiated namespace used as type and value",
			files: &[
				(
					"src/n.ts",
					"export namespace N { export const v = 1; export interface T { a: number } }",
				),
				(
					"src/main.ts",
					r#"import { N } from "./n";
export function f(body: N.T) { return N.v; }
"#,
				),
			],
			importee: "N",
			want: true,
		},
	];
	for case in &cases {
		let directory = elision_project(case.files);
		let root = directory.path().canonicalize().expect("the directory resolves");
		let importee = case.importee;
		let got = with_elision_state(&root, "src/main.ts", move |state| {
			let (_, import_name) =
				find_named_import(state, importee).unwrap_or_else(|| panic!("import {importee} not found"));
			let parent = import_name.parent().expect("the import name has a parent");
			is_referenced_alias_value(state, parent, import_name)
		});
		assert_eq!(got, case.want, "{}: isReferencedAliasValue", case.name);
	}
}
