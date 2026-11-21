//! A project that sets `rootDirs` without `rootDir`, with one entry outside its tsconfig's
//! directory: rbxtsc 3.0.0 (TypeScript 5) compiles it, laying out `out/` from the common
//! directory of the sources (tsrs-mirror patch 0014).

use super::{expect_compiled, keys, large_stack, output, testdata};
use crate::project::compile_project;

#[test]
fn compile_project_root_dirs_outside_the_config_directory() {
	let files = expect_compiled(large_stack(|| compile_project(&testdata("rootdirs_game/game"))));
	let wants = [
		(
			"out/common/src/shared/greeting.luau",
			"-- Compiled with sloptor v3.0.0-beta.5\n\
			local greeting = \"hello\"\n\
			return {\n\
			\tgreeting = greeting,\n\
			}\n",
		),
		(
			"out/game/src/server/main.server.luau",
			"-- Compiled with sloptor v3.0.0-beta.5\n\
			local TS = require(game:GetService(\"ReplicatedStorage\"):WaitForChild(\"include\"):WaitForChild(\"RuntimeLib\"))\n\
			local greeting = TS.import(script, game:GetService(\"ReplicatedStorage\"), \"shared\", \"greeting\").greeting\n\
			print(greeting)\n",
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
