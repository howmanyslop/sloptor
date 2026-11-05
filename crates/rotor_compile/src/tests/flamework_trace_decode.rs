//! `flamework_trace_decode_test.go`. Go's benchmark has no port.

use std::sync::Mutex;

use rotor_support::gopath;
use rotor_tsrs::printer::{PrintHandlers, PrinterOptions, new_printer, new_text_writer};
use rotor_tsrs::sourcemap::{Generator, new_generator};
use rotor_tsrs::tspath::ComparePathsOptions;

use super::large_stack;
use crate::flamework_pipeline::{prepare_flamework_pipeline, print_flamework_source};
use crate::project::{ProjectOptions, lock, new_project_program_with_options, project_source_files};
use crate::source_trace::{SourcePosition, SourceTraceMap};

/// `generatedFlameworkTrace`: the generator, whose raw map the test reads.
fn generated_flamework_trace(text: &str) -> Generator {
	let mut generator = new_generator(
		"printed.ts",
		"",
		"/project",
		ComparePathsOptions {
			use_case_sensitive_file_names: true,
			current_directory: "/project".to_owned(),
		},
	);
	let source = generator.add_source("/project/original.ts");
	let name = generator.add_name("greeting");
	for result in [
		generator.set_source_content(source, text),
		generator.add_generated_mapping(0, 0),
		generator.add_named_source_mapping(0, 2, source, 0, 1, name),
		// A backtracking source position emits a duplicate generated position.
		generator.add_source_mapping(0, 2, source, 0, 0),
		generator.add_source_mapping(0, 5, source, 1, 2),
		generator.add_generated_mapping(0, 8),
		generator.add_source_mapping(2, 1, source, 1, 4),
		generator.add_source_mapping(3, 0, source, 2, 0),
	] {
		result.unwrap_or_else(|error| panic!("{error}"));
	}
	generator
}

#[test]
fn flamework_trace_direct_matches_generated_json() {
	let position = |line, column| Some(SourcePosition { line, column });
	for (name, text) in [
		("LF", "first\nsecond\nthird\n".to_owned()),
		("CRLFNonBMP", "😀first\r\n𐐀second\r\nthird\r\n".to_owned()),
		("LargeInlineSource", "const greeting = \"😀<&>\";\r\n".repeat(32768)),
	] {
		let mut generator = generated_flamework_trace(&text);
		// RawSourceMap must commit the final pending mapping; the test expects it.
		let raw = generator.raw_source_map();
		let encoded = generator.string();
		let old =
			SourceTraceMap::new(&encoded, "/authored/😀.ts", &text).unwrap_or_else(|error| panic!("{name}: {error}"));
		let direct = SourceTraceMap::from_mappings(i64::from(raw.version), &raw.mappings, "/authored/😀.ts", &text)
			.unwrap_or_else(|error| panic!("{name}: {error}"));
		let want = SourceTraceMap::for_test(
			"/authored/😀.ts",
			&text,
			&[(0, 2, 0, 1), (0, 2, 0, 0), (0, 5, 1, 2), (2, 1, 1, 4), (3, 0, 2, 0)],
		);
		for (route, trace) in [("JSON", &old), ("direct", &direct)] {
			assert_eq!(*trace, want, "{name}: {route} trace");
			assert!(
				trace.original_source_file_name() == "/authored/😀.ts" && trace.original_source_text() == text,
				"{name}: {route} lost original filename/text"
			);
			// Go's negative queries have no `usize` spelling; the port keeps the rest.
			for (line, column, want) in [
				(0, 0, None),
				(0, 1, None),
				(0, 2, position(0, 0)),
				(0, 4, position(0, 0)),
				(0, 5, position(1, 2)),
				// Unmapped segments and empty lines retain the preceding source mapping.
				(0, 8, position(1, 2)),
				(1, 0, position(1, 2)),
				(2, 0, position(1, 2)),
				(2, 1, position(1, 4)),
				(3, 0, position(2, 0)),
				(i32::MAX as usize, i32::MAX as usize, position(2, 0)),
			] {
				let query = SourcePosition { line, column };
				assert_eq!(
					trace.original_position(query),
					want,
					"{name}: {route} OriginalPosition({query:?})"
				);
			}
		}
	}
}

/// A validation case: its name, version and mappings, the trace mappings it gives as (generated
/// line, generated column, source line, source column), and its error.
type ValidationCase<'a> = (&'a str, i64, String, Vec<(u32, u32, u32, u32)>, &'a str);

