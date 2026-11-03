//! Render state: the output buffer, indentation, and solved temporary names.

use crate::render::Renderer;
use crate::render::positions::ByteRange;
use crate::{ListNodeId, TempId};

const INDENT_CHARACTER: char = '\t';

/// The state of one render: the output text, the indentation depth, and the
/// names chosen for temporary identifiers.
#[derive(Clone, Debug, Default)]
pub struct RenderState {
	indent: usize,
	/// Solved names, indexed by [`TempId`].
	pub(crate) seen_temp_nodes: Vec<Option<String>>,
	temp_id_fallback: usize,
	list_nodes_stack: Vec<ListNodeId>,
	pub(crate) writer: String,
	pub(crate) positions: bool,
	pub(crate) ranges: Vec<ByteRange>,
	pub(crate) content_end: usize,
	pub(crate) current_range: usize,
	/// Ignores `multiline` layout flags on type nodes; `layout_types`
	/// measures a type's one-line width with it.
	pub(crate) single_line: bool,
}

impl RenderState {
	/// Creates an empty state.
	#[must_use]
	pub fn new() -> Self {
		Self::default()
	}

	pub(crate) fn temp_name(&self, temp: TempId) -> Option<&str> {
		self.seen_temp_nodes.get(temp.index()).and_then(Option::as_deref)
	}

	pub(crate) fn is_temp_solved(&self, temp: TempId) -> bool {
		self.temp_name(temp).is_some()
	}

	pub(crate) fn set_temp_name(&mut self, temp: TempId, name: String) {
		let index = temp.index();
		if self.seen_temp_nodes.len() <= index {
			self.seen_temp_nodes.resize(index + 1, None);
		}
		self.seen_temp_nodes[index] = Some(name);
	}

	/// The solved name of `temp`. The scoped solver should have named it;
	/// the `_N` counter fallback mirrors upstream's safety net.
	pub(crate) fn get_temp_name(&mut self, temp: TempId) -> String {
		if let Some(name) = self.temp_name(temp) {
			return name.to_owned();
		}
		let name = format!("_{}", self.temp_id_fallback);
		self.temp_id_fallback += 1;
		self.set_temp_name(temp, name.clone());
		name
	}

	pub(crate) fn push_list_node(&mut self, link: ListNodeId) {
		self.list_nodes_stack.push(link);
	}

	pub(crate) fn peek_list_node(&self) -> Option<ListNodeId> {
		self.list_nodes_stack.last().copied()
	}

	pub(crate) fn pop_list_node(&mut self) {
		self.list_nodes_stack.pop();
	}

	pub(crate) const fn push_indent(&mut self) {
		self.indent += 1;
	}

	pub(crate) const fn pop_indent(&mut self) {
		self.indent = self.indent.saturating_sub(1);
	}

	/// `text` at the current indentation.
	#[must_use]
	pub fn indented(&self, text: &str) -> String {
		let mut out = String::with_capacity(self.indent + text.len());
		out.extend(core::iter::repeat_n(INDENT_CHARACTER, self.indent));
		out.push_str(text);
		out
	}

	/// `text` at the current indentation, then a newline.
	#[must_use]
	pub fn line(&self, text: &str) -> String {
		let mut out = self.indented(text);
		out.push('\n');
		out
	}

	/// Runs `callback` one level deeper and returns its result.
	pub fn block(&mut self, callback: impl FnOnce(&mut Self) -> String) -> String {
		self.push_indent();
		let result = callback(self);
		self.pop_indent();
		result
	}
}

impl Renderer<'_> {
	/// Indentation is layout: it does not extend a node's content range.
	pub(crate) fn write_indent(&mut self) {
		self.s
			.writer
			.extend(core::iter::repeat_n(INDENT_CHARACTER, self.s.indent));
	}

	pub(crate) fn newline(&mut self) {
		self.s.writer.push('\n');
	}

	pub(crate) fn write(&mut self, text: &str) {
		self.s.writer.push_str(text);
		if self.s.positions {
			self.s.content_end = self.s.writer.len();
		}
	}

	pub(crate) fn closing(&mut self, keyword: &str) {
		if self.s.positions {
			let offset = self.s.writer.len();
			self.s.ranges[self.s.current_range].closing = Some(offset);
		}
		self.write(keyword);
	}

	pub(crate) fn operator(&mut self, text: &str) {
		if self.s.positions {
			let offset = self.s.writer.len();
			self.s.ranges[self.s.current_range].operator = Some(offset);
		}
		self.write(text);
	}
}
