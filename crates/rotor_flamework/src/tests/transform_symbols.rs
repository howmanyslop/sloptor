//! `transform_symbols_test.go`

use super::{StateSpec, collect_call_expressions, with_state};
use crate::transform_symbols::node_uid;

#[test]
fn node_uid_resolves_import_alias_to_declaration() {
	let spec =
		StateSpec::new(
			vec![
			("package.json", r#"{"name":"fixture-game","version":"1.0.0"}"#.to_owned()),
			(
				"tsconfig.json",
				r#"{"compilerOptions":{"strict":true,"rootDir":"src","outDir":"out"},"include":["src/**/*.ts"]}"#.to_owned(),
			),
			("src/payload.ts", "export interface Payload { readonly value: string }".to_owned()),
			(
				"src/main.ts",
				"import type { Payload as Alias } from \"./payload\";\ndeclare function id<T>(): string;\nid<Alias>();"
					.to_owned(),
			),
		],
			"src/main.ts",
		);
	let id = with_state(&spec, |state, file| {
		let call = collect_call_expressions(file)[0];
		let type_argument = call.type_arguments()[0];
		node_uid(state, type_argument)
	})
	.expect("nodeUID");

	assert_eq!(id, "payload@Payload");
}

#[test]
fn node_uid_reads_only_the_package_name_of_a_declaration_package() {
	// Go decodes `struct { Name string }`, so a `version` of any type does not fail the lookup.
	let spec = StateSpec::new(
		vec![
			(
				"package.json",
				r#"{"name":"fixture-game","version":"1.0.0"}"#.to_owned(),
			),
			(
				"tsconfig.json",
				r#"{"compilerOptions":{"strict":true,"rootDir":"src","outDir":"out"},"include":["src/**/*.ts"]}"#
					.to_owned(),
			),
			(
				"node_modules/@rbxts/x/package.json",
				r#"{"name":"@rbxts/x","version":1,"types":"index.d.ts"}"#.to_owned(),
			),
			(
				"node_modules/@rbxts/x/index.d.ts",
				"export interface Payload { readonly value: string }".to_owned(),
			),
			(
				"src/main.ts",
				"import type { Payload } from \"@rbxts/x\";\ndeclare function id<T>(): string;\nid<Payload>();"
					.to_owned(),
			),
		],
		"src/main.ts",
	);
	let id = with_state(&spec, |state, file| {
		let call = collect_call_expressions(file)[0];
		let type_argument = call.type_arguments()[0];
		node_uid(state, type_argument)
	})
	.expect("nodeUID");

	assert_eq!(id, "@rbxts/x:index@Payload");
}
