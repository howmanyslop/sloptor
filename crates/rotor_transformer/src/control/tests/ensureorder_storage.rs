//! `ensureorder_storage_test.go` (the tests; `BenchmarkEnsureTransformOrderStorage` measures Go
//! allocation and has no counterpart). Go also peeks at the private prereq stack to check that
//! captured lists are consumed only after every transform; that stack is private to
//! `crate::state`, so those two internal checks are left out and the rendered order pins the
//! splice instead.

use rotor_luau::TemporaryIdentifier;

use super::with_var_args_state;
use crate::diagnostics;
use crate::dispatch::transform_expression;
use crate::ensureorder::ensure_transform_order_with;
use crate::prelude::*;
use crate::tests::{render_list, with_source};

fn call(ast: &mut Ast, name: &str) -> NodeId {
	let callee = ast.global_id(name);
	let args = ast.new_list();
	ast.call(callee, args)
}

fn call_statement(state: &mut State<'_>, name: &str) {
	let expression = call(&mut state.ast, name);
	let statement = state.ast.call_statement(expression);
	state.prereq(statement);
}

fn temp_index(state: &State<'_>, node: NodeId) -> usize {
	state
		.ast
		.get::<TemporaryIdentifier>(node)
		.map(|temp| temp.id.index())
		.expect("a temporary identifier")
}

#[test]
fn ensure_transform_order_capture_and_splice_order() {
	let source = "
let changing = 1;
const fixed = 2;
let primitive = 7;
let prepared = 8;
let tail = 3;
[changing, fixed, primitive, prepared, 9, 10, tail];
";
	with_var_args_state(source, |s| {
		let statements = s.source_file.statements().nodes();
		let last = statements[statements.len() - 1];
		let nodes = last
			.as_expression_statement()
			.expression
			.as_array_literal_expression()
			.elements
			.nodes();
		let mut seen = 0;
		let mut result = Vec::new();
		let prereqs = s.capture_statements(|s| {
			call_statement(s, "before");
			result = ensure_transform_order_with(s, nodes, |s, node| {
				let index = seen;
				assert!(
					index < nodes.len() && node == nodes[index],
					"transforms must run once each in source order"
				);
				assert!(
					s.multi.is_defined_as_let_cache.is_empty(),
					"mutability queries must follow all captures"
				);
				seen += 1;
				s.diags.add(diagnostics::no_any(node));
				match index {
					0 => s.ast.id("changing"),
					1 => {
						call_statement(s, "first");
						s.ast.id("fixed")
					}
					2 => s.ast.num(7.0),
					3 => {
						let prepare = call(&mut s.ast, "prepare");
						s.push_to_var(Some(prepare), "exp")
					}
					4 => call(&mut s.ast, "read"),
					5 => {
						call_statement(s, "mutate");
						call_statement(s, "after");
						s.ast.id("changed")
					}
					_ => s.ast.id("tail"),
				}
			});
		});
		assert_eq!(result.len(), nodes.len(), "lost expressions");
		assert_eq!(seen, nodes.len(), "lost captures");
		// The transform-created temp precedes both ordering temps, even though its declaration is
		// emitted between theirs.
		let first = temp_index(s, result[0]);
		let prepared = temp_index(s, result[3]);
		let second = temp_index(s, result[4]);
		assert!(prepared < first && first < second, "temporary creation order changed");
		for (index, mutable) in [(0, true), (1, false), (2, true), (3, true), (6, true)] {
			let symbol = s.ck().get_symbol_at_location(nodes[index]).expect("a symbol");
			assert_eq!(
				s.multi.is_defined_as_let_cache.get(&symbol).copied(),
				Some(mutable),
				"identifier {index}: mutability"
			);
		}
		let diagnostics = s.diags.flush();
		assert_eq!(diagnostics.len(), nodes.len(), "diagnostic count");
		for (index, diagnostic) in diagnostics.iter().enumerate() {
			assert!(
				diagnostic.node == Some(nodes[index]) && diagnostic.code == "noAny",
				"diagnostic {index} lost its source order or identity"
			);
		}
		let use_callee = s.ast.global_id("use");
		let args = s.ast.list_from(result.iter().copied());
		let use_call = s.ast.call(use_callee, args);
		let use_statement = s.ast.call_statement(use_call);
		s.ast.list_push(prereqs, use_statement);
		let want = "before()
local _exp = changing
first()
local _exp_1 = prepare()
local _exp_2 = read()
mutate()
after()
use(_exp, fixed, 7, _exp_1, _exp_2, changed, tail)
";
		assert_eq!(render_list(s, prereqs), want);
	});
}

#[test]
fn ensure_transform_order_small_inputs() {
	for (name, source, prereq, want) in [
		("empty", "[];", false, "return \n"),
		("one", "[1];", false, "return 1\n"),
		("one_with_prereq", "[1];", true, "prepare()\nreturn read()\n"),
		("many_without_prereqs", "[1, 2, 3];", false, "return 1, 2, 3\n"),
	] {
		with_source(source, |s, expression| {
			let nodes = expression.as_array_literal_expression().elements.nodes();
			let mut result = Vec::new();
			let prereqs = s.capture_statements(|s| {
				result = ensure_transform_order_with(s, nodes, |s, node| {
					if prereq {
						call_statement(s, "prepare");
						return call(&mut s.ast, "read");
					}
					transform_expression(s, node)
				});
			});
			assert_eq!(result.len(), nodes.len(), "{name}: result shape");
			let values = s.ast.list_from::<element::Expression>(result.iter().copied());
			let return_statement = s.ast.return_statement(values);
			s.ast.list_push(prereqs, return_statement);
			assert_eq!(render_list(s, prereqs), want, "{name}");
			let diagnostics = s.diags.flush();
			assert!(
				diagnostics.is_empty(),
				"{name}: unexpected diagnostics: {diagnostics:?}"
			);
		});
	}
}
