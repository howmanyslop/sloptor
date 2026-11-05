//! `call_test.go`. `TestPropertyCallMacro` and `TestContextualMethodArrayCallbacksInjectSelf`
//! need the macros (slice iv) and function expressions (slice ii).

use super::{render_file_statements, render_list, transformer_testdata, with_state};
use crate::statementlist::transform_statement_list;

#[test]
fn calls_against_rbxtsc() {
	with_state(&transformer_testdata("calls"), "src/calls.ts", |s| {
		let want = r#"obj:method()
obj.callback(1)
obj["then"](obj)
lookup["a b"](5)
local arr = { 10, 20, 30 }
arr[1] += 1
local i = 2
arr[i + 1] *= 2
local prev = arr[i]
arr[i + 1] += 1
local _mutObj = mutObj
local _original = i
i += 1
_mutObj.cb(_original)
print(obj, lookup, arr, prev, i, mutObj)
"#;
		assert_eq!(render_file_statements(s), want);
		// isMethod consults the program-wide per-symbol cache; the method-bearing types above
		// must have left entries behind.
		assert!(
			!s.multi.is_method_cache.is_empty(),
			"IsMethodCache is empty, want per-symbol isMethod entries"
		);
	});
}

/// One function type declaring both a callback (`this: void`) and a method signature is
/// upstream's `noMixedTypeCall`; the call still emits as a method.
#[test]
fn no_mixed_type_call() {
	with_state(&transformer_testdata("calls"), "src/mixed.ts", |s| {
		let file = s.source_file;
		let statements = transform_statement_list(s, Some(file.as_node()), file.statements().nodes(), None);
		assert_eq!(render_list(s, statements), "mixed:fn(1)\n");
		let diagnostics = s.diags.flush();
		assert!(
			diagnostics
				.iter()
				.any(|diagnostic| diagnostic.code == "noMixedTypeCall"),
			"expected noMixedTypeCall diagnostic; got: {diagnostics:?}"
		);
	});
}
