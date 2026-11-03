//! Small adapters between Go's `*ast.Node` idioms and the tsrs handles.

use rotor_tsrs::ast::{self, Kind, Node, NodeList};
use rotor_tsrs::{P, tscore};

/// Go `node.Pos()`: `-1` for a synthesized node.
#[must_use]
pub fn node_pos(node: P<Node>) -> i64 {
	text_pos(node.pos())
}

/// Go `node.End()`: `-1` for a synthesized node.
#[must_use]
pub fn node_end(node: P<Node>) -> i64 {
	text_pos(node.end())
}

/// A tsrs position as Go's int, `-1` when synthesized.
#[must_use]
pub fn text_pos(pos: tscore::TextPos) -> i64 {
	pos.source_offset().map_or(-1, i64::from)
}

/// Go `nodeList.Nodes`, treating an absent list as empty.
#[must_use]
pub fn nodes(list: Option<P<NodeList>>) -> &'static [P<Node>] {
	list.map_or(&[], |list| list.nodes())
}

/// Go `ast.IsIdentifier`.
#[must_use]
pub fn is_identifier(node: P<Node>) -> bool {
	node.kind() == Kind::Identifier
}

/// Whether `node` is `ancestor` or one of its descendants (`traversal.ts` `isAncestorOf`).
#[must_use]
pub fn is_ancestor_of(ancestor: P<Node>, node: P<Node>) -> bool {
	let mut current = Some(node);
	while let Some(candidate) = current {
		if candidate == ancestor {
			return true;
		}
		current = candidate.parent();
	}
	false
}

/// Go `ast.FindAncestor(node, ast.IsStatement)`.
#[must_use]
pub fn find_statement_ancestor(node: P<Node>) -> Option<P<Node>> {
	ast::find_ancestor(node, ast::is_statement)
}

/// The index of `node` in `nodes`, or `None` (Go's `-1`).
#[must_use]
pub fn index_of_node(nodes: &[P<Node>], node: P<Node>) -> Option<usize> {
	nodes.iter().position(|&candidate| candidate == node)
}
