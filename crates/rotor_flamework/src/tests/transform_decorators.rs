//! `transform_decorators_test.go`

use super::{
	find_named_class, parse_ts, print_source_file, print_with_statements, replace_class_syntax_list,
	with_class_analysis_input, with_decorator_fixture,
};
use crate::plan::{ClassPlan, DecoratorPlan};
use crate::transform_class::transform_flamework_class;
use crate::transform_decorators_imports::FLAMEWORK_PRELUDE_MODULE;
use crate::transform_imports::prepend_flamework_reflect_import;

fn class_plan(decorators: &[(&str, &str)]) -> ClassPlan {
	ClassPlan {
		internal_id: "fixture:out/service@Counter".to_owned(),
		decorators: decorators
			.iter()
			.map(|(name, internal_id)| DecoratorPlan {
				name: (*name).to_owned(),
				internal_id: (*internal_id).to_owned(),
			})
			.collect(),
		..ClassPlan::default()
	}
}

/// The class `Counter` transformed under its plan and printed in its file.
fn transform_counter(source: &str, plan: ClassPlan) -> String {
	with_decorator_fixture(source, plan, |state, file| {
		let class_node = find_named_class(file.as_node(), "Counter");
		let plan = state.plans[0].clone();
		let transformed = transform_flamework_class(state, &plan, class_node).expect("transformFlameworkClass");
		print_with_statements(
			&state.factory,
			file,
			replace_class_syntax_list(file, class_node, transformed),
		)
	})
}

/// Whether the transform kept the original class, and its diagnostics: file match, range
/// containment and exact range against the class, and message.
fn invalid_counter(source: &str, plan: ClassPlan) -> (bool, Vec<(bool, bool, bool, String)>) {
	with_decorator_fixture(source, plan, |state, file| {
		let class_node = find_named_class(file.as_node(), "Counter");
		let plan = state.plans[0].clone();
		let transformed = transform_flamework_class(state, &plan, class_node).expect("transformFlameworkClass");
		let (start, end) = (
			crate::transform_diagnostics::text_pos(class_node.pos()),
			crate::transform_diagnostics::text_pos(class_node.end()),
		);
		let diagnostics = state
			.ordered_diagnostics()
			.iter()
			.map(|diagnostic| {
				(
					diagnostic.file == Some(file),
					diagnostic.pos() >= start && diagnostic.end() <= end,
					diagnostic.pos() == start && diagnostic.end() == end,
					diagnostic.string().to_owned(),
				)
			})
			.collect();
		(transformed == class_node, diagnostics)
	})
}

