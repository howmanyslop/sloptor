//! `state_test.go`

use rotor_luau::{Identifier, PropertyAccessExpression, TemporaryIdentifier};

use super::{repo_root, transformer_testdata, with_state, with_state_on, with_test_state};
use crate::prelude::*;
use crate::state::{MultiState, ProjectType};
use crate::valuetoidstr::value_to_id_str;

#[test]
fn capture() {
	with_test_state(|s| {
		let (expression, prereqs) = s.capture(|s| {
			let x = s.ast.id("x");
			let one = s.ast.num(1.0);
			let declaration = s.ast.variable_declaration(x, Some(one.into()));
			s.prereq(declaration);
			s.ast.id("x")
		});
		assert_eq!(s.ast.list_size(prereqs), 1, "prereq not captured");
		assert!(s.ast.get::<Identifier>(expression).is_some(), "wrong expr");
		// stack must be balanced: capturing again works
		let (_, again) = s.capture(|s| s.ast.nil());
		assert_eq!(s.ast.list_size(again), 0, "stack leaked");
	});
}

#[test]
fn push_to_var_if_complex() {
	with_test_state(|s| {
		// simple kinds pass through untouched
		let id = s.ast.id("x");
		assert_eq!(s.push_to_var_if_complex(id, "val"), id, "identifier must not be pushed");
		// complex expressions get a temp + prereq
		let (_, prereqs) = s.capture(|s| {
			let a = s.ast.id("a");
			let b = s.ast.id("b");
			let sum = s.ast.binary(a, BinaryOperator::Add, b);
			s.push_to_var_if_complex(sum, "sum")
		});
		assert_eq!(s.ast.list_size(prereqs), 1, "complex expr must produce a local prereq");
	});
}

#[test]
fn push_to_var_if_non_id() {
	with_test_state(|s| {
		// Identifier and TemporaryIdentifier pass through untouched...
		let id = s.ast.id("x");
		assert_eq!(s.push_to_var_if_non_id(id, "val"), id, "identifier must not be pushed");
		let temp = s.ast.temp_id("t");
		assert_eq!(
			s.push_to_var_if_non_id(temp, "val"),
			temp,
			"temporary identifier must not be pushed"
		);
		// ...but simple literals (which PushToVarIfComplex would skip) ARE pushed.
		let prereqs = s.capture_statements(|s| {
			let hi = s.ast.str("hi");
			let pushed = s.push_to_var_if_non_id(hi, "str");
			assert!(
				s.ast.get::<TemporaryIdentifier>(pushed).is_some(),
				"non-identifier must become a temp"
			);
		});
		assert_eq!(
			s.ast.list_size(prereqs),
			1,
			"non-identifier must produce a local prereq"
		);
	});
}

/// Go asserts with a panic; the port records an internal error and keeps the stack balanced.
#[test]
fn no_prereqs_reports_an_internal_error() {
	with_test_state(|s| {
		// happy path: no prereqs created
		let expression = s.no_prereqs(|s| s.ast.nil());
		assert_eq!(s.ast.kind(expression), LuauKind::NilLiteral, "wrong expr");
		assert!(s.internal_error().is_none());
		// adding a prereq inside NoPrereqs is the upstream assert
		s.no_prereqs(|s| {
			let statement = s.ast.break_statement();
			s.prereq(statement);
			s.ast.nil()
		});
		assert_eq!(
			s.internal_error(),
			Some("transformer: NoPrereqs callback created prerequisite statements")
		);
		// the stack stayed balanced
		let (_, prereqs) = s.capture(|s| s.ast.nil());
		assert!(s.ast.list_is_empty(prereqs), "prereq stack corrupted after the assert");
	});
}

#[test]
fn nested_capture_isolation() {
	with_test_state(|s| {
		let (_, outer) = s.capture(|s| {
			let a = s.ast.id("a");
			let one = s.ast.num(1.0);
			let declaration = s.ast.variable_declaration(a, Some(one.into()));
			s.prereq(declaration);
			let (_, inner) = s.capture(|s| {
				for (name, value) in [("b", 2.0), ("c", 3.0)] {
					let id = s.ast.id(name);
					let value = s.ast.num(value);
					let declaration = s.ast.variable_declaration(id, Some(value.into()));
					s.prereq(declaration);
				}
				s.ast.id("b")
			});
			assert_eq!(
				s.ast.list_size(inner),
				2,
				"inner capture must see exactly its own prereqs"
			);
			s.ast.id("a")
		});
		// inner prereqs must NOT leak into the outer list
		assert_eq!(s.ast.list_size(outer), 1, "outer capture leaked inner prereqs");
	});
}

