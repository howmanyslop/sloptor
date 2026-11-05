//! `imports_test.go`: import emission per project type, end to end.

use super::{expect_compiled, keys, large_stack, output, testdata};
use crate::project::{Outputs, compile_project};

fn assert_wants(files: &Outputs, wants: &[(&str, &str)]) {
	for (name, want) in wants {
		assert!(
			files.contains_key(*name),
			"{name}: missing from CompileProject output ({:?})",
			keys(files)
		);
		assert_eq!(output(files, name), *want, "{name}");
	}
}

#[test]
#[expect(clippy::too_many_lines, reason = "the Go test's table of expected outputs")]
fn compile_project_imports_model() {
	let files = expect_compiled(large_stack(|| compile_project(&testdata("imports_model"))));
	let wants: &[(&str, &str)] = &[
		(
			"out/_scratch_main.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
				"local TS = require(script.Parent.include.RuntimeLib)\n",
				"local __scratch_util = TS.import(script, script.Parent, \"_scratch_util\")\n",
				"local greeter = __scratch_util.default\n",
				"local VALUE = __scratch_util.VALUE\n",
				"local g = __scratch_util.greet\n",
				"local x = VALUE + greeter()\n",
				"print(g(\"world\"), x)\n",
				"return nil\n",
			),
		),
		(
			"out/_scratch_util.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
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
			"out/_scratch_once.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
				"local TS = require(script.Parent.include.RuntimeLib)\n",
				"local g = TS.import(script, script.Parent, \"_scratch_util\").greet\n",
				"print(g(\"once\"))\n",
				"return nil\n",
			),
		),
		(
			"out/_scratch_twice.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
				"local TS = require(script.Parent.include.RuntimeLib)\n",
				"local __scratch_util = TS.import(script, script.Parent, \"_scratch_util\")\n",
				"local VALUE = __scratch_util.VALUE\n",
				"local greet = __scratch_util.greet\n",
				"print(greet(\"x\"), VALUE)\n",
				"return nil\n",
			),
		),
		(
			"out/_scratch_default_once.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
				"local TS = require(script.Parent.include.RuntimeLib)\n",
				"local greeter = TS.import(script, script.Parent, \"_scratch_util\").default\n",
				"print(greeter())\n",
				"return nil\n",
			),
		),
		(
			"out/_scratch_ns.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
				"local TS = require(script.Parent.include.RuntimeLib)\n",
				"local util = TS.import(script, script.Parent, \"_scratch_util\")\n",
				"print(util.VALUE)\n",
				"return nil\n",
			),
		),
		(
			"out/_scratch_typeonly.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
				"local n = 1\n",
				"print(n)\n",
				"return nil\n",
			),
		),
		(
			"out/_scratch_elide.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
				"local s = \"types\"\n",
				"print(s)\n",
				"return nil\n",
			),
		),
		(
			"out/_scratch_sideeffect.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
				"local TS = require(script.Parent.include.RuntimeLib)\n",
				"TS.import(script, script.Parent, \"_scratch_util\")\n",
				"return nil\n",
			),
		),
		(
			"out/_scratch_eq.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
				"local TS = require(script.Parent.include.RuntimeLib)\n",
				"local util = TS.import(script, script.Parent, \"_scratch_util\")\n",
				"print(util.VALUE)\n",
				"return nil\n",
			),
		),
		(
			"out/_scratch_eqchain.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
				"local TS = require(script.Parent.include.RuntimeLib)\n",
				"local VfxForge = TS.import(script, script.Parent, \"_scratch_eqchain_index\")\n",
				"VfxForge.emit(\"effect\")\n",
				"return nil\n",
			),
		),
		(
			"out/_scratch_eqns.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
				"local A = Ambient\n",
				"print(A.val)\n",
				"return nil\n",
			),
		),
		(
			"out/_scratch_synthmain.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
				"local TS = require(script.Parent.include.RuntimeLib)\n",
				"local synth = TS.import(script, script.Parent, \"_scratch_synth\")\n",
				"print(synth())\n",
				"return nil\n",
			),
		),
		(
			"out/_scratch_dynamic.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
				"local TS = require(script.Parent.include.RuntimeLib)\n",
				"local p = TS.Promise.new(function(resolve)\n",
				"\tresolve(TS.import(script, script.Parent, \"_scratch_util\"))\n",
				"end)\n",
				"print(p)\n",
				"return nil\n",
			),
		),
		(
			"out/_scratch_pkg.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
				"local TS = require(script.Parent.include.RuntimeLib)\n",
				"local dummy = TS.import(script, script.Parent, \"node_modules\", \"@rbxts\", \"dummy\", \"out\").dummy\n",
				"print(dummy())\n",
				"return nil\n",
			),
		),
		(
			"out/_scratch_sub/inner.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
				"local TS = require(script.Parent.Parent.include.RuntimeLib)\n",
				"local VALUE = TS.import(script, script.Parent.Parent, \"_scratch_util\").VALUE\n",
				"print(VALUE)\n",
				"return nil\n",
			),
		),
		(
			"out/_scratch_types.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
				"local tag = \"types\"\n",
				"return {\n",
				"\ttag = tag,\n",
				"}\n",
			),
		),
	];
	assert_wants(&files, wants);
	assert_eq!(files.len(), wants.len(), "produced files ({:?})", keys(&files));
}

