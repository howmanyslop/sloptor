//! `transform_decorators_uid_test.go`

use super::{TransformSpec, run_transform};

/// `writeCrossFileDecoratorFixture` and `transformCrossFileDecoratorFixture`: a Flamework
/// decorator declared in one project file and applied in another, so the declaration site and
/// the use site disagree about which UID the decorator should carry. The consumer, printed.
fn transform_cross_file_decorator_fixture() -> String {
	let output = run_transform(TransformSpec {
		tsconfig: r#"{"compilerOptions":{"experimentalDecorators":true,"moduleResolution":"node","rootDir":"src","outDir":"out"},"files":["src/decorators.ts","src/consumer.ts"]}"#,
		files: vec![
			(
				"src/decorators.ts",
				[
					r#"type MethodDecorator = ((target: object, property: string) => void) & { _flamework_Decorator: "Method" };"#,
					"export declare function BindToAxis(action: string): MethodDecorator;",
					r#"export declare function BoundClass(): ((target: object) => void) & { _flamework_Decorator: "Class" };"#,
				]
				.join("\n"),
			),
			(
				"src/consumer.ts",
				[
					r#"import { BindToAxis, BoundClass } from "./decorators";"#,
					"@BoundClass()",
					"export class Bound {",
					r#"	@BindToAxis("pressing")"#,
					"\tpublic sprint(): void {}",
					"}",
				]
				.join("\n"),
			),
		],
		targets: vec!["src/decorators.ts", "src/consumer.ts"],
		out_dir: "out",
		..TransformSpec::default()
	});
	assert!(output.error.is_none(), "{:?}", output.error);
	output.printed[1].clone()
}

// Reflect.decorate must key on the file the decorator is declared in. Keying on the file applying
// it silently breaks Modding.getDecorators and friends, which resolve `typeof Decorator` to the
// declaration UID, and makes two use sites of the same decorator disagree with each other.
#[test]
fn transform_keys_decorator_uid_on_declaration_site_when_applied_from_another_file() {
	let printed = transform_cross_file_decorator_fixture();

	for want in [r#""decorators@BindToAxis""#, r#""decorators@BoundClass""#] {
		assert!(
			printed.contains(want),
			"decorator UID {want} missing from transformed consumer:\n{printed}"
		);
	}
	assert!(
		!printed.contains("consumer@BindToAxis") && !printed.contains("consumer@BoundClass"),
		"decorator UID was keyed on the use site:\n{printed}"
	);
}
