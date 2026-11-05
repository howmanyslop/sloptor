//! Rendered ranges for source maps.

use crate::list::element;
use crate::render::{RenderResult, RenderState, Renderer};
use crate::{Ast, List, NodeId, Position};

/// One rendered node occurrence.
///
/// Coordinates are 0-based with UTF-16 columns. `end` is exclusive; surrounding indentation and newlines are
/// excluded. `closing` points at the `end` or `until` keyword, when present.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
	/// The rendered node.
	pub node: NodeId,
	/// Where the node's text starts.
	pub start: Position,
	/// Where the node's text ends (exclusive).
	pub end: Position,
	/// The closing keyword.
	pub closing: Option<Position>,
	/// The operator token.
	pub operator: Option<Position>,
}

#[derive(Clone, Copy, Debug)]
pub struct ByteRange {
	node: NodeId,
	start: usize,
	end: usize,
	pub(crate) closing: Option<usize>,
	pub(crate) operator: Option<usize>,
}

/// Renders the same text as [`render_ast`](crate::render::render_ast), with
/// ranges.
///
/// Ranges come in emission order, with enclosing nodes before children
/// that start at the same position. A node rendered more than once has a range
/// for each occurrence.
///
/// # Errors
///
/// When the AST cannot be rendered.
pub fn render_ast_with_positions(
	ast: &Ast,
	statements: List<element::Statement>,
) -> RenderResult<(String, Vec<Range>)> {
	render_ast_with_positions_bytes(ast, statements)
		.map(|(text, ranges)| (crate::utf8::into_string_lossy(text), ranges))
}

/// [`render_ast_with_positions`] as the bytes Go writes. Columns count UTF-16 units the way Go's
/// renderer does: an invalid byte is one unit.
///
/// # Errors
///
/// When the AST cannot be rendered.
pub fn render_ast_with_positions_bytes(
	ast: &Ast,
	statements: List<element::Statement>,
) -> RenderResult<(Vec<u8>, Vec<Range>)> {
	let mut state = RenderState::new();
	state.positions = true;
	state.prepare_temp_names(ast, statements.into())?;
	Renderer { ast, s: &mut state }.statements(statements)?;
	let ranges = positions_for_ranges(&state.writer, &state.ranges);
	Ok((state.writer, ranges))
}

impl RenderState {
	pub(crate) fn begin_range(&mut self, node: NodeId) -> usize {
		let index = self.ranges.len();
		self.ranges.push(ByteRange {
			node,
			start: self.writer.len(),
			end: 0,
			closing: None,
			operator: None,
		});
		index
	}

	pub(crate) fn end_range(&mut self, index: usize) {
		self.ranges[index].end = self.content_end;
	}
}

#[derive(Clone, Copy)]
enum Target {
	Start,
	End,
	Closing,
	Operator,
}

fn positions_for_ranges(bytes: &[u8], offsets: &[ByteRange]) -> Vec<Range> {
	let mut ranges: Vec<Range> = offsets
		.iter()
		.map(|offset| Range {
			node: offset.node,
			start: Position::default(),
			end: Position::default(),
			closing: offset.closing.map(|_| Position::default()),
			operator: offset.operator.map(|_| Position::default()),
		})
		.collect();
	let mut coordinates: Vec<(usize, usize, Target)> = Vec::with_capacity(3 * offsets.len());
	for (index, offset) in offsets.iter().enumerate() {
		coordinates.push((offset.start, index, Target::Start));
		coordinates.push((offset.end, index, Target::End));
		if let Some(closing) = offset.closing {
			coordinates.push((closing, index, Target::Closing));
		}
		if let Some(operator) = offset.operator {
			coordinates.push((operator, index, Target::Operator));
		}
	}
	coordinates.sort_by_key(|&(offset, _, _)| offset);

	let mut position = Position::default();
	let mut byte_offset = 0;
	for (offset, index, target) in coordinates {
		while byte_offset < offset {
			let c = bytes[byte_offset];
			if c == b'\n' {
				position.line += 1;
				position.column = 0;
				byte_offset += 1;
			} else if c.is_ascii() {
				position.column += 1;
				byte_offset += 1;
			} else {
				let (rune, width) = crate::utf8::decode_rune(&bytes[byte_offset..]);
				position.column += crate::utf8::utf16_units(rune);
				byte_offset += width.max(1);
			}
		}
		let range = &mut ranges[index];
		match target {
			Target::Start => range.start = position,
			Target::End => range.end = position,
			Target::Closing => range.closing = Some(position),
			Target::Operator => range.operator = Some(position),
		}
	}
	ranges
}
