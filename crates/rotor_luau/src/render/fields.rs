//! Map fields and interpolated string parts.

use crate::render::{RenderResult, Renderer};
use crate::{InterpolatedStringPart, MapField, NodeData, NodeId, is_valid_identifier};

impl Renderer<'_> {
	/// `renderMapField.ts`
	pub(crate) fn render_map_field(&mut self, node: &MapField) -> RenderResult {
		match self.ast.data(node.index) {
			NodeData::StringLiteral(literal) if is_valid_identifier(&literal.value) => {
				self.identifier_key(node.index, &literal.value);
			}
			_ => {
				self.write("[");
				self.render(node.index)?;
				self.write("]");
			}
		}
		self.write(" = ");
		self.render(node.value)
	}

	/// Identifier-shaped string indices are emitted without quotes.
	pub(crate) fn identifier_key(&mut self, node: NodeId, value: &str) {
		if !self.s.positions {
			self.write(value);
			return;
		}
		let index = self.s.begin_range(node);
		self.write(value);
		self.s.end_range(index);
	}

	/// `renderInterpolatedStringPart.ts`: escapes braces, but not braces
	/// within `\u{...}` escapes, then escapes newlines (a CR with an
	/// optional LF after it, or an LF on its own).
	pub(crate) fn render_interpolated_string_part(&mut self, node: &InterpolatedStringPart) {
		let text = escape_interpolated_part(&node.text);
		self.write(&text);
	}
}

/// The length of a `\u{hex+}` escape at the start of `bytes`, if any.
fn unicode_escape_len(bytes: &[u8]) -> Option<usize> {
	let rest = bytes.strip_prefix(b"\\u{")?;
	let digits = rest.iter().take_while(|c| c.is_ascii_hexdigit()).count();
	(digits > 0 && rest.get(digits) == Some(&b'}')).then_some(3 + digits + 1)
}

pub fn escape_interpolated_part(text: &str) -> String {
	let bytes = text.as_bytes();
	let mut out = String::with_capacity(text.len());
	let mut start = 0;
	let mut i = 0;
	while i < bytes.len() {
		if let Some(len) = unicode_escape_len(&bytes[i..]) {
			i += len;
			continue;
		}
		let escape_len = match bytes[i] {
			b'\r' if bytes.get(i + 1) == Some(&b'\n') => 2,
			b'{' | b'}' | b'\n' | b'\r' => 1,
			_ => {
				i += 1;
				continue;
			}
		};
		out.push_str(&text[start..i]);
		out.push('\\');
		out.push_str(&text[i..i + escape_len]);
		i += escape_len;
		start = i;
	}
	out.push_str(&text[start..]);
	out
}
