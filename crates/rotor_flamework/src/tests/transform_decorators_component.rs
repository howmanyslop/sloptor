//! `transform_decorators_component_test.go`

use super::{TransformSpec, run_transform};

#[test]
fn transform_rewrites_imported_component_config_when_attributes_and_instance_are_typed() {
	let file = |name: &'static str, lines: &[&str]| (name, lines.join("\n"));
	let output = run_transform(TransformSpec {
		tsconfig: r#"{"compilerOptions":{"experimentalDecorators":true,"strict":true,"rootDir":"src","outDir":"out","moduleResolution":"node"},"files":["src/globals.d.ts","src/component.ts"]}"#,
		files: vec![
			file(
				"src/globals.d.ts",
				&[
					"interface Instance { readonly _nominal_Instance: unique symbol; } interface Part extends Instance { readonly _nominal_Part: unique symbol; }",
				],
			),
			file(
				"node_modules/@flamework/components/package.json",
				&[r#"{"name":"@flamework/components","version":"1.3.2","types":"out/index.d.ts"}"#],
			),
			file(
				"node_modules/@flamework/components/flamework.build",
				&[r#"{"version":1,"flameworkVersion":"1.3.2","identifierPrefix":"$c","identifiers":{}}"#],
			),
			file(
				"node_modules/@flamework/components/out/index.d.ts",
				&[r#"export { BaseComponent } from "./baseComponent"; export { Component } from "./components";"#],
			),
			file(
				"node_modules/@flamework/components/out/baseComponent.d.ts",
				&[
					"export declare class BaseComponent<A = {}, I extends Instance = Instance> {",
					"  /** @metadata intrinsic-component-attributes */",
					"  attributes: A;",
					"  /** @metadata intrinsic-component-instance */",
					"  instance: I;",
					"}",
				],
			),
			file(
				"node_modules/@flamework/components/out/components.d.ts",
				&[
					"export interface ComponentConfig { tag?: string; attributes?: object; instanceGuard?: unknown; }",
					"/** @metadata flamework:implements flamework:parameters injectable intrinsic-component-decorator */",
					"export declare const Component: (opts?: ComponentConfig) => ClassDecorator & { _flamework_Decorator: never };",
				],
			),
			file(
				"node_modules/@flamework/core/package.json",
				&[r#"{"name":"@flamework/core","version":"1.3.2","types":"out/index.d.ts"}"#],
			),
			file(
				"node_modules/@flamework/core/flamework.build",
				&[r#"{"version":1,"flameworkVersion":"1.3.2","identifierPrefix":"$","identifiers":{}}"#],
			),
			file(
				"node_modules/@flamework/core/out/index.d.ts",
				&[r#"export { Service } from "./flamework";"#],
			),
			file(
				"node_modules/@flamework/core/out/flamework.d.ts",
				&["export declare const Service: () => ClassDecorator & { _flamework_Decorator: never };"],
			),
			file(
				"src/component.ts",
				&[
					r#"import { BaseComponent, Component } from "@flamework/components";"#,
					r#"import { Service } from "@flamework/core";"#,
					"interface Attributes { active: boolean; label: string; retries: number; }",
					"@Service()",
					r#"@Component({ tag: "Fixture" })"#,
					"export class FixtureComponent extends BaseComponent<Attributes, Part> {}",
				],
			),
		],
		targets: vec!["src/component.ts"],
		config: rotor_config::config::FlameworkConfig {
			hash_prefix: "fixture".to_owned(),
			..rotor_config::config::FlameworkConfig::default()
		},
		out_dir: "out",
		..TransformSpec::default()
	});
	assert!(output.error.is_none(), "{:?}", output.error);
	let printed = &output.printed[0];

	for want in [
		r#""attributes": {"#,
		r#""active": t["boolean"]"#,
		r#""label": t["string"]"#,
		r#""retries": t["number"]"#,
		r#""instanceGuard": t["instanceIsA"]("Part")"#,
		r#""$c:components@Component""#,
		r#""$:flamework@Service""#,
		"// (Flamework) FixtureComponent metadata",
		"// (Flamework) FixtureComponent decorators",
	] {
		assert!(printed.contains(want), "component output missing {want:?}:\n{printed}");
	}
}
