//! `guard_boolean_test.go`

use super::{emit_node, guard_test_type_node, with_guard_state};
use crate::guard::build_flamework_guard;

// A union holding both boolean literals is a full `boolean`. The bare `boolean` type takes a fast
// path in buildUnion, but inside a union the type is no longer identical to
// checker.GetBooleanType() and used to fall through to the literal collector, emitting
// t.literal(false, true).
#[test]
fn build_flamework_guard_collapses_boolean_literal_pairs_inside_unions() {
	let tests = [
		("OptionalBool", r#"t["optional"](t["boolean"])"#),
		// The collapsed boolean leads, matching the reference, which pushes it onto the type list
		// before walking the remaining members.
		("BoolOrString", r#"t["union"](t["boolean"], t["string"])"#),
		("BoolOrLiteral", r#"t["union"](t["boolean"], t["literal"]("ready"))"#),
		// One boolean literal is not a boolean.
		("SingleTrue", r#"t["optional"](t["literal"](true))"#),
		("TrueOrString", r#"t["union"](t["string"], t["literal"](true))"#),
		(
			"Shape",
			r#"t["interface"]({ "flag": t["optional"](t["boolean"]), "name": t["string"] })"#,
		),
	];
	let source = r#"
type OptionalBool = boolean | undefined;
type BoolOrString = boolean | string;
type BoolOrLiteral = boolean | "ready";
type SingleTrue = true | undefined;
type TrueOrString = true | string;
type Shape = { flag?: boolean; name: string };
"#;
	let results = with_guard_state(source, |state, source_file| {
		tests
			.iter()
			.map(|(name, _)| {
				let type_node = guard_test_type_node(source_file, name);
				let type_value = state.ck().get_type_from_type_node(type_node);
				build_flamework_guard(state, type_value, type_node)
					.map(|guard| emit_node(guard, source_file))
					.map_err(|error| error.message().to_owned())
			})
			.collect::<Vec<_>>()
	});

	for ((name, want), got) in tests.iter().zip(results) {
		assert_eq!(got.as_deref(), Ok(*want), "{name}");
	}
}
