//! `getmoduletree_folder_test.go`: `$getModuleTree` on a folder, a rotor extension.

use super::{expect_compiled, large_stack, output, testdata};
use crate::project::compile_project;

#[test]
fn get_module_tree_folder() {
	let files = expect_compiled(large_stack(|| compile_project(&testdata("gmtree_folder_model"))));
	let want = "-- Compiled with sloptor v3.0.0-beta.3\n\
		print({ script.Parent, { \"shared\", \"systems\" } })\n\
		print({ script.Parent, { \"systems2\" } })\n\
		print({ script.Parent, { \"shared\", \"systems\" } })\n\
		return nil\n";
	assert_eq!(output(&files, "out/main.luau"), want);
}
