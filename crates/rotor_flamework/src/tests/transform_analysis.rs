//! `transform_analysis_test.go`

use super::{print_source_file, transform_input, with_class_analysis_input};
use crate::build_info::{BuildClass, BuildDecorator};
use crate::plan::{ClassPlan, FileAnalysis};
use crate::transform_analysis::{FlameworkClassAnalysis, analyze_flamework_classes};

const GAME: (&str, &str) = ("package.json", r#"{"name":"fixture-game","version":"1.0.0"}"#);
const PLAIN: (&str, &str) = (
	"tsconfig.json",
	r#"{"compilerOptions":{"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}"#,
);
const DECORATED: (&str, &str) = (
	"tsconfig.json",
	r#"{"compilerOptions":{"experimentalDecorators":true,"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}"#,
);

fn analyze(files: &[(&str, &str)]) -> FlameworkClassAnalysis {
	with_class_analysis_input(files, "src/service.ts", |input| {
		analyze_flamework_classes(input).expect("AnalyzeFlameworkClasses")
	})
}

#[test]
fn analyze_flamework_classes_preserves_planned_class_transform_output() {
	let printed = with_class_analysis_input(
		&[GAME, PLAIN, ("src/service.ts", "export class Service {}\n")],
		"src/service.ts",
		|input| {
			let analyses = vec![FileAnalysis {
				file_id: "src/service.ts".to_owned(),
				classes: vec![ClassPlan {
					internal_id: "fixture-game:out/service@Service".to_owned(),
					..ClassPlan::default()
				}],
				..FileAnalysis::default()
			}];
			print_source_file(transform_input(input, analyses).expect("Transform").files[0])
		},
	);

	for fragment in [
		"export class Service",
		r#"Reflect["defineMetadata"](Service, "identifier", "service@Service");"#,
	] {
		assert!(
			printed.contains(fragment),
			"planned transform missing {fragment:?}:\n{printed}"
		);
	}
}

#[test]
fn discover_flamework_classes_transforms_discovered_class_after_expression_visitor_clones_it() {
	let source = [
		r#"type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };"#,
		"declare const Service: () => FlameworkDecorator;",
		"type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };",
		r#"declare function textMacro(value?: Generic<"payload", "text">): string;"#,
		"@Service() export class Consumer {}",
		"export const text = textMacro();",
	]
	.join("\n");
	let printed = with_class_analysis_input(
		&[GAME, DECORATED, ("src/service.ts", &source)],
		"src/service.ts",
		|input| print_source_file(transform_input(input, Vec::new()).expect("Transform").files[0]),
	);

	assert!(
		printed.contains(r#"Reflect["defineMetadata"](Consumer, "identifier", "service@Consumer");"#),
		"transformed source missing discovered identifier:\n{printed}"
	);
}

#[test]
fn discover_flamework_classes_discovers_class_with_only_member_decorator() {
	let source = [
		r#"type FlameworkDecorator = MethodDecorator & { _flamework_Decorator: "Method" };"#,
		"declare const Observe: () => FlameworkDecorator;",
		"export class Consumer { @Observe() run(): void {} }",
	]
	.join("\n");
	let analysis = analyze(&[GAME, DECORATED, ("src/service.ts", &source)]);

	let classes = analysis.classes();
	assert!(
		classes.len() == 1 && classes[0].internal_id == "fixture-game:out/service@Consumer",
		"discovered classes = {classes:?}"
	);
	let files = analysis.files();
	assert!(
		files.len() == 1 && files[0].classes[0].contains_legacy_decorator,
		"member decorator provenance = {files:?}"
	);
}

#[test]
fn discover_flamework_classes_discovers_namespaced_decorator() {
	let source = [
		r#"type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };"#,
		"namespace Decorators { export declare const Service: () => FlameworkDecorator }",
		"@Decorators.Service() export class Consumer {}",
	]
	.join("\n");
	let analysis = analyze(&[GAME, DECORATED, ("src/service.ts", &source)]);

	let classes = analysis.classes();
	assert!(
		classes.len() == 1 && classes[0].decorator_ids.join(",") == "fixture-game:out/service@Decorators.Service",
		"discovered namespaced decorator classes = {classes:?}"
	);
	let files = analysis.files();
	assert!(
		files.len() == 1 && files[0].classes[0].contains_legacy_decorator,
		"namespaced decorator provenance = {files:?}"
	);
}

#[test]
fn discover_flamework_classes_discovers_decorator_and_inferred_type_ids() {
	let source = [
		r#"type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };"#,
		"/** @metadata reflect */",
		"declare const Service: () => FlameworkDecorator;",
		"class Dependency {}",
		"@Service() export class Consumer { constructor(dependency = new Dependency()) {} }",
	]
	.join("\n");
	let analysis = analyze(&[GAME, DECORATED, ("src/service.ts", &source)]);

	let files = analysis.files();
	assert!(
		files.len() == 1 && files[0].classes.len() == 1,
		"discovered files = {files:?}, want one class"
	);
	let class = &analysis.classes()[0];
	assert_eq!(class.internal_id, "fixture-game:out/service@Consumer");
	assert_eq!(class.decorator_ids.join(","), "fixture-game:out/service@Service");
	assert_eq!(class.constructor_type_ids.join(","), "service@Dependency");
}

#[test]
fn discover_flamework_classes_uses_package_build_fallback() {
	let files = with_class_analysis_input(
		&[
			("package.json", r#"{"name":"fixture-package","version":"1.0.0"}"#),
			PLAIN,
			("src/service.ts", "export class Service {}"),
		],
		"src/service.ts",
		|input| {
			input
				.project
				.add_class(&BuildClass {
					file_path: "out/service.lua".to_owned(),
					internal_id: "fixture-package:out/service@Service".to_owned(),
					decorators: vec![BuildDecorator {
						name: "Service".to_owned(),
						internal_id: "@flamework/core:out/services@Service".to_owned(),
					}],
				})
				.expect("AddClass");
			analyze_flamework_classes(input)
				.expect("AnalyzeFlameworkClasses")
				.files()
		},
	);

	assert!(
		files.len() == 1 && files[0].classes.len() == 1 && files[0].classes[0].decorators.len() == 1,
		"fallback analysis = {files:?}"
	);
	assert!(
		!files[0].classes[0].contains_legacy_decorator,
		"build fallback incorrectly marked source decorator provenance: {files:?}"
	);
}

#[test]
fn discover_flamework_classes_records_package_build_class_once() {
	let (first, second, classes) = with_class_analysis_input(
		&[
			("package.json", r#"{"name":"@scope/fixture","version":"1.0.0"}"#),
			PLAIN,
			("src/service.ts", "/** @metadata reflect */ export class Service {}"),
		],
		"src/service.ts",
		|input| {
			let first = analyze_flamework_classes(input).expect("first AnalyzeFlameworkClasses");
			let second = analyze_flamework_classes(input).expect("second AnalyzeFlameworkClasses");
			(
				first,
				second,
				input.project.build_info_snapshot().classes.unwrap_or_default(),
			)
		},
	);

	assert!(
		first.classes().len() == 1 && second.classes().len() == 1,
		"first = {first:?} second = {second:?}"
	);
	assert!(
		!first.files()[0].classes[0].contains_legacy_decorator
			&& !second.files()[0].classes[0].contains_legacy_decorator,
		"reflect-only discovery incorrectly marked legacy decorator provenance"
	);
	assert!(
		classes.len() == 1 && classes[0].internal_id == "@scope/fixture:service@Service",
		"build classes = {classes:?}"
	);
}

#[test]
fn discover_flamework_classes_resolves_external_package_decorator_and_type_uids() {
	let declarations = [
		r#"export type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };"#,
		"/** @metadata reflect */ export declare const ExternalDecorator: () => FlameworkDecorator;",
		"export declare class ExternalDependency {}",
	]
	.join("\n");
	let source = [
		r#"import { ExternalDecorator, ExternalDependency } from "@scope/ext";"#,
		"@ExternalDecorator() export class Consumer { constructor(value: ExternalDependency) {} }",
	]
	.join("\n");
	let analysis = analyze(&[
		GAME,
		(
			"tsconfig.json",
			r#"{"compilerOptions":{"experimentalDecorators":true,"rootDir":"src","outDir":"out","moduleResolution":"node"},"files":["src/service.ts"]}"#,
		),
		(
			"node_modules/@scope/ext/package.json",
			r#"{"name":"@scope/ext","version":"1.0.0","types":"index.d.ts"}"#,
		),
		(
			"node_modules/@scope/ext/flamework.build",
			r#"{"version":1,"flameworkVersion":"1.3.2","identifierPrefix":"ext","identifiers":{}}"#,
		),
		("node_modules/@scope/ext/index.d.ts", &declarations),
		("src/service.ts", &source),
	]);

	assert_eq!(
		analysis.classes().len(),
		1,
		"classes = {:?}, want one",
		analysis.classes()
	);
	let class = &analysis.classes()[0];
	assert_eq!(class.decorator_ids.join(","), "@scope/ext:index@ExternalDecorator");
	assert_eq!(class.constructor_type_ids.join(","), "ext:index@ExternalDependency");
}

#[test]
fn discover_flamework_classes_ignores_unsupported_decorator_and_malformed_metadata() {
	let analysis = analyze(&[
		GAME,
		DECORATED,
		(
			"src/service.ts",
			"/** @metadata {@link Missing constraint} */ declare const ns: { Decorator(): ClassDecorator }; @ns.Decorator() export class Ignored {}",
		),
	]);

	assert!(
		analysis.files().is_empty() && analysis.diagnostics().is_empty(),
		"unsupported analysis = {analysis:?}"
	);
}
