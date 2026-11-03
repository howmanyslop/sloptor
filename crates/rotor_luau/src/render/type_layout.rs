//! Multiline layout for wide type tables and alias-body unions.

use crate::render::visit::{self, Visitor};
use crate::render::{RenderError, RenderResult, RenderState, Renderer};
use crate::{Ast, NodeData, NodeId, NodeOrList, TypeTable, TypeUnion};

/// The column past which [`layout_types`] breaks a table type or an
/// alias-body union across lines, like `StyLua`'s hanging layout.
pub const DEFAULT_TYPE_LINE_WIDTH: usize = 80;

struct Layout {
	measure: RenderState,
	width: usize,
	/// Whether each node is some alias's body, indexed by [`NodeId`].
	alias_bodies: Vec<bool>,
	decisions: Vec<(NodeId, bool)>,
	error: Option<RenderError>,
}

impl Layout {
	/// Renders `node` on one line and compares its width in characters to
	/// the limit.
	fn too_wide(&mut self, ast: &Ast, node: NodeId) -> bool {
		if self.width == 0 || self.error.is_some() {
			return false;
		}
		self.measure.writer.clear();
		if let Err(error) = (Renderer {
			ast,
			s: &mut self.measure,
		})
		.render(node)
		{
			self.error = Some(error);
			return false;
		}
		crate::utf8::rune_count(&self.measure.writer) > self.width
	}
}

impl Visitor for Layout {
	fn before(&mut self, ast: &Ast, node: NodeId) {
		if let NodeData::TypeAlias(alias) = ast.data(node)
			&& let Some(body) = alias.body
		{
			self.alias_bodies[body.index()] = true;
		}
	}

	fn after(&mut self, ast: &Ast, node: NodeId) {
		match ast.data(node) {
			NodeData::TypeTable(_) => {
				let multiline = self.too_wide(ast, node);
				self.decisions.push((node, multiline));
			}
			NodeData::TypeUnion(_) => {
				let multiline = self.alias_bodies[node.index()] && self.too_wide(ast, node);
				self.decisions.push((node, multiline));
			}
			_ => {}
		}
	}
}

/// Marks wide table types and alias-body unions under `root` as multiline.
///
/// A type is wide when its own single-line text is wider than `width`
/// characters. Children are measured before parents, so a type that contains a too-wide type breaks too. It is
/// opt-in: [`render_ast`](crate::render::render_ast) never calls it, so output
/// stays single-line unless a caller asks. A width of zero clears every flag.
///
/// # Errors
///
/// When a measured type cannot be rendered; no flag changes then.
pub fn layout_types(ast: &mut Ast, root: impl Into<NodeOrList>, width: usize) -> RenderResult {
	let mut measure = RenderState::new();
	measure.single_line = true;
	let mut layout = Layout {
		measure,
		width,
		alias_bodies: vec![false; ast.node_count()],
		decisions: Vec::new(),
		error: None,
	};
	visit::visit_node_or_list(ast, root.into(), &mut layout);
	if let Some(error) = layout.error {
		return Err(error);
	}
	for (node, multiline) in layout.decisions {
		if let Some(table) = ast.get_mut::<TypeTable>(node) {
			table.multiline = multiline;
		} else if let Some(union) = ast.get_mut::<TypeUnion>(node) {
			union.multiline = multiline;
		}
	}
	Ok(())
}
