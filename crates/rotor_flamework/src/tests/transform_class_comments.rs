//! `transform_class_comments_test.go`

use super::{TransformSpec, run_transform};

/// `transformSectionCommentFixture`
fn transform_section_comment_fixture(source: &str) -> String {
	let output = run_transform(TransformSpec {
		tsconfig: r#"{"compilerOptions":{"experimentalDecorators":true,"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}"#,
		files: vec![("src/service.ts", source.to_owned())],
		targets: vec!["src/service.ts"],
		out_dir: "out",
		..TransformSpec::default()
	});
	assert!(output.error.is_none(), "{:?}", output.error);
	output.printed[0].clone()
}

#[test]
fn transform_labels_every_section_with_its_own_declaration() {
	let source = [
		r#"type FW = ClassDecorator & MethodDecorator & PropertyDecorator & { _flamework_Decorator: "Class" };"#,
		"declare const Bound: () => FW;",
		"/** @metadata flamework:type */",
		"declare const Typed: () => FW;",
		"@Bound()",
		"export class Counter {",
		r#" @Typed() label = "idle";"#,
		" @Bound() sprint(): void {}",
		"}",
		"",
	]
	.join("\n");
	let printed = transform_section_comment_fixture(&source);
	for want in [
		"// (Flamework) Counter metadata",
		"// (Flamework) Counter.label metadata",
		"// (Flamework) Counter decorators",
		"// (Flamework) Counter.label decorators",
		"// (Flamework) Counter.sprint decorators",
	] {
		assert_eq!(
			printed.matches(want).count(),
			1,
			"expected exactly one {want:?}:\n{printed}"
		);
	}
}

#[test]
fn transform_omits_class_decorator_section_when_only_members_are_decorated() {
	let source = [
		r#"type FW = MethodDecorator & PropertyDecorator & { _flamework_Decorator: "Class" };"#,
		"declare const Bound: () => FW;",
		"export class Counter {",
		" @Bound() count = 0;",
		" @Bound() sprint(): void {}",
		"}",
		"",
	]
	.join("\n");
	let printed = transform_section_comment_fixture(&source);
	assert!(!printed.contains("// (Flamework) Counter decorators"), "{printed}");
	for want in [
		"// (Flamework) Counter.count decorators",
		"// (Flamework) Counter.sprint decorators",
	] {
		assert_eq!(
			printed.matches(want).count(),
			1,
			"expected exactly one {want:?}:\n{printed}"
		);
	}
}
