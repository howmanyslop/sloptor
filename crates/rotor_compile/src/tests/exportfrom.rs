//! `exportfrom_test.go`: `export ... from` and `export =`, end to end.

use super::{expect_compiled, keys, large_stack, output, testdata};
use crate::project::compile_project;

macro_rules! header {
	() => {
		concat!(
			"-- Compiled with sloptor v3.0.0-beta.3\n",
			"local TS = require(script.Parent.include.RuntimeLib)\n",
		)
	};
}

#[test]
#[expect(clippy::too_many_lines, reason = "the Go test's table of expected outputs")]
fn compile_project_export_from_model() {
	let files = expect_compiled(large_stack(|| compile_project(&testdata("exportfrom_model"))));
	let wants: &[(&str, &str)] = &[
		(
			"out/_scratch_util.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.3\n",
				"local VALUE = 123\n",
				"local function greet(name)\n",
				"\treturn `Hello, {name}`\n",
				"end\n",
				"local function default()\n",
				"\treturn VALUE\n",
				"end\n",
				"return {\n",
				"\tgreet = greet,\n",
				"\tdefault = default,\n",
				"\tVALUE = VALUE,\n",
				"}\n",
			),
		),
		(
			"out/_scratch_renamed.luau",
			concat!(
				header!(),
				"local exports = {}\n",
				"exports.greet = TS.import(script, script.Parent, \"_scratch_util\").greet\n",
				"return exports\n",
			),
		),
		(
			"out/_scratch_realias.luau",
			concat!(
				header!(),
				"local exports = {}\n",
				"exports.hello = TS.import(script, script.Parent, \"_scratch_util\").greet\n",
				"return exports\n",
			),
		),
		(
			"out/_scratch_retwo.luau",
			concat!(
				header!(),
				"local exports = {}\n",
				"local __scratch_util = TS.import(script, script.Parent, \"_scratch_util\")\n",
				"exports.greet = __scratch_util.greet\n",
				"exports.VALUE = __scratch_util.VALUE\n",
				"return exports\n",
			),
		),
		(
			"out/_scratch_restar.luau",
			concat!(
				header!(),
				"local exports = {}\n",
				"for _k, _v in TS.import(script, script.Parent, \"_scratch_util\") or {} do\n",
				"\texports[_k] = _v\n",
				"end\n",
				"return exports\n",
			),
		),
		(
			"out/_scratch_remix.luau",
			concat!(
				header!(),
				"local exports = {}\n",
				"local own = 1\n",
				"for _k, _v in TS.import(script, script.Parent, \"_scratch_util\") or {} do\n",
				"\texports[_k] = _v\n",
				"end\n",
				"exports.hello = TS.import(script, script.Parent, \"_scratch_util\").greet\n",
				"exports.own = own\n",
				"return exports\n",
			),
		),
		(
			"out/_scratch_rens.luau",
			concat!(
				header!(),
				"local exports = {}\n",
				"exports.util = TS.import(script, script.Parent, \"_scratch_util\")\n",
				"return exports\n",
			),
		),
		(
			"out/_scratch_reimport.luau",
			concat!(
				header!(),
				"local exports = {}\n",
				"local VALUE = TS.import(script, script.Parent, \"_scratch_util\").VALUE\n",
				"exports.greet = TS.import(script, script.Parent, \"_scratch_util\").greet\n",
				"print(VALUE)\n",
				"return exports\n",
			),
		),
		(
			"out/_scratch_retypeonly.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.3\n",
				"local n = 5\n",
				"print(n)\n",
				"return nil\n",
			),
		),
		(
			"out/_scratch_respec_typeonly.luau",
			concat!(
				header!(),
				"local exports = {}\n",
				"exports.greet = TS.import(script, script.Parent, \"_scratch_util\").greet\n",
				"return exports\n",
			),
		),
		(
			"out/_scratch_eqtable.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.3\n",
				"return {\n",
				"\ta = 1,\n",
				"\tb = \"two\",\n",
				"}\n",
			),
		),
		(
			"out/_scratch_eqfunc.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.3\n",
				"local function f()\n",
				"\treturn 1\n",
				"end\n",
				"return f\n",
			),
		),
		(
			"out/_scratch_eqmid.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.3\n",
				"local t = {\n",
				"\ta = 1,\n",
				"}\n",
				"local exports = t\n",
				"print(t.a)\n",
				"return exports\n",
			),
		),
	];
	for (name, want) in wants {
		assert!(
			files.contains_key(*name),
			"{name}: missing from CompileProject output ({:?})",
			keys(&files)
		);
		assert_eq!(output(&files, name), *want, "{name}");
	}
	assert_eq!(files.len(), wants.len(), "produced files ({:?})", keys(&files));
}
