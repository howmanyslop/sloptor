//! The parallel per-file loop of `transform_parallel.go`. Go has no test file for it; this
//! checks the parallel loop against the serial one on the same fixture.

use super::{temp_dir, write_transform_fixture};
use crate::project::{ProjectOptions, open_project};
use crate::transform_parallel::{PARALLEL_OVERRIDE, PARALLEL_RAN};
use crate::transform_state::TransformInput;

/// A fixture with planned classes, unplanned macro files and an unplanned guard diagnostic.
fn write_parallel_fixture(directory: &str) {
	write_transform_fixture(
		directory,
		"package.json",
		r#"{"name":"fixture-game","version":"1.0.0"}"#,
	);
	write_transform_fixture(
		directory,
		"tsconfig.json",
		r#"{"compilerOptions":{"experimentalDecorators":true,"strict":true,"rootDir":"src","outDir":"out"},"include":["src/**/*.ts"]}"#,
	);
	write_transform_fixture(
		directory,
		"src/macros.ts",
		&[
			r#"export type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };"#,
			"/** @metadata reflect */",
			"export declare const Service: () => FlameworkDecorator;",
			"export type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };",
			"export type Caller<M extends string> = { _flamework_macro_caller: M };",
			r#"export declare function inspect<T>(value?: Generic<T, "id">): string;"#,
			r#"export declare function label(value?: Caller<"text">): string;"#,
			r#"export declare function guardMacro<T>(value?: Generic<T, "guard">): unknown;"#,
		]
		.join("\n"),
	);
	for index in 0..4 {
		write_transform_fixture(
			directory,
			&format!("src/planned{index}.ts"),
			&[
				r#"import { Service } from "./macros";"#,
				&format!("export class Dependency{index} {{}}"),
				&format!(
					"@Service() export class Planned{index} {{ constructor(dependency: Dependency{index}) {{}} }}"
				),
			]
			.join("\n"),
		);
	}
	for index in 0..12 {
		let padding = "// padding\n".repeat(index);
		write_transform_fixture(
			directory,
			&format!("src/unplanned{index}.ts"),
			&format!(
				"{padding}import {{ inspect, label }} from \"./macros\";\ninterface Payload{index} {{ value: number }}\nexport const id{index} = inspect<Payload{index}>();\nexport const text{index} = label();\n"
			),
		);
	}
	write_transform_fixture(
		directory,
		"src/guards.ts",
		"import { guardMacro } from \"./macros\";\nexport const broken = guardMacro<`prefix-${string}`>();\nexport const fine = guardMacro<{ value: string }>();\n",
	);
}

/// The fixture transformed with the parallel loop forced on or off: each file printed, the
/// ordered diagnostics, the build info, and whether the parallel loop produced the result.
fn transform_fixture(parallel: bool) -> (Vec<(String, String)>, Vec<String>, String, bool) {
	let (_dir, directory) = temp_dir();
	write_parallel_fixture(&directory);
	rotor_tsrs::with_large_stack(|| {
		PARALLEL_OVERRIDE.set(Some(parallel));
		PARALLEL_RAN.set(false);
		let tsconfig = super::join(&[&directory, "tsconfig.json"]);
		let program = rotor_tsrs::load_project(
			std::path::Path::new(&tsconfig),
			&rotor_tsrs::ProjectOptions {
				checkers: Some(4),
				single_threaded: Some(false),
				skip_validation: true,
			},
		)
		.expect("load program")
		.program;
		let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
		let prefix = format!("{}/src/", rotor_support::gopath::to_slash(&directory));
		let mut files: Vec<_> = program
			.source_files()
			.iter()
			.copied()
			.filter(|file| file.file_name().starts_with(&prefix))
			.collect();
		files.sort_by(|left, right| left.file_name().cmp(right.file_name()));
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
			files,
			project: &mut project,
			analyses: Vec::new(),
			macro_runtime: None,
			print: None,
		})
		.expect("Transform");
		let printed = result
			.files
			.iter()
			.zip(&result.sources)
			.map(|(file, source)| {
				let name = file.file_name().trim_start_matches(&prefix).to_owned();
				let text = rotor_tsrs::printer::new_printer(
					rotor_tsrs::printer::PrinterOptions::default(),
					rotor_tsrs::printer::PrintHandlers::default(),
					Some(source.emit_context()),
				)
				.emit_source_file(*file);
				(name, text)
			})
			.collect();
		let diagnostics = result
			.diagnostics
			.iter()
			.map(|diagnostic| {
				let file = diagnostic
					.file
					.map(|file| file.file_name().trim_start_matches(&prefix).to_owned());
				format!(
					"{file:?}:{}:{}:{}",
					diagnostic.pos(),
					diagnostic.end(),
					diagnostic.string()
				)
			})
			.collect();
		let build_info =
			String::from_utf8(project.build_info().marshal_ordered_json().expect("marshal")).expect("utf-8");
		(printed, diagnostics, build_info, PARALLEL_RAN.get())
	})
}

#[test]
fn transform_parallel_loop_matches_serial_loop() {
	let (serial_printed, serial_diagnostics, serial_build_info, serial_ran) = transform_fixture(false);
	let (parallel_printed, parallel_diagnostics, parallel_build_info, parallel_ran) = transform_fixture(true);

	assert!(!serial_ran, "the serial run took the parallel loop");
	assert!(parallel_ran, "the parallel run fell back to the serial loop");
	assert!(
		serial_printed
			.iter()
			.any(|(_, text)| text.contains(r#"label("label()" as never)"#))
			&& serial_printed
				.iter()
				.any(|(_, text)| text.contains(r#"Reflect["defineMetadata"]"#)),
		"the fixture did not exercise macros and classes:\n{serial_printed:#?}"
	);
	assert!(!serial_diagnostics.is_empty(), "the fixture produced no diagnostics");
	assert_eq!(parallel_printed, serial_printed, "printed files differ");
	assert_eq!(parallel_diagnostics, serial_diagnostics, "diagnostics differ");
	assert_eq!(parallel_build_info, serial_build_info, "build info differs");
}
