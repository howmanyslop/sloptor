//! `transform_class_accessors_test.go`

use super::{find_named_class, print_with_statements, replace_class_syntax_list, with_decorator_fixture};
use crate::plan::{ClassPlan, DecoratorPlan};
use crate::transform_class::transform_flamework_class;

#[test]
fn transform_flamework_class_strips_and_lowers_accessor_decorators_in_source_order() {
	let source = [
		r#"type FW = MethodDecorator & { _flamework_Decorator: "Class" };"#,
		"declare const Decorate: (label: string) => FW;",
		"class Counter {",
		r#" @Decorate("get") get value(): number { return 1 }"#,
		r#" @Decorate("set") set value(next: number) { void next }"#,
		"}",
	]
	.join("\n");
	let plan = ClassPlan {
		internal_id: "fixture:out/service@Counter".to_owned(),
		decorators: vec![DecoratorPlan {
			name: "Decorate".to_owned(),
			internal_id: "fixture:out/service@Decorate".to_owned(),
		}],
		..ClassPlan::default()
	};
	let printed = with_decorator_fixture(&source, plan, |state, file| {
		let class_node = find_named_class(file.as_node(), "Counter");
		let plan = state.plans[0].clone();
		let transformed = transform_flamework_class(state, &plan, class_node).expect("transformFlameworkClass");
		print_with_statements(
			&state.factory,
			file,
			replace_class_syntax_list(file, class_node, transformed),
		)
	});

	assert!(
		!printed.contains("\n    @Decorate") && printed.matches(r#"Reflect["decorate"]"#).count() == 2,
		"accessor decorator lowering mismatch:\n{printed}"
	);
	let get_index = printed.find(r#"["get"], "value", false"#);
	let set_index = printed.find(r#"["set"], "value", false"#);
	assert!(
		get_index.is_some() && set_index > get_index,
		"accessor decorator order mismatch:\n{printed}"
	);
}
