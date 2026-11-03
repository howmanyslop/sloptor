//! `transform_driver_test.go`

use rotor_config::config::FlameworkConfig;

use super::{new_transform_program, print_source_file, program_file, temp_dir, write_transform_fixture};
use crate::plan::{ClassPlan, FileAnalysis};
use crate::project::{ProjectOptions, open_project};
use crate::transform::transform;
use crate::transform_state::TransformInput;

#[test]
fn transform_emits_exact_class_identifier_metadata_when_class_is_planned() {
	rotor_tsrs::with_large_stack(|| {
		let (_dir, directory) = temp_dir();
		write_transform_fixture(
			&directory,
			"package.json",
			r#"{"name":"fixture-game","version":"1.0.0"}"#,
		);
		write_transform_fixture(
			&directory,
			"tsconfig.json",
			r#"{"compilerOptions":{"rootDir":"src","outDir":"out"},"include":["src/**/*.ts"]}"#,
		);
		write_transform_fixture(&directory, "src/service.ts", "export class Service {}\n");
		let program = new_transform_program(&directory);
		let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
		let source_file = program_file(program, &directory, "src/service.ts");
		let mut project = open_project(&ProjectOptions {
			project_dir: directory.clone(),
			root_dir: "src".to_owned(),
			out_dir: "out".to_owned(),
			config: FlameworkConfig::default(),
			..ProjectOptions::default()
		})
		.unwrap();
		let result = transform(TransformInput {
			program,
			checker: &mut checker,
			files: vec![source_file],
			project: &mut project,
			analyses: vec![FileAnalysis {
				file_id: "src/service.ts".to_owned(),
				classes: vec![ClassPlan {
					internal_id: "fixture-game:out/service@Service".to_owned(),
					..ClassPlan::default()
				}],
				..FileAnalysis::default()
			}],
			macro_runtime: None,
			print: None,
		})
		.unwrap();
		assert_eq!(result.files.len(), 1);
		let printed = print_source_file(result.files[0]);
		let want = [
			r#"import { Reflect as Reflect } from "@flamework/core";"#,
			"export class Service {",
			"    static {",
			r#"        Reflect["defineMetadata"](Service, "identifier", "service@Service");"#,
			"    }",
			"}",
			"",
		]
		.join("\n");
		assert_eq!(printed, want);
		assert!(result.diagnostics.is_empty());
		assert!(result.sources.len() == 1 && result.sources[0].changed());
		let trace = result.sources[0].trace();
		assert_eq!(trace.original_file_name(), source_file.file_name());
		assert_eq!(trace.original_text(), source_file.text());
		assert_eq!(
			result.plans[0].classes()[0].internal_id,
			"fixture-game:out/service@Service"
		);
		assert_eq!(
			project.plans()[0].classes()[0].internal_id,
			"fixture-game:out/service@Service"
		);
	});
}

#[test]
fn transform_diagnostics_orders_by_file_position_and_code_when_added_out_of_order() {
	use rotor_tsrs::diagnostics::Category;

	use crate::transform_diagnostics::Diagnostic;
	rotor_tsrs::with_large_stack(|| {
		let (_dir, directory) = temp_dir();
		write_transform_fixture(
			&directory,
			"package.json",
			r#"{"name":"fixture-game","version":"1.0.0"}"#,
		);
		write_transform_fixture(
			&directory,
			"tsconfig.json",
			r#"{"compilerOptions":{},"files":["src/a.ts","src/z.ts"]}"#,
		);
		write_transform_fixture(&directory, "src/a.ts", "class A {}");
		write_transform_fixture(&directory, "src/z.ts", "class Z {}");
		let program = new_transform_program(&directory);
		let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
		let first_file = program_file(program, &directory, "src/a.ts");
		let second_file = program_file(program, &directory, "src/z.ts");
		let mut project = open_project(&ProjectOptions {
			project_dir: directory.clone(),
			root_dir: "src".to_owned(),
			..ProjectOptions::default()
		})
		.unwrap();
		let mut state =
			crate::transform_state::new_transform_state_base(program, &mut checker, &mut project, None, &[]);
		let diagnostic = |file, pos, code, message: &str| Diagnostic {
			file: Some(file),
			pos,
			end: pos + 1,
			category: Category::Error,
			code,
			string_code: None,
			message: message.to_owned(),
			related_information: Vec::new(),
		};
		state.add_diagnostic(diagnostic(second_file, 2, 20, "second"));
		state.add_diagnostic(diagnostic(first_file, 4, 30, "later"));
		state.add_diagnostic(diagnostic(first_file, 1, 10, "first"));
		let ordered = state.ordered_diagnostics();
		let positions: Vec<i32> = ordered
			.iter()
			.map(crate::transform_diagnostics::Diagnostic::pos)
			.collect();
		assert_eq!(positions, [1, 4, 2]);
	});
}

// TestTransform_returnsInvalidInputError_whenConcreteCompilerInputsAreNil is not portable:
// Rust's TransformInput holds references, which cannot be nil.

