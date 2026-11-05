//! Ports of `internal/luau/render/*_test.go`.

mod dsl;
mod expressions;
mod fallback_order;
mod golden;
mod interpolated;
mod positions;
mod solve_temp_ids;
mod statements;
mod types;
mod util;

use crate::render::solve_temp_ids::solve_temp_ids;
use crate::render::{Range, RenderState, render, render_ast, render_ast_with_positions};
use crate::{Ast, List, NodeId, Payload, Position, TempId, TemporaryIdentifier, element};

/// Every AST-based renderer example runs through both public APIs.
fn render_both(ast: &Ast, statements: List<element::Statement>) -> String {
	let plain = render_ast(ast, statements).expect("render_ast");
	let (positioned, _) = render_ast_with_positions(ast, statements).expect("render_ast_with_positions");
	assert_eq!(positioned, plain, "position text differs from plain text");
	plain
}

/// Renders an expression standalone after solving its temporaries, then
/// checks it renders inside a return statement through both public APIs.
fn render_expr(ast: &mut Ast, expression: NodeId) -> String {
	let mut state = RenderState::new();
	solve_temp_ids(&mut state, ast, expression.into()).expect("solve");
	let text = render(&mut state, ast, expression).expect("render");
	let statement = ast.return_statement(expression);
	let statements = stmts(ast, &[statement]);
	render_both(ast, statements);
	text
}

fn stmts(ast: &mut Ast, values: &[NodeId]) -> List<element::Statement> {
	ast.list_from(values.iter().copied())
}

fn exprs(ast: &mut Ast, values: &[NodeId]) -> List<element::Expression> {
	ast.list_from(values.iter().copied())
}

fn ids(ast: &mut Ast, values: &[NodeId]) -> List<element::AnyIdentifier> {
	ast.list_from(values.iter().copied())
}

/// A payload field: `field!(ast, CallExpression, node).expression`.
fn payload<T: Payload>(ast: &Ast, node: NodeId) -> &T {
	ast.get::<T>(node)
		.unwrap_or_else(|| panic!("{} is not the expected kind", ast.kind(node)))
}

fn head<T>(ast: &Ast, list: List<T>) -> NodeId {
	ast.link_value(ast.list_head(list).expect("non-empty list"))
}

fn tail<T>(ast: &Ast, list: List<T>) -> NodeId {
	ast.link_value(ast.list_tail(list).expect("non-empty list"))
}

fn temp_of(ast: &Ast, node: NodeId) -> TempId {
	payload::<TemporaryIdentifier>(ast, node).id
}

fn node_range(node: NodeId, line: u32, column: u32, end_line: u32, end_column: u32) -> Range {
	Range {
		node,
		start: Position::new(line, column),
		end: Position::new(end_line, end_column),
		closing: None,
		operator: None,
	}
}

fn closed_range(
	node: NodeId,
	(line, column): (u32, u32),
	(end_line, end_column): (u32, u32),
	(closing_line, closing_column): (u32, u32),
) -> Range {
	Range {
		closing: Some(Position::new(closing_line, closing_column)),
		..node_range(node, line, column, end_line, end_column)
	}
}

fn returned_of(ast: &Ast, statement: NodeId) -> NodeId {
	match payload::<crate::ReturnStatement>(ast, statement).expression {
		crate::NodeOrList::Node(node) => node,
		crate::NodeOrList::List(_) => panic!("return of a list"),
	}
}
