//! Renders a Luau [`Ast`] to text: the port of upstream `LuauRenderer`.
//!
//! The Go renderer panics on an AST it cannot render; here every such case is
//! a [`RenderError`] carrying the same message.

mod ending;
mod expressions;
mod fields;
mod parens;
mod positions;
mod solve_temp_ids;
mod state;
mod statements;
mod type_layout;
mod types;
mod util;
mod visit;

use core::fmt;

pub use positions::{Range, render_ast_with_positions};
pub use state::RenderState;
pub use type_layout::{DEFAULT_TYPE_LINE_WIDTH, layout_types};

use crate::list::element;
use crate::{Ast, List, NodeData, NodeId, NodeOrList};
use visit::Visitor;

/// Why an AST could not be rendered. The message matches the Go renderer's
/// panic text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderError {
	message: String,
}

impl RenderError {
	pub(crate) fn new(message: impl Into<String>) -> Self {
		Self {
			message: message.into(),
		}
	}

	/// The message.
	#[must_use]
	pub fn message(&self) -> &str {
		&self.message
	}
}

impl fmt::Display for RenderError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(&self.message)
	}
}

impl core::error::Error for RenderError {}

pub(crate) type RenderResult<T = ()> = Result<T, RenderError>;

pub(crate) fn fail<T>(message: impl Into<String>) -> RenderResult<T> {
	Err(RenderError::new(message))
}

/// Renders one node, dispatching on its kind (upstream `render.ts`
/// `KIND_TO_RENDERER`).
///
/// Unlike [`render_ast`], standalone rendering does not solve scoped temporary
/// names: it reserves fallback names in the old evaluation order (map-field
/// values before indices) and then renders. Output accumulates in `state`;
/// the return value is this call's text.
///
/// # Errors
///
/// When the AST cannot be rendered.
pub fn render(state: &mut RenderState, ast: &Ast, node: NodeId) -> RenderResult<String> {
	state.reserve_fallback_temp_names(ast, NodeOrList::Node(node));
	let start = state.writer.len();
	Renderer { ast, s: state }.render(node)?;
	Ok(state.writer[start..].to_owned())
}

/// Solves temporary names, then renders `statements` (upstream
/// `renderAST()`).
///
/// # Errors
///
/// When the AST cannot be rendered.
pub fn render_ast(ast: &Ast, statements: List<element::Statement>) -> RenderResult<String> {
	let mut state = RenderState::new();
	state.prepare_temp_names(ast, statements.into())?;
	Renderer { ast, s: &mut state }.statements(statements)?;
	Ok(state.writer)
}

/// Renders a statement list with an existing state and returns this call's
/// text, without solving temporary names.
#[cfg(test)]
pub(crate) fn render_statements(
	state: &mut RenderState,
	ast: &Ast,
	statements: List<element::Statement>,
) -> RenderResult<String> {
	let start = state.writer.len();
	Renderer { ast, s: state }.statements(statements)?;
	Ok(state.writer[start..].to_owned())
}

/// A render in progress: the AST being read and the state being written.
pub(crate) struct Renderer<'a> {
	pub(crate) ast: &'a Ast,
	pub(crate) s: &'a mut RenderState,
}

