//! `transform_binary_test.go`

use super::{TransformSpec, run_transform};

/// `writeDependencyMacroFixture` and `transformDependencyMacroFixture`: a project whose only
/// Flamework surface is `Dependency<T>()`, the macro that rewrites to
/// `Flamework.resolveDependency` and therefore needs a synthesized `Flamework` binding. The
/// consumer, printed.
fn transform_dependency_macro_fixture(source: &[&str]) -> String {
	let output = run_transform(TransformSpec {
		tsconfig: r#"{"compilerOptions":{"moduleResolution":"node","rootDir":"src","outDir":"out"},"files":["src/consumer.ts"]}"#,
		files: vec![
			(
				"node_modules/@flamework/core/package.json",
				r#"{"name":"@flamework/core","types":"index.d.ts"}"#.to_owned(),
			),
			(
				"node_modules/@flamework/core/index.d.ts",
				[
					"export type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };",
					"export declare namespace Flamework {",
					" function resolveDependency<T>(id: string): T;",
					"}",
					"/** @metadata macro {@link Flamework.resolveDependency intrinsic-flamework-rewrite} */",
					r#"export declare function Dependency<T>(id?: Generic<T, "id">): T;"#,
				]
				.join("\n"),
			),
			("src/consumer.ts", source.join("\n")),
		],
		targets: vec!["src/consumer.ts"],
		out_dir: "out",
		..TransformSpec::default()
	});
	assert!(output.error.is_none(), "{:?}", output.error);
	output.printed[0].clone()
}

// A macro reached through an assignment used to lose the imports its rewrite depends on,
// emitting `Flamework["resolveDependency"]` with no `Flamework` binding in scope.
#[test]
fn transform_imports_rewrite_namespace_when_macro_is_under_an_assignment() {
	let printed = transform_dependency_macro_fixture(&[
		r#"import { Dependency } from "@flamework/core";"#,
		"interface Controller { value: number }",
		"let controller: Controller | undefined;",
		"task.spawn(() => {",
		"\tcontroller = Dependency<Controller>();",
		"});",
		"export = controller;",
	]);

	assert!(
		printed.contains(r#"Flamework["resolveDependency"]"#),
		"assignment macro was not rewritten:\n{printed}"
	);
	assert!(
		printed.contains(r#"import { Flamework as Flamework } from "@flamework/core";"#),
		"rewrite namespace was not imported:\n{printed}"
	);
}

// The same loss applied to every expression form that owns its rewrite: compound assignment and
// the unary update operators.
#[test]
fn transform_imports_rewrite_namespace_when_macro_is_under_a_compound_assignment() {
	let printed = transform_dependency_macro_fixture(&[
		r#"import { Dependency } from "@flamework/core";"#,
		"let total = 0;",
		"total += Dependency<number>();",
		"export = total;",
	]);

	assert!(
		printed.contains(r#"Flamework["resolveDependency"]"#),
		"compound assignment macro was not rewritten:\n{printed}"
	);
	assert!(
		printed.contains(r#"import { Flamework as Flamework } from "@flamework/core";"#),
		"rewrite namespace was not imported:\n{printed}"
	);
}
