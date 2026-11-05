//! Statement renderers.

use crate::list::element;
use crate::render::util::get_safe_bracket_equals;
use crate::render::{RenderResult, Renderer, fail};
use crate::{
	Assignment, CallStatement, Comment, DoStatement, ForStatement, FunctionDeclaration, IfStatement, ListKind,
	MethodDeclaration, NodeData, NodeId, NodeOrList, NumericForStatement, RepeatStatement, ReturnStatement,
	VariableDeclaration, WhileStatement, js_number_parse,
};

impl Renderer<'_> {
	pub(crate) fn expr_or_list(&mut self, value: NodeOrList) -> RenderResult {
		match value {
			NodeOrList::Node(node) => self.render(node),
			NodeOrList::List(list) => {
				let message = match list.kind {
					ListKind::Expression => "empty expression list",
					ListKind::WritableExpression => "empty writable expression list",
					ListKind::AnyIdentifier => "empty identifier list",
					_ => return fail("renderExprOrList: unsupported value"),
				};
				if self.ast.header(list.id).head.is_none() {
					return fail(message);
				}
				let ast = self.ast;
				for (index, value) in ast.any_list_iter(list.id).enumerate() {
					if index > 0 {
						self.write(", ");
					}
					self.render(value)?;
				}
				Ok(())
			}
		}
	}

	pub(crate) fn render_return_statement(&mut self, node: &ReturnStatement) -> RenderResult {
		// Empty tuple returns preserve upstream's trailing space.
		self.write("return ");
		let empty_tuple = node
			.expression
			.list()
			.is_some_and(|list| list.kind == ListKind::Expression && self.ast.header(list.id).head.is_none());
		if !empty_tuple {
			self.expr_or_list(node.expression)?;
		}
		self.newline();
		Ok(())
	}

	pub(crate) fn render_assignment(&mut self, node: NodeId, assignment: &Assignment) -> RenderResult {
		self.expr_or_list(assignment.left)?;
		self.write(" ");
		self.operator(assignment.operator.as_str());
		self.write(" ");
		self.expr_or_list(assignment.right)?;
		let ending = self.ending(node)?;
		self.write(ending);
		self.newline();
		Ok(())
	}

	pub(crate) fn render_call_statement(&mut self, node: NodeId, statement: &CallStatement) -> RenderResult {
		self.render(statement.expression)?;
		let ending = self.ending(node)?;
		self.write(ending);
		self.newline();
		Ok(())
	}

	pub(crate) fn render_comment(&mut self, node: &Comment) {
		let text = node.text.replace("\r\n", "\n").replace('\r', "\n");
		if text.contains('\n') {
			let equals = get_safe_bracket_equals(&text);
			self.write("--[");
			self.write(&equals);
			self.write("[");
			self.newline();
			self.s.push_indent();
			for line in text.split('\n') {
				self.write_indent();
				self.write(line);
				self.newline();
			}
			self.s.pop_indent();
			self.write_indent();
			self.write("]");
			self.write(&equals);
			self.write("]");
			self.newline();
			return;
		}
		self.write("--");
		self.write(&text);
		self.newline();
	}

	fn end_block(&mut self, statements: crate::List<element::Statement>) -> RenderResult {
		self.newline();
		self.block(statements)?;
		self.write_indent();
		self.closing("end");
		self.newline();
		Ok(())
	}

	pub(crate) fn render_do_statement(&mut self, node: &DoStatement) -> RenderResult {
		self.write("do");
		self.end_block(node.statements)
	}

	pub(crate) fn render_while_statement(&mut self, node: &WhileStatement) -> RenderResult {
		self.write("while ");
		self.render(node.condition)?;
		self.write(" do");
		self.end_block(node.statements)
	}

	pub(crate) fn render_repeat_statement(&mut self, node: &RepeatStatement) -> RenderResult {
		self.write("repeat");
		self.newline();
		self.block(node.statements)?;
		self.write_indent();
		self.closing("until ");
		self.render(node.condition)?;
		self.newline();
		Ok(())
	}

	pub(crate) fn render_if_statement(&mut self, node: &IfStatement, keyword: &str) -> RenderResult {
		self.write(keyword);
		self.render(node.condition)?;
		self.write(" then");
		self.newline();
		self.block(node.statements)?;
		let ast = self.ast;
		match node.else_body {
			NodeOrList::Node(link) => {
				if let NodeData::IfStatement(next) = ast.data(link) {
					self.write_indent();
					if self.s.positions {
						let parent = self.s.current_range;
						let index = self.s.begin_range(link);
						self.s.current_range = index;
						self.render_if_statement(next, "elseif ")?;
						self.s.end_range(index);
						self.s.current_range = parent;
						self.s.ranges[parent].closing = self.s.ranges[index].closing;
					} else {
						self.render_if_statement(next, "elseif ")?;
					}
					return Ok(());
				}
			}
			NodeOrList::List(list) => {
				if let Some(statements) = list.typed::<element::Statement>()
					&& ast.list_is_non_empty(statements)
				{
					self.write_indent();
					self.write("else");
					self.newline();
					self.block(statements)?;
				}
			}
		}
		self.write_indent();
		self.closing("end");
		self.newline();
		Ok(())
	}

	pub(crate) fn render_numeric_for_statement(&mut self, node: &NumericForStatement) -> RenderResult {
		self.write("for ");
		self.render(node.id)?;
		self.write(" = ");
		self.render(node.start)?;
		self.write(", ");
		self.render(node.end)?;
		if let Some(step) = node.step {
			let is_one = match self.ast.data(step) {
				NodeData::NumberLiteral(literal) => {
					js_number_parse(&literal.value).is_ok_and(|value| value.to_bits() == 1f64.to_bits())
				}
				_ => false,
			};
			if !is_one {
				self.write(", ");
				self.render(step)?;
			}
		}
		self.write(" do");
		self.end_block(node.statements)
	}

	pub(crate) fn render_for_statement(&mut self, node: &ForStatement) -> RenderResult {
		self.write("for ");
		if self.ast.list_is_empty(node.ids) {
			self.write("_");
		} else {
			self.comma_list(node.ids)?;
		}
		self.write(" in ");
		self.render(node.expression)?;
		self.write(" do");
		self.end_block(node.statements)
	}

	pub(crate) fn render_function_declaration(
		&mut self,
		node: NodeId,
		declaration: &FunctionDeclaration,
	) -> RenderResult {
		if declaration.localize {
			if !self.ast.kind(declaration.name).is_any_identifier() {
				return fail("local function cannot be a property");
			}
			self.write("local ");
		}
		self.write("function ");
		self.render(declaration.name)?;
		self.type_generics(&declaration.generics, false)?;
		self.write("(");
		self.parameters(node)?;
		self.write(")");
		self.return_type(node)?;
		self.end_block(declaration.statements)
	}

	pub(crate) fn render_method_declaration(&mut self, node: NodeId, declaration: &MethodDeclaration) -> RenderResult {
		self.write("function ");
		self.render(declaration.expression)?;
		self.write(":");
		self.write(&declaration.name);
		self.type_generics(&declaration.generics, false)?;
		self.write("(");
		self.parameters(node)?;
		self.write(")");
		self.return_type(node)?;
		self.end_block(declaration.statements)
	}

	pub(crate) fn render_variable_declaration(
		&mut self,
		node: NodeId,
		declaration: &VariableDeclaration,
	) -> RenderResult {
		self.write("local ");
		self.bindings(declaration.left)?;
		if let Some(right) = declaration.right {
			self.write(" = ");
			self.expr_or_list(right)?;
		}
		let ending = self.ending(node)?;
		self.write(ending);
		self.newline();
		Ok(())
	}
}