#[test]
fn runtime_lib() {
	with_test_state(|s| {
		assert!(!s.uses_runtime_lib, "UsesRuntimeLib must start false");
		let expression = s.runtime_lib(None, "instanceof");
		assert!(s.uses_runtime_lib, "RuntimeLib must set UsesRuntimeLib");
		let access = s
			.ast
			.get::<PropertyAccessExpression>(expression)
			.expect("RuntimeLib must return a PropertyAccessExpression")
			.clone();
		assert_eq!(access.name, "instanceof");
		let base = s
			.ast
			.get::<Identifier>(access.expression)
			.expect("base must be an Identifier");
		assert_eq!(base.name, "TS");
		// no warning outside ReplicatedFirst game files
		assert!(s.diags.flush().is_empty(), "unexpected diagnostics");

		// ReplicatedFirst game files warn on every call (upstream: not deduped)
		s.project_type = Some(ProjectType::Game);
		s.is_in_replicated_first = true;
		s.runtime_lib(None, "async");
		s.runtime_lib(None, "await");
		let diagnostics = s.diags.flush();
		assert_eq!(diagnostics.len(), 2);
		for diagnostic in diagnostics {
			assert_eq!(diagnostic.code, "runtimeLibUsedInReplicatedFirst");
			assert!(diagnostic.warning);
		}
	});
}

/// The valueToIdStr hint derivation and the upstream `name || valueToIdStr(expression)` fallback.
#[test]
fn push_to_var_name_hints() {
	with_test_state(|s| {
		type Build = fn(&mut Ast) -> NodeId;
		let cases: [(&str, Build, &str, &str); 7] = [
			// explicit hint always wins
			(
				"explicit hint",
				|ast| {
					let (a, b) = (ast.id("a"), ast.id("b"));
					ast.binary(a, BinaryOperator::Add, b)
				},
				"sum",
				"sum",
			),
			// Identifier X -> "x" (uncapitalized)
			("identifier", |ast| ast.id("Foo"), "", "foo"),
			// PropertyAccess A.B -> "b"
			(
				"property access",
				|ast| {
					let a = ast.id("A");
					ast.property_access(a, "Bar")
				},
				"",
				"bar",
			),
			// CallExpression X.new() -> "x"
			(
				"constructor call",
				|ast| {
					let foo = ast.id("Foo");
					let new = ast.property_access(foo, "new");
					let args = ast.new_list();
					ast.call(new, args)
				},
				"",
				"foo",
			),
			// non-"new" calls and other expressions -> anonymous ""
			(
				"plain call",
				|ast| {
					let f = ast.id("f");
					let args = ast.new_list();
					ast.call(f, args)
				},
				"",
				"",
			),
			(
				"binary",
				|ast| {
					let (a, b) = (ast.id("a"), ast.id("b"));
					ast.binary(a, BinaryOperator::Add, b)
				},
				"",
				"",
			),
			// invalid Luau identifier names -> anonymous ""
			(
				"reserved word",
				|ast| {
					let a = ast.id("a");
					ast.property_access(a, "end")
				},
				"",
				"",
			),
		];
		for (name, build, hint, want) in cases {
			s.capture_statements(|s| {
				let expression = build(&mut s.ast);
				let temp = s.push_to_var(Some(expression), hint);
				let temp = s.ast.get::<TemporaryIdentifier>(temp).expect("a temp");
				assert_eq!(temp.name, want, "{name}: temp name hint");
			});
		}
	});
}

#[test]
fn value_to_id_str_cases() {
	with_test_state(|s| {
		// X.new() nested through a property: A.B.new() -> "b"
		let a = s.ast.id("A");
		let inner = s.ast.property_access(a, "B");
		let new = s.ast.property_access(inner, "new");
		let args = s.ast.new_list();
		let call = s.ast.call(new, args);
		assert_eq!(value_to_id_str(&s.ast, call), "b");
		// temp identifiers are NOT luau.isIdentifier upstream -> ""
		let temp = s.ast.temp_id("t");
		assert_eq!(value_to_id_str(&s.ast, temp), "");
	});
}

#[test]
fn get_type_memoization() {
	with_state(&repo_root().join("testdata/spike-unit/spike"), "src/main.ts", |s| {
		assert!(
			!s.source_file_text.is_empty(),
			"SourceFileText must be captured from the source file"
		);

		// first top-level variable declaration node
		let declaration = s
			.source_file
			.statements()
			.nodes()
			.iter()
			.find(|statement| statement.kind() == Kind::VariableStatement)
			.map(|statement| {
				statement
					.as_variable_statement()
					.declaration_list
					.as_variable_declaration_list()
					.declarations
					.nodes()[0]
			})
			.expect("a variable declaration in the fixture");

		let first = s.get_type(declaration);
		assert_eq!(s.get_type(declaration), first, "GetType must return the cached type");
		// prove the second call reads the cache, not the checker: poison the cache
		let sentinel = s.ck().get_any_type();
		assert_ne!(sentinel, first);
		s.poison_type_cache(declaration, sentinel);
		assert_eq!(s.get_type(declaration), sentinel, "GetType must be memoized by node");
	});
}

/// A `MultiState` already bound to another checker refuses the new state with an internal error
/// (Go makes one `MultiState` per checker group).
#[test]
fn multi_state_refuses_a_second_checker() {
	let mut multi = MultiState::new();
	multi.checker_id = Some(u32::MAX);
	with_state_on(&transformer_testdata("control"), "src/while.ts", multi, |s| {
		assert_eq!(
			s.internal_error(),
			Some("transformer: MultiState is shared across checkers; make one per checker group")
		);
	});
}
