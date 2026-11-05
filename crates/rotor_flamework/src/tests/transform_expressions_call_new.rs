//! `transform_expressions_call_new_test.go`

use rotor_tsrs::P;
use rotor_tsrs::ast::{Kind, NodeFlags, SourceFile};

use super::{StateSpec, collect_call_expressions, parse_ts, print_source_file, with_expression_fixture, with_state};
use crate::ErrorKind;
use crate::transform::{
	call_may_be_flamework_macro, source_needs_flamework_expression_transform, transform_source_file,
};
use crate::transform_expressions::{
	default_flamework_macro_runtime, transform_flamework_expressions_in_source_file,
	transform_flamework_expressions_in_source_file_with_runtime,
};
use crate::transform_macro_types::{GuardBuildResult, MacroRuntime};
use crate::transform_state::TransformState;

/// Go's `<prefix>\d+ as never\)` match.
fn has_injected_line(printed: &str, prefix: &str) -> bool {
	printed.match_indices(prefix).any(|(start, _)| {
		let rest = &printed[start + prefix.len()..];
		let digits = rest.len()
			- rest
				.trim_start_matches(|character: char| character.is_ascii_digit())
				.len();
		digits > 0 && rest[digits..].starts_with(" as never)")
	})
}

fn assert_reparses(printed: &str) {
	assert!(
		parse_ts("/reparsed.ts", printed).diagnostics().is_empty(),
		"reparse diagnostics:\n{printed}"
	);
}

/// The file transformed by `transformSourceFile` with the state's runtime.
fn transform_file(state: &mut TransformState<'_>, file: P<SourceFile>) -> P<SourceFile> {
	let runtime = state.macro_runtime();
	transform_source_file(state, file, &runtime).expect("transformSourceFile")
}

fn expressions_printed(source: &str) -> String {
	with_expression_fixture(source, |state, file| {
		print_source_file(
			transform_flamework_expressions_in_source_file(state, file)
				.expect("transformFlameworkExpressionsInSourceFile"),
		)
	})
}

