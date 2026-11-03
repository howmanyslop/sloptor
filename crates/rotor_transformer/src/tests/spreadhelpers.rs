//! `spreadhelpers_test.go`

use rotor_luau::TemporaryIdentifier;

use super::{with_parsed_binding_pattern, with_parsed_expression};
use crate::prelude::*;
use crate::spreadhelpers::{array_like_expression_contains_spread, get_target_id_for_binding_pattern};

#[test]
fn rest_gates() {
	let patterns = [
		(
			"array binding pattern with rest element",
			"const [a, ...rest] = value;",
			true,
		),
		(
			"nested array binding pattern with rest element",
			"const [a, [b, ...rest]] = value;",
			true,
		),
		(
			"array binding pattern without rest element",
			"const [a, [b]] = value;",
			false,
		),
	];
	for (name, source, want) in patterns {
		with_parsed_binding_pattern(source, |_, node| {
			assert_eq!(array_like_expression_contains_spread(node), want, "{name}");
		});
	}
	let expressions = [
		("array literal with spread element", "[1, ...rest];", true),
		("nested array literal with spread element", "[[1, ...rest]];", true),
		("array literal without spread element", "[1, [2]];", false),
	];
	for (name, source, want) in expressions {
		with_parsed_expression(source, |_, node| {
			assert_eq!(array_like_expression_contains_spread(node), want, "{name}");
		});
	}
}

#[test]
fn target_id_for_binding_pattern() {
	type Value = fn(&mut Ast) -> NodeId;
	let cases: [(&str, &str, Value, bool, usize); 5] = [
		(
			"array binding pattern without initializers reuses identifier",
			"const [a, b] = value;",
			|ast| ast.id("value"),
			true,
			0,
		),
		(
			"object binding pattern without initializers reuses identifier",
			"const { a, b } = value;",
			|ast| ast.id("value"),
			true,
			0,
		),
		(
			"array binding pattern with initializer allocates temp",
			"const [a = 1] = value;",
			|ast| ast.id("value"),
			false,
			1,
		),
		(
			"object binding pattern with initializer allocates temp",
			"const { a = 1 } = value;",
			|ast| ast.id("value"),
			false,
			1,
		),
		(
			"complex value allocates temp even without initializers",
			"const [a, b] = value;",
			|ast| {
				let (left, right) = (ast.id("left"), ast.id("right"));
				ast.binary(left, BinaryOperator::Add, right)
			},
			false,
			1,
		),
	];
	for (name, source, value, want_reuse, want_prereqs) in cases {
		with_parsed_binding_pattern(source, |s, pattern| {
			let value = value(&mut s.ast);
			let mut got = None;
			let prereqs = s.capture_statements(|s| {
				got = Some(get_target_id_for_binding_pattern(s, pattern, value));
			});
			let got = got.expect("the callback ran");
			assert_eq!(s.ast.list_size(prereqs), want_prereqs, "{name}: prereqs");
			if want_reuse {
				assert_eq!(got, value, "{name}: want reuse of the value");
			} else {
				assert_ne!(got, value, "{name}: want a temp identifier");
				assert!(
					s.ast.get::<TemporaryIdentifier>(got).is_some(),
					"{name}: want a TemporaryIdentifier"
				);
			}
		});
	}
}
