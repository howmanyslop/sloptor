//! The `;` that separates a statement ending in an indexable expression from a
//! next statement that starts with `(`.

use crate::list::element;
use crate::render::{RenderResult, Renderer, fail};
use crate::{Ast, NodeData, NodeId, NodeOrList};

fn ends_with_indexable_expression_inner(ast: &Ast, node: NodeId) -> bool {
	if ast.kind(node).is_indexable_expression() {
		return true;
	}
	match ast.data(node) {
		NodeData::BinaryExpression(n) => ends_with_indexable_expression_inner(ast, n.right),
		NodeData::UnaryExpression(n) => ends_with_indexable_expression_inner(ast, n.expression),
		NodeData::IfExpression(n) => ends_with_indexable_expression_inner(ast, n.alternative),
		_ => false,
	}
}

fn last_expr_of(ast: &Ast, value: NodeOrList) -> RenderResult<Option<NodeId>> {
	match value {
		NodeOrList::List(list) => {
			let message = if list.typed::<element::Expression>().is_some() {
				"empty expression list"
			} else if list.typed::<element::WritableExpression>().is_some() {
				"empty writable list"
			} else if list.typed::<element::AnyIdentifier>().is_some() {
				"empty identifier list"
			} else {
				return Ok(None);
			};
			ast.header(list.id)
				.tail
				.map_or_else(|| fail(message), |tail| Ok(Some(ast.link_value(tail))))
		}
		NodeOrList::Node(node) => Ok(ast.kind(node).is_expression().then_some(node)),
	}
}

fn ends_with_indexable_expression(ast: &Ast, node: NodeId) -> RenderResult<bool> {
	let value = match ast.data(node) {
		NodeData::CallStatement(_) => return Ok(true),
		NodeData::VariableDeclaration(n) => n.right.unwrap_or(n.left),
		NodeData::Assignment(n) => n.right,
		_ => return Ok(false),
	};
	Ok(last_expr_of(ast, value)?.is_some_and(|last| ends_with_indexable_expression_inner(ast, last)))
}

fn starts_with_parenthesis_inner(ast: &Ast, node: NodeId) -> bool {
	match ast.data(node) {
		NodeData::ParenthesizedExpression(_) => true,
		NodeData::CallExpression(n) => starts_with_parenthesis_inner(ast, n.expression),
		NodeData::MethodCallExpression(n) => starts_with_parenthesis_inner(ast, n.expression),
		NodeData::PropertyAccessExpression(n) => starts_with_parenthesis_inner(ast, n.expression),
		NodeData::ComputedIndexExpression(n) => starts_with_parenthesis_inner(ast, n.expression),
		_ => false,
	}
}

fn starts_with_parenthesis(ast: &Ast, node: NodeId) -> RenderResult<bool> {
	match ast.data(node) {
		NodeData::CallStatement(n) => Ok(match ast.data(n.expression) {
			NodeData::CallExpression(call) => starts_with_parenthesis_inner(ast, call.expression),
			NodeData::MethodCallExpression(call) => starts_with_parenthesis_inner(ast, call.expression),
			_ => false,
		}),
		NodeData::Assignment(n) => match n.left {
			NodeOrList::List(list) => {
				if list.typed::<element::WritableExpression>().is_none() {
					return Ok(false);
				}
				ast.header(list.id).head.map_or_else(
					|| fail("empty assignment left list"),
					|head| Ok(starts_with_parenthesis_inner(ast, ast.link_value(head))),
				)
			}
			NodeOrList::Node(left) => Ok(ast.kind(left).is_expression() && starts_with_parenthesis_inner(ast, left)),
		},
		_ => Ok(false),
	}
}

impl Renderer<'_> {
	fn next_non_comment(&self) -> Option<NodeId> {
		let mut next = self.ast.link_next(self.s.peek_list_node()?);
		while let Some(link) = next {
			let value = self.ast.link_value(link);
			if !matches!(self.ast.data(value), NodeData::Comment(_)) {
				return Some(value);
			}
			next = self.ast.link_next(link);
		}
		None
	}

	pub(crate) fn ending(&self, node: NodeId) -> RenderResult<&'static str> {
		let Some(next) = self.next_non_comment() else {
			return Ok("");
		};
		if ends_with_indexable_expression(self.ast, node)? && starts_with_parenthesis(self.ast, next)? {
			return Ok(";");
		}
		Ok("")
	}
}