#[test]
#[expect(clippy::too_many_lines, reason = "one Go test")]
fn flamework_trace_direct_validation_parity() {
	let coordinate_map = |generated_column: u32, source_line: u32, source_column: u32| -> String {
		let mut generator = new_generator("printed.ts", "", "/project", ComparePathsOptions::default());
		let source = generator.add_source("/project/original.ts");
		generator
			.add_source_mapping(0, generated_column, source, source_line, source_column)
			.unwrap_or_else(|error| panic!("{error}"));
		generator.raw_source_map().mappings
	};
	let max = i32::MAX.unsigned_abs();
	let overflow_error = "parse transformer trace map: coordinate exceeds 32-bit range";
	let cases: Vec<ValidationCase<'_>> = vec![
		("empty", 3, String::new(), vec![], ""),
		("unmapped only", 3, "A,K;C".to_owned(), vec![], ""),
		(
			"zero version",
			0,
			String::new(),
			vec![],
			"parse transformer trace map: version = 0, want 3",
		),
		(
			"version before VLQ",
			2,
			"%".to_owned(),
			vec![],
			"parse transformer trace map: version = 2, want 3",
		),
		(
			"invalid character",
			3,
			"%".to_owned(),
			vec![],
			"parse transformer trace map mappings: Invalid character in VLQ",
		),
		(
			"truncated VLQ",
			3,
			"g".to_owned(),
			vec![],
			"parse transformer trace map mappings: Error in decoding base64VLQFormatDecode, past the mapping string",
		),
		(
			"missing source line",
			3,
			"AA".to_owned(),
			vec![],
			"parse transformer trace map mappings: Unsupported Format: No entries after sourceIndex",
		),
		(
			"missing source column",
			3,
			"AAA".to_owned(),
			vec![],
			"parse transformer trace map mappings: Unsupported Format: No entries after sourceLine",
		),
		(
			"negative generated column",
			3,
			"D".to_owned(),
			vec![],
			"parse transformer trace map mappings: Invalid generatedCharacter found",
		),
		(
			"negative source line",
			3,
			"AADA".to_owned(),
			vec![],
			"parse transformer trace map mappings: Invalid sourceLine found",
		),
		(
			"32 bit boundary",
			3,
			coordinate_map(max, max, max),
			vec![(0, max, max, max)],
			"",
		),
		(
			"generated column overflow",
			3,
			coordinate_map(max + 1, 0, 0),
			vec![],
			overflow_error,
		),
		(
			"source line overflow",
			3,
			coordinate_map(0, max + 1, 0),
			vec![],
			overflow_error,
		),
		(
			"source column overflow",
			3,
			coordinate_map(0, 0, max + 1),
			vec![],
			overflow_error,
		),
	];
	for (name, version, mappings, want, want_error) in cases {
		// Go marshals a `RawSourceMap`; the trace reads only its version and mappings.
		let encoded = format!(
			r#"{{"version":{version},"file":"","sources":null,"names":null,"mappings":{}}}"#,
			crate::gojson::string(&mappings)
		);
		let old = SourceTraceMap::new(&encoded, "original.ts", "😀\r\n");
		let direct = SourceTraceMap::from_mappings(version, &mappings, "original.ts", "😀\r\n");
		for (route, result) in [("JSON", &old), ("direct", &direct)] {
			let got = result.as_ref().err().map_or("", String::as_str);
			assert_eq!(got, want_error, "{name}: {route} error");
		}
		assert_eq!(old, direct, "{name}: JSON trace, direct trace");
		if want_error.is_empty() {
			let direct = direct.expect("a trace");
			assert_eq!(
				direct,
				SourceTraceMap::for_test("original.ts", "😀\r\n", &want),
				"{name}: mappings"
			);
		}
	}
}

#[test]
fn flamework_trace_external_json_errors() {
	for (raw, want) in [
		("", "parse transformer trace map: unexpected end of JSON input"),
		("{", "parse transformer trace map: unexpected end of JSON input"),
		(
			"[]",
			"parse transformer trace map: json: cannot unmarshal array into Go value of type compile.rawSourceMap",
		),
		(
			r#"{"version":"3"}"#,
			"parse transformer trace map: json: cannot unmarshal string into Go struct field rawSourceMap.version of type int",
		),
		(
			r#"{"version":3,"mappings":1}"#,
			"parse transformer trace map: json: cannot unmarshal number into Go struct field rawSourceMap.mappings of type string",
		),
		(
			r#"{"version":3,"sources":[1]}"#,
			"parse transformer trace map: json: cannot unmarshal number into Go struct field rawSourceMap.sources of type string",
		),
		(
			r#"{"version":3,"sourcesContent":[1]}"#,
			"parse transformer trace map: json: cannot unmarshal number into Go struct field rawSourceMap.sourcesContent of type string",
		),
		(
			r#"{"version":3}x"#,
			"parse transformer trace map: invalid character 'x' after top-level value",
		),
		("null", "parse transformer trace map: version = 0, want 3"),
	] {
		let result = SourceTraceMap::new(raw, "original.ts", "text");
		assert_eq!(
			result.as_ref().err().map(String::as_str),
			Some(want),
			"newSourceTraceMap({raw:?}) = {result:?}"
		);
	}
}

