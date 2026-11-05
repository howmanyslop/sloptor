//! `audit_test.go`: the macro registration audit. Missing registrations fail the compile with
//! upstream's `MacroManager` text while the declaring packages are present; checker-light
//! projects without them pass the gate.

use std::fs;

use super::vector_math::build_audit_project;
use super::{large_stack, testdata};
use crate::compile::compile_file;
use crate::project::compile_project;

#[test]
fn macro_registration_audit() {
	let temp = build_audit_project();
	let dir = temp.dir();

	if let Err(failure) = large_stack(|| compile_file(&dir, "src/main.ts")) {
		panic!("intact project failed: {failure} (diags: {:?})", failure.messages());
	}

	let call_macros_path = temp.join("node_modules/@rbxts/compiler-types/types/callMacros.d.ts");
	let data = fs::read_to_string(&call_macros_path).expect("callMacros.d.ts reads");
	let broken = data.replacen("declare function classIs", "declare function classIsRenamed", 1);
	assert_ne!(
		broken, data,
		"fixture drift: callMacros.d.ts has no classIs declaration"
	);
	fs::write(&call_macros_path, broken).expect("callMacros.d.ts writes");

	let want_msg = "MacroManager could not find symbol for classIs\nYou may need to update your @rbxts/compiler-types!";
	let want_err = "compile: macro registration failure";

	match large_stack(|| compile_file(&dir, "src/main.ts")) {
		Ok((_, diags)) => panic!("CompileFile error = nil, want {want_err:?} (diags: {diags:?})"),
		Err(failure) => {
			assert_eq!(
				failure.to_string(),
				want_err,
				"CompileFile error (diags: {:?})",
				failure.messages()
			);
			assert_eq!(failure.messages(), [want_msg], "CompileFile diags");
		}
	}

	match large_stack(|| compile_project(&dir)) {
		Ok(outputs) => panic!(
			"CompileProject error = nil, want {want_err:?} (outputs: {:?})",
			outputs.keys()
		),
		Err(failure) => {
			assert_eq!(
				failure.to_string(),
				want_err,
				"CompileProject error (diags: {:?})",
				failure.messages()
			);
			assert_eq!(failure.messages(), [want_msg], "CompileProject diags");
		}
	}
}

#[test]
fn macro_registration_audit_gating() {
	let dir = testdata("imports_model");
	if let Err(failure) = large_stack(|| compile_file(&dir, "src/_scratch_once.ts")) {
		panic!(
			"checker-light project failed the audit gate: {failure} (diags: {:?})",
			failure.messages()
		);
	}
}

#[test]
fn macro_only_class_assert() {
	let temp = build_audit_project();

	let set_dts_path = temp.join("node_modules/@rbxts/compiler-types/types/Set.d.ts");
	let data = fs::read_to_string(&set_dts_path).expect("Set.d.ts reads");
	let anchor = "interface ReadonlySet<T> extends Iterable<T> {";
	let patched = data.replacen(
		anchor,
		&format!("{anchor}\n\tunion(this: ReadonlySet<T>, other: ReadonlySet<T>): Set<T>;"),
		1,
	);
	assert_ne!(patched, data, "fixture drift: Set.d.ts has no ReadonlySet declaration");
	fs::write(&set_dts_path, patched).expect("Set.d.ts writes");

	temp.write(
		"src/main.ts",
		"const a = new Set<number>([1]);\nconst b = new Set<number>([2]);\nconst c = a.union(b);\nprint(c);\nexport {};\n",
	);

	let want_err = "internal compiler error: Macro ReadonlySet.union() is not implemented!";
	let dir = temp.dir();
	match large_stack(|| compile_file(&dir, "src/main.ts")) {
		Ok((_, diags)) => panic!("CompileFile error = nil, want {want_err:?} (diags: {diags:?})"),
		Err(failure) => assert_eq!(
			failure.to_string(),
			want_err,
			"CompileFile error (diags: {:?})",
			failure.messages()
		),
	}
}