#[test]
fn transform_flamework_expressions_in_source_file_preserves_call_child_recursion() {
	let printed = expressions_printed(
		r#"
import { BaseComponent } from "./base";
interface Attributes { count: number }
class Counter extends BaseComponent<Attributes> {}
declare function next(): number;
declare function consume(value: number): void;
const counter = new Counter();
consume(counter.attributes.count = next());
"#,
	);

	assert!(
		printed.contains(r#"consume(counter[SYMBOL_ATTRIBUTE_SETTER]("count", next()));"#),
		"transformed source did not preserve call child recursion:\n{printed}"
	);
}

/// Go's parent walk: every child's parent is the node visiting it.
fn parents_match(node: rotor_tsrs::P<rotor_tsrs::ast::Node>) -> bool {
	node.iter_children()
		.into_iter()
		.all(|child| child.parent() == Some(node) && parents_match(child))
}

#[test]
fn transform_source_file_preserves_checker_parents_when_source_file_is_reused() {
	let source = r#"
import { BaseComponent } from "./base";
interface Attributes { count: number }
class Counter extends BaseComponent<Attributes> {}
declare function next(): number;
declare function consume<T>(value: T): void;
const counter = new Counter();
class Host<T> {
	accept(value: T): void {}
	method(value: T): void {
		consume(counter.attributes.count = next());
		this.accept(value);
	}
}
"#;
	let printed = with_expression_fixture(source, |state, source_file| {
		let accept_call = collect_call_expressions(source_file)
			.into_iter()
			.find(|call| {
				call.expression().is_some_and(|expression| {
					rotor_tsrs::ast::is_property_access_expression(expression)
						&& expression.name().is_some_and(|name| name.text() == "accept")
				})
			})
			.expect("accept call not found");
		state.emit_context.add_synthetic_leading_comment(
			accept_call.parent().expect("accept statement"),
			Kind::SingleLineCommentTrivia,
			" (Flamework) Host metadata",
			true,
		);
		let first_transformed = transform_file(state, source_file);
		assert_ne!(
			first_transformed, source_file,
			"first transform reused source file after a structural change"
		);
		assert!(
			first_transformed.as_node().flags().intersects(NodeFlags::Reparsed),
			"first transform did not DeepCloneReparse the changed source file"
		);
		assert!(
			parents_match(first_transformed.as_node()),
			"transformed tree parents mismatch"
		);
		let first_printed = rotor_tsrs::printer::new_printer(
			rotor_tsrs::printer::PrinterOptions::default(),
			rotor_tsrs::printer::PrintHandlers::default(),
			Some(state.emit_context),
		)
		.emit_source_file(first_transformed);
		assert!(
			first_printed.contains("// (Flamework) Host metadata"),
			"first transform lost generated Flamework comment:\n{first_printed}"
		);
		let containing_class = rotor_tsrs::ast::find_ancestor(accept_call, rotor_tsrs::ast::is_class_like)
			.expect("accept call has no containing class");
		assert!(
			!rotor_tsrs::ast::node_is_synthesized(containing_class) && containing_class.symbol().is_some(),
			"containing class, want bound parse-tree declaration"
		);
		let fresh = Box::leak(rotor_tsrs::checker::new_checker(state.program));
		state.checker = fresh;
		print_source_file(transform_flamework_expressions_in_source_file(state, source_file).expect("second transform"))
	});

	assert!(
		printed.contains(r#"consume(counter[SYMBOL_ATTRIBUTE_SETTER]("count", next()));"#),
		"second transform did not preserve Flamework attribute transform:\n{printed}"
	);
}

/// Whether the prefilter admits the fixture, and whether `transformSourceFile` returned it as is.
fn unchanged_source(source: &str) -> (bool, bool) {
	with_expression_fixture(source, |state, source_file| {
		let needs = source_needs_flamework_expression_transform(state, source_file);
		(needs, transform_file(state, source_file) == source_file)
	})
}

#[test]
fn transform_source_file_returns_original_source_file_when_no_structural_change_occurs() {
	let (needs, reused) = unchanged_source("export const stable = true;\n");

	assert!(!needs, "no-call fixture unexpectedly entered the expression walk");
	assert!(reused, "transformSourceFile() did not return the original source file");
}

#[test]
fn call_may_be_flamework_macro_admits_type_args_known_names_and_surface() {
	// Ordinary calls, type-argument macros, and same-file Flamework surface: only
	// Flamework-capable calls pay GetResolvedSignature.
	let admitted = |source: &str| {
		with_expression_fixture(source, |state, file| {
			collect_call_expressions(file)
				.into_iter()
				.map(|call| call_may_be_flamework_macro(state, call))
				.collect::<Vec<_>>()
		})
	};
	let ordinary = admitted(
		"\nexport function add(left: number, right: number) { return left + right; }\nexport const value = add(1, 2);\n",
	);
	let typed = admitted("\ndeclare function inspect<T>(value?: T): T;\ninspect<string>();\n");
	let surface = admitted(
		r#"
type DeclarationUid = { readonly _flamework_intrinsic: ["declaration-uid"] };
/** @metadata macro */
declare function declarationUid(value?: DeclarationUid): unknown;
declarationUid();
"#,
	);
	let middleware = admitted(
		r"
interface Configuration { readonly middleware: number; }
/** @metadata macro {@link config intrinsic-middleware} */
declare function createServer(config: Configuration): void;
createServer({ middleware: 1 });
",
	);

	assert!(
		!ordinary.contains(&true),
		"ordinary add() was treated as a Flamework macro"
	);
	assert!(typed[0], "inspect<string>() should admit type-argument macros");
	assert!(surface[0], "declarationUid() should admit same-file Flamework surface");
	assert!(
		middleware[0],
		"createServer() should admit known Flamework callee names"
	);
}

#[test]
fn transform_source_file_skips_expression_walk_when_file_has_no_flamework_surface() {
	let (needs, reused) = unchanged_source(
		"\nexport function add(left: number, right: number) { return left + right; }\nexport function run() {\n\treturn add(add(1, 2), add(3, 4));\n}\n",
	);

	assert!(!needs, "fixture unexpectedly matched Flamework expression surface");
	assert!(reused, "transformSourceFile() did not reuse the original source file");
}

#[test]
fn transform_source_file_walks_expressions_when_macro_is_declared_in_an_imported_module() {
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
				"src/macros.ts",
				[
					"type Caller<M extends string> = { _flamework_macro_caller: M };",
					r#"export declare function callerText(value?: Caller<"text">): string;"#,
				]
				.join("\n"),
			),
			(
				"src/main.ts",
				[
					r#"import { callerText } from "./macros";"#,
					"export const label = callerText();",
				]
				.join("\n"),
			),
		],
		"src/main.ts",
	);
	let (needs, printed) = with_state(&spec, |state, source_file| {
		let needs = source_needs_flamework_expression_transform(state, source_file);
		(needs, print_source_file(transform_file(state, source_file)))
	});

	assert!(
		needs,
		"call site importing a macro declaration was rejected by the expression prefilter"
	);
	assert!(
		printed.contains(r#"callerText("callerText()" as never)"#),
		"imported caller macro was not expanded:\n{printed}"
	);
}