#[test]
fn compile_project_imports_game() {
	let files = expect_compiled(large_stack(|| compile_project(&testdata("imports_game"))));
	let wants: &[(&str, &str)] = &[
		(
			"out/_scratch_once.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
				"local TS = require(game:GetService(\"ReplicatedStorage\"):WaitForChild(\"include\"):WaitForChild(\"RuntimeLib\"))\n",
				"local g = TS.import(script, game:GetService(\"ReplicatedStorage\"), \"out\", \"_scratch_util\").greet\n",
				"print(g(\"once\"))\n",
				"return nil\n",
			),
		),
		(
			"out/_scratch_pkg.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
				"local TS = require(game:GetService(\"ReplicatedStorage\"):WaitForChild(\"include\"):WaitForChild(\"RuntimeLib\"))\n",
				"local dummy = TS.import(script, game:GetService(\"ReplicatedStorage\"), \"node_modules\", \"@rbxts\", \"dummy\", \"out\").dummy\n",
				"print(dummy())\n",
				"return nil\n",
			),
		),
		(
			"out/_scratch_dynamic.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
				"local TS = require(game:GetService(\"ReplicatedStorage\"):WaitForChild(\"include\"):WaitForChild(\"RuntimeLib\"))\n",
				"local p = TS.Promise.new(function(resolve)\n",
				"\tresolve(TS.import(script, game:GetService(\"ReplicatedStorage\"), \"out\", \"_scratch_util\"))\n",
				"end)\n",
				"print(p)\n",
				"return nil\n",
			),
		),
		(
			"out/_scratch_sub/inner.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
				"local TS = require(game:GetService(\"ReplicatedStorage\"):WaitForChild(\"include\"):WaitForChild(\"RuntimeLib\"))\n",
				"local VALUE = TS.import(script, game:GetService(\"ReplicatedStorage\"), \"out\", \"_scratch_util\").VALUE\n",
				"print(VALUE)\n",
				"return nil\n",
			),
		),
	];
	assert_wants(&files, wants);
}

#[test]
fn compile_project_imports_package() {
	let files = expect_compiled(large_stack(|| compile_project(&testdata("imports_package"))));
	let wants: &[(&str, &str)] = &[
		(
			"out/_scratch_pkg.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
				"local TS = _G[script]\n",
				"local dummy = TS.import(script, TS.getModule(script, \"@rbxts\", \"dummy\").out).dummy\n",
				"print(dummy())\n",
				"return nil\n",
			),
		),
		(
			"out/_scratch_once.luau",
			concat!(
				"-- Compiled with sloptor v3.0.0-beta.2\n",
				"local TS = _G[script]\n",
				"local g = TS.import(script, script.Parent, \"_scratch_util\").greet\n",
				"print(g(\"once\"))\n",
				"return nil\n",
			),
		),
	];
	assert_wants(&files, wants);
}
