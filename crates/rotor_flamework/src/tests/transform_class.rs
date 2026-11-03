//! `transform_class_test.go`

use super::{StateSpec, parse_ts, print_with_statements, replace_class_syntax_list, with_state};
use crate::plan::{ClassPlan, DecoratorPlan, FileAnalysis};
use crate::transform_class::transform_flamework_class;

#[test]
fn transform_flamework_class_emits_reflection_and_decorators_when_class_is_planned() {
	let source = [
		r#"type FlameworkDecorator = ClassDecorator & MethodDecorator & { _flamework_Decorator: "Class" };"#,
		"/** @metadata flamework:implements flamework:parameters */",
		"declare const First: (label: string) => FlameworkDecorator;",
		"/** @metadata flamework:implements flamework:parameters */",
		"declare const Second: () => FlameworkDecorator;",
		"interface OnStart { onStart(): Logger; }",
		"class Logger {}",
		r#"@First("fixture")"#,
		"@Second()",
		"export class Consumer implements OnStart {",
		"\tconstructor(logger: Logger) {}",
		"\t/** @metadata flamework:type */",
		"\tvalue: Logger;",
		"\t/** @metadata flamework:return_type */",
		r#"	@First("member")"#,
		"\tonStart(): Logger { return new Logger(); }",
		"}",
		"",
	]
	.join("\n");
	let mut spec = StateSpec::new(
		vec![
			("package.json", r#"{"name":"fixture-game","version":"1.0.0"}"#.to_owned()),
			(
				"tsconfig.json",
				r#"{"compilerOptions":{"experimentalDecorators":true,"rootDir":"src","outDir":"out"},"include":["src/**/*.ts"]}"#
					.to_owned(),
			),
			("src/service.ts", source),
		],
		"src/service.ts",
	);
	spec.config.hash_prefix = "fixture".to_owned();
	let decorator = |name: &str| DecoratorPlan {
		name: name.to_owned(),
		internal_id: format!("fixture-game:out/service@{name}"),
	};
	spec.analyses = Some(vec![FileAnalysis {
		file_id: "src/service.ts".to_owned(),
		classes: vec![
			ClassPlan {
				internal_id: "fixture-game:out/service@Logger".to_owned(),
				..ClassPlan::default()
			},
			ClassPlan {
				internal_id: "fixture-game:out/service@Consumer".to_owned(),
				decorators: vec![decorator("First"), decorator("Second")],
				..ClassPlan::default()
			},
		],
		..FileAnalysis::default()
	}]);
	let printed = with_state(&spec, |state, source_file| {
		let class_node = *source_file
			.statements()
			.nodes()
			.iter()
			.find(|statement| {
				rotor_tsrs::ast::is_class_declaration(**statement)
					&& statement.name().is_some_and(|name| name.text() == "Consumer")
			})
			.expect("Consumer class not found");
		let plan = state.plans[0].clone();
		let transformed = transform_flamework_class(state, &plan, class_node).expect("transformFlameworkClass");
		print_with_statements(
			&state.factory,
			source_file,
			replace_class_syntax_list(source_file, class_node, transformed),
		)
	});

	for fragment in [
		"export class Consumer implements OnStart",
		r#"Reflect["defineMetadata"](Consumer, "identifier", "fixture:service@Consumer");"#,
		r#"Reflect["defineMetadata"](Consumer, "flamework:parameters", ["fixture:service@Logger"]);"#,
		r#"Reflect["defineMetadata"](Consumer, "flamework:implements", ["fixture:service@OnStart"]);"#,
		r#"Reflect["defineMetadata"](Consumer, "flamework:type", "fixture:service@Logger", "value");"#,
		r#"Reflect["defineMetadata"](Consumer, "flamework:return_type", "fixture:service@Logger", "onStart");"#,
		r#"Reflect["decorate"](Consumer, "fixture:service@Second", Second, []);"#,
		r#"Reflect["decorate"](Consumer, "fixture:service@First", First, ["fixture"]);"#,
		r#"Reflect["decorate"](Consumer, "fixture:service@First", First, ["member"], "onStart", false);"#,
	] {
		assert!(
			printed.contains(fragment),
			"transformed class missing {fragment:?}:\n{printed}"
		);
	}
	assert!(
		!printed.contains("@First(") && !printed.contains("@Second("),
		"Flamework decorators were not stripped:\n{printed}"
	);
	assert!(
		parse_ts("/transformed.ts", &printed).diagnostics().is_empty(),
		"reparse diagnostics:\n{printed}"
	);
}
