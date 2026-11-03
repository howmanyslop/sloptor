//! Shared rendering helpers: lists, bindings, parameters, and statements.

use crate::list::element;
use crate::render::types::TypeSlot;
use crate::render::{RenderResult, Renderer, fail};
use crate::{List, NodeData, NodeId, NodeOrList};

/// The `=` padding needed so that `[`+eq+`[` and `]`+eq+`]` safely enclose
/// `text`. A literal port of upstream `getSafeBracketEquals.ts`.
pub fn get_safe_bracket_equals(text: &str) -> String {
	let mut equals = String::new();
	loop {
		let closing = format!("]{equals}]");
		let tail = format!("]{equals}");
		if !text.contains(&closing) && !text.ends_with(&tail) {
			return equals;
		}
		equals.push('=');
	}
}

impl Renderer<'_> {
	/// Renders `values` separated by `, `.
	pub(crate) fn comma_list<T>(&mut self, values: List<T>) -> RenderResult {
		let ast = self.ast;
		for (index, value) in ast.list_iter(values).enumerate() {
			if index > 0 {
				self.write(", ");
			}
			self.render(value)?;
		}
		Ok(())
	}

	/// Renders an identifier where it is bound, with its `: T` annotation.
	pub(crate) fn binding(&mut self, identifier: NodeId) -> RenderResult {
		self.render(identifier)?;
		if let Some(annotation) = self.ast.binding_type(identifier) {
			self.write(": ");
			self.render_type(Some(annotation), TypeSlot::Top)?;
		}
		Ok(())
	}

	pub(crate) fn typed_bindings(&mut self, identifiers: List<element::AnyIdentifier>) -> RenderResult {
		let ast = self.ast;
		for (index, value) in ast.list_iter(identifiers).enumerate() {
			if index > 0 {
				self.write(", ");
			}
			self.binding(value)?;
		}
		Ok(())
	}

	/// Renders the left side of a `local`: one identifier or a list, each
	/// with its binding annotation.
	pub(crate) fn bindings(&mut self, left: NodeOrList) -> RenderResult {
		match left {
			NodeOrList::List(list) => {
				if let Some(identifiers) = list.typed::<element::AnyIdentifier>() {
					if self.ast.list_is_empty(identifiers) {
						return fail("empty identifier list");
					}
					return self.typed_bindings(identifiers);
				}
				self.expr_or_list(left)
			}
			NodeOrList::Node(node) if self.ast.kind(node).is_any_identifier() => self.binding(node),
			NodeOrList::Node(_) => self.expr_or_list(left),
		}
	}

	pub(crate) fn parameters(&mut self, node: NodeId) -> RenderResult {
		let Some(params) = self.ast.param_data(node) else {
			return fail("render: parameters got nil parameter list");
		};
		self.typed_bindings(params.params)?;
		if params.has_dot_dot_dot {
			if self.ast.list_is_non_empty(params.params) {
				self.write(", ");
			}
			self.write("...");
			if let Some(var_args_type) = params.var_args_type {
				self.write(": ");
				self.render_type(Some(var_args_type), TypeSlot::Top)?;
			}
		}
		Ok(())
	}

	pub(crate) fn return_type(&mut self, node: NodeId) -> RenderResult {
		if let Some(annotation) = self.ast.return_type_data(node) {
			self.write(": ");
			self.render_type(Some(annotation), TypeSlot::Top)?;
		}
		Ok(())
	}

	pub(crate) fn statements(&mut self, statements: List<element::Statement>) -> RenderResult {
		let ast = self.ast;
		let mut has_final_statement = false;
		for link in ast.list_links(statements) {
			let value = ast.link_value(link);
			if has_final_statement && !matches!(ast.data(value), NodeData::Comment(_)) {
				return fail("Cannot render statement after break, continue, or return!");
			}
			has_final_statement = has_final_statement || ast.kind(value).is_final_statement();
			self.s.push_list_node(link);
			self.render(value)?;
			self.s.pop_list_node();
		}
		Ok(())
	}

	pub(crate) fn block(&mut self, statements: List<element::Statement>) -> RenderResult {
		self.s.push_indent();
		self.statements(statements)?;
		self.s.pop_indent();
		Ok(())
	}
}
