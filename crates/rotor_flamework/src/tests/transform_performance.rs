//! `transform_performance_test.go`
//!
//! Go's `testing.AllocsPerRun` counts have no Rust counterpart without a counting global
//! allocator, so the two allocation tests keep their semantic assertions (unchanged leaves keep
//! their identity; class-only analysis of an unrelated import graph stays empty). Go's nil-state
//! and nil-node checks have no counterpart either: Rust has no nil state or node.

use std::cell::Cell;
use std::fmt::Write as _;

use rotor_tsrs::P;
use rotor_tsrs::ast::{Kind, Node, NodeFlags, TokenFlags};
use rotor_tsrs::checker::Type;

use super::{
	collect_call_expressions, find_expression_kind, join, new_transform_program, parse_ts, program_file, temp_dir,
	with_expression_fixture, write_transform_fixture,
};
use crate::project::{ProjectOptions, open_project};
use crate::transform::{call_may_be_flamework_macro, source_declares_flamework_expression_surface};
use crate::transform_analysis::analyze_flamework_classes;
use crate::transform_expressions::{
	transform_flamework_expression_with_runtime, transform_flamework_expressions_in_source_file_with_runtime,
};
use crate::transform_macro_types::{GuardBuildResult, MacroRuntime};
use crate::transform_state::{TransformInput, TransformState, new_transform_state_base};

/// Go's bare `&TransformState{}`: a state over an unrelated fixture, whose caches start empty.
#[test]
fn flamework_surface_caches_distinguish_source_files_at_the_same_path() {
	with_expression_fixture("export {};\n", |state, _| {
		for text in [
			"export const ordinary = 1;",
			"export interface Marker { _flamework_intrinsic: string; }",
			"export const ordinary = 2;",
			"export const api = Reflect.get;",
		] {
			let file = parse_ts("/same.ts", text);
			let want = text.contains("_flamework") || text.contains("Reflect.");
			for _ in 0..2 {
				assert_eq!(
					source_declares_flamework_expression_surface(state, file),
					want,
					"surface for {text:?}"
				);
			}
			assert_eq!(
				state.flamework_expression_surface(file),
				Some(want),
				"direct expression predicate was not cached for {text:?}"
			);
		}
	});
}

#[test]
fn flamework_surface_caches_follow_rebuilt_programs_with_the_same_project() {
	let (_dir, directory) = temp_dir();
	write_transform_fixture(
		&directory,
		"package.json",
		r#"{"name":"fixture-game","version":"1.0.0"}"#,
	);
	write_transform_fixture(
		&directory,
		"tsconfig.json",
		r#"{"compilerOptions":{"strict":true,"rootDir":"src","outDir":"out"},"include":["src/**/*.ts"]}"#,
	);
	write_transform_fixture(
		&directory,
		"src/main.ts",
		"import { enqueue } from './api';\nenqueue();\n",
	);
	rotor_tsrs::with_large_stack(|| {
		let mut project = open_project(&ProjectOptions {
			project_dir: directory.clone(),
			root_dir: "src".to_owned(),
			out_dir: "out".to_owned(),
			..ProjectOptions::default()
		})
		.expect("OpenProject");
		let mut previous = None;
		for is_macro in [false, true, false] {
			let declaration = if is_macro {
				"type Caller = string & { _flamework_macro_caller: 'text' };\nexport declare function enqueue(value?: Caller): void;\n"
			} else {
				"export declare function enqueue(): void;\n"
			};
			write_transform_fixture(&directory, "src/api.ts", declaration);
			let program = new_transform_program(&directory);
			let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
			let file = program_file(program, &directory, "src/main.ts");
			assert_ne!(Some(file), previous, "rebuild fixture reused the previous source AST");
			previous = Some(file);
			// Bare states are used by static addPaths analysis and intentionally do not
			// initialize transitive expression admission on a predicate query.
			{
				let mut bare = new_transform_state_base(program, &mut checker, &mut project, None, &[]);
				let call = collect_call_expressions(file)[0];
				assert!(
					!call_may_be_flamework_macro(&mut bare, call) && bare.flamework_expression_surfaces.is_none(),
					"bare-state predicate unexpectedly initialized reachability"
				);
			}
			let result = crate::transform::transform(TransformInput {
				program,
				checker: &mut checker,
				files: vec![file],
				project: &mut project,
				analyses: Vec::new(),
				macro_runtime: None,
				print: None,
			})
			.unwrap_or_else(|error| panic!("transform (macro={is_macro}) error={error}"));
			assert!(
				result.diagnostics.is_empty(),
				"transform (macro={is_macro}) diagnostics"
			);
			let printed = rotor_tsrs::printer::new_printer(
				rotor_tsrs::printer::PrinterOptions::default(),
				rotor_tsrs::printer::PrintHandlers::default(),
				Some(result.sources[0].emit_context()),
			)
			.emit_source_file(result.files[0]);
			let want_call = if is_macro {
				r#"enqueue("enqueue()" as never);"#
			} else {
				"enqueue();"
			};
			assert!(
				printed.contains(want_call) && result.sources[0].changed() == is_macro,
				"transform (macro={is_macro}) changed={} output:\n{printed}",
				result.sources[0].changed()
			);
		}
	});
}