impl Renderer<'_> {
	pub(crate) fn render(&mut self, node: NodeId) -> RenderResult {
		if self.ast.kind(node).is_statement() {
			self.write_indent();
		}
		if !self.s.positions {
			return self.render_node(node);
		}
		let parent = self.s.current_range;
		let index = self.s.begin_range(node);
		self.s.current_range = index;
		self.render_node(node)?;
		self.s.end_range(index);
		self.s.current_range = parent;
		Ok(())
	}

	pub(crate) fn render_node(&mut self, node: NodeId) -> RenderResult {
		let ast = self.ast;
		match ast.data(node) {
			NodeData::Identifier(n) => self.render_identifier(n),
			NodeData::TemporaryIdentifier(n) => self.render_temporary_identifier(n),
			NodeData::ComputedIndexExpression(n) => self.render_computed_index_expression(n),
			NodeData::PropertyAccessExpression(n) => self.render_property_access_expression(n),
			NodeData::CallExpression(n) => self.render_call_expression(n),
			NodeData::MethodCallExpression(n) => self.render_method_call_expression(n),
			NodeData::ParenthesizedExpression(n) => self.render_parenthesized_expression(n),
			NodeData::None => fail("Cannot render None"),
			NodeData::NilLiteral => {
				self.write("nil");
				Ok(())
			}
			NodeData::FalseLiteral => {
				self.write("false");
				Ok(())
			}
			NodeData::TrueLiteral => {
				self.write("true");
				Ok(())
			}
			NodeData::NumberLiteral(n) => {
				self.render_number_literal(n);
				Ok(())
			}
			NodeData::StringLiteral(n) => {
				self.render_string_literal(node, n);
				Ok(())
			}
			NodeData::VarArgsLiteral => {
				self.write("...");
				Ok(())
			}
			NodeData::FunctionExpression(n) => self.render_function_expression(node, n),
			NodeData::BinaryExpression(n) => self.render_binary_expression(node, n),
			NodeData::UnaryExpression(n) => self.render_unary_expression(node, n),
			NodeData::IfExpression(n) => self.render_if_expression(node, n),
			NodeData::InterpolatedString(n) => self.render_interpolated_string(n),
			NodeData::Array(n) => self.render_array(n),
			NodeData::Map(n) => self.render_map(n),
			NodeData::Set(n) => self.render_set(n),
			NodeData::MixedTable(n) => self.render_mixed_table(n),
			NodeData::CastExpression(n) => self.render_cast_expression(node, n),
			NodeData::Assignment(n) => self.render_assignment(node, n),
			NodeData::BreakStatement => {
				self.write("break");
				self.newline();
				Ok(())
			}
			NodeData::CallStatement(n) => self.render_call_statement(node, n),
			NodeData::ContinueStatement => {
				self.write("continue");
				self.newline();
				Ok(())
			}
			NodeData::DoStatement(n) => self.render_do_statement(n),
			NodeData::WhileStatement(n) => self.render_while_statement(n),
			NodeData::RepeatStatement(n) => self.render_repeat_statement(n),
			NodeData::IfStatement(n) => self.render_if_statement(n, "if "),
			NodeData::NumericForStatement(n) => self.render_numeric_for_statement(n),
			NodeData::ForStatement(n) => self.render_for_statement(n),
			NodeData::FunctionDeclaration(n) => self.render_function_declaration(node, n),
			NodeData::MethodDeclaration(n) => self.render_method_declaration(node, n),
			NodeData::VariableDeclaration(n) => self.render_variable_declaration(node, n),
			NodeData::ReturnStatement(n) => self.render_return_statement(n),
			NodeData::Comment(n) => {
				self.render_comment(n);
				Ok(())
			}
			NodeData::TypeAlias(n) => self.render_type_alias(n),
			NodeData::TypeFunctionDeclaration(n) => self.render_type_function_declaration(node, n),
			NodeData::MapField(n) => self.render_map_field(n),
			NodeData::InterpolatedStringPart(n) => {
				self.render_interpolated_string_part(n);
				Ok(())
			}
			NodeData::TypeReference(n) => self.render_type_reference(n),
			NodeData::TypeStringSingleton(n) => {
				self.render_type_string_singleton(n);
				Ok(())
			}
			NodeData::TypeBooleanSingleton(n) => {
				self.write(if n.value { "true" } else { "false" });
				Ok(())
			}
			NodeData::TypeOptional(n) => self.render_type_optional(n),
			NodeData::TypeUnion(n) => self.render_type_union(n),
			NodeData::TypeIntersection(n) => self.render_type_intersection(n),
			NodeData::TypeTable(n) => self.render_type_table(n),
			NodeData::TypeArray(n) => self.render_type_array(n),
			NodeData::TypeFunction(n) => self.render_type_function(n),
			NodeData::TypeTypeof(n) => self.render_type_typeof(n),
			NodeData::TypePack(n) => self.render_type_pack(n),
			NodeData::TypeVariadic(n) => self.render_type_variadic(n),
			NodeData::TypeGenericPack(n) => self.render_type_generic_pack(n),
			NodeData::TypeTableProp(n) => self.render_type_table_prop(n),
			NodeData::TypeTableIndexer(n) => self.render_type_table_indexer(n),
		}
	}
}

impl RenderState {
	fn reserve_fallback_temp_names(&mut self, ast: &Ast, root: NodeOrList) {
		struct Reserve<'s> {
			state: &'s mut RenderState,
		}
		impl Visitor for Reserve<'_> {
			const RENDER_ORDER: bool = true;

			fn before(&mut self, ast: &Ast, node: NodeId) {
				if let NodeData::TemporaryIdentifier(temp) = ast.data(node) {
					self.state.get_temp_name(temp.id);
				}
			}
		}
		visit::visit_node_or_list(ast, root, &mut Reserve { state: self });
	}

	fn prepare_temp_names(&mut self, ast: &Ast, root: NodeOrList) -> RenderResult {
		struct NeedsFallback<'s> {
			state: &'s RenderState,
			found: bool,
		}
		impl Visitor for NeedsFallback<'_> {
			const RENDER_ORDER: bool = true;

			fn before(&mut self, ast: &Ast, node: NodeId) {
				if let NodeData::TemporaryIdentifier(temp) = ast.data(node)
					&& !self.state.is_temp_solved(temp.id)
				{
					self.found = true;
				}
			}
		}
		for iterator in solve_temp_ids::solve_temp_ids(self, ast, root)? {
			let mut check = NeedsFallback {
				state: self,
				found: false,
			};
			visit::visit_node(ast, iterator, &mut check);
			if check.found {
				// Iterator expressions are intentionally outside the scoped
				// solver. Only ASTs with actual fallback temps need a
				// compatibility pass. Walk from the root to preserve
				// value-before-index order even across separate iterators
				// nested in map key/value callbacks.
				self.reserve_fallback_temp_names(ast, root);
				return Ok(());
			}
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests;
