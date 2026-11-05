//! `transform_integration_test.go`

use super::{parse_ts, print_source_file, transform_input, with_class_analysis_input};
use crate::plan::{ClassPlan, FileAnalysis};
use crate::project::{ProjectOptions, open_project};
use crate::transform_state::TransformInput;

const GAME: (&str, &str) = ("package.json", r#"{"name":"fixture-game","version":"1.0.0"}"#);

/// A transform of `target` in a fixture with the `fixture` hash prefix: the printed file, the
/// plan class counts, and whether the source metadata keeps exact identities.
fn transform_with_prefix(
	files: &[(&str, &str)],
	target: &str,
	analyses: Vec<FileAnalysis>,
	hash_prefix: &str,
) -> (String, Vec<usize>, bool) {
	with_class_analysis_input(files, target, |input| {
		let mut project = open_project(&ProjectOptions {
			project_dir: input.project.directory.clone(),
			root_dir: "src".to_owned(),
			out_dir: "out".to_owned(),
			config: rotor_config::config::FlameworkConfig {
				hash_prefix: hash_prefix.to_owned(),
				..rotor_config::config::FlameworkConfig::default()
			},
			..ProjectOptions::default()
		})
		.expect("OpenProject");
		let source_file = input.files[0];
		let result = crate::transform::transform(TransformInput {
			program: input.program,
			checker: input.checker,
			files: vec![source_file],
			project: &mut project,
			analyses,
			macro_runtime: None,
			print: None,
		})
		.expect("Transform");
		let exact = result.sources.len() == 1
			&& result.sources[0].changed()
			&& result.sources[0].original() == source_file
			&& result.sources[0].transformed() == result.files[0];
		(
			print_source_file(result.files[0]),
			result.plans.iter().map(|plan| plan.classes().len()).collect(),
			exact,
		)
	})
}

#[test]
fn transform_discovers_and_transforms_classes_and_macros_when_analyses_are_omitted() {
	let source = [
		r#"type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };"#,
		"declare const Service: () => FlameworkDecorator;",
		"type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };",
		r#"declare function textMacro(value?: Generic<"payload", "text">): string;"#,
		"@Service() export class Consumer {}",
		"export const text = textMacro();",
	]
	.join("\n");
	let (printed, plans, exact) = transform_with_prefix(
		&[
			GAME,
			(
				"tsconfig.json",
				r#"{"compilerOptions":{"experimentalDecorators":true,"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}"#,
			),
			("src/service.ts", &source),
		],
		"src/service.ts",
		Vec::new(),
		"fixture",
	);

	assert_eq!(plans, [1], "Transform() plans, want one discovered class");
	for want in [
		r#"import { Reflect as Reflect } from "@flamework/core";"#,
		r#"Reflect["defineMetadata"](Consumer, "identifier", "fixture:service@Consumer");"#,
		r#"Reflect["decorate"](Consumer, "fixture:service@Service", Service, []);"#,
		r#"export const text = textMacro("\"payload\"" as never);"#,
	] {
		assert!(
			printed.contains(want),
			"transformed source missing {want:?}:\n{printed}"
		);
	}
	assert!(
		!printed.contains("@Service()"),
		"Flamework decorator was not stripped:\n{printed}"
	);
	assert!(
		parse_ts("/transformed.ts", &printed).diagnostics().is_empty(),
		"reparse diagnostics:\n{printed}"
	);
	assert!(exact, "source metadata, want exact original and transformed identities");
}

#[test]
fn transform_resolves_nested_classes_by_full_internal_id_when_simple_names_repeat() {
	let source = [
		r#"type FW = ClassDecorator & { _flamework_Decorator: "Class" };"#,
		"declare const Service: () => FW;",
		"namespace Left {",
		" @Service() export class Same {}",
		"}",
		"namespace Right {",
		" @Service() export class Same {}",
		"}",
	]
	.join("\n");
	let (printed, plans, _) = transform_with_prefix(
		&[
			GAME,
			(
				"tsconfig.json",
				r#"{"compilerOptions":{"experimentalDecorators":true,"rootDir":"src","outDir":"out"},"files":["src/nested.ts"]}"#,
			),
			("src/nested.ts", &source),
		],
		"src/nested.ts",
		Vec::new(),
		"fixture",
	);

	for want in [
		r#"Reflect["defineMetadata"](Same, "identifier", "fixture:nested@Left.Same");"#,
		r#"Reflect["defineMetadata"](Same, "identifier", "fixture:nested@Right.Same");"#,
	] {
		assert_eq!(
			printed.matches(want).count(),
			1,
			"nested class output count for {want:?}:\n{printed}"
		);
	}
	assert_eq!(plans, [2], "nested class plans, want two unique full IDs");
}

#[test]
fn transform_merges_explicit_characterization_without_duplicate_plans() {
	let (printed, plans, _) = transform_with_prefix(
		&[
			GAME,
			(
				"tsconfig.json",
				r#"{"compilerOptions":{"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}"#,
			),
			("src/service.ts", "/** @metadata reflect */ export class Service {}"),
		],
		"src/service.ts",
		vec![FileAnalysis {
			file_id: "src/service.ts".to_owned(),
			classes: vec![ClassPlan {
				internal_id: "fixture-game:out/service@Service".to_owned(),
				..ClassPlan::default()
			}],
			..FileAnalysis::default()
		}],
		"",
	);

	assert_eq!(plans, [1], "merged plans, want one file and one class");
	assert!(
		printed.contains(r#"Reflect["defineMetadata"](Service, "identifier", "service@Service");"#),
		"explicit characterization identifier missing:\n{printed}"
	);
}

#[test]
fn transform_suppresses_native_only_discovery_diagnostics() {
	let source = [
		"interface Required { required(): void }",
		"/** @metadata reflect {@link Required constraint} */",
		"interface Marked {}",
		"export class Invalid implements Marked {}",
	]
	.join("\n");
	let diagnostics = with_class_analysis_input(
		&[
			GAME,
			(
				"tsconfig.json",
				r#"{"compilerOptions":{"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}"#,
			),
			("src/service.ts", &source),
		],
		"src/service.ts",
		|input| transform_input(input, Vec::new()).expect("Transform").diagnostics.len(),
	);

	assert_eq!(
		diagnostics, 0,
		"Transform() diagnostics, want pinned upstream public-path parity"
	);
}

#[test]
fn transform_places_macro_imports_and_prerequisites_before_owning_class() {
	let source = [
		"type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };",
		"interface Leaf { id: string; value: number }",
		"interface Repeated { first: Leaf; second: Leaf; third: Leaf }",
		r#"declare function guardMacro(value?: Generic<Repeated, "guard">): unknown;"#,
		r#"type FW = ClassDecorator & { _flamework_Decorator: "Class" };"#,
		"declare const Decorate: (guard: unknown) => FW;",
		"@Decorate(guardMacro()) export class Counter {}",
	]
	.join("\n");
	let printed = with_class_analysis_input(
		&[
			GAME,
			(
				"tsconfig.json",
				r#"{"compilerOptions":{"experimentalDecorators":true,"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}"#,
			),
			(
				"node_modules/@flamework/core/package.json",
				r#"{"name":"@flamework/core","version":"1.3.2"}"#,
			),
			(
				"node_modules/@flamework/core/node_modules/@rbxts/t/package.json",
				r#"{"name":"@rbxts/t","version":"3.2.1"}"#,
			),
			("src/service.ts", &source),
		],
		"src/service.ts",
		|input| {
			input.project.config.optimizations.guard_generation_dedup_limit = Some(2);
			print_source_file(transform_input(input, Vec::new()).expect("Transform").files[0])
		},
	);

	let import_index = printed.find(r#"import { t as t } from "@flamework/core/out/prelude";"#);
	let prerequisite_index = printed.find("const dedup =");
	let class_index = printed.find("export class Counter");
	let decorator_index = printed.find(r#"Reflect["decorate"]"#);
	assert!(
		import_index.is_some()
			&& prerequisite_index > import_index
			&& class_index > prerequisite_index
			&& decorator_index > class_index,
		"macro import/prerequisite/class/decorator order mismatch:\n{printed}"
	);
	assert_eq!(
		printed.matches(r#"from "@flamework/core/out/prelude""#).count(),
		1,
		"macro import was not deduplicated:\n{printed}"
	);
}

#[test]
fn transform_shares_guard_library_import_between_macros_and_component_decorators_when_project_owns_rbxts_t() {
	let core = [
		"type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };",
		"export declare namespace Flamework {",
		"  /** @metadata macro */",
		r#"  function createGuard<T>(meta?: Generic<T, "guard">): unknown;"#,
		"}",
	]
	.join("\n");
	let components = [
		"export declare class BaseComponent<A = {}, I extends Instance = Instance> {",
		"  /** @metadata intrinsic-component-attributes */",
		"  attributes: A;",
		"  /** @metadata intrinsic-component-instance */",
		"  instance: I;",
		"}",
		"export interface ComponentConfig { tag?: string; attributes?: object; instanceGuard?: unknown; }",
		"/** @metadata intrinsic-component-decorator */",
		r#"export declare const Component: (opts?: ComponentConfig) => ClassDecorator & { _flamework_Decorator: "Class" };"#,
	]
	.join("\n");
	let globals = [
		"interface Instance { readonly _nominal_Instance: unique symbol; }",
		"interface BasePart extends Instance { readonly _nominal_BasePart: unique symbol; }",
	]
	.join("\n");
	let thing = [
		r#"import { Flamework } from "@flamework/core";"#,
		r#"import { BaseComponent, Component } from "@flamework/components";"#,
		"interface Config { speed: number; }",
		"export const isConfig = Flamework.createGuard<Config>();",
		"interface Attributes { enabled: boolean; }",
		r#"@Component({ tag: "Thing" })"#,
		"export class Thing extends BaseComponent<Attributes, BasePart> {}",
	]
	.join("\n");
	let printed = with_class_analysis_input(
		&[
			GAME,
			(
				"tsconfig.json",
				r#"{"compilerOptions":{"experimentalDecorators":true,"strict":true,"rootDir":"src","outDir":"out","moduleResolution":"node"},"files":["src/globals.d.ts","src/thing.ts"]}"#,
			),
			(
				"node_modules/@rbxts/t/package.json",
				r#"{"name":"@rbxts/t","version":"3.2.1"}"#,
			),
			(
				"node_modules/@flamework/core/package.json",
				r#"{"name":"@flamework/core","version":"1.3.2","types":"out/index.d.ts"}"#,
			),
			("node_modules/@flamework/core/out/index.d.ts", &core),
			(
				"node_modules/@flamework/components/package.json",
				r#"{"name":"@flamework/components","version":"1.3.2","types":"out/index.d.ts"}"#,
			),
			("node_modules/@flamework/components/out/index.d.ts", &components),
			("src/globals.d.ts", &globals),
			("src/thing.ts", &thing),
		],
		"src/thing.ts",
		|input| print_source_file(transform_input(input, Vec::new()).expect("Transform").files[0]),
	);

	assert!(
		printed.contains(r#""enabled": t["boolean"]"#) && printed.contains(r#""speed": t["number"]"#),
		"guard macro and component attributes were not both transformed:\n{printed}"
	);
	assert_eq!(
		printed.matches(r#"from "@rbxts/t";"#).count(),
		1,
		"@rbxts/t import count:\n{printed}"
	);
	assert!(
		!printed.contains(r#"from "@flamework/core/out/prelude""#),
		"component decorator imported t from the prelude instead of the resolved guard library:\n{printed}"
	);
}
