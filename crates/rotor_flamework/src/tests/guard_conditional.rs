//! `guard_conditional_test.go`

use super::{guard_outputs, guard_test_type_node, parse_ts, with_guard_state};
use crate::{ErrorDetail, ErrorKind};

#[test]
fn build_flamework_guard_emits_resolved_guard_for_instantiated_conditional() {
	let results = guard_outputs(
		"\ntype InstantiatedConditional = \"ready\" extends string ? string : number;\n",
		&["InstantiatedConditional"],
	);

	assert_eq!(results[0].as_deref().ok(), Some(r#"t["string"]"#), "{:?}", results[0]);
}

#[test]
fn build_flamework_guard_emits_union_for_unresolved_generic_conditional() {
	let results = guard_outputs(
		"\ntype UnresolvedGenericConditional<T> = T extends string ? string : number;\n",
		&["UnresolvedGenericConditional"],
	);

	let got = results[0].as_deref().expect("buildFlameworkGuard");
	assert_eq!(got, r#"t["union"](t["string"], t["number"])"#);
	let reparsed = parse_ts("/guard-reparse.ts", &format!("const guard = {got};"));
	assert!(reparsed.diagnostics().is_empty(), "reparse diagnostics");
}

#[test]
fn build_flamework_guard_returns_upstream_diagnostic_for_unconstrained_generic_conditional() {
	let mut results = guard_outputs(
		"\ntype UnconstrainedGenericConditional<T> = T extends string ? T : number;\n",
		&["UnconstrainedGenericConditional"],
	);

	let error = results.remove(0).expect_err("buildFlameworkGuard");
	assert!(
		error.is(ErrorKind::UnsupportedGuardType),
		"error = {error}, want ErrUnsupportedGuardType"
	);
	let Some(ErrorDetail::Guard(generation)) = error.detail() else {
		panic!("error = {error:?}, want GuardGenerationError");
	};
	assert_eq!(generation.reason, "could not find constraint of type parameter");
}

#[test]
fn checker_combined_mapper_instantiation() {
	let tests: [(&str, &str, &[&str], &[&str]); 2] = [
		(
			"nested_object",
			r#"
type Box<T> = { value: T };
type Wrap<U> = Box<{ wrapped: U }>;
type Actual = Wrap<string>;
type Expected = string;
declare const wrongNumber: Wrap<number>;
declare const wrongBoolean: Wrap<boolean>;
const okW: Actual = { value: { wrapped: "a" } };
const badW: Actual = wrongNumber;
const badWAgain: Actual = wrongBoolean;
"#,
			&["value", "wrapped"],
			&["badW", "badWAgain"],
		),
		(
			"conditional_tail",
			r#"
type Loop<T, N extends unknown[]> = N["length"] extends 3 ? T : Loop<T, [...N, 0]>;
type Actual = Loop<"x", []>;
type Expected = "x";
const okL: Actual = "x";
const badL: Actual = "y";
const badLAgain: Actual = "z";
"#,
			&[],
			&["badL", "badLAgain"],
		),
	];
	for (name, source, property_path, bad_names) in tests {
		with_guard_state(source, |state, source_file| {
			let node = guard_test_type_node(source_file, "Actual");
			let want = state
				.ck()
				.get_type_from_type_node(guard_test_type_node(source_file, "Expected"));
			let before = state.checker.total_instantiation_count;
			let actual = state.ck().get_type_from_type_node(node);
			let mut leaf = actual;
			for property_name in property_path {
				let property = state
					.ck()
					.get_property_of_type(leaf, property_name)
					.unwrap_or_else(|| panic!("{name}: missing property {property_name:?}"));
				leaf = state.ck().get_type_of_symbol(property);
			}
			let delta = state.checker.total_instantiation_count - before;
			assert_eq!(leaf, want, "{name}: resolved type, want expected type");
			assert_ne!(delta, 0, "{name}: generic query performed no instantiations");
			assert_eq!(
				state.ck().get_type_from_type_node(node),
				actual,
				"{name}: repeated result"
			);

			// Check diagnostics after the query so checking assignments cannot warm it.
			let diagnostics = state
				.checker
				.get_diagnostics_exported(&rotor_tsrs::Context::default(), source_file);
			assert_eq!(
				diagnostics.len(),
				bad_names.len(),
				"{name}: want generic assignment errors"
			);
			for (diagnostic, bad_name) in diagnostics.iter().zip(bad_names) {
				let declaration_pos = source
					.find(&format!("const {bad_name}:"))
					.unwrap_or_else(|| panic!("missing bad assignment {bad_name:?}"));
				let want_pos = u32::try_from(declaration_pos + "const ".len()).expect("pos");
				assert!(
					diagnostic.code() == 2322
						&& diagnostic.file() == Some(source_file)
						&& diagnostic.pos() == Some(want_pos)
						&& diagnostic.len() == u32::try_from(bad_name.len()).ok(),
					"{name}: diagnostic = TS{} {:?}+{:?}, want TS2322 on {bad_name} at {want_pos}",
					diagnostic.code(),
					diagnostic.pos(),
					diagnostic.len()
				);
			}
		});
	}
}
