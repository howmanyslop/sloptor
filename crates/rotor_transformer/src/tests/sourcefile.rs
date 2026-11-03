//! `sourcefile_test.go`

use super::{render_list, with_test_state};
use crate::exports::{ExportShape, choose_export_shape};
use crate::prelude::*;
use crate::sourcefile::{ensure_module_return, prepend_header};

#[test]
fn choose_export_shape_cases() {
	let cases = [
		("no exports", false, false, false, false, ExportShape::None),
		("export equals", true, false, false, false, ExportShape::ExportEquals),
		(
			"export equals wins over everything",
			true,
			true,
			true,
			true,
			ExportShape::ExportEquals,
		),
		(
			"export from forces exports table",
			false,
			true,
			false,
			false,
			ExportShape::ExportsTable,
		),
		(
			"mutable export forces exports table",
			false,
			false,
			true,
			false,
			ExportShape::ExportsTable,
		),
		(
			"mutable + immutable still exports table",
			false,
			false,
			true,
			true,
			ExportShape::ExportsTable,
		),
		(
			"only immutable exports return map",
			false,
			false,
			false,
			true,
			ExportShape::ReturnMap,
		),
	];
	for (name, export_equals, export_from, mutable, immutable, want) in cases {
		assert_eq!(
			choose_export_shape(export_equals, export_from, mutable, immutable),
			want,
			"{name}"
		);
	}
}

#[test]
fn ensure_module_return_cases() {
	with_test_state(|s| {
		// empty ModuleScript gets return nil
		let list = s.ast.new_list();
		ensure_module_return(&mut s.ast, list, true);
		assert_eq!(render_list(s, list), "return nil\n");

		// non-return last statement gets return nil
		let list = s.ast.new_list();
		let x = s.ast.id("x");
		let one = s.ast.num(1.0);
		let declaration = s.ast.variable_declaration(x, Some(one.into()));
		s.ast.list_push(list, declaration);
		ensure_module_return(&mut s.ast, list, true);
		assert_eq!(render_list(s, list), "local x = 1\nreturn nil\n");

		// existing return suppresses return nil
		let list = s.ast.new_list();
		let exports = s.ast.global_id("exports");
		let statement = s.ast.return_statement(exports);
		s.ast.list_push(list, statement);
		ensure_module_return(&mut s.ast, list, true);
		assert_eq!(render_list(s, list), "return exports\n");

		// trailing comments are skipped when finding the return
		let list = s.ast.new_list();
		let exports = s.ast.global_id("exports");
		let statement = s.ast.return_statement(exports);
		s.ast.list_push(list, statement);
		let comment = s.ast.comment(" trailing");
		s.ast.list_push(list, comment);
		ensure_module_return(&mut s.ast, list, true);
		assert_eq!(render_list(s, list), "return exports\n-- trailing\n");

		// non-ModuleScript never gets return nil
		let list = s.ast.new_list();
		let x = s.ast.id("x");
		let one = s.ast.num(1.0);
		let declaration = s.ast.variable_declaration(x, Some(one.into()));
		s.ast.list_push(list, declaration);
		ensure_module_return(&mut s.ast, list, false);
		assert_eq!(render_list(s, list), "local x = 1\n");
	});
}

fn push_return_nil(s: &mut State<'_>, list: Statements) {
	let nil = s.ast.nil();
	let statement = s.ast.return_statement(nil);
	s.ast.list_push(list, statement);
}

fn push_comment(s: &mut State<'_>, list: Statements, text: &str) {
	let comment = s.ast.comment(text);
	s.ast.list_push(list, comment);
}

#[test]
fn prepend_header_cases() {
	// header is the first line
	with_test_state(|s| {
		let list = s.ast.new_list();
		push_return_nil(s, list);
		prepend_header(s, list);
		assert_eq!(
			render_list(s, list),
			"-- Compiled with sloptor v3.0.0-beta.2\nreturn nil\n"
		);
	});

	// directive comments hoist above the header
	with_test_state(|s| {
		let list = s.ast.new_list();
		push_comment(s, list, "!strict");
		push_comment(s, list, " regular comment");
		push_return_nil(s, list);
		prepend_header(s, list);
		assert_eq!(
			render_list(s, list),
			"--!strict\n-- Compiled with sloptor v3.0.0-beta.2\n-- regular comment\nreturn nil\n"
		);
	});

	// the runtime-lib import joins the header after the version comment; without a Rojo
	// context the state lands on the Package `_G[script]` form
	with_test_state(|s| {
		s.uses_runtime_lib = true;
		let list = s.ast.new_list();
		push_comment(s, list, "!strict");
		push_return_nil(s, list);
		prepend_header(s, list);
		assert_eq!(
			render_list(s, list),
			"--!strict\n-- Compiled with sloptor v3.0.0-beta.2\nlocal TS = _G[script]\nreturn nil\n"
		);
	});

	// the directive scan stops at the first non-directive
	with_test_state(|s| {
		let list = s.ast.new_list();
		push_comment(s, list, " leading");
		push_comment(s, list, "!native"); // not at head: stays below the header
		push_return_nil(s, list);
		prepend_header(s, list);
		assert_eq!(
			render_list(s, list),
			"-- Compiled with sloptor v3.0.0-beta.2\n-- leading\n--!native\nreturn nil\n"
		);
	});
}
