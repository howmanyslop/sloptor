//! `transform_oracle_test.go`

use super::oracle_corpus::flamework_testdata;
use super::{
	TransformSpec, new_transform_program, parse_ts, print_source_file, program_file, run_transform, temp_dir,
	write_transform_fixture,
};
use crate::project::{ProjectOptions, open_project};
use crate::transform_state::TransformInput;

#[test]
fn task4_direct_transform_preserves_oracle_identifier_through_final_luau() {
	let (_dir, directory) = temp_dir();
	write_transform_fixture(
		&directory,
		"package.json",
		r#"{"name":"rotor-flamework-task4-oracle","version":"1.0.0"}"#,
	);
	write_transform_fixture(
		&directory,
		"tsconfig.json",
		r#"{"compilerOptions":{"rootDir":"src","outDir":"out"},"files":["src/shared/macros.ts"]}"#,
	);
	write_transform_fixture(
		&directory,
		"src/shared/macros.ts",
		&[
			"interface Payload { readonly count: number }",
			"type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };",
			r#"declare function inspect(value?: Generic<Payload, "id">): string;"#,
			"export const payloadId = inspect();",
		]
		.join("\n"),
	);
	let oracle_type_script = std::fs::read_to_string(flamework_testdata("task4/expected/transformed.ts"))
		.expect("read transformed TypeScript oracle");
	let oracle_luau = std::fs::read_to_string(flamework_testdata("task4/expected/luau/shared/macros.luau"))
		.expect("read final Luau oracle");
	let (printed, luau, diagnostics) = rotor_tsrs::with_large_stack(|| {
		let program = new_transform_program(&directory);
		let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
		let source_file = program_file(program, &directory, "src/shared/macros.ts");
		let source_path = source_file.file_name().to_owned();
		let mut project = open_project(&ProjectOptions {
			project_dir: directory.clone(),
			root_dir: "src".to_owned(),
			out_dir: "out".to_owned(),
			config: rotor_config::config::FlameworkConfig {
				hash_prefix: "task4".to_owned(),
				..rotor_config::config::FlameworkConfig::default()
			},
			..ProjectOptions::default()
		})
		.expect("OpenProject");
		let result = crate::transform::transform(TransformInput {
			program,
			checker: &mut checker,
			files: vec![source_file],
			project: &mut project,
			analyses: Vec::new(),
			macro_runtime: None,
			print: None,
		})
		.expect("Transform");
		drop(checker);
		let printed = print_source_file(result.files[0]);
		assert!(
			parse_ts(&source_path, &printed).diagnostics().is_empty(),
			"reparse diagnostics:\n{printed}"
		);
		write_transform_fixture(&directory, "src/shared/macros.ts", &printed);
		let luau_program = new_transform_program(&directory);
		let mut luau_checker = luau_program.get_type_checker(&rotor_tsrs::Context::default());
		let luau_source = program_file(luau_program, &directory, "src/shared/macros.ts");
		let mut multi = rotor_transformer::MultiState::new();
		let mut state = rotor_transformer::State::new(
			luau_program,
			&mut luau_checker,
			luau_source,
			&mut multi,
			rotor_transformer::StateOptions::default(),
		);
		let statements = rotor_transformer::transform_source_file(&mut state);
		let luau = rotor_luau::render::render_ast(&state.ast, statements).expect("render");
		let diagnostics = state.diags.has_errors().then(|| state.diags.flush().len());
		(printed, luau, diagnostics)
	});

	let want_identifier = "task4:shared/macros@Payload";
	for (name, artifact) in [
		("native transformed TypeScript", printed.as_str()),
		("reparsed Rotor final Luau", luau.as_str()),
		("upstream transformed oracle", oracle_type_script.as_str()),
		("upstream final Luau oracle", oracle_luau.as_str()),
	] {
		assert!(
			artifact.contains(want_identifier),
			"{name} missing controlled identifier {want_identifier:?}:\n{artifact}"
		);
	}
	assert_eq!(diagnostics, None, "Rotor final-Luau diagnostics");
}

