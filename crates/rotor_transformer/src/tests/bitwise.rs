//! `bitwise_test.go`. `TestBigIntPropertyDiagnostic` exercises `objectAccessor` (`binding.go`),
//! slice (i)'s.

use rotor_tsrs::ast as tsast;

use super::{render_list, with_parsed_expression};
use crate::access::get_literal_number_value;
use crate::bitwise::{create_bitwise_call, flatten_bitwise_chain};
use crate::call::should_wrap_lua_tuple;
use crate::prelude::*;

#[test]
fn bitwise_flattening() {
	with_parsed_expression("a & b & c;", |s, expression| {
		let operands = flatten_bitwise_chain(Kind::AmpersandToken, expression.as_binary_expression().left);
		assert_eq!(operands.len(), 2, "flattened left spine length");
		assert_eq!(
			(operands[0].text(), operands[1].text()),
			("a", "b"),
			"flattened operands"
		);

		let (a, b, c) = (s.ast.id("a"), s.ast.id("b"), s.ast.id("c"));
		let call = create_bitwise_call(s, Kind::AmpersandToken, vec![a, b, c]);
		let statement = s.ast.call_statement(call);
		let list = s.ast.list_from([statement]);
		assert_eq!(render_list(s, list), "bit32.band(a, b, c)\n", "rendered bitwise call");
		let (x, three) = (s.ast.id("x"), s.ast.num(3.0));
		let compound = create_bitwise_call(s, Kind::AmpersandEqualsToken, vec![x, three]);
		let statement = s.ast.call_statement(compound);
		let list = s.ast.list_from([statement]);
		assert_eq!(
			render_list(s, list),
			"bit32.band(x, 3)\n",
			"rendered compound bitwise call"
		);
	});
	with_parsed_expression("a & b | c;", |_, mixed| {
		let operands = flatten_bitwise_chain(Kind::BarToken, mixed.as_binary_expression().left);
		assert_eq!(operands.len(), 1, "mixed-precedence chain length");
	});
}

#[test]
fn range_literal_step() {
	with_parsed_expression("a;", |s, _| {
		let minus_one = s.ast.num(-1.0);
		assert_eq!(get_literal_number_value(&s.ast, minus_one), Some(-1.0));
		let step = s.ast.id("step");
		assert_eq!(get_literal_number_value(&s.ast, step), None);
	});
}

#[test]
fn nullable_lua_tuple() {
	// Spread assignment: [a, ...rest] = f() — should wrap LuaTuple
	with_parsed_expression("[a, ...rest] = f();", |s, expression| {
		let call_node = expression.as_binary_expression().right.get();
		let f = s.ast.id("f");
		let args = s.ast.new_list();
		let call = s.ast.call(f, args);
		assert!(
			should_wrap_lua_tuple(s, call_node, call),
			"spread assignment unexpectedly consumed a LuaTuple call"
		);
	});
	// Optional chain: f?.()[0] — should wrap LuaTuple because call has QuestionDotToken
	with_parsed_expression("f?.()[0];", |s, optional| {
		assert!(tsast::is_element_access_expression(optional));
		let optional_call = optional.as_element_access_expression().expression;
		let f = s.ast.id("f");
		let args = s.ast.new_list();
		let call = s.ast.call(f, args);
		assert!(
			should_wrap_lua_tuple(s, optional_call, call),
			"optional-chain call unexpectedly skipped LuaTuple wrapping"
		);
	});
}
