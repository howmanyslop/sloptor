//! The truthiness half of `types_test.go`, over `testdata/typeprobe/src/probe.ts`. The predicate
//! half is `tests/types.rs`.

use super::{collect_identifiers, render_list, render_node, transformer_testdata, with_state};
use crate::prelude::*;
use crate::truthiness::{create_truthiness_checks, will_create_truthiness_checks};

fn with_probe<R: Send>(test: impl FnOnce(&mut State<'_>) -> R + Send) -> R {
	with_state(&transformer_testdata("typeprobe"), "src/probe.ts", test)
}

/// The unique identifier named `name` (Go's `mustOneIdentifier`).
fn probe_node(state: &State<'_>, name: &str) -> P<Node> {
	let identifiers = collect_identifiers(state.source_file.as_node(), name);
	assert_eq!(
		identifiers.len(),
		1,
		"found {} identifiers named {name}, want 1",
		identifiers.len()
	);
	identifiers[0]
}

#[test]
fn will_create_truthiness_checks_cases() {
	with_probe(|s| {
		for (name, want) in [
			("bool", false),
			("five", false),
			("num", true),
			("str", true),
			("zero", true),
			("numStr", true),
			("strOpt", true),
			("unk", true),
			("derived", false),
		] {
			let node = probe_node(s, name);
			let t = s.get_type(node);
			assert_eq!(
				will_create_truthiness_checks(s, t),
				want,
				"WillCreateTruthinessChecks({name})"
			);
		}
	});
}

#[test]
fn create_truthiness_checks_cases() {
	with_probe(|s| {
		for (name, want) in [
			("bool", "x"),
			("five", "x"),
			("derived", "x"),
			("num", "x ~= 0 and x == x and x"),
			("zero", "x ~= 0 and x == x and x"),    // TS#32778: 0-possible adds the NaN check
			("zeroOne", "x ~= 0 and x == x and x"), // ditto (no NaN-able member)
			("str", r#"x ~= "" and x"#),
			("empty", r#"x ~= "" and x"#),
			("strOpt", r#"x ~= "" and x"#), // undefined needs only the value check
			("numStr", r#"x ~= 0 and x == x and x ~= "" and x"#),
			("unk", r#"x ~= 0 and x == x and x ~= "" and x"#),
		] {
			let node = probe_node(s, name);
			let (result, prereqs) = s.capture(|s| {
				let x = s.ast.id("x");
				let t = s.get_type(node);
				create_truthiness_checks(s, x, node, t)
			});
			assert!(
				s.ast.list_is_empty(prereqs),
				"{name}: unexpected prerequisite statements"
			);
			assert_eq!(render_node(s, result), want, "{name}");
		}
		let diagnostics = s.diags.flush();
		assert!(
			diagnostics.is_empty(),
			"unexpected diagnostics (LogTruthyChanges off): {diagnostics:?}"
		);
	});
}

#[test]
fn create_truthiness_checks_pushes_complex_exp_to_var() {
	with_probe(|s| {
		let call = |s: &mut State<'_>| {
			let f = s.ast.global_id("f");
			let args = s.ast.new_list();
			s.ast.call(f, args)
		};

		// checks needed + complex expression -> pushToVarIfComplex("value")
		let node = probe_node(s, "num");
		let (result, prereqs) = s.capture(|s| {
			let expression = call(s);
			let t = s.get_type(node);
			create_truthiness_checks(s, expression, node, t)
		});
		let statement = s.ast.return_statement(result);
		s.ast.list_push(prereqs, statement);
		assert_eq!(
			render_list(s, prereqs),
			"local _value = f()\nreturn _value ~= 0 and _value == _value and _value\n"
		);

		// no checks needed -> the complex expression stays inline
		let bool_node = probe_node(s, "bool");
		let (result, prereqs) = s.capture(|s| {
			let expression = call(s);
			let t = s.get_type(bool_node);
			create_truthiness_checks(s, expression, bool_node, t)
		});
		assert!(s.ast.list_is_empty(prereqs), "boolean type must not push to var");
		assert_eq!(render_node(s, result), "f()");
	});
}

#[test]
fn create_truthiness_checks_truthy_change_warning() {
	with_probe(|s| {
		s.log_truthy_changes = true;
		for (name, message) in [
			("numStr", r#"Value will be checked against 0, NaN, """#),
			("num", "Value will be checked against 0, NaN"),
			("zero", "Value will be checked against 0, NaN"), // the workaround lists NaN too
			("str", r#"Value will be checked against """#),
			("bool", ""),
			("five", ""),
		] {
			let node = probe_node(s, name);
			s.capture_statements(|s| {
				let x = s.ast.id("x");
				let t = s.get_type(node);
				create_truthiness_checks(s, x, node, t);
			});
			let diagnostics = s.diags.flush();
			if message.is_empty() {
				assert!(diagnostics.is_empty(), "{name}: unexpected diagnostics {diagnostics:?}");
				continue;
			}
			assert_eq!(diagnostics.len(), 1, "{name}");
			assert_eq!(diagnostics[0].code, "truthyChange", "{name}");
			assert!(diagnostics[0].warning, "{name}");
			assert_eq!(diagnostics[0].message, message, "{name}");
		}
	});
}