/// What the print callback recorded for one file: the printed route's text and trace, then the
/// JSON route's, then the authored text and file name.
type Routes = (String, SourceTraceMap, String, SourceTraceMap, String, String);

#[test]
fn flamework_trace_printed_source_matches_json_route() {
	let temp = super::write_project("@scope/sparse-native-update", "");
	std::fs::remove_file(temp.join("src/main.ts")).expect("remove main.ts");
	temp.write(
		"src/main.ts",
		"// 😀 authored\r\nexport const greeting = \"𐐀\";\r\nexport function identity(value: number) { return value; }\r\n",
	);
	let routes: Mutex<Vec<Routes>> = Mutex::new(Vec::new());
	large_stack(|| {
		let (dir, program) = new_project_program_with_options(&temp.dir(), "", &ProjectOptions::default())
			.unwrap_or_else(|failure| panic!("newProjectProgram: {} ({:?})", failure.error, failure.messages()));
		let sources = project_source_files(program);
		temp.write("rotor.toml", "[flamework]\n");
		let pipeline = match prepare_flamework_pipeline(&dir, program, &ProjectOptions::default()) {
			Ok(Some(pipeline)) => pipeline,
			Ok(None) => panic!("prepareFlameworkPipeline returned nil pipeline"),
			Err(failure) => panic!("prepareFlameworkPipeline: {} ({:?})", failure.error, failure.messages()),
		};
		let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
		let mut project = lock(&pipeline.project);
		// Go prints after the transform returns; the port prints in the callback, while each
		// file's emit context is current.
		let result = rotor_flamework::transform(rotor_flamework::TransformInput {
			program,
			checker: &mut checker,
			files: sources,
			project: &mut project,
			analyses: Vec::new(),
			macro_runtime: None,
			print: Some(Box::new(|metadata: &rotor_flamework::SourceMetadata| {
				let file = metadata.transformed();
				let overlay = print_flamework_source(file, metadata)
					.map_err(|error| rotor_flamework::Error::msg(error.to_string()))?;
				// The old native operation is the control: same printer/options and pending-map
				// flush, followed by Marshal and the external JSON constructor.
				let mut writer = new_text_writer("\n", 0);
				let directory = gopath::dir(file.file_name());
				let mut generator = new_generator(
					&gopath::base(file.file_name()),
					"",
					&directory,
					ComparePathsOptions {
						use_case_sensitive_file_names: true,
						current_directory: directory.clone(),
					},
				);
				new_printer(
					PrinterOptions {
						source_map: true,
						inline_sources: true,
						..PrinterOptions::default()
					},
					PrintHandlers::default(),
					Some(metadata.emit_context()),
				)
				.write(
					file.as_node(),
					Some(metadata.original()),
					&mut *writer,
					Some(&mut generator),
				);
				let encoded = generator.string();
				let old = SourceTraceMap::new(
					&encoded,
					metadata.trace().original_file_name(),
					metadata.trace().original_text(),
				)
				.map_err(rotor_flamework::Error::msg)?;
				lock(&routes).push((
					overlay.text,
					(*overlay.trace.expect("a trace")).clone(),
					writer.string(),
					old,
					metadata.original().text().to_owned(),
					metadata.file_name().to_owned(),
				));
				Ok(None)
			})),
		});
		drop(project);
		drop(checker);
		let result = result.unwrap_or_else(|error| panic!("Transform: {error}"));
		assert!(result.diagnostics.is_empty(), "Transform: {:?}", result.diagnostics);
	});
	let routes = routes.into_inner().expect("routes");
	assert!(!routes.is_empty(), "no source printed");
	for (text, trace, want_text, old, original_text, file_name) in routes {
		assert_eq!(text, want_text, "printed source, JSON route");
		assert!(
			trace == old
				&& trace.mapping_count() != 0
				&& trace.original_source_text() == original_text
				&& trace.original_source_file_name() == file_name,
			"printed trace differs from JSON route or lost authored source: {file_name}"
		);
	}
}
