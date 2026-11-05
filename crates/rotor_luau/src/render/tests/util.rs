//! Port of `internal/luau/render/util_test.go`.

use super::dsl::{bin, id};
use crate::render::RenderState;
use crate::render::parens::needs_parentheses;
use crate::render::util::get_safe_bracket_equals;
use crate::{Ast, BinaryOperator as B};

#[test]
fn get_safe_bracket_equals_cases() {
	let cases = [
		("hello", ""),
		("a ]] b", "="),
		("a ]=] b", ""),
		("ends with ]", "="),
		("a ]=] ]] b", "=="),
		("x]", "="),
	];
	for (input, want) in cases {
		assert_eq!(
			get_safe_bracket_equals(input),
			want,
			"get_safe_bracket_equals({input:?})"
		);
	}
}

#[test]
fn needs_parentheses_cases() {
	let mut a = Ast::new();
	// (a + b) * c: a lower precedence child on the left of a higher
	// precedence parent.
	let inner = bin(id("a"), B::Add, id("b")).build(&mut a);
	bin(inner, B::Multiply, id("c")).build(&mut a);
	assert!(needs_parentheses(&a, inner), "(a + b) * c: left + needs parens under *");

	// a + b * c: a higher precedence child needs none.
	let inner = bin(id("b"), B::Multiply, id("c")).build(&mut a);
	bin(id("a"), B::Add, inner).build(&mut a);
	assert!(
		!needs_parentheses(&a, inner),
		"a + b * c: right * needs no parens under +"
	);

	// a - (b - c): equal precedence on the right needs parentheses.
	let inner = bin(id("b"), B::Subtract, id("c")).build(&mut a);
	bin(id("a"), B::Subtract, inner).build(&mut a);
	assert!(
		needs_parentheses(&a, inner),
		"a - (b - c): equal precedence right operand needs parens"
	);

	// a - b - c: left-associative, so the left child needs none.
	let inner = bin(id("a"), B::Subtract, id("b")).build(&mut a);
	bin(inner, B::Subtract, id("c")).build(&mut a);
	assert!(
		!needs_parentheses(&a, inner),
		"(a - b) - c: left operand needs no parens"
	);
}

#[test]
fn render_state_indent() {
	let mut state = RenderState::new();
	assert_eq!(state.line("x"), "x\n", "line at depth 0");
	let out = state.block(|state| state.line("y"));
	assert_eq!(out, "\ty\n", "line at depth 1");
}