#[test]
fn transform_preserves_unchanged_source_metadata_when_no_structural_change_occurs() {
	let (files_original, metadata_unchanged) =
		with_expression_fixture("export const stable = true;\n", |base, source_file| {
			let result = crate::transform::transform(crate::transform_state::TransformInput {
				program: base.program,
				checker: base.checker,
				files: vec![source_file],
				project: base.project,
				analyses: Vec::new(),
				macro_runtime: None,
				print: None,
			})
			.expect("Transform");
			(
				result.files == [source_file],
				result.sources.len() == 1
					&& result.sources[0].original() == source_file
					&& result.sources[0].transformed() == source_file
					&& !result.sources[0].changed(),
			)
		});

	assert!(files_original, "Transform() files, want original source file");
	assert!(
		metadata_unchanged,
		"Transform() source metadata, want unchanged original source"
	);
}

#[test]
fn transform_flamework_expressions_in_source_file_expands_call_and_new_macro_arguments() {
	let printed = expressions_printed(
		r#"
type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };
declare function callMacro(value?: Generic<"call", "text">): void;
declare class NewMacro {
	constructor(value?: Generic<"new", "text">);
}
callMacro();
new NewMacro();
"#,
	);

	for want in [
		r#"callMacro("\"call\"" as never);"#,
		r#"new NewMacro("\"new\"" as never);"#,
	] {
		assert!(
			printed.contains(want),
			"transformed source missing {want:?}:\n{printed}"
		);
	}
	assert_reparses(&printed);
}

#[expect(
	clippy::unnecessary_wraps,
	reason = "a GuardBuilder, whose signature returns a Result"
)]
fn prepare_guard(
	state: &mut TransformState<'_>,
	_: rotor_tsrs::P<rotor_tsrs::ast::Node>,
	_: rotor_tsrs::P<rotor_tsrs::checker::Type>,
) -> Result<GuardBuildResult, crate::Error> {
	let factory = &state.factory;
	let prepare = factory.new_call_expression(
		factory.new_identifier("prepareGuard"),
		None,
		None,
		factory.new_node_list(Vec::new()),
		NodeFlags::None,
	);
	Ok(GuardBuildResult {
		expression: Some(factory.new_identifier("generatedGuard")),
		statements: vec![factory.new_expression_statement(prepare)],
	})
}

#[test]
fn transform_flamework_expressions_in_source_file_with_runtime_inserts_macro_prerequisites_before_owning_statement() {
	let printed = with_expression_fixture(
		r#"
type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };
declare function prepareGuard(): void;
declare function guardMacro(value?: Generic<string, "guard">): void;
guardMacro();
"#,
		|state, file| {
			let runtime = MacroRuntime {
				build_guard: Some(prepare_guard),
				..MacroRuntime::default()
			};
			print_source_file(
				transform_flamework_expressions_in_source_file_with_runtime(state, file, &runtime)
					.expect("transformFlameworkExpressionsInSourceFileWithRuntime"),
			)
		},
	);

	let prerequisite = printed.find("prepareGuard();");
	let owner = printed.find("guardMacro(generatedGuard as never);");
	assert!(
		prerequisite.is_some() && owner.is_some() && prerequisite <= owner,
		"macro prerequisite was not emitted before its owner:\n{printed}"
	);
	assert_reparses(&printed);
}

#[test]
fn transform_flamework_expressions_in_source_file_with_runtime_returns_typed_malformed_macro_error() {
	let printed = with_expression_fixture(
		r"
type Invalid<T> = { _flamework_macro_generic: [T, number] };
declare function invalidMacro(value?: Invalid<string>): void;
invalidMacro();
",
		|state, file| {
			print_source_file(
				transform_flamework_expressions_in_source_file_with_runtime(state, file, &MacroRuntime::default())
					.expect("transformFlameworkExpressionsInSourceFileWithRuntime"),
			)
		},
	);

	assert!(
		printed.contains("invalidMacro();"),
		"malformed generic call was not preserved:\n{printed}"
	);
}

#[test]
fn flamework_macro_random_index_returns_typed_error_for_nonpositive_upper_bound() {
	let runtime = default_flamework_macro_runtime();

	let error = runtime.random_index.expect("random index")(0).expect_err("RandomIndex(0)");

	assert!(error.is(ErrorKind::InvalidMacroRandomIndex), "{error}");
}

