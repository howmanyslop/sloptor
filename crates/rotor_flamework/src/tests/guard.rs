//! `guard_test.go`

use super::{emit_node, guard_outputs, guard_test_type_node, parse_ts, with_guard_state};
use crate::guard::build_configured_flamework_guard;
use crate::{ErrorDetail, ErrorKind};

/// Checks each named guard against its expected text.
fn assert_guards(source: &str, tests: &[(&str, &str)], reparse: bool) {
	let names: Vec<&str> = tests.iter().map(|(name, _)| *name).collect();
	for ((name, want), got) in tests.iter().zip(guard_outputs(source, &names)) {
		let got = got.unwrap_or_else(|error| panic!("{name}: buildFlameworkGuard() error = {error}"));
		assert_eq!(got, *want, "{name}");
		if reparse {
			let reparsed = parse_ts("/guard-reparse.ts", &format!("const guard = {got};"));
			assert!(reparsed.diagnostics().is_empty(), "{name}: reparse diagnostics");
		}
	}
}

#[test]
fn build_flamework_guard_emits_upstream_guards_for_representative_types() {
	assert_guards(
		r#"
type StringValue = string;
type NumberValue = number;
type BooleanValue = boolean;
type LiteralValue = "ready" | 42;
type OptionalValue = string | undefined;
type TupleValue = [string, number, boolean];
type ArrayValue = string[];
type MapValue = Map<string, number>;
type SetValue = Set<boolean>;
type ObjectValue = { name: string; count?: number };
type CallbackValue = (value: string) => number;
"#,
		&[
			("StringValue", r#"t["string"]"#),
			("NumberValue", r#"t["number"]"#),
			("BooleanValue", r#"t["boolean"]"#),
			("LiteralValue", r#"t["literal"]("ready", 42)"#),
			("OptionalValue", r#"t["optional"](t["string"])"#),
			(
				"TupleValue",
				r#"t["strictArray"](t["string"], t["number"], t["boolean"])"#,
			),
			("ArrayValue", r#"t["array"](t["string"])"#),
			("MapValue", r#"t["map"](t["string"], t["number"])"#),
			("SetValue", r#"t["set"](t["boolean"])"#),
			(
				"ObjectValue",
				r#"t["interface"]({ "name": t["string"], "count": t["optional"](t["number"]) })"#,
			),
			("CallbackValue", r#"t["callback"]"#),
		],
		true,
	);
}

#[test]
fn build_configured_flamework_guard_deduplicates_repeated_object_guard_at_configured_limit() {
	let (statements, guard) = with_guard_state(
		"\ninterface Leaf { id: string; value: number }\ninterface Repeated { first: Leaf; second: Leaf; third: Leaf }\ntype Target = Repeated;\n",
		|state, source_file| {
			state.project.config.optimizations.guard_generation_dedup_limit = Some(2);
			let type_node = guard_test_type_node(source_file, "Target");
			let type_value = state.ck().get_type_from_type_node(type_node);
			let result =
				build_configured_flamework_guard(state, type_value, type_node).expect("buildConfiguredFlameworkGuard");
			(
				result
					.statements
					.iter()
					.map(|statement| emit_node(*statement, source_file))
					.collect::<Vec<_>>(),
				emit_node(result.expression.expect("guard"), source_file),
			)
		},
	);

	assert_eq!(
		statements,
		[r#"const dedup = t["interface"]({ "id": t["string"], "value": t["number"] });"#]
	);
	assert_eq!(
		guard,
		r#"t["interface"]({ "first": dedup, "second": dedup, "third": dedup })"#
	);
}

#[test]
fn build_configured_flamework_guard_preserves_option_presence_and_zero_threshold() {
	let (absent, explicit_zero) = with_guard_state(
		"\ninterface Leaf { value: string }\ninterface Container { leaf: Leaf }\ntype Target = Container;\n",
		|state, source_file| {
			let type_node = guard_test_type_node(source_file, "Target");
			let type_value = state.ck().get_type_from_type_node(type_node);
			// The optimization is absent.
			let absent = build_configured_flamework_guard(state, type_value, type_node)
				.expect("absent buildConfiguredFlameworkGuard")
				.statements
				.len();
			// An explicit zero is clamped to upstream's minimum threshold of one.
			state.project.config.optimizations.guard_generation_dedup_limit = Some(0);
			let explicit_zero = build_configured_flamework_guard(state, type_value, type_node)
				.expect("zero buildConfiguredFlameworkGuard")
				.statements
				.len();
			(absent, explicit_zero)
		},
	);

	// Upstream does not calculate or emit dedup declarations without the option.
	assert_eq!(absent, 0, "absent optimization emitted statements");
	// Both one-use object guards receive reusable declarations.
	assert_eq!(explicit_zero, 2, "zero optimization statements");
}

#[test]
fn build_flamework_guard_returns_typed_error_for_impossible_types() {
	let names = ["UnsupportedClassValue", "TemplateValue"];
	let results = guard_outputs(
		"\nclass UnsupportedClass {}\ntype UnsupportedClassValue = UnsupportedClass;\ntype TemplateValue = `prefix-${string}`;\n",
		&names,
	);

	for (name, result) in names.iter().zip(results) {
		let error = result.expect_err(name);
		assert!(
			error.is(ErrorKind::UnsupportedGuardType),
			"{name}: error = {error}, want ErrUnsupportedGuardType"
		);
		let Some(ErrorDetail::Guard(generation)) = error.detail() else {
			panic!("{name}: error = {error:?}, want GuardGenerationError");
		};
		assert!(
			!generation.type_name.is_empty() && !generation.reason.is_empty(),
			"{name}: error = {generation:?}, want populated GuardGenerationError"
		);
	}
}

#[test]
fn build_flamework_guard_emits_upstream_guards_for_composite_and_nominal_types() {
	assert_guards(
		r#"
enum Direction { Up, Down }
type EnumValue = Direction;
type PromiseValue = Promise<string>;
type UnknownValue = unknown;
type AnyValue = any;
type UndefinedValue = undefined;
type DictionaryValue = { [key: string]: number };
type IntersectionValue = { left: string } & { right: number };
type LargeUnionValue = "a" | "b" | "c";
interface Instance { readonly _nominal_Instance: unique symbol }
interface Part extends Instance { readonly _nominal_Part: unique symbol }
type InstanceValue = Part & { child: string };
"#,
		&[
			("EnumValue", r#"t["literal"](0, 1)"#),
			("PromiseValue", r#"Promise["is"]"#),
			("UnknownValue", r#"t["union"](t["any"], t["none"])"#),
			("AnyValue", r#"t["any"]"#),
			("UndefinedValue", r#"t["none"]"#),
			("DictionaryValue", r#"t["map"](t["string"], t["number"])"#),
			(
				"IntersectionValue",
				r#"t["intersection"](t["interface"]({ "left": t["string"] }), t["interface"]({ "right": t["number"] }))"#,
			),
			("LargeUnionValue", r#"t["literalList"](["a", "b", "c"])"#),
			(
				"InstanceValue",
				r#"t["intersection"](t["instanceIsA"]("Part"), t["children"]({ "child": t["string"] }))"#,
			),
		],
		false,
	);
}

#[test]
fn build_flamework_guard_returns_typed_error_for_recursive_object() {
	let results = guard_outputs(
		"\ninterface Recursive { next: Recursive }\ntype RecursiveValue = Recursive;\n",
		&["RecursiveValue"],
	);

	let error = results.into_iter().next().expect("result").expect_err("RecursiveValue");
	assert!(
		error.is(ErrorKind::UnsupportedGuardType),
		"error = {error}, want ErrUnsupportedGuardType"
	);
}
