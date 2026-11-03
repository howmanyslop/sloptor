//! `arrayappendoptimization_test.go`.

use crate::statementlist::transform_statement_list;
use crate::tests::{render_list, transformer_testdata, with_state};

/// Go's `renderArrayAppendFixture`: the file's statement list with `optimizedArrayAppends` set to
/// `enabled`, requiring exactly the diagnostic codes `expected_diagnostics`, in order.
fn render_array_append_fixture(name: &str, enabled: bool, expected_diagnostics: &[&str]) -> String {
	let rel_path = format!("src/{name}.ts");
	with_state(&transformer_testdata("arrayappendoptimization"), &rel_path, |s| {
		s.optimized_array_appends = enabled;
		let file = s.source_file;
		let statements = transform_statement_list(s, Some(file.as_node()), file.statements().nodes(), None);
		let diagnostics = s.diags.flush();
		let codes: Vec<&str> = diagnostics.iter().map(|diagnostic| diagnostic.code).collect();
		assert_eq!(codes, expected_diagnostics, "diagnostics = {diagnostics:?}");
		render_list(s, statements)
	})
}

#[test]
fn optimized_array_appends_emits_loop_carried_index() {
	let want = "local result = {}
local _resultLength = 0
for _, value in source do
	-- ▼ Array.push ▼
	_resultLength += 1
	result[_resultLength] = value
	-- ▲ Array.push ▲
end
print(result)
";
	assert_eq!(render_array_append_fixture("basic", true, &[]), want);
}

#[test]
fn optimized_array_appends_handles_labeled_loops() {
	let got = render_array_append_fixture("labeled", true, &[]);

	assert!(
		!got.contains("table.insert"),
		"labeled loop retained table.insert:\n{got}"
	);
	assert!(
		got.contains("local _resultLength = 0") && got.contains("result[_resultLength] = value"),
		"labeled loop did not use a loop-carried index:\n{got}"
	);
}

#[test]
fn optimized_array_appends_skips_zero_argument_push() {
	let want = "local result = {}
for index = 0, 2 do
	local _ = #result
end
";
	assert_eq!(render_array_append_fixture("zero", true, &[]), want);
}

#[test]
fn optimized_array_appends_is_opt_in() {
	let got = render_array_append_fixture("basic", false, &[]);

	assert!(
		got.contains("table.insert(result, value)"),
		"disabled optimization changed Array.push output:\n{got}"
	);
	assert!(
		!got.contains("_resultLength"),
		"disabled optimization emitted an append counter:\n{got}"
	);
}

#[test]
fn optimized_array_appends_supports_every_loop_shape() {
	let got = render_array_append_fixture("loops", true, &[]);

	assert!(
		!got.contains("table.insert"),
		"supported loop retained table.insert:\n{got}"
	);
	for target in ["fromFor", "fromForOf", "fromWhile", "fromDo"] {
		assert!(
			got.contains(&format!("local _{target}Length = 0")) && got.contains(&format!("{target}[_{target}Length]")),
			"{target} was not optimized:\n{got}"
		);
	}
}

#[test]
fn optimized_array_appends_preserves_argument_order_and_return_length() {
	let got = render_array_append_fixture("multiple", true, &[]);

	assert!(
		!got.contains("table.insert"),
		"multi-argument push retained table.insert:\n{got}"
	);
	let first = got.find("nextValue(\"first\")");
	let second = got.find("nextValue(\"second\")");
	let first_write = got.find("_resultLength += 1");
	assert!(
		matches!((first, second, first_write), (Some(first), Some(second), Some(write)) if first <= second && second <= write),
		"push arguments were not evaluated before indexed writes:\n{got}"
	);
	assert_eq!(
		got.matches("_resultLength += 1").count(),
		4,
		"multi-site push emitted the wrong number of increments:\n{got}"
	);
	assert!(
		got.contains("local _length = _resultLength") && got.contains("local length = _length"),
		"push return value did not use the loop-carried length:\n{got}"
	);
	assert!(
		got.contains("local unchanged = #result"),
		"zero-argument push did not retain its length result:\n{got}"
	);
}

#[test]
fn optimized_array_appends_supports_nested_and_conditional_sites() {
	let got = render_array_append_fixture("nested", true, &[]);

	assert!(
		!got.contains("table.insert"),
		"nested append site retained table.insert:\n{got}"
	);
	assert_eq!(
		got.matches("_resultLength += 1").count(),
		3,
		"nested append sites did not share one counter:\n{got}"
	);
	assert_eq!(
		got.matches("local _resultLength = 0").count(),
		1,
		"nested append sites emitted more than one counter:\n{got}"
	);
}

#[test]
fn optimized_array_appends_avoids_generated_name_collisions() {
	let got = render_array_append_fixture("collision", true, &[]);

	assert!(
		got.contains("local _resultLength = 7") && got.contains("local _resultLength_1 = 0"),
		"generated counter collided with a source binding:\n{got}"
	);
}

#[test]
fn optimized_array_appends_falls_back_when_safety_cannot_be_proved() {
	let got = render_array_append_fixture("fallbacks", true, &["noVarArgsMacroSpread", "noAny"]);

	assert!(
		!got.contains("Length = 0"),
		"unsafe append target was optimized:\n{got}"
	);
	for target in [
		"nonEmpty",
		"mutable",
		"separated",
		"observed",
		"aliased",
		"captured",
		"mutated",
		"indexed",
		"spread",
		"anyValues",
		"condition",
		"increment",
		"iterated",
		"first",
	] {
		assert!(
			got.contains(&format!("table.insert({target}")),
			"{target} did not retain the Array.push fallback:\n{got}"
		);
	}
	assert!(
		got.contains("custom:push(value)"),
		"user-defined push did not retain a method call:\n{got}"
	);
}

#[test]
fn optimized_array_appends_rejects_out_of_loop_captures() {
	let got = render_array_append_fixture("capture", true, &[]);

	assert!(
		!got.contains("_resultLength"),
		"captured array was optimized with a stale counter:\n{got}"
	);
	assert_eq!(
		got.matches("table.insert(result").count(),
		2,
		"captured array did not retain both Array.push calls:\n{got}"
	);
	assert_eq!(
		got.matches("table.insert(later").count(),
		2,
		"array captured by a later declaration did not retain both Array.push calls:\n{got}"
	);
}

#[test]
fn optimized_array_appends_snapshots_consumed_return_values() {
	let got = render_array_append_fixture("valueuses", true, &[]);

	assert!(
		!got.contains("table.insert"),
		"consumed push retained table.insert:\n{got}"
	);
	for snapshot in [
		"local _length = _resultLength",
		"local _length_1 = _resultLength",
		"local _length_2 = _resultLength",
	] {
		assert!(
			got.contains(snapshot),
			"missing return-value snapshot {snapshot:?}:\n{got}"
		);
	}
	assert!(
		got.contains("print(_length, _length_1)") && got.contains("result[_resultLength] = _length_2"),
		"later pushes can change an earlier returned length:\n{got}"
	);
}
