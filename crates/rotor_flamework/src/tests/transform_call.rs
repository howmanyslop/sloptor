//! `transform_call_test.go`

use super::{StateSpec, collect_call_expressions, emit_node, with_state};
use crate::transform_call::transform_flamework_call_expression;
use crate::transform_macro_types::MacroRuntime;

#[test]
fn transform_flamework_call_expression_injects_caller_line_for_cross_module_method_call() {
	// A call site that does NOT import Flamework surface, but calls a method whose declaration
	// has an optional Modding.Caller<"line"> parameter.
	let spec = StateSpec::new(
		vec![
			("package.json", r#"{"name":"fixture-game","version":"1.0.0"}"#.to_owned()),
			(
				"tsconfig.json",
				r#"{"compilerOptions":{"rootDir":"src","outDir":"out","moduleResolution":"node","strict":true},"include":["src/**/*.ts"]}"#
					.to_owned(),
			),
			(
				"src/store.ts",
				r#"
type CallerMetadata = { line: number; };
declare namespace Modding {
    export type Caller<M extends keyof CallerMetadata> = CallerMetadata[M] & { _flamework_macro_caller: M };
}
export interface Store {
    enqueue(name: string, value: unknown, callsiteId?: Modding.Caller<"line">): void;
}
"#
				.to_owned(),
			),
			(
				"src/main.ts",
				r#"
import { Store } from "./store";
declare const store: Store;
store.enqueue("settings.userSettings", { value: 1 });
"#
				.to_owned(),
			),
		],
		"src/main.ts",
	);
	let got = with_state(&spec, |state, source_file| {
		let calls = collect_call_expressions(source_file);
		assert_eq!(calls.len(), 1, "call count");
		let result = transform_flamework_call_expression(state, calls[0], &MacroRuntime::default())
			.expect("transformFlameworkCallExpression");
		emit_node(result.expression, source_file)
	});

	assert!(
		got.contains(r#""settings.userSettings""#),
		"transformed call lost the key argument: {got}"
	);
	// Go's `\d+ as never\)$`.
	let digits_end = got.strip_suffix(" as never)").map(|head| {
		let trimmed = head.trim_end_matches(|character: char| character.is_ascii_digit());
		head.len() - trimmed.len()
	});
	assert!(
		digits_end.is_some_and(|count| count > 0),
		"transformed call missing injected Caller<\"line\"> line number: {got}"
	);
}
