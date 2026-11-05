//! `array_append_optimization_test.go`

use std::collections::BTreeMap;

use super::{fixture_project_dir, large_stack};
use crate::ProjectOptions;
use crate::compile::compile_file_detailed_with_options;

const SOURCE: &str = "declare const source: ReadonlyArray<number>;
const result = new Array<number>();
for (const value of source) {
	result.push(value);
}
print(result);

const optional: Array<number> | undefined = [];
for (const value of source) {
	optional?.push(value);
}

export const exported = new Array<number>();
for (const value of source) {
	exported.push(value);
}
";

fn compile(enabled: bool) -> String {
	let project = fixture_project_dir();
	let input = format!("{project}/src/01_literals.ts");
	let opts = ProjectOptions {
		optimized_array_appends: enabled,
		overlays: BTreeMap::from([(input, SOURCE.to_owned())]),
		..ProjectOptions::default()
	};
	let compiled = match large_stack(|| compile_file_detailed_with_options(&project, "src/01_literals.ts", &opts)) {
		Ok(compiled) => compiled,
		Err(failure) => panic!("compile: {failure} (diagnostics: {:?})", failure.messages()),
	};
	assert!(
		compiled.diagnostics.is_empty(),
		"diagnostics: {:?}",
		compiled.diagnostics
	);
	String::from_utf8_lossy(&compiled.text).into_owned()
}

#[test]
fn compile_file_optimized_array_appends_option() {
	let baseline = compile(false);
	assert!(
		baseline.contains("table.insert(result, value)") && !baseline.contains("_resultLength"),
		"default output did not retain Array.push parity:\n{baseline}"
	);

	let optimized = compile(true);
	assert!(
		!optimized.contains("table.insert(result")
			&& optimized.contains("local _resultLength = 0")
			&& optimized.contains("result[_resultLength] = value"),
		"optimized project output did not use indexed appends:\n{optimized}"
	);
	assert!(
		!optimized.contains("_optionalLength") && optimized.contains("table.insert(_result, value)"),
		"optional call did not retain its fallback:\n{optimized}"
	);
	assert!(
		!optimized.contains("_exportedLength") && optimized.contains("table.insert(exported, value)"),
		"exported array did not retain its fallback:\n{optimized}"
	);
}