#[test]
fn decorator_lowering_emits_exact_accessor_static_component_and_import_output() {
	let source = [
		"// @rbxts-transform-debug",
		r#"import { Reflect as RuntimeReflect } from "@flamework/core";"#,
		r#"type FW = ClassDecorator & MethodDecorator & PropertyDecorator & { _flamework_Decorator: "Class" };"#,
		"namespace Decorators {",
		" /** @metadata intrinsic-component-decorator */",
		" export declare const Component: (config?: object) => FW;",
		" export declare const Member: () => FW;",
		"}",
		"class Base<A> {",
		" /** @metadata intrinsic-component-attributes */",
		" readonly attributes!: A",
		"}",
		"interface Attributes { active: boolean }",
		"@Decorators.Component({})",
		"class Counter extends Base<Attributes> {",
		" @Decorators.Member() static count = 0;",
		" @Decorators.Member() get value(): number { return 1 }",
		" @Decorators.Member() set value(next: number) { void next }",
		"}",
		"",
	]
	.join("\n");
	let plan = class_plan(&[
		("Component", "fixture:out/service@Component"),
		("Member", "fixture:out/service@Member"),
	]);
	let printed = with_decorator_fixture(&source, plan, |state, file| {
		let class_node = find_named_class(file.as_node(), "Counter");
		let plan = state.plans[0].clone();
		let transformed = transform_flamework_class(state, &plan, class_node).expect("transformFlameworkClass");
		let statements = replace_class_syntax_list(file, class_node, transformed);
		let output = state
			.factory
			.update_source_file(
				file.as_node(),
				state.factory.new_node_list(statements),
				file.end_of_file_token,
			)
			.as_source_file_p();
		print_source_file(prepend_flamework_reflect_import(&state.factory, output))
	});

	for want in [
		"// @rbxts-transform-debug",
		r#"import { Reflect as RuntimeReflect } from "@flamework/core";"#,
		// The UID carries every named ancestor of the declaration, so a decorator declared in
		// `namespace Decorators` is `Decorators.Component`, matching the reference
		// getDeclarationName.
		r#"RuntimeReflect["decorate"](Counter, "fixture:service@Decorators.Component", Decorators.Component, [{"#,
		r#""attributes": {"#,
		r#""active": t["boolean"]"#,
		r#"Decorators.Member, [], "count", true"#,
		r#"Decorators.Member, [], "value", false"#,
	] {
		assert!(
			printed.contains(want),
			"transformed output missing {want:?}:\n{printed}"
		);
	}
	assert!(
		printed.matches(r#"from "@flamework/core""#).count() == 1
			&& !printed.contains("@Decorators.Component(")
			&& !printed.contains("@Decorators.Member("),
		"decorator import/stripping mismatch:\n{printed}"
	);
	assert!(
		parse_ts("/output.ts", &printed).diagnostics().is_empty(),
		"reparse diagnostics:\n{printed}"
	);
}

const INVALID_CONFIG: [&str; 4] = [
	r#"type FW = ClassDecorator & { _flamework_Decorator: "Class" };"#,
	"/** @metadata intrinsic-component-decorator */",
	"declare const Component: (config: unknown) => FW;",
	r#"@Component("invalid") class Counter {}"#,
];

#[test]
fn decorator_lowering_reports_invalid_component_config_signature() {
	let (original, diagnostics) = invalid_counter(
		&INVALID_CONFIG.join("\n"),
		class_plan(&[("Component", "fixture:out/service@Component")]),
	);

	assert!(
		original && diagnostics.len() == 1,
		"invalid signature result = ({original}, {diagnostics:?})"
	);
}

#[test]
fn decorator_lowering_preserves_original_class_and_adds_located_diagnostic_when_config_invalid() {
	let (original, diagnostics) = invalid_counter(
		&INVALID_CONFIG.join("\n"),
		class_plan(&[("Component", "fixture:out/service@Component")]),
	);

	assert!(
		original,
		"transformFlameworkClass() did not fall back to the original class"
	);
	assert!(
		diagnostics.len() == 1 && diagnostics[0].0 && diagnostics[0].1,
		"located diagnostics = {diagnostics:?}, want one diagnostic within original class"
	);
	assert_eq!(diagnostics[0].3, "Decorators are not valid here.");
}

#[test]
fn transform_flamework_class_omits_identifier_metadata_when_only_reflect_is_requested() {
	let printed = transform_counter("/** @metadata reflect */ class Counter {}", class_plan(&[]));

	assert!(
		!printed.contains(r#""identifier""#),
		"reflection-only identifier parity mismatch:\n{printed}"
	);
}

#[test]
fn transform_omits_identifier_metadata_for_discovered_reflect_only_class() {
	let printed = with_class_analysis_input(
		&[
			("package.json", r#"{"name":"fixture-game","version":"1.0.0"}"#),
			(
				"tsconfig.json",
				r#"{"compilerOptions":{"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}"#,
			),
			("src/service.ts", "/** @metadata reflect */ export class Counter {}"),
		],
		"src/service.ts",
		|input| {
			let result = crate::transform::transform(crate::transform_state::TransformInput {
				program: input.program,
				checker: input.checker,
				files: input.files.clone(),
				project: input.project,
				analyses: Vec::new(),
				macro_runtime: None,
				print: None,
			})
			.expect("Transform");
			print_source_file(result.files[0])
		},
	);

	assert!(
		!printed.contains(r#""identifier""#),
		"discovered reflect-only class emitted identifier metadata:\n{printed}"
	);
}

#[test]
fn decorator_lowering_preserves_original_class_and_reports_invalid_placement() {
	let (original, diagnostics) = invalid_counter(
		&[
			r#"type FW = PropertyDecorator & MethodDecorator & { _flamework_Decorator: "Class" };"#,
			"/** @metadata intrinsic-component-decorator */",
			"declare const Component: (config?: object) => FW;",
			"class Counter {",
			" @Component({}) first = 1;",
			" @Component({}) second() {}",
			"}",
		]
		.join("\n"),
		class_plan(&[("Component", "fixture:out/service@Component")]),
	);

	assert!(
		original && diagnostics.len() == 1,
		"invalid placement result = ({original}, {diagnostics:?})"
	);
	assert!(
		diagnostics[0].0 && diagnostics[0].2 && diagnostics[0].3 == "Decorators are not valid here.",
		"invalid placement diagnostic = {:?}, want located class diagnostic",
		diagnostics[0]
	);
}

#[test]
fn decorator_lowering_preserves_parameter_decorators_because_upstream_does_not_emit_them() {
	let printed = transform_counter(
		&[
			r#"type FW = ParameterDecorator & { _flamework_Decorator: "Class" };"#,
			"declare const Parameter: () => FW;",
			"class Counter { method(@Parameter() value: string) { void value } }",
		]
		.join("\n"),
		class_plan(&[("Parameter", "fixture:out/service@Parameter")]),
	);

	assert!(
		printed.contains("@Parameter()") && !printed.contains(r#""fixture:service@Parameter""#),
		"parameter decorator parity mismatch:\n{printed}"
	);
}

#[test]
fn decorator_lowering_emits_macro_prerequisites_before_class_and_decorator_call() {
	let source = [
		"type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };",
		"interface Leaf { id: string; value: number }",
		"interface Repeated { first: Leaf; second: Leaf; third: Leaf }",
		r#"declare function guardMacro(value?: Generic<Repeated, "guard">): unknown;"#,
		r#"type FW = ClassDecorator & { _flamework_Decorator: "Class" };"#,
		"declare const Decorate: (guard: unknown) => FW;",
		"@Decorate(guardMacro()) class Counter {}",
	]
	.join("\n");
	let printed = with_decorator_fixture(
		&source,
		class_plan(&[("Decorate", "fixture:out/service@Decorate")]),
		|state, file| {
			state.guard_library = FLAMEWORK_PRELUDE_MODULE.to_owned();
			state.project.config.optimizations.guard_generation_dedup_limit = Some(2);
			let class_node = find_named_class(file.as_node(), "Counter");
			let plan = state.plans[0].clone();
			let transformed = transform_flamework_class(state, &plan, class_node).expect("transformFlameworkClass");
			print_with_statements(
				&state.factory,
				file,
				replace_class_syntax_list(file, class_node, transformed),
			)
		},
	);

	let import_statement = r#"import { t } from "@flamework/core/out/prelude";"#;
	let index = |needle: &str| printed.find(needle);
	let (import_index, prerequisite, class_declaration, decorator_call) = (
		index(import_statement),
		index("const dedup ="),
		index("class Counter"),
		index(r#"Reflect["decorate"]"#),
	);
	assert!(
		import_index.is_some()
			&& prerequisite > import_index
			&& class_declaration > prerequisite
			&& decorator_call > class_declaration,
		"decorator prerequisite/class/call order mismatch:\n{printed}"
	);
	assert_eq!(
		printed.matches(import_statement).count(),
		1,
		"decorator macro runtime import was not deduplicated:\n{printed}"
	);
	assert!(
		printed.contains(r#"guardMacro(t["interface"]({ "first": dedup, "second": dedup, "third": dedup }) as never)"#),
		"decorator macro argument was not transformed:\n{printed}"
	);
}
