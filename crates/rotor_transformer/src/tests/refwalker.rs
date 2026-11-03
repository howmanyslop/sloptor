//! `refwalker_test.go`, over `testdata/typeprobe/src/refs.ts`.

use rotor_tsrs::ast as tsast;

use super::{transformer_testdata, with_state};
use crate::prelude::*;
use crate::refwalker::{for_each_symbol_reference, is_symbol_referenced};
use crate::util::node_pos;

struct Definitions {
	outer: P<Node>,
	inner: P<Node>,
	lonely: P<Node>,
}

fn declaration_name(statement: P<Node>) -> P<Node> {
	assert_eq!(statement.kind(), Kind::VariableStatement);
	statement
		.as_variable_statement()
		.declaration_list
		.as_variable_declaration_list()
		.declarations
		.nodes()[0]
		.as_variable_declaration()
		.name
}

/// The declaration names of the outer `counter`, the shadowed inner `counter`, and `lonely`.
fn with_refs<R: Send>(test: impl FnOnce(&mut State<'_>, Definitions) -> R + Send) -> R {
	with_state(&transformer_testdata("typeprobe"), "src/refs.ts", |s| {
		let statements = s.source_file.statements().nodes();
		let shadow_function = statements
			.iter()
			.copied()
			.rfind(|&statement| tsast::is_function_declaration(statement))
			.expect("shadow function not found");
		let body = shadow_function.body().expect("the function has a body");
		let definitions = Definitions {
			outer: declaration_name(statements[0]),
			inner: declaration_name(body.statements()[0]),
			lonely: declaration_name(statements[statements.len() - 1]),
		};
		test(s, definitions)
	})
}

/// The outer `counter` references, write then reads, in document order, skipping the
/// definition and the shadowed identifiers.
#[test]
fn walker_finds_references_in_order() {
	with_refs(|s, definitions| {
		let mut refs = Vec::new();
		let found = for_each_symbol_reference(s.checker, definitions.outer, s.source_file.as_node(), &mut |token| {
			refs.push(token);
			false
		});
		assert!(!found, "walker returned true although no callback returned true");
		assert_eq!(refs.len(), 4);

		// 1: assignment LHS, a write; 2-4: reads.
		let writes: Vec<bool> = refs
			.iter()
			.map(|&reference| tsast::is_write_access(reference))
			.collect();
		assert_eq!(writes, [true, false, false, false]);

		// In order: each ref starts after the previous one.
		for pair in refs.windows(2) {
			assert!(node_pos(pair[1]) > node_pos(pair[0]), "refs out of document order");
		}

		// 3: the shorthand `{ counter }`, matched through the shorthand value symbol.
		assert!(refs[2].parent().is_some_and(tsast::is_shorthand_property_assignment));

		// 4: inside the arrow function.
		assert!(tsast::find_ancestor(refs[3], tsast::is_arrow_function).is_some());
	});
}

/// A callback returning true stops the walk after the first reference.
#[test]
fn walker_early_out() {
	with_refs(|s, definitions| {
		let mut visited = 0;
		let found = for_each_symbol_reference(s.checker, definitions.outer, s.source_file.as_node(), &mut |_| {
			visited += 1;
			true
		});
		assert!(found, "walker must return true when the callback does");
		assert_eq!(visited, 1, "early-out");
	});
}

/// Only references inside the container are reported: the arrow function holds one of the four.
#[test]
fn walker_scoped_container() {
	with_refs(|s, definitions| {
		fn find(node: P<Node>) -> Option<P<Node>> {
			if tsast::is_arrow_function(node) {
				return Some(node);
			}
			let mut found = None;
			node.for_each_child(&mut |child| {
				found = find(child);
				found.is_some()
			});
			found
		}
		let arrow = find(s.source_file.as_node()).expect("arrow function not found");

		let mut visited = 0;
		for_each_symbol_reference(s.checker, definitions.outer, arrow, &mut |_| {
			visited += 1;
			false
		});
		assert_eq!(visited, 1);
	});
}

/// The inner `counter` resolves to its own symbol: a compound write and a return read, both in
/// the function.
#[test]
fn walker_shadowed_symbol() {
	with_refs(|s, definitions| {
		let mut refs = Vec::new();
		for_each_symbol_reference(s.checker, definitions.inner, s.source_file.as_node(), &mut |token| {
			refs.push(token);
			false
		});
		assert_eq!(refs.len(), 2);
		assert!(
			tsast::is_write_access(refs[0]),
			"compound assignment LHS must be a write access"
		);
		assert!(
			!tsast::is_write_access(refs[1]),
			"return-statement read must not be a write access"
		);
		for reference in refs {
			assert!(tsast::find_ancestor(reference, tsast::is_function_declaration).is_some());
		}
	});
}

/// Positive for a referenced symbol, negative for one never referenced.
#[test]
fn is_symbol_referenced_cases() {
	with_refs(|s, definitions| {
		let file = s.source_file.as_node();
		assert!(is_symbol_referenced(s.checker, definitions.outer, file));
		assert!(!is_symbol_referenced(s.checker, definitions.lonely, file));
	});
}