#[test]
fn transform_flamework_expressions_in_source_file_does_not_reenter_generated_guard_calls() {
	let printed = with_expression_fixture(
		r#"
type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };
interface Leaf { id: string; value: number }
interface Repeated { first: Leaf; second: Leaf; third: Leaf }
declare function guardMacro(value?: Generic<Repeated, "guard">): unknown;
guardMacro();
"#,
		|state, file| {
			state.project.config.optimizations.guard_generation_dedup_limit = Some(2);
			print_source_file(
				transform_flamework_expressions_in_source_file(state, file)
					.expect("transformFlameworkExpressionsInSourceFile"),
			)
		},
	);

	assert!(
		printed.contains("guardMacro(t[") && printed.contains("as never)"),
		"generated guard macro was not preserved as a call argument:\n{printed}"
	);
	assert_reparses(&printed);
}

#[test]
fn transform_flamework_expressions_in_source_file_imports_prelude_for_flamework_create_guard() {
	let spec = StateSpec::new(
		vec![
			("package.json", r#"{"name":"fixture-game","version":"1.0.0"}"#.to_owned()),
			(
				"tsconfig.json",
				r#"{"compilerOptions":{"strict":true,"rootDir":"src","outDir":"out"},"files":["src/shared/guards.ts"]}"#
					.to_owned(),
			),
			(
				"src/shared/guards.ts",
				[
					"type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };",
					"declare namespace Flamework {",
					"  /** @metadata macro */",
					r#"  function createGuard<T>(meta?: Generic<T, "guard">): unknown;"#,
					"}",
					"interface Leaf { readonly id: string; readonly value: number }",
					"interface Repeated { readonly first: Leaf; readonly second: Leaf; readonly third: Leaf }",
					"export const repeatedGuard = Flamework.createGuard<Repeated>();",
					r#"export const unionGuard = Flamework.createGuard<"ready" | number | undefined>();"#,
				]
				.join("\n"),
			),
		],
		"src/shared/guards.ts",
	);
	let (printed, diagnostics) = with_state(&spec, |state, file| {
		state.project.config.optimizations.guard_generation_dedup_limit = Some(2);
		let printed = print_source_file(
			transform_flamework_expressions_in_source_file(state, file)
				.expect("transformFlameworkExpressionsInSourceFile"),
		);
		let diagnostics: Vec<String> = state
			.ordered_diagnostics()
			.iter()
			.map(|diagnostic| diagnostic.string().to_owned())
			.collect();
		(printed, diagnostics)
	});

	let guard_import = r#"import { t } from "@rbxts/t";"#;
	assert_eq!(
		printed.matches(guard_import).count(),
		1,
		"guard import count:\n{printed}"
	);
	assert!(
		!diagnostics.is_empty(),
		"guard diagnostics are empty, want missing-core warnings"
	);
	assert!(
		diagnostics
			.iter()
			.all(|diagnostic| diagnostic == "Flamework core was not found, guard generation may not work."),
		"guard diagnostics = {diagnostics:?}, want only exact missing-core warnings"
	);
	let prerequisite = printed.find("const dedup =");
	let consumer = printed.find("Flamework.createGuard<Repeated>(");
	assert!(
		prerequisite.is_some() && consumer.is_some() && prerequisite <= consumer,
		"guard prerequisite was not emitted before its createGuard consumer:\n{printed}"
	);
	assert_eq!(
		printed.matches("Flamework.createGuard<").count(),
		2,
		"createGuard calls were not preserved:\n{printed}"
	);
	assert_reparses(&printed);
}

#[expect(clippy::too_many_lines, reason = "one Go test, ported whole")]
#[test]
fn transform_source_file_injects_caller_line_through_transitive_type_only_imports() {
	// A three-module fixture: store.ts declares Modding.Caller<"line"> via _flamework_macro_caller
	// and the overloaded enqueue; context.ts uses import type for Store and re-exports an
	// interface; main.ts uses only import type for Context, destructures, calls the 3-argument
	// overload, and has no Flamework surface text. Diamond, cycle and disconnected files sit in
	// the same program.
	let lines = |lines: &[&str]| lines.join("\n");
	let spec = StateSpec::new(
		vec![
			("package.json", r#"{"name":"fixture-game","version":"1.0.0"}"#.to_owned()),
			(
				"tsconfig.json",
				r#"{"compilerOptions":{"strict":true,"rootDir":"src","outDir":"out","moduleResolution":"node"},"include":["src/**/*.ts"]}"#
					.to_owned(),
			),
			(
				"src/store.ts",
				lines(&[
					"type CallerMetadata = { line: number; };",
					"declare namespace Modding {",
					"    export type Caller<M extends keyof CallerMetadata> = CallerMetadata[M] & { _flamework_macro_caller: M };",
					"}",
					"export interface Store {",
					r#"    enqueue(player: unknown, transform: unknown, callsiteId?: Modding.Caller<"line">): void;"#,
					r#"    enqueue(player: unknown, key: string, transform: unknown, callsiteId?: Modding.Caller<"line">): void;"#,
					"}",
				]),
			),
			(
				"src/context.ts",
				lines(&[
					r#"import type { Store } from "./store";"#,
					"export interface Context {",
					"    data: Store;",
					"}",
				]),
			),
			(
				"src/main.ts",
				lines(&[
					r#"import type { Context } from "./context";"#,
					"declare const ctx: Context;",
					"const { data } = ctx;",
					"declare const player: unknown;",
					"declare const transform: unknown;",
					r#"data.enqueue(player, "settings.userSettings", transform);"#,
				]),
			),
			(
				"src/mid1.ts",
				r#"import type { Store } from "./store"; export interface M1 { s: Store }"#.to_owned(),
			),
			(
				"src/mid2.ts",
				r#"import type { Store } from "./store"; export interface M2 { s: Store }"#.to_owned(),
			),
			(
				"src/diamondMain.ts",
				lines(&[
					r#"import type { M1 } from "./mid1";"#,
					r#"import type { M2 } from "./mid2";"#,
					"declare const m1: M1; declare const m2: M2;",
					"declare const player: unknown; declare const transform: unknown;",
					r#"declare const data: import("./store").Store;"#,
					r#"data.enqueue(player, "diamond.key", transform);"#,
				]),
			),
			(
				"src/cycleB.ts",
				r#"import type { Store } from "./store"; export interface CB { s: Store }"#.to_owned(),
			),
			(
				"src/cycleA.ts",
				lines(&[
					r#"import type { CB } from "./cycleB";"#,
					"declare const cb: CB;",
					"declare const player: unknown; declare const transform: unknown;",
					r#"declare const data: import("./store").Store;"#,
					r#"data.enqueue(player, "cycle.key", transform);"#,
				]),
			),
			(
				"src/disconnected.ts",
				"export function plain(left: number, right: number): number { return left + right; }".to_owned(),
			),
		],
		"src/main.ts",
	);
	with_state(&spec, |state, source_file| {
		let directory = rotor_support::gopath::dir(source_file.file_name());
		let program_file = |name: &str| {
			state
				.program
				.get_source_file(&format!("{directory}/{name}"))
				.unwrap_or_else(|| panic!("{name} not loaded"))
		};
		let (diamond_file, cycle_file, disconnected_file) = (
			program_file("diamondMain.ts"),
			program_file("cycleA.ts"),
			program_file("disconnected.ts"),
		);

		assert!(
			source_needs_flamework_expression_transform(state, source_file),
			"call site reaching macro declaration only through type-only imports was rejected by the expression prefilter"
		);
		let transformed = transform_file(state, source_file);
		assert_ne!(
			transformed, source_file,
			"transformSourceFile returned original source file with no structural change; expected injection of caller line"
		);
		let printed = print_source_file(transformed);
		assert!(
			printed.contains(r#"data.enqueue(player, "settings.userSettings", transform"#),
			"transformed call lost the original three arguments:\n{printed}"
		);
		assert!(
			has_injected_line(&printed, r#"enqueue(player, "settings.userSettings", transform, "#),
			"transformed call missing injected Caller<\"line\"> line number as fourth argument:\n{printed}"
		);

		// Diamond via type-only: multiple mids reach store; diamondMain reaches surface
		// transitively. Must admit and inject.
		assert!(
			source_needs_flamework_expression_transform(state, diamond_file),
			"diamond caller through type-only diamond not admitted"
		);
		let diamond = print_source_file(transform_file(state, diamond_file));
		assert!(
			has_injected_line(&diamond, r#"enqueue(player, "diamond.key", transform, "#),
			"diamond missing injection:\n{diamond}"
		);

		// Cycle (type-only) containing the macro declarer reach. Must terminate and inject at the
		// caller.
		assert!(
			source_needs_flamework_expression_transform(state, cycle_file),
			"cycle caller not admitted"
		);
		let cycle = print_source_file(transform_file(state, cycle_file));
		assert!(
			has_injected_line(&cycle, r#"enqueue(player, "cycle.key", transform, "#),
			"cycle missing exact injection (or did not terminate):\n{cycle}"
		);

		// Disconnected ordinary source in the same program: must remain ineligible and keep the
		// original source pointer.
		assert!(
			!source_needs_flamework_expression_transform(state, disconnected_file),
			"disconnected ordinary source was incorrectly admitted"
		);
		assert_eq!(
			transform_file(state, disconnected_file),
			disconnected_file,
			"disconnected must reuse original source pointer (no expression walk)"
		);
	});
}