#[test]
fn flamework_expression_leaves_preserve_identity_without_allocations() {
	with_expression_fixture("export const value = 1;\n", |state, file| {
		let parsed = find_expression_kind(file.as_node(), Kind::NumericLiteral);
		let factory = state.factory.clone();
		let mut leaves: Vec<P<Node>> = vec![
			parsed,
			factory.new_identifier("value"),
			factory.new_private_identifier("#value"),
			factory.new_string_literal("text", TokenFlags::None),
			factory.new_numeric_literal("2", TokenFlags::None),
			factory.new_big_int_literal("2n", TokenFlags::None),
			factory.new_regular_expression_literal("/value/", TokenFlags::None),
			factory.new_no_substitution_template_literal("text", TokenFlags::None),
		];
		for kind in [
			Kind::NullKeyword,
			Kind::TrueKeyword,
			Kind::FalseKeyword,
			Kind::ThisKeyword,
			Kind::SuperKeyword,
			Kind::ImportKeyword,
		] {
			leaves.push(factory.new_keyword_expression(kind));
		}
		for leaf in leaves {
			let parent = leaf.parent();
			for _ in 0..10 {
				let result = transform_flamework_expression_with_runtime(state, leaf, &MacroRuntime::default())
					.unwrap_or_else(|error| panic!("{:?}: {error}", leaf.kind()));
				assert!(
					result.expression == leaf && result.prerequisites.is_empty() && result.imports.is_empty(),
					"{:?}: leaf transform changed the leaf",
					leaf.kind()
				);
			}
			assert!(
				leaf.parent() == parent && state.diagnostics.is_empty(),
				"{:?}: leaf transform changed parent identity or diagnostics",
				leaf.kind()
			);
		}
	});
}

