//! `transform_macros_test.go`

use std::rc::Rc;

use super::{StateSpec, collect_call_expressions, emit_node, with_state};
use crate::transform_macro_types::MacroRuntime;
use crate::transform_macros::transform_flamework_call;

fn fixed_uuid(uuid: &'static str) -> MacroRuntime {
	MacroRuntime {
		uuid: Some(Rc::new(move || Ok(uuid.to_owned()))),
		..MacroRuntime::default()
	}
}

/// Every call in `src/main.ts` transformed with `runtime`, printed; `prime` calls are transformed
/// first and not printed.
fn transform_calls(tsconfig: &str, lines: &[&str], runtime: fn() -> MacroRuntime, prime: usize) -> Vec<String> {
	let spec = StateSpec::new(
		vec![
			(
				"package.json",
				r#"{"name":"fixture-game","version":"1.0.0"}"#.to_owned(),
			),
			("tsconfig.json", tsconfig.to_owned()),
			("src/main.ts", lines.join("\n")),
		],
		"src/main.ts",
	);
	with_state(&spec, |state, file| {
		let runtime = runtime();
		collect_call_expressions(file)
			.into_iter()
			.enumerate()
			.filter_map(|(index, call)| {
				let result = transform_flamework_call(state, call, &runtime)
					.unwrap_or_else(|error| panic!("transformFlameworkCall({index}) error = {error}"));
				(index >= prime).then(|| emit_node(result.expression.expect("macro expression"), file))
			})
			.collect()
	})
}

const STRICT: &str = r#"{"compilerOptions":{"strict":true},"include":["src/**/*.ts"]}"#;

#[test]
fn transform_flamework_call_builds_generic_literal_caller_and_tuple_macros() {
	let printed = transform_calls(
		STRICT,
		&[
			"type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };",
			"type Caller<M extends string> = { _flamework_macro_caller: M };",
			"type Many<T> = { _flamework_macro_many: T };",
			"declare function inspect<T>(value?: T): T;",
			r#"inspect<Generic<"payload", "text">>();"#,
			r#"inspect<Many<["left", 7, true, undefined]>>();"#,
			r#"inspect<Many<Array<"left" | 7>>>();"#,
			r#"inspect<Caller<"line">>();"#,
		],
		|| fixed_uuid("00000000-0000-4000-8000-000000000000"),
		0,
	);

	assert_eq!(
		printed,
		[
			r#"inspect<Generic<"payload", "text">>("\"payload\"" as never)"#,
			"inspect<Many<[\n    \"left\",\n    7,\n    true,\n    undefined\n]>>([\n    \"left\" as never,\n    7 as never,\n    true as never,\n    undefined as never\n] as never)",
			"inspect<Many<Array<\"left\" | 7>>>([\n    \"left\" as never,\n    7 as never\n] as never)",
			r#"inspect<Caller<"line">>(8 as never)"#,
		]
	);
}

#[test]
fn transform_flamework_call_builds_mapped_generic_many_from_original_program_call() {
	let printed = transform_calls(
		r#"{"compilerOptions":{"noLib":true,"strict":true},"files":["src/main.ts"]}"#,
		&[
			"interface GenericMetadata<T> { id: string; text: string; }",
			"type Many<T> = T & { _flamework_macro_many: T };",
			"type Generic<T, M extends keyof GenericMetadata<T>> = GenericMetadata<T>[M] & { _flamework_macro_generic: [T, M] };",
			"type GenericMany<T, M extends keyof GenericMetadata<T>> = Many<{ [K in M]: Generic<T, K> }>;",
			"declare function inspect<T>(value?: Many<T>): T;",
			"interface Payload { count: number; enabled: boolean; label: string; }",
			r#"inspect<GenericMany<Payload, "id" | "text">>();"#,
		],
		MacroRuntime::default,
		0,
	);

	assert_eq!(printed.len(), 1, "call count");
	assert!(
		printed[0].contains(r#""id":"#) && printed[0].contains(r#""text":"#),
		"mapped GenericMany output = {:?}, want id and text members",
		printed[0]
	);
}

#[test]
fn transform_flamework_call_preserves_caller_many_literal_creation_order() {
	let printed = transform_calls(
		STRICT,
		&[
			"interface GenericMetadata<T> { id: string; text: string; }",
			"interface Payload { value: string; }",
			"type Many<T> = T & { _flamework_macro_many: T };",
			"type Generic<T, M extends keyof GenericMetadata<T>> = GenericMetadata<T>[M] & { _flamework_macro_generic: [T, M] };",
			"type GenericMany<T, M extends keyof GenericMetadata<T>> = Many<{ [K in M]: Generic<T, K> }>;",
			"type Caller<M extends string> = { _flamework_macro_caller: M };",
			"type CallerMany<M extends string> = Many<{ [K in M]: Caller<K> }>;",
			"declare function inspect<T>(value?: Many<T>): T;",
			r#"inspect<GenericMany<Payload, "id" | "text">>();"#,
			r#"inspect<CallerMany<"line" | "character" | "text" | "uuid">>();"#,
		],
		|| fixed_uuid("00000000-0000-4000-8000-000000000004"),
		1,
	);

	assert_eq!(
		printed,
		[
			r#"inspect<CallerMany<"line" | "character" | "text" | "uuid">>({ "text": "inspect<CallerMany<\"line\" | \"character\" | \"text\" | \"uuid\">>()" as never, "line": 10 as never, "character": 1 as never, "uuid": "00000000-0000-4000-8000-000000000004" as never } as never)"#
		]
	);
}
