//! `transform_component_test.go`

use rotor_tsrs::ast::Kind;

use super::{
	StateSpec, emit_node, find_expression_kind, find_named_class, parse_ts, with_expression_fixture, with_state,
};
use crate::transform_decorators_imports::{FLAMEWORK_PRELUDE_MODULE, decorator_runtime_imports};
use crate::transform_expressions::macro_import_statements;
use crate::transform_macro_types::MacroImport;
use crate::transform_update::update_flamework_component_config;

#[expect(clippy::too_many_lines, reason = "one Go test, ported whole")]
#[test]
fn update_flamework_component_config_generates_imported_component_payload_and_prelude_import() {
	let file = |name: &'static str, lines: &[&str]| (name, lines.join("\n"));
	let spec = StateSpec::new(
		vec![
			file("package.json", &[r#"{"name":"fixture-game","version":"1.0.0"}"#]),
			file(
				"tsconfig.json",
				&[
					r#"{"compilerOptions":{"experimentalDecorators":true,"strict":true,"rootDir":"src","outDir":"out","moduleResolution":"node"},"files":["src/globals.d.ts","src/component.ts"]}"#,
				],
			),
			file(
				"node_modules/@flamework/core/package.json",
				&[r#"{"name":"@flamework/core","version":"1.3.2"}"#],
			),
			file(
				"node_modules/@flamework/core/node_modules/@rbxts/t/package.json",
				&[r#"{"name":"@rbxts/t","version":"3.2.1"}"#],
			),
			file(
				"node_modules/@flamework/components/package.json",
				&[r#"{"name":"@flamework/components","version":"1.3.2","types":"out/index.d.ts"}"#],
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
					"/** @metadata intrinsic-component-decorator */",
					"export declare const Component: (opts?: ComponentConfig) => ClassDecorator;",
				],
			),
			file(
				"src/globals.d.ts",
				&[
					"interface Instance { readonly _nominal_Instance: unique symbol; }",
					"interface Part extends Instance { readonly _nominal_Part: unique symbol; }",
				],
			),
			file(
				"src/component.ts",
				&[
					r#"import { BaseComponent as ImportedBaseComponent, Component as ImportedComponent } from "@flamework/components";"#,
					"interface Attributes { active: boolean; label: string; retries: number; }",
					"declare const customGuard: unknown;",
					r#"@ImportedComponent({ tag: "Fixture", attributes: { active: customGuard } })"#,
					"export class FixtureComponent extends ImportedBaseComponent<Attributes, Part> {}",
					"export class DefaultInstanceComponent extends ImportedBaseComponent<Attributes> {}",
				],
			),
		],
		"src/component.ts",
	);
	let (printed, imports, default_names, import_text) = with_state(&spec, |state, source_file| {
		let class_declaration = find_named_class(source_file.as_node(), "FixtureComponent");
		let config = find_expression_kind(source_file.as_node(), Kind::ObjectLiteralExpression);
		let properties = update_flamework_component_config(state, class_declaration, config.properties().to_vec())
			.expect("updateFlameworkComponentConfig");
		let object = state
			.factory
			.new_object_literal_expression(state.factory.new_node_list(properties), true);
		let imports = decorator_runtime_imports(state, Some(source_file), &[object], &[], Vec::new());
		let default_properties = update_flamework_component_config(
			state,
			find_named_class(source_file.as_node(), "DefaultInstanceComponent"),
			Vec::new(),
		)
		.expect("updateFlameworkComponentConfig() default instance");
		let default_names: Vec<String> = default_properties
			.iter()
			.map(|property| property.name().map(|name| name.text().to_owned()).unwrap_or_default())
			.collect();
		let imported = macro_import_statements(&state.factory, &imports);
		let import_text = imported.first().map(|statement| emit_node(*statement, source_file));
		(emit_node(object, source_file), imports, default_names, import_text)
	});

	assert_eq!(
		default_names,
		["attributes"],
		"default instance component config, want only attributes guards"
	);
	let want_import = MacroImport {
		module: FLAMEWORK_PRELUDE_MODULE.to_owned(),
		export: "t".to_owned(),
		local: "t".to_owned(),
	};
	assert_eq!(imports, [want_import]);
	for want in [
		r#"tag: "Fixture""#,
		"active: customGuard",
		r#""label": t["string"]"#,
		r#""retries": t["number"]"#,
		r#""instanceGuard": t["instanceIsA"]("Part")"#,
	] {
		assert!(printed.contains(want), "component config missing {want:?}:\n{printed}");
	}
	assert!(
		!printed.contains(r#""active": t["boolean"]"#),
		"component config overwrote custom attribute guard:\n{printed}"
	);
	let config_source = printed.strip_suffix(';').unwrap_or(&printed);
	let reparsed = parse_ts(
		"/component-config.ts",
		&format!(
			"{}\nconst config = {config_source};\n",
			import_text.expect("import statement")
		),
	);
	assert!(
		reparsed.diagnostics().is_empty(),
		"component config reparse diagnostics:\n{printed}"
	);
}

/// The fixture's component config updated for `class_name`, printed as an object literal.
fn updated_config(source: &str, class_name: &str, with_config: bool) -> String {
	with_expression_fixture(source, |state, source_file| {
		let class_declaration = find_named_class(source_file.as_node(), class_name);
		let properties = if with_config {
			find_expression_kind(source_file.as_node(), Kind::ObjectLiteralExpression)
				.properties()
				.to_vec()
		} else {
			Vec::new()
		};
		let properties = update_flamework_component_config(state, class_declaration, properties)
			.expect("updateFlameworkComponentConfig");
		let object = state
			.factory
			.new_object_literal_expression(state.factory.new_node_list(properties), true);
		emit_node(object, source_file)
	})
}

#[test]
fn update_flamework_component_config_merges_generated_attribute_guards_with_custom_overrides() {
	let printed = updated_config(
		r"
class ComponentBase<A> {
	/** @metadata intrinsic-component-attributes */
	readonly attributes!: A;
}
interface Attributes { active: boolean; label: string; retries: number }
class Counter extends ComponentBase<Attributes> {}
declare const customGuard: unknown;
const config = { attributes: { active: customGuard } };
",
		"Counter",
		true,
	);

	for want in [
		"active: customGuard",
		r#""label": t["string"]"#,
		r#""retries": t["number"]"#,
	] {
		assert!(printed.contains(want), "component config missing {want:?}: {printed}");
	}
	assert!(
		!printed.contains(r#""active": t["boolean"]"#),
		"component config overwrote custom active guard: {printed}"
	);
}

#[test]
fn update_flamework_component_config_generates_instance_guard_when_subclass_narrows_instance() {
	let printed = updated_config(
		r"
declare class Instance { private _nominal_Instance: void }
declare class Part extends Instance { private _nominal_Part: void }
class ComponentBase { readonly instance!: Instance }
class Counter extends ComponentBase {
	/** @metadata intrinsic-component-instance */
	declare readonly instance: Part;
}
",
		"Counter",
		false,
	);

	let want = r#""instanceGuard": t["instanceIsA"]("Part")"#;
	assert!(printed.contains(want), "component config = {printed}, want {want:?}");
}