#[test]
fn analyze_flamework_classes_allocations_ignore_unrelated_import_graph() {
	let (_dir, directory) = temp_dir();
	write_transform_fixture(
		&directory,
		"package.json",
		r#"{"name":"fixture-game","version":"1.0.0"}"#,
	);
	write_transform_fixture(
		&directory,
		"tsconfig.json",
		r#"{"compilerOptions":{"noLib":true,"rootDir":"src","outDir":"out"},"files":["src/main.ts"]}"#,
	);
	write_transform_fixture(&directory, "src/main.ts", "export const value = 1;\n");
	rotor_tsrs::with_large_stack(|| {
		let small = new_transform_program(&directory);
		for index in 0..128 {
			let mut source = String::new();
			if index > 0 {
				let _ = writeln!(source, "import './file{}';", index - 1);
			}
			source.push_str("export const value = 1;\n");
			write_transform_fixture(&directory, &format!("src/file{index}.ts"), &source);
		}
		write_transform_fixture(
			&directory,
			"tsconfig.json",
			r#"{"compilerOptions":{"noLib":true,"rootDir":"src","outDir":"out"},"files":["src/main.ts","src/file127.ts"]}"#,
		);
		let large = new_transform_program(&directory);
		let mut project = open_project(&ProjectOptions {
			project_dir: directory.clone(),
			root_dir: "src".to_owned(),
			out_dir: "out".to_owned(),
			..ProjectOptions::default()
		})
		.expect("OpenProject");
		let name = rotor_support::gopath::to_slash(&join(&[&directory, "src/main.ts"])).into_owned();
		for program in [small, large] {
			let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
			let file = program.get_source_file(&name).expect("main.ts");
			for _ in 0..3 {
				let analysis = analyze_flamework_classes(&mut TransformInput {
					program,
					checker: &mut checker,
					files: vec![file],
					project: &mut project,
					analyses: Vec::new(),
					macro_runtime: None,
					print: None,
				})
				.expect("AnalyzeFlameworkClasses");
				assert!(
					analysis.files().is_empty() && analysis.diagnostics().is_empty(),
					"class analysis = {analysis:?}; want empty analysis"
				);
			}
		}
	});
}

thread_local! {
	static GUARD_CALLS: Cell<usize> = const { Cell::new(0) };
}

#[expect(
	clippy::unnecessary_wraps,
	reason = "a GuardBuilder, whose signature returns a Result"
)]
fn numbered_guard(state: &mut TransformState<'_>, _: P<Node>, _: P<Type>) -> Result<GuardBuildResult, crate::Error> {
	let calls = GUARD_CALLS.get() + 1;
	GUARD_CALLS.set(calls);
	let factory = &state.factory;
	let alloc = rotor_tsrs::tscore::alloc_str;
	let prepare = factory.new_call_expression(
		factory.new_identifier(alloc(&format!("prepare{calls}"))),
		None,
		None,
		factory.new_node_list(Vec::new()),
		NodeFlags::None,
	);
	Ok(GuardBuildResult {
		expression: Some(factory.new_identifier(alloc(&format!("generatedGuard{calls}")))),
		statements: vec![factory.new_expression_statement(prepare)],
	})
}

#[test]
fn flamework_expression_leaves_preserve_nested_prerequisite_order() {
	let (printed, calls, diagnostics) = with_expression_fixture(
		r#"
type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };
declare function guardMacro(value?: Generic<string, "guard">): unknown;
declare function consume(value: unknown): void;
consume({ left: guardMacro(), right: [guardMacro(), "literal"] });
"#,
		|state, file| {
			let runtime = MacroRuntime {
				build_guard: Some(numbered_guard),
				..MacroRuntime::default()
			};
			let transformed = transform_flamework_expressions_in_source_file_with_runtime(state, file, &runtime)
				.expect("transformFlameworkExpressionsInSourceFileWithRuntime");
			let printed = rotor_tsrs::printer::new_printer(
				rotor_tsrs::printer::PrinterOptions::default(),
				rotor_tsrs::printer::PrintHandlers::default(),
				Some(state.emit_context),
			)
			.emit_source_file(transformed);
			(printed, GUARD_CALLS.get(), state.diagnostics.len())
		},
	);

	let want = "prepare1();\nprepare2();\nconsume({ left: guardMacro(generatedGuard1 as never), right: [guardMacro(generatedGuard2 as never), \"literal\"] });\n";
	assert!(
		calls == 2 && printed.ends_with(want) && diagnostics == 0,
		"nested transform calls={calls} diagnostics={diagnostics} output:\n{printed}\nwant suffix:\n{want}"
	);
}