#[test]
fn transform_merges_only_generated_named_imports_and_preserves_authored_import() {
	let output = run_transform(TransformSpec {
		tsconfig: r#"{"compilerOptions":{"moduleResolution":"node","rootDir":"src","outDir":"out"},"files":["src/macros.ts"]}"#,
		files: vec![
			(
				"node_modules/@flamework/core/package.json",
				r#"{"name":"@flamework/core","types":"index.d.ts"}"#.to_owned(),
			),
			(
				"node_modules/@flamework/core/index.d.ts",
				[
					r#"type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };"#,
					"export declare const Service: () => FlameworkDecorator;",
					"export declare namespace Dependency {}",
					"export declare namespace Modding {}",
					"export declare interface OnStart {}",
				]
				.join("\n"),
			),
			(
				"src/macros.ts",
				[
					"// retained import comment",
					r#"import { Dependency, Modding, type OnStart, Service } from "@flamework/core";"#,
					"type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };",
					"declare namespace Flamework {",
					" function _id<T>(value: string): string;",
					" /** @metadata macro {@link _id intrinsic-flamework-rewrite} */",
					r#" function id<T>(value?: Generic<T, "id">): string;"#,
					"}",
					"interface Payload { value: number }",
					"@Service() export class Consumer {}",
					"export const payloadId = Flamework.id<Payload>();",
				]
				.join("\n"),
			),
		],
		targets: vec!["src/macros.ts"],
		out_dir: "out",
		plain_print: true,
		..TransformSpec::default()
	});
	assert!(output.error.is_none(), "{:?}", output.error);
	let printed = &output.printed[0];

	let generated = r#"import { Reflect as Reflect, Flamework as Flamework } from "@flamework/core";"#;
	let authored = r#"import { Dependency, Modding, type OnStart, Service } from "@flamework/core";"#;
	assert!(
		printed.matches(r#"from "@flamework/core""#).count() == 2
			&& printed.matches(generated).count() == 1
			&& printed.matches(authored).count() == 1,
		"generated imports were not merged separately from the authored import:\n{printed}"
	);
	let (generated_index, comment_index, authored_index) = (
		printed.find(generated),
		printed.find("// retained import comment"),
		printed.find(authored),
	);
	assert!(
		comment_index.is_some() && generated_index > comment_index && authored_index > generated_index,
		"generated/authored import order or comment placement mismatch:\n{printed}"
	);
	assert!(
		parse_ts("/merged.ts", printed).diagnostics().is_empty(),
		"merged import reparse diagnostics"
	);
}

#[test]
fn transform_preserves_empty_generated_import_when_authored_specifier_wins() {
	let (_dir, directory) = temp_dir();
	write_transform_fixture(
		&directory,
		"package.json",
		r#"{"name":"fixture-game","version":"1.0.0"}"#,
	);
	write_transform_fixture(
		&directory,
		"tsconfig.json",
		r#"{"compilerOptions":{"moduleResolution":"node","rootDir":"src","outDir":"out"},"files":["src/macros.ts"]}"#,
	);
	write_transform_fixture(
		&directory,
		"node_modules/@flamework/core/package.json",
		r#"{"name":"@flamework/core","types":"index.d.ts"}"#,
	);
	write_transform_fixture(
		&directory,
		"node_modules/@flamework/core/index.d.ts",
		&[
			"export type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };",
			"export declare namespace Flamework {",
			" function _id<T>(value: string): string;",
			" /** @metadata macro {@link _id intrinsic-flamework-rewrite} */",
			r#" function id<T>(value?: Generic<T, "id">): string;"#,
			"}",
			"export declare namespace Modding {}",
		]
		.join("\n"),
	);
	write_transform_fixture(
		&directory,
		"src/macros.ts",
		&[
			r#"import { Flamework, Modding } from "@flamework/core";"#,
			"interface Payload { value: number }",
			"export const payloadId = Flamework.id<Payload>();",
		]
		.join("\n"),
	);
	let (printed, semantic) = rotor_tsrs::with_large_stack(|| {
		let program = new_transform_program(&directory);
		let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
		let source_file = program_file(program, &directory, "src/macros.ts");
		let mut project = open_project(&ProjectOptions {
			project_dir: directory.clone(),
			root_dir: "src".to_owned(),
			out_dir: "out".to_owned(),
			..ProjectOptions::default()
		})
		.expect("OpenProject");
		let result = crate::transform::transform(TransformInput {
			program,
			checker: &mut checker,
			files: vec![source_file],
			project: &mut project,
			analyses: Vec::new(),
			macro_runtime: None,
			print: None,
		})
		.expect("Transform");
		let printed = rotor_tsrs::printer::new_printer(
			rotor_tsrs::printer::PrinterOptions::default(),
			rotor_tsrs::printer::PrintHandlers::default(),
			Some(result.sources[0].emit_context()),
		)
		.emit_source_file(result.files[0]);
		drop(checker);
		write_transform_fixture(&directory, "src/macros.ts", &printed);
		let semantic_program = new_transform_program(&directory);
		let semantic_source = program_file(semantic_program, &directory, "src/macros.ts");
		let semantic = semantic_program
			.get_semantic_diagnostics(&rotor_tsrs::Context::default(), Some(semantic_source))
			.len();
		(printed, semantic)
	});

	assert!(
		printed.starts_with(
			"import {} from \"@flamework/core\";\nimport { Flamework, Modding } from \"@flamework/core\";"
		),
		"generated empty import and authored import parity mismatch:\n{printed}"
	);
	assert!(
		parse_ts("/empty-generated.ts", &printed).diagnostics().is_empty(),
		"empty generated import reparse diagnostics"
	);
	assert_eq!(semantic, 0, "empty generated import semantic diagnostics");
}
