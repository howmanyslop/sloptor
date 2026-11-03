//! `transform_reflection_test.go`

use super::{StateSpec, find_named_class, parse_ts, print_with_statements, replace_class_syntax_list, with_state};
use crate::plan::{ClassPlan, DecoratorPlan, FileAnalysis};
use crate::transform_class::transform_flamework_class;

/// `transformReflectionFixture`
fn transform_reflection_fixture(lines: &[&str]) -> String {
	let mut spec = StateSpec::new(
		vec![
			("package.json", r#"{"name":"fixture","version":"1.0.0"}"#.to_owned()),
			(
				"tsconfig.json",
				r#"{"compilerOptions":{"experimentalDecorators":true,"strict":true,"rootDir":"src","outDir":"out"},"files":["src/subject.ts"]}"#
					.to_owned(),
			),
			("src/subject.ts", lines.join("\n")),
		],
		"src/subject.ts",
	);
	spec.config.hash_prefix = "fixture".to_owned();
	spec.analyses = Some(vec![FileAnalysis {
		file_id: "src/subject.ts".to_owned(),
		classes: vec![
			ClassPlan {
				internal_id: "fixture:out/subject@Dependency".to_owned(),
				..ClassPlan::default()
			},
			ClassPlan {
				internal_id: "fixture:out/subject@Subject".to_owned(),
				decorators: vec![DecoratorPlan {
					name: "Service".to_owned(),
					internal_id: "fixture:out/subject@Service".to_owned(),
				}],
				..ClassPlan::default()
			},
		],
		..FileAnalysis::default()
	}]);
	let printed = with_state(&spec, |state, source_file| {
		let class_node = find_named_class(source_file.as_node(), "Subject");
		let plan = state.plans[0].clone();
		let transformed = transform_flamework_class(state, &plan, class_node).expect("transformFlameworkClass");
		print_with_statements(
			&state.factory,
			source_file,
			replace_class_syntax_list(source_file, class_node, transformed),
		)
	});
	assert!(
		parse_ts("/reflection.ts", &printed).diagnostics().is_empty(),
		"reparse diagnostics:\n{printed}"
	);
	printed
}

/// `reflectionStatementsFor`: the `defineMetadata` statements for `property`.
fn reflection_statements_for(printed: &str, property: &str) -> String {
	let property_suffix = format!(r#", "{property}");"#);
	printed
		.split('\n')
		.map(str::trim)
		.filter(|line| line.starts_with(r#"Reflect["defineMetadata"]"#) && line.ends_with(&property_suffix))
		.collect::<Vec<_>>()
		.join("\n")
}

#[test]
fn member_reflection_preserves_existing_ordered_metadata() {
	let printed = transform_reflection_fixture(&[
		r#"type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };"#,
		"/** @metadata flamework:implements flamework:parameters */",
		"declare const Service: () => FlameworkDecorator;",
		"interface Lifecycle { start(): Dependency; }",
		"class Dependency {}",
		"@Service()",
		"export class Subject implements Lifecycle {",
		"  constructor(dependency: Dependency) {}",
		"  /** @metadata flamework:type */",
		"  field: Dependency;",
		"  /** @metadata flamework:return_type */",
		"  start(): Dependency { return new Dependency(); }",
		"}",
	]);

	let start = printed
		.find(r#"Reflect["defineMetadata"]"#)
		.expect("reflection block not found");
	let end = printed[start..].find("\n    }").expect("reflection block not found");
	let reflection = &printed[start..start + end];
	let want = [
		r#"Reflect["defineMetadata"](Subject, "identifier", "fixture:subject@Subject");"#,
		r#"        Reflect["defineMetadata"](Subject, "flamework:parameters", ["fixture:subject@Dependency"]);"#,
		r#"        Reflect["defineMetadata"](Subject, "flamework:implements", ["fixture:subject@Lifecycle"]);"#,
		r#"        Reflect["defineMetadata"](Subject, "flamework:type", "fixture:subject@Dependency", "field");"#,
		r#"        Reflect["defineMetadata"](Subject, "flamework:return_type", "fixture:subject@Dependency", "start");"#,
	]
	.join("\n");
	assert_eq!(reflection, want);
}

#[test]
fn member_reflection_emits_exact_method_and_parameter_metadata() {
	let printed = transform_reflection_fixture(&[
		r#"type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };"#,
		"declare const Service: () => FlameworkDecorator;",
		"interface Options { count: number; }",
		"interface Reflected {",
		"  /** @metadata flamework:return_type flamework:return_guard flamework:parameters flamework:parameter_names flamework:parameter_guards */",
		"  execute(value: string, binding: Options): { ok: boolean };",
		"}",
		"@Service()",
		"export class Subject implements Reflected {",
		"  execute(value: string, { count }: Options) { return { ok: count > value.length }; }",
		"}",
	]);

	let metadata = reflection_statements_for(&printed, "execute");
	let want = [
		r#"Reflect["defineMetadata"](Subject, "flamework:return_type", "fixture:subject@Subject.execute", "execute");"#,
		r#"Reflect["defineMetadata"](Subject, "flamework:return_guard", t["interface"]({ "ok": t["boolean"] }), "execute");"#,
		r#"Reflect["defineMetadata"](Subject, "flamework:parameters", ["$p:string", "fixture:subject@Options"], "execute");"#,
		r#"Reflect["defineMetadata"](Subject, "flamework:parameter_names", ["value", "_binding_"], "execute");"#,
		r#"Reflect["defineMetadata"](Subject, "flamework:parameter_guards", [t["string"], t["interface"]({ "count": t["number"] })], "execute");"#,
	]
	.join("\n");
	assert_eq!(metadata, want, "transformed TypeScript:\n{printed}");
}

#[test]
fn field_reflection_emits_inferred_type_and_guard_requested_by_interface() {
	let printed = transform_reflection_fixture(&[
		r#"type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };"#,
		"declare const Service: () => FlameworkDecorator;",
		"interface Reflected {",
		"  /** @metadata flamework:type flamework:guard */",
		"  label: string;",
		"}",
		"@Service()",
		r#"export class Subject implements Reflected { label = "ready"; }"#,
	]);

	let metadata = reflection_statements_for(&printed, "label");
	let want = [
		r#"Reflect["defineMetadata"](Subject, "flamework:type", "$p:string", "label");"#,
		r#"Reflect["defineMetadata"](Subject, "flamework:guard", t["string"], "label");"#,
	]
	.join("\n");
	assert_eq!(metadata, want, "transformed TypeScript:\n{printed}");
}

#[test]
fn field_reflection_emits_explicit_type_and_guard() {
	let printed = transform_reflection_fixture(&[
		r#"type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };"#,
		"declare const Service: () => FlameworkDecorator;",
		"interface Options { count: number; }",
		"@Service()",
		"export class Subject {",
		"  /** @metadata flamework:type flamework:guard */",
		"  options: Options;",
		"}",
	]);

	let metadata = reflection_statements_for(&printed, "options");
	let want = [
		r#"Reflect["defineMetadata"](Subject, "flamework:type", "fixture:subject@Options", "options");"#,
		r#"Reflect["defineMetadata"](Subject, "flamework:guard", t["interface"]({ "count": t["number"] }), "options");"#,
	]
	.join("\n");
	assert_eq!(metadata, want, "transformed TypeScript:\n{printed}");
}

#[test]
fn method_reflection_emits_constructor_parameter_metadata() {
	let printed = transform_reflection_fixture(&[
		r#"type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };"#,
		"declare const Service: () => FlameworkDecorator;",
		"interface Dependency { ready: boolean; }",
		"interface Contract {}",
		"/** @metadata flamework:parameters flamework:parameter_names flamework:parameter_guards flamework:implements */",
		"@Service()",
		"export class Subject implements Contract { constructor(dependency: Dependency, { enabled }: { enabled: boolean }) {} }",
	]);

	let mut previous = None;
	for want in [
		r#"Reflect["defineMetadata"](Subject, "identifier", "fixture:subject@Subject");"#,
		r#"Reflect["defineMetadata"](Subject, "flamework:parameters", ["fixture:subject@Dependency", "fixture:subject@Subject"]);"#,
		r#"Reflect["defineMetadata"](Subject, "flamework:parameter_names", ["dependency", "_binding_"]);"#,
		r#"Reflect["defineMetadata"](Subject, "flamework:parameter_guards", [t["interface"]({ "ready": t["boolean"] }), t["interface"]({ "enabled": t["boolean"] })]);"#,
		r#"Reflect["defineMetadata"](Subject, "flamework:implements", ["fixture:subject@Contract"]);"#,
	] {
		let position = printed
			.find(want)
			.unwrap_or_else(|| panic!("constructor metadata missing {want:?}:\n{printed}"));
		assert!(
			Some(position) > previous,
			"constructor metadata out of order at {want:?}:\n{printed}"
		);
		previous = Some(position);
	}
}
