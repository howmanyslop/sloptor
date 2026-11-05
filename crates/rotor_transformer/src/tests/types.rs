//! The predicate half of `types_test.go`, over `testdata/typeprobe/src/probe.ts`, through the
//! transformer's `IsDefinitelyType` / `IsPossiblyType` entry points.

use rotor_tsrs::jsnum::Number;
use rotor_tsrs::types::TypeCheck;

use super::{collect_identifiers, transformer_testdata, with_state};
use crate::prelude::*;
use crate::types::{check, is_definitely_type, is_possibly_type};

type Case = (&'static str, &'static str, &'static [TypeCheck], bool);

fn with_probe<R: Send>(test: impl FnOnce(&mut State<'_>) -> R + Send) -> R {
	with_state(&transformer_testdata("typeprobe"), "src/probe.ts", test)
}

/// The checker type of the unique identifier named `name` (Go's `probeType`).
fn probe_type(state: &mut State<'_>, name: &str) -> P<Type> {
	let identifiers = collect_identifiers(state.source_file.as_node(), name);
	assert_eq!(
		identifiers.len(),
		1,
		"found {} identifiers named {name}, want 1",
		identifiers.len()
	);
	state.get_type(identifiers[0])
}

/// The first node of `kind` under `root`, in source order (Go's `findFirstKind`).
fn find_first_kind(root: P<Node>, kind: Kind) -> P<Node> {
	fn visit(node: P<Node>, kind: Kind, found: &mut Option<P<Node>>) -> bool {
		if found.is_some() {
			return true;
		}
		if node.kind() == kind {
			*found = Some(node);
			return true;
		}
		node.for_each_child(&mut |child| visit(child, kind, found))
	}
	let mut found = None;
	root.for_each_child(&mut |child| visit(child, kind, &mut found));
	found.unwrap_or_else(|| panic!("no node of kind {kind:?} found"))
}

#[test]
fn is_definitely_type_cases() {
	with_probe(|s| {
		let cases: [Case; 20] = [
			// leaves
			("number is definitely number", "num", &[TypeCheck::Number], true),
			("number is not definitely string", "num", &[TypeCheck::String], false),
			("string is definitely string", "str", &[TypeCheck::String], true),
			("literal 5 is definitely number", "five", &[TypeCheck::Number], true),
			(
				"literal empty string is definitely string",
				"empty",
				&[TypeCheck::String],
				true,
			),
			("boolean is definitely boolean", "bool", &[TypeCheck::Boolean], true),
			// multiple callbacks = OR
			(
				"number is definitely string-or-number",
				"num",
				&[TypeCheck::String, TypeCheck::Number],
				true,
			),
			// union: EVERY member must match
			("0 | 1 is definitely number", "zeroOne", &[TypeCheck::Number], true),
			(
				"number | string is not definitely number",
				"numStr",
				&[TypeCheck::Number],
				false,
			),
			(
				"number | string is not definitely string",
				"numStr",
				&[TypeCheck::String],
				false,
			),
			(
				"number | string is definitely string-or-number",
				"numStr",
				&[TypeCheck::String, TypeCheck::Number],
				true,
			),
			(
				"string | undefined is not definitely string",
				"strOpt",
				&[TypeCheck::String],
				false,
			),
			// intersection: SOME member suffices
			(
				"branded string is definitely string",
				"branded",
				&[TypeCheck::String],
				true,
			),
			// any/unknown match nothing definitely
			("unknown is not definitely string", "unk", &[TypeCheck::String], false),
			("any is not definitely string", "anyVal", &[TypeCheck::String], false),
			// interfaces (exercises the class/interface base-type recursion)
			("interface is definitely object", "derived", &[TypeCheck::Object], true),
			(
				"interface is not definitely string",
				"derived",
				&[TypeCheck::String],
				false,
			),
			// generic constraint lookup: T extends string resolves to string
			(
				"T extends string is definitely string",
				"conT",
				&[TypeCheck::String],
				true,
			),
			(
				"T extends string is not definitely number",
				"conT",
				&[TypeCheck::Number],
				false,
			),
			// unconstrained type variable matches nothing definitely
			(
				"unconstrained U is not definitely string",
				"unconU",
				&[TypeCheck::String],
				false,
			),
		];
		for (name, variable, callbacks, want) in cases {
			let t = probe_type(s, variable);
			assert_eq!(is_definitely_type(s, t, callbacks), want, "{name}");
		}
	});
}

#[test]
#[expect(clippy::too_many_lines, reason = "the Go case table, kept whole")]
fn is_possibly_type_cases() {
	with_probe(|s| {
		let cases: [Case; 32] = [
			// truthiness-driving queries (digest §6 predicate semantics)
			(
				"number possibly 0",
				"num",
				&[TypeCheck::NumberLiteral(Number(0.0))],
				true,
			),
			("number possibly NaN", "num", &[TypeCheck::NaN], true),
			(
				"number not possibly empty string",
				"num",
				&[TypeCheck::EmptyString],
				false,
			),
			("string possibly empty string", "str", &[TypeCheck::EmptyString], true),
			(
				"string not possibly 0",
				"str",
				&[TypeCheck::NumberLiteral(Number(0.0))],
				false,
			),
			("string not possibly NaN", "str", &[TypeCheck::NaN], false),
			// literal types: isNumberLiteralType compares the literal value
			(
				"literal 5 not possibly 0",
				"five",
				&[TypeCheck::NumberLiteral(Number(0.0))],
				false,
			),
			(
				"literal 5 possibly 5",
				"five",
				&[TypeCheck::NumberLiteral(Number(5.0))],
				true,
			),
			("literal 5 not possibly NaN", "five", &[TypeCheck::NaN], false),
			(
				"literal 0 possibly 0",
				"zero",
				&[TypeCheck::NumberLiteral(Number(0.0))],
				true,
			),
			("literal 0 not possibly NaN", "zero", &[TypeCheck::NaN], false),
			// union: SOME member suffices
			(
				"0 | 1 possibly 0",
				"zeroOne",
				&[TypeCheck::NumberLiteral(Number(0.0))],
				true,
			),
			("0 | 1 not possibly NaN", "zeroOne", &[TypeCheck::NaN], false),
			(
				"string | undefined possibly string",
				"strOpt",
				&[TypeCheck::String],
				true,
			),
			(
				"string | undefined possibly undefined",
				"strOpt",
				&[TypeCheck::Undefined],
				true,
			),
			(
				"string | undefined possibly empty string",
				"strOpt",
				&[TypeCheck::EmptyString],
				true,
			),
			(
				"string | undefined not possibly 0",
				"strOpt",
				&[TypeCheck::NumberLiteral(Number(0.0))],
				false,
			),
			// intersection: SOME member suffices
			("branded string possibly string", "branded", &[TypeCheck::String], true),
			(
				"branded string not possibly number",
				"branded",
				&[TypeCheck::Number],
				false,
			),
			// any/unknown are possibly anything
			(
				"unknown possibly 0",
				"unk",
				&[TypeCheck::NumberLiteral(Number(0.0))],
				true,
			),
			("unknown possibly NaN", "unk", &[TypeCheck::NaN], true),
			("unknown possibly empty string", "unk", &[TypeCheck::EmptyString], true),
			("unknown possibly undefined", "unk", &[TypeCheck::Undefined], true),
			("any possibly string", "anyVal", &[TypeCheck::String], true),
			// unconstrained type variable is possibly anything
			("unconstrained U possibly number", "unconU", &[TypeCheck::Number], true),
			(
				"unconstrained U possibly undefined",
				"unconU",
				&[TypeCheck::Undefined],
				true,
			),
			// constrained type variable resolves to its constraint first
			(
				"T extends string possibly empty string",
				"conT",
				&[TypeCheck::EmptyString],
				true,
			),
			(
				"T extends string not possibly number",
				"conT",
				&[TypeCheck::Number],
				false,
			),
			// the rbxts `defined` special case: possibly anything EXCEPT a pure
			// undefined query
			("defined possibly string", "definedVal", &[TypeCheck::String], true),
			(
				"defined not possibly undefined (sole callback)",
				"definedVal",
				&[TypeCheck::Undefined],
				false,
			),
			(
				"defined possibly undefined-or-string (two callbacks)",
				"definedVal",
				&[TypeCheck::Undefined, TypeCheck::String],
				true,
			),
			// interfaces
			("interface not possibly string", "derived", &[TypeCheck::String], false),
		];
		for (name, variable, callbacks, want) in cases {
			let t = probe_type(s, variable);
			assert_eq!(is_possibly_type(s, t, callbacks), want, "{name}");
		}
	});
}

/// The fresh boolean literal types survive only on `const x = true/false` declaration names;
/// `getTypeAtLocation` on expressions strips freshness (upstream too). Probe both.
#[test]
fn direct_predicates() {
	with_probe(|s| {
		let fresh_true = probe_type(s, "troof");
		let fresh_false = probe_type(s, "falsef");
		let true_keyword = find_first_kind(s.source_file.as_node(), Kind::TrueKeyword);
		let regular_true_expression = s.get_type(true_keyword);
		let named = |s: &mut State<'_>, name: &str| probe_type(s, name);
		let cases: [(&str, TypeCheck, P<Type>, bool); 15] = [
			("isAnyType(any)", TypeCheck::Any, named(s, "anyVal"), true),
			("isAnyType(unknown)", TypeCheck::Any, named(s, "unk"), false),
			(
				"isBooleanType(true literal)",
				TypeCheck::Boolean,
				named(s, "troo"),
				true,
			),
			// isBooleanLiteralType compares identity against the REGULAR true/false types; a fresh
			// literal-expression type does NOT match.
			(
				"isBooleanLiteralType(true) on fresh true",
				TypeCheck::BooleanLiteral(true),
				fresh_true,
				false,
			),
			(
				"isBooleanLiteralType(false) on fresh true",
				TypeCheck::BooleanLiteral(false),
				fresh_true,
				false,
			),
			(
				"isBooleanLiteralType(false) on fresh false",
				TypeCheck::BooleanLiteral(false),
				fresh_false,
				false,
			),
			(
				"isBooleanLiteralType(true) on regular true (annotation)",
				TypeCheck::BooleanLiteral(true),
				named(s, "troo"),
				true,
			),
			(
				"isBooleanLiteralType(true) on regular true (expression)",
				TypeCheck::BooleanLiteral(true),
				regular_true_expression,
				true,
			),
			// non-literal fallback: plain boolean members count as possibly-false
			(
				"isBooleanLiteralType(false) falls back for non-literals",
				TypeCheck::BooleanLiteral(false),
				named(s, "num"),
				false,
			),
			("isNaNType(number)", TypeCheck::NaN, named(s, "num"), true),
			("isNaNType(literal 5)", TypeCheck::NaN, named(s, "five"), false),
			(
				"isEmptyStringType(empty literal)",
				TypeCheck::EmptyString,
				named(s, "empty"),
				true,
			),
			(
				"isEmptyStringType(string)",
				TypeCheck::EmptyString,
				named(s, "str"),
				true,
			),
			("isUndefinedType(number)", TypeCheck::Undefined, named(s, "num"), false),
			("isObjectType(interface)", TypeCheck::Object, named(s, "derived"), true),
		];
		for (name, callback, t, want) in cases {
			assert_eq!(check(s, callback, t), want, "{name}");
		}
	});
}
