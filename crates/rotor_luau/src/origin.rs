//! Source anchors for generated nodes.

use crate::{Ast, NodeId, OriginId};

/// A 0-based source coordinate with a UTF-16 column.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Position {
	/// The 0-based line.
	pub line: u32,
	/// The 0-based UTF-16 column.
	pub column: u32,
}

impl Position {
	/// Builds a position.
	#[must_use]
	pub const fn new(line: u32, column: u32) -> Self {
		Self { line, column }
	}
}

/// Anchors a generated node to its source start and optional closing token.
/// Shallow clones share the same [`OriginId`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Origin {
	/// Where the node starts.
	pub start: Position,
	/// The closing token, such as `end` or `until`.
	pub closing: Option<Position>,
	/// The operator token.
	pub operator: Option<Position>,
	/// Distinguishes an operator-only anchor from the valid start 0:0.
	pub start_unset: bool,
}

impl Ast {
	/// Stores `origin` in the origin table.
	pub fn add_origin(&mut self, origin: Origin) -> OriginId {
		let id = OriginId::from_index(self.origin_table.len());
		self.origin_table.push(origin);
		id
	}

	/// The stored value of `origin`.
	#[must_use]
	pub fn origin_data(&self, origin: OriginId) -> &Origin {
		&self.origin_table[origin.index()]
	}

	/// The origin of `node`, if any.
	#[must_use]
	pub fn origin(&self, node: NodeId) -> Option<OriginId> {
		self.origins[node.index()]
	}

	/// Fills missing fields of `node`'s origin, preserving more precise
	/// anchors. Existing origins may be shared by shallow clones, so filling
	/// never mutates them: a merge that changes anything stores a new origin.
	pub fn set_origin(&mut self, node: NodeId, origin: OriginId) {
		let Some(previous_id) = self.origins[node.index()] else {
			self.origins[node.index()] = Some(origin);
			return;
		};
		let previous = self.origin_table[previous_id.index()];
		let incoming = self.origin_table[origin.index()];
		let mut merged = previous;
		if merged.start_unset && !incoming.start_unset {
			merged.start = incoming.start;
			merged.start_unset = false;
		}
		if merged.closing.is_none() {
			merged.closing = incoming.closing;
		}
		if merged.operator.is_none() {
			merged.operator = incoming.operator;
		}
		if merged != previous {
			let merged_id = self.add_origin(merged);
			self.origins[node.index()] = Some(merged_id);
		}
	}

	/// Makes an occurrence-specific shallow clone without changing the
	/// original node or its parent. Temporary identifiers keep their rendering
	/// ID.
	pub fn with_origin(&mut self, node: NodeId, origin: Option<OriginId>) -> NodeId {
		let clone = self.shallow_clone(node);
		self.parents[clone.index()] = None;
		self.origins[clone.index()] = origin;
		clone
	}
}

#[cfg(test)]
mod tests;
