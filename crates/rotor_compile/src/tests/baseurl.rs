//! `baseurl_test.go`: the `baseUrl` to `paths` rewrite, end to end.

use super::{expect_compiled, keys, large_stack, output, testdata};
use crate::project::compile_project;

#[test]
fn compile_project_base_url_paths_rewrite() {
	let files = expect_compiled(large_stack(|| compile_project(&testdata("baseurl_model"))));
	let wants = [
		(
			"out/main.luau",
			"-- Compiled with sloptor v3.0.0-beta.4\n\
			local TS = require(script.Parent.include.RuntimeLib)\n\
			local _mod = TS.import(script, script.Parent, \"shared\", \"mod\")\n\
			local sharedFn = _mod.sharedFn\n\
			local SHARED_VALUE = _mod.SHARED_VALUE\n\
			local dummy = TS.import(script, script.Parent, \"node_modules\", \"@rbxts\", \"dummy\", \"out\").dummy\n\
			print(sharedFn(), SHARED_VALUE, dummy())\n\
			return nil\n",
		),
		(
			"out/shared/mod.luau",
			"-- Compiled with sloptor v3.0.0-beta.4\n\
			local SHARED_VALUE = 5\n\
			local function sharedFn()\n\
			\treturn SHARED_VALUE * 2\n\
			end\n\
			return {\n\
			\tsharedFn = sharedFn,\n\
			\tSHARED_VALUE = SHARED_VALUE,\n\
			}\n",
		),
	];
	for (name, want) in wants {
		assert!(
			files.contains_key(name),
			"{name}: missing from CompileProject output ({:?})",
			keys(&files)
		);
		assert_eq!(output(&files, name), want, "{name}");
	}
	assert_eq!(files.len(), wants.len(), "produced files ({:?})", keys(&files));
}