#[test]
fn transform_uses_supplied_macro_runtime_for_controlled_uuid_and_shuffle() {
	use std::cell::Cell;
	use std::rc::Rc;

	use crate::transform_macro_types::MacroRuntime;
	rotor_tsrs::with_large_stack(|| {
		let (_dir, directory) = temp_dir();
		write_transform_fixture(
			&directory,
			"package.json",
			r#"{"name":"fixture-game","version":"1.0.0"}"#,
		);
		write_transform_fixture(
			&directory,
			"tsconfig.json",
			r#"{"compilerOptions":{"strict":true},"files":["src/main.ts"]}"#,
		);
		write_transform_fixture(
			&directory,
			"src/main.ts",
			&[
				"type Caller<M extends string> = { _flamework_macro_caller: M };",
				"type Intrinsic<N extends string, T> = { _flamework_intrinsic: [N, T] };",
				"declare function inspect<T>(value?: T): T;",
				r#"inspect<Caller<"uuid">>();"#,
				r#"inspect<Intrinsic<"shuffle-array", ["left", "right"]>>();"#,
			]
			.join("\n"),
		);
		let program = new_transform_program(&directory);
		let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
		let source_file = program_file(program, &directory, "src/main.ts");
		let mut project = open_project(&ProjectOptions {
			project_dir: directory.clone(),
			root_dir: "src".to_owned(),
			config: FlameworkConfig {
				obfuscation: true,
				..FlameworkConfig::default()
			},
			..ProjectOptions::default()
		})
		.unwrap();
		let uuid_calls = Rc::new(Cell::new(0));
		let random_calls = Rc::new(Cell::new(0));
		let (uuid_counter, random_counter) = (Rc::clone(&uuid_calls), Rc::clone(&random_calls));
		let runtime = MacroRuntime {
			uuid: Some(Rc::new(move || {
				uuid_counter.set(uuid_counter.get() + 1);
				Ok("11111111-1111-4111-8111-111111111111".to_owned())
			})),
			random_index: Some(Rc::new(move |_| {
				random_counter.set(random_counter.get() + 1);
				Ok(0)
			})),
			build_guard: None,
		};
		let result = transform(TransformInput {
			program,
			checker: &mut checker,
			files: vec![source_file],
			project: &mut project,
			analyses: Vec::new(),
			macro_runtime: Some(runtime),
			print: None,
		})
		.unwrap();
		let printed = print_source_file(result.files[0]);
		assert!(
			printed.contains(r#""11111111-1111-4111-8111-111111111111" as never"#),
			"{printed}"
		);
		let right = printed.find(r#""right" as never"#);
		let left = printed.find(r#""left" as never"#);
		assert!(
			matches!((right, left), (Some(right), Some(left)) if right < left),
			"{printed}"
		);
		assert_eq!((uuid_calls.get(), random_calls.get()), (1, 1));
	});
}

#[test]
fn new_transform_state_preserves_defaults_when_macro_runtime_override_is_partial() {
	use std::rc::Rc;

	use crate::transform_macro_types::MacroRuntime;
	rotor_tsrs::with_large_stack(|| {
		let (_dir, directory) = temp_dir();
		write_transform_fixture(
			&directory,
			"package.json",
			r#"{"name":"fixture-game","version":"1.0.0"}"#,
		);
		write_transform_fixture(
			&directory,
			"tsconfig.json",
			r#"{"compilerOptions":{},"files":["src/main.ts"]}"#,
		);
		write_transform_fixture(&directory, "src/main.ts", "export {};\n");
		let program = new_transform_program(&directory);
		let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
		let mut project = open_project(&ProjectOptions {
			project_dir: directory.clone(),
			root_dir: "src".to_owned(),
			..ProjectOptions::default()
		})
		.unwrap();
		let mut override_runtime = MacroRuntime {
			uuid: Some(Rc::new(|| Ok("controlled".to_owned()))),
			..MacroRuntime::default()
		};
		let state = crate::transform_state::new_transform_state(
			program,
			&mut checker,
			&mut project,
			Some(&override_runtime),
			&[],
		);
		override_runtime.uuid = Some(Rc::new(|| Ok("mutated".to_owned())));
		let runtime = state.macro_runtime();
		let uuid = runtime.uuid.as_ref().map(|uuid| uuid());
		assert!(matches!(uuid, Some(Ok(ref text)) if text == "controlled"));
		assert!(runtime.random_index.is_some() && runtime.build_guard.is_some());
	});
}

#[test]
fn transform_expands_macro_whose_marker_appears_through_a_mapped_parameter() {
	let printed = super::transform_fixture_source(&[
		"type Caller<M extends string> = { _flamework_macro_caller: M };",
		"type Same<T> = { [K in keyof T]: T[K] };",
		"declare function inspect<T>(value?: Same<T>): T;",
		r#"inspect<Caller<"uuid">>();"#,
	]);
	assert!(
		printed.contains(r#""11111111-1111-4111-8111-111111111111" as never"#),
		"{printed}"
	);
}

#[test]
fn transform_leaves_an_ordinary_call_with_an_omitted_argument_alone() {
	let printed =
		super::transform_fixture_source(&["declare function plain(value?: { size: number }): void;", "plain();"]);
	assert!(
		!printed.contains("as never") && printed.contains("plain();"),
		"{printed}"
	);
}
