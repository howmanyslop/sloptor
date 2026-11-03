//! `statements_test.go`: `checkVariableHoist` over `testdata/casehoist`.

use super::{transformer_testdata, with_state};
use crate::prelude::*;
use crate::statements::check_variable_hoist;

/// The fixture's switch clauses, through the first function's body.
fn case_clauses(state: &State<'_>) -> Vec<P<Node>> {
	let function = state.source_file.statements().nodes()[0];
	assert!(
		rotor_tsrs::ast::is_function_declaration(function),
		"expected FunctionDeclaration first"
	);
	let body = function.body().expect("the function has a body");
	let switch = body
		.statements()
		.iter()
		.copied()
		.rfind(|&statement| rotor_tsrs::ast::is_switch_statement(statement))
		.expect("switch statement not found");
	switch
		.as_switch_statement()
		.case_block
		.as_case_block()
		.clauses
		.nodes()
		.to_vec()
}

/// The declaration name of the clause's first statement, a variable statement in the fixture.
fn clause_declaration_name(clause: P<Node>) -> P<Node> {
	let statement = clause.statements()[0];
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

fn with_case_hoist<R: Send>(test: impl FnOnce(&mut State<'_>, Vec<P<Node>>) -> R + Send) -> R {
	with_state(&transformer_testdata("casehoist"), "src/case.ts", |state| {
		let clauses = case_clauses(state);
		test(state, clauses)
	})
}

/// A case-clause declaration referenced from a sibling clause is hoisted, recorded against the
/// declaring clause, and memoized.
#[test]
fn check_variable_hoist_cross_clause() {
	with_case_hoist(|s, clauses| {
		let shared_name = clause_declaration_name(clauses[0]);
		let symbol = s
			.ck()
			.get_symbol_at_location(shared_name)
			.expect("no symbol for shared");

		check_variable_hoist(s, shared_name, symbol);

		assert_eq!(
			s.is_hoisted.get(&symbol),
			Some(&true),
			"IsHoisted[shared] = false, want true"
		);
		assert_eq!(s.hoists_by_statement.get(&clauses[0]), Some(&vec![shared_name]));

		// Memoized: the decision is never reconsidered.
		check_variable_hoist(s, shared_name, symbol);
		assert_eq!(
			s.hoists_by_statement[&clauses[0]].len(),
			1,
			"second checkVariableHoist call must not double-register"
		);
	});
}

/// A declaration referenced only within its own clause records no decision.
#[test]
fn check_variable_hoist_own_clause_only() {
	with_case_hoist(|s, clauses| {
		let only_mine_name = clause_declaration_name(clauses[2]);
		let symbol = s
			.ck()
			.get_symbol_at_location(only_mine_name)
			.expect("no symbol for onlyMine");

		check_variable_hoist(s, only_mine_name, symbol);

		assert!(
			!s.is_hoisted.contains_key(&symbol),
			"IsHoisted[onlyMine] recorded, want no decision"
		);
		assert!(s.hoists_by_statement.get(&clauses[2]).is_none_or(Vec::is_empty));
	});
}

/// Declarations in a trailing default clause stay inline.
#[test]
fn check_variable_hoist_default_clause() {
	with_case_hoist(|s, clauses| {
		let default_clause = *clauses.last().expect("a default clause");
		let fallback_name = clause_declaration_name(default_clause);
		let symbol = s
			.ck()
			.get_symbol_at_location(fallback_name)
			.expect("no symbol for fallback");

		check_variable_hoist(s, fallback_name, symbol);

		assert!(
			!s.is_hoisted.contains_key(&symbol),
			"IsHoisted[fallback] recorded, want no decision"
		);
	});
}

/// An ordinary block-scoped declaration records no decision.
#[test]
fn check_variable_hoist_non_case_scope() {
	with_case_hoist(|s, _| {
		let function = s.source_file.statements().nodes()[0];
		let body = function.body().expect("the function has a body");
		let result_name = body.statements()[0]
			.as_variable_statement()
			.declaration_list
			.as_variable_declaration_list()
			.declarations
			.nodes()[0]
			.as_variable_declaration()
			.name;
		let symbol = s
			.ck()
			.get_symbol_at_location(result_name)
			.expect("no symbol for result");

		check_variable_hoist(s, result_name, symbol);

		assert!(
			!s.is_hoisted.contains_key(&symbol),
			"IsHoisted[result] recorded, want no decision"
		);
	});
}
