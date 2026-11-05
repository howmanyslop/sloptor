//! `transform_reflection_multifile_test.go`

use super::{TransformSpec, run_transform};

#[test]
fn transform_emits_constructor_reflection_when_program_has_imported_interface() {
	let output = run_transform(TransformSpec {
		tsconfig: r#"{"compilerOptions":{"experimentalDecorators":true,"strict":true,"rootDir":"src","outDir":"out"},"files":["src/core.ts","src/service.ts"]}"#,
		files: vec![
			(
				"src/core.ts",
				[
					r#"export type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };"#,
					"export declare const Service: () => FlameworkDecorator;",
					"/** @metadata flamework:parameters */",
					"export interface OnStart { onStart(): void; }",
				]
				.join("\n"),
			),
			(
				"src/service.ts",
				[
					r#"import { OnStart, Service } from "./core";"#,
					"declare function print(value: unknown): void;",
					"@Service() export class LoggerService {}",
					"@Service() export class ConsumerService implements OnStart {",
					"  constructor(private readonly logger: LoggerService) {}",
					"  onStart(): void { print(this.logger); }",
					"}",
				]
				.join("\n"),
			),
		],
		targets: vec!["src/service.ts"],
		config: rotor_config::config::FlameworkConfig {
			hash_prefix: "fixture".to_owned(),
			..rotor_config::config::FlameworkConfig::default()
		},
		out_dir: "out",
		..TransformSpec::default()
	});
	assert!(output.error.is_none(), "{:?}", output.error);
	let printed = &output.printed[0];

	assert!(
		printed.contains(
			r#"Reflect["defineMetadata"](ConsumerService, "flamework:parameters", ["fixture:service@LoggerService"]);"#
		),
		"constructor reflection missing from transformed source:\n{printed}"
	);
}
