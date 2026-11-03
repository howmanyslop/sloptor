//! Expression renderers.

use crate::render::parens::needs_parentheses;
use crate::render::util::get_safe_bracket_equals;
use crate::render::{RenderResult, Renderer, fail};
use crate::{
	Array, BinaryExpression, CallExpression, ComputedIndexExpression, FunctionExpression, Identifier, IfExpression,
	InterpolatedString, Map, MethodCallExpression, MixedTable, NodeData, NodeId, NumberLiteral,
	ParenthesizedExpression, PropertyAccessExpression, Set, StringLiteral, TemporaryIdentifier, UnaryExpression,
	UnaryOperator, is_valid_identifier, is_valid_number_literal, js_number_parse, js_number_string,
};

impl Renderer<'_> {
	pub(crate) fn render_identifier(&mut self, node: &Identifier) -> RenderResult {
		if !is_valid_identifier(&node.name) {
			return fail(format!("Invalid Luau Identifier: \"{}\"", node.name));
		}
		self.write(&node.name);
		Ok(())
	}

	pub(crate) fn render_temporary_identifier(&mut self, node: &TemporaryIdentifier) -> RenderResult {
		let name = self.s.get_temp_name(node.id);
		if !is_valid_identifier(&name) {
			return fail(format!("Invalid Temporary Identifier: \"{name}\""));
		}
		self.write(&name);
		Ok(())
	}

	pub(crate) fn render_computed_index_expression(&mut self, node: &ComputedIndexExpression) -> RenderResult {
		self.render(node.expression)?;
		if let NodeData::StringLiteral(literal) = self.ast.data(node.index)
			&& is_valid_identifier(&literal.value)
		{
			self.write(".");
			self.identifier_key(node.index, &literal.value);
			return Ok(());
		}
		self.write("[");
		self.render(node.index)?;
		self.write("]");
		Ok(())
	}

	pub(crate) fn render_property_access_expression(&mut self, node: &PropertyAccessExpression) -> RenderResult {
		self.render(node.expression)?;
		if is_valid_identifier(&node.name) {
			self.write(".");
			self.write(&node.name);
			return Ok(());
		}
		self.write("[\"");
		self.write(&node.name);
		self.write("\"]");
		Ok(())
	}

	pub(crate) fn render_call_expression(&mut self, node: &CallExpression) -> RenderResult {
		self.render(node.expression)?;
		self.write("(");
		self.comma_list(node.args)?;
		self.write(")");
		Ok(())
	}

	pub(crate) fn render_method_call_expression(&mut self, node: &MethodCallExpression) -> RenderResult {
		if !is_valid_identifier(&node.name) {
			return fail(format!("invalid method name: {}", node.name));
		}
		self.render(node.expression)?;
		self.write(":");
		self.write(&node.name);
		self.write("(");
		self.comma_list(node.args)?;
		self.write(")");
		Ok(())
	}

	pub(crate) fn render_parenthesized_expression(&mut self, node: &ParenthesizedExpression) -> RenderResult {
		let mut expression = node.expression;
		while let NodeData::ParenthesizedExpression(inner) = self.ast.data(expression) {
			expression = inner.expression;
		}
		if self.ast.kind(expression).is_simple() {
			return self.render(node.expression);
		}
		self.write("(");
		self.render(node.expression)?;
		self.write(")");
		Ok(())
	}

	pub(crate) fn render_number_literal(&mut self, node: &NumberLiteral) {
		if is_valid_number_literal(&node.value) {
			self.write(&node.value);
			return;
		}
		match js_number_parse(&node.value) {
			Ok(value) => self.write(&js_number_string(value)),
			Err(_) => self.write("NaN"),
		}
	}

	fn needs_bracket_spacing(&self, node: NodeId) -> bool {
		let Some(parent) = self.ast.parent(node) else {
			return false;
		};
		match self.ast.data(parent) {
			NodeData::MapField(field) => field.index == node,
			NodeData::ComputedIndexExpression(index) => index.index == node,
			NodeData::Set(_) => true,
			_ => false,
		}
	}

	pub(crate) fn render_string_literal(&mut self, node: NodeId, literal: &StringLiteral) {
		let value = literal.value.as_str();
		let is_multiline = value.contains('\n');
		if !is_multiline && !value.contains('"') {
			self.write("\"");
			self.write(value);
			self.write("\"");
			return;
		}
		if !is_multiline && !value.contains('\'') {
			self.write("'");
			self.write(value);
			self.write("'");
			return;
		}
		let equals = get_safe_bracket_equals(value);
		let spacing = self.needs_bracket_spacing(node);
		if spacing {
			self.write(" ");
		}
		self.write("[");
		self.write(&equals);
		self.write("[");
		self.write(value);
		self.write("]");
		self.write(&equals);
		self.write("]");
		if spacing {
			self.write(" ");
		}
	}

	pub(crate) fn render_function_expression(&mut self, node: NodeId, function: &FunctionExpression) -> RenderResult {
		self.write("function");
		self.type_generics(&function.generics, false)?;
		self.write("(");
		self.parameters(node)?;
		self.write(")");
		self.return_type(node)?;
		if self.ast.list_is_empty(function.statements) {
			self.write(" ");
			self.closing("end");
			return Ok(());
		}
		self.newline();
		self.block(function.statements)?;
		self.write_indent();
		self.closing("end");
		Ok(())
	}

	pub(crate) fn render_binary_expression(&mut self, node: NodeId, binary: &BinaryExpression) -> RenderResult {
		let parens = needs_parentheses(self.ast, node);
		if parens {
			self.write("(");
		}
		self.render(binary.left)?;
		self.write(" ");
		self.operator(binary.operator.as_str());
		self.write(" ");
		self.render(binary.right)?;
		if parens {
			self.write(")");
		}
		Ok(())
	}

	fn unary_needs_space(&self, unary: &UnaryExpression) -> bool {
		if unary.operator == UnaryOperator::Not {
			return true;
		}
		matches!(self.ast.data(unary.expression), NodeData::UnaryExpression(inner) if inner.operator == UnaryOperator::Negate)
	}

	pub(crate) fn render_unary_expression(&mut self, node: NodeId, unary: &UnaryExpression) -> RenderResult {
		let parens = needs_parentheses(self.ast, node);
		if parens {
			self.write("(");
		}
		self.write(unary.operator.as_str());
		if self.unary_needs_space(unary) {
			self.write(" ");
		}
		self.render(unary.expression)?;
		if parens {
			self.write(")");
		}
		Ok(())
	}

	pub(crate) fn render_if_expression(&mut self, node: NodeId, expression: &IfExpression) -> RenderResult {
		let parens = needs_parentheses(self.ast, node);
		if parens {
			self.write("(");
		}
		self.if_expression(expression, "if ")?;
		if parens {
			self.write(")");
		}
		Ok(())
	}

	fn if_expression(&mut self, node: &IfExpression, keyword: &str) -> RenderResult {
		self.write(keyword);
		self.render(node.condition)?;
		self.write(" then ");
		self.render(node.expression)?;
		self.write(" ");
		let ast = self.ast;
		if let NodeData::IfExpression(alternative) = ast.data(node.alternative) {
			if self.s.positions {
				let index = self.s.begin_range(node.alternative);
				self.if_expression(alternative, "elseif ")?;
				self.s.end_range(index);
			} else {
				self.if_expression(alternative, "elseif ")?;
			}
			return Ok(());
		}
		self.write("else ");
		self.render(node.alternative)
	}

	pub(crate) fn render_interpolated_string(&mut self, node: &InterpolatedString) -> RenderResult {
		self.write("`");
		let ast = self.ast;
		for part in ast.list_iter(node.parts) {
			if matches!(ast.data(part), NodeData::InterpolatedStringPart(_)) {
				self.render(part)?;
				continue;
			}
			self.write("{");
			let table = ast.kind(part).is_table();
			if table {
				self.write("(");
			}
			self.render(part)?;
			if table {
				self.write(")");
			}
			self.write("}");
		}
		self.write("`");
		Ok(())
	}

	pub(crate) fn render_array(&mut self, node: &Array) -> RenderResult {
		if self.ast.list_is_empty(node.members) {
			self.write("{}");
			return Ok(());
		}
		self.write("{ ");
		self.comma_list(node.members)?;
		self.write(" }");
		Ok(())
	}

	fn multiline_fields<T>(&mut self, fields: crate::List<T>) -> RenderResult {
		if self.ast.list_is_empty(fields) {
			self.write("{}");
			return Ok(());
		}
		self.write("{");
		self.newline();
		self.s.push_indent();
		let ast = self.ast;
		for field in ast.list_iter(fields) {
			self.write_indent();
			self.render(field)?;
			self.write(",");
			self.newline();
		}
		self.s.pop_indent();
		self.write_indent();
		self.write("}");
		Ok(())
	}

	pub(crate) fn render_map(&mut self, node: &Map) -> RenderResult {
		self.multiline_fields(node.fields)
	}

	pub(crate) fn render_mixed_table(&mut self, node: &MixedTable) -> RenderResult {
		self.multiline_fields(node.fields)
	}

	pub(crate) fn render_set(&mut self, node: &Set) -> RenderResult {
		if self.ast.list_is_empty(node.members) {
			self.write("{}");
			return Ok(());
		}
		self.write("{");
		self.newline();
		self.s.push_indent();
		let ast = self.ast;
		for member in ast.list_iter(node.members) {
			self.write_indent();
			self.write("[");
			self.render(member)?;
			self.write("] = true,");
			self.newline();
		}
		self.s.pop_indent();
		self.write_indent();
		self.write("}");
		Ok(())
	}
}
