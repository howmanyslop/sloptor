//! `jecs_rebase.go`: what the sidecar needs to rebuild, for a file native jecs rewrote, the tree
//! the JavaScript jecs transformer would have handed the next plugin.
//!
//! [`TransformerRebase`] holds the printed rewrite's nodes, each with the range it was written at
//! in the file on disk, or none when jecs made it up. A plugin that maps its own edits back
//! through those ranges, as the React compiler applier does, then reports the positions it
//! reported under the JavaScript transformer, and so does the trace the sidecar prints.

use std::collections::HashSet;

use rotor_tsrs::P;
use rotor_tsrs::ast::{Kind, Node, SourceFile};
use rotor_tsrs::scanner::get_token_pos_of_node;

use crate::transformer_host::TransformerRebase;

/// `newJecsRebase`: pairs the tree native jecs printed with the parse of what it printed. `None`
/// when the two do not line up node for node; the sidecar then works from the printed text
/// alone.
pub(crate) fn new_jecs_rebase(
	original: P<SourceFile>,
	transformed: P<SourceFile>,
	printed: &str,
) -> Option<TransformerRebase> {
	let opts = original.parse_options().clone();
	let script_kind = rotor_tsrs::tscore::get_script_kind_from_file_name(original.file_name());
	let parsed = rotor_tsrs::parser::parse_source_file(opts, printed, script_kind);
	let mut from_original: HashSet<P<Node>> = HashSet::new();
	collect_nodes(original.as_node(), &mut from_original);

	let printed_offset = utf16_offsets(printed);
	let original_offset = utf16_offsets(original.text());
	let mut pairing = Pairing {
		parsed,
		from_original: &from_original,
		printed_offset: &printed_offset,
		original_offset: &original_offset,
		ranges: Vec::new(),
	};
	let statements = transformed.statements().nodes();
	let reparsed_statements = parsed.statements().nodes();
	if statements.len() != reparsed_statements.len() {
		return None;
	}
	for (&statement, &reparsed) in statements.iter().zip(reparsed_statements) {
		if !pairing.pair(statement, reparsed) {
			return None;
		}
	}
	Some(TransformerRebase {
		text: original.text().to_owned(),
		ranges: pairing.ranges,
	})
}

/// Every node under `node` (not `node` itself), Go's `collect`.
fn collect_nodes(node: P<Node>, into: &mut HashSet<P<Node>>) {
	node.for_each_child(&mut |child| {
		into.insert(child);
		collect_nodes(child, into);
		false
	});
}

struct Pairing<'a> {
	parsed: P<SourceFile>,
	from_original: &'a HashSet<P<Node>>,
	printed_offset: &'a [i64],
	original_offset: &'a [i64],
	ranges: Vec<i64>,
}

impl Pairing<'_> {
	fn pair(&mut self, node: P<Node>, reparsed: P<Node>) -> bool {
		let start = self.printed_offset[get_token_pos_of_node(reparsed, self.parsed, false) as usize];
		let reparsed_end = self.printed_offset[offset(reparsed.end())];
		// The printer parenthesizes expressions and types where precedence demands it; the tree
		// it printed has no node there.
		if matches!(reparsed.kind(), Kind::ParenthesizedExpression | Kind::ParenthesizedType)
			&& node.kind() != reparsed.kind()
		{
			self.ranges.extend([start, reparsed_end, -1, -1, 1]);
			let Some(&inner) = child_nodes(reparsed).first() else {
				return false;
			};
			return self.pair(node, inner);
		}
		if node.kind() != reparsed.kind() {
			return false;
		}
		let (mut pos, mut end) = (-1, -1);
		if let (Some(node_pos), Some(node_end)) = (node.pos().source_offset(), node.end().source_offset())
			&& node_end >= node_pos
			&& (node_end as usize) < self.original_offset.len()
		{
			pos = self.original_offset[node_pos as usize];
			end = self.original_offset[node_end as usize];
		}
		let synthesized = i64::from(!self.from_original.contains(&node));
		self.ranges.extend([start, reparsed_end, pos, end, synthesized]);
		let children = child_nodes(node);
		let reparsed_children = child_nodes(reparsed);
		if children.len() != reparsed_children.len() {
			return false;
		}
		children
			.into_iter()
			.zip(reparsed_children)
			.all(|(child, reparsed_child)| self.pair(child, reparsed_child))
	}
}

/// A parsed node's offset.
fn offset(pos: rotor_tsrs::tscore::TextPos) -> usize {
	pos.source_offset().unwrap_or(0) as usize
}

/// `childNodes`
fn child_nodes(node: P<Node>) -> Vec<P<Node>> {
	let mut children = Vec::new();
	node.for_each_child(&mut |child| {
		children.push(child);
		false
	});
	children
}

/// `utf16Offsets`: every byte offset of `text`, and its end, mapped to the UTF-16 offset
/// TypeScript gives the same position.
pub(crate) fn utf16_offsets(text: &str) -> Vec<i64> {
	let mut offsets = vec![0; text.len() + 1];
	let mut units: i64 = 0;
	for (index, character) in text.char_indices() {
		for slot in &mut offsets[index..index + character.len_utf8()] {
			*slot = units;
		}
		units += i64::try_from(character.len_utf16()).unwrap_or(1);
	}
	offsets[text.len()] = units;
	offsets
}
