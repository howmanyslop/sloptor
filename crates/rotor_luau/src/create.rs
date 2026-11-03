//! Constructors, one per kind. Each adopts its children: a child that already
//! has a parent is attached as a shallow clone (upstream `create()`).

use crate::jsnumber::js_number_string;
use crate::list::element;
use crate::nodes::{
	Array, Assignment, BinaryExpression, CallExpression, CallStatement, Comment, ComputedIndexExpression, DoStatement,
	ForStatement, FunctionDeclaration, FunctionExpression, Identifier, IfExpression, IfStatement, InterpolatedString,
	InterpolatedStringPart, Map, MapField, MethodCallExpression, MethodDeclaration, MixedTable, NumberLiteral,
	NumericForStatement, ParenthesizedExpression, PropertyAccessExpression, RepeatStatement, ReturnStatement, Set,
	StringLiteral, TemporaryIdentifier, UnaryExpression, VariableDeclaration, WhileStatement,
};
use crate::{AssignmentOperator, Ast, BinaryOperator, List, NodeData, NodeId, NodeOrList, UnaryOperator};

impl Ast {
	/// `name`
	pub fn id(&mut self, name: impl Into<String>) -> NodeId {
		self.alloc(Identifier {
			name: name.into(),
			annotation: None,
		})
	}

	/// A temporary named `_name` (or `_` for an empty hint) at render time.
	pub fn temp_id(&mut self, name: impl Into<String>) -> NodeId {
		let id = self.next_temp_id();
		self.alloc(TemporaryIdentifier {
			name: name.into(),
			id,
			exact_name: false,
			annotation: None,
		})
	}

	/// A temporary named exactly `name` unless that collides.
	pub fn exact_temp_id(&mut self, name: impl Into<String>) -> NodeId {
		let id = self.next_temp_id();
		self.alloc(TemporaryIdentifier {
			name: name.into(),
			id,
			exact_name: true,
			annotation: None,
		})
	}

	/// `expression[index]`
	pub fn computed_index(&mut self, expression: NodeId, index: NodeId) -> NodeId {
		let node = self.reserve();
		let expression = self.adopt(node, expression);
		let index = self.adopt(node, index);
		self.fill(node, ComputedIndexExpression { expression, index })
	}

	/// `expression.name`
	pub fn property_access(&mut self, expression: NodeId, name: impl Into<String>) -> NodeId {
		let node = self.reserve();
		let expression = self.adopt(node, expression);
		self.fill(
			node,
			PropertyAccessExpression {
				expression,
				name: name.into(),
			},
		)
	}

	/// `expression(args)`
	pub fn call(&mut self, expression: NodeId, args: List<element::Expression>) -> NodeId {
		let node = self.reserve();
		let expression = self.adopt(node, expression);
		let args = self.adopt_list(node, args);
		self.fill(node, CallExpression { expression, args })
	}

	/// `expression:name(args)`
	pub fn method_call(
		&mut self,
		name: impl Into<String>,
		expression: NodeId,
		args: List<element::Expression>,
	) -> NodeId {
		let node = self.reserve();
		let expression = self.adopt(node, expression);
		let args = self.adopt_list(node, args);
		self.fill(
			node,
			MethodCallExpression {
				name: name.into(),
				expression,
				args,
			},
		)
	}

	/// `(expression)`
	pub fn parenthesized(&mut self, expression: NodeId) -> NodeId {
		let node = self.reserve();
		let expression = self.adopt(node, expression);
		self.fill(node, ParenthesizedExpression { expression })
	}

	/// The `None` placeholder.
	pub fn none(&mut self) -> NodeId {
		self.alloc(NodeData::None)
	}

	/// `nil`
	pub fn nil(&mut self) -> NodeId {
		self.alloc(NodeData::NilLiteral)
	}

	/// `...`
	pub fn var_args(&mut self) -> NodeId {
		self.alloc(NodeData::VarArgsLiteral)
	}

	/// `true` or `false`
	pub fn bool(&mut self, value: bool) -> NodeId {
		self.alloc(if value {
			NodeData::TrueLiteral
		} else {
			NodeData::FalseLiteral
		})
	}

	/// A number literal kept as `value`.
	pub fn number_literal(&mut self, value: impl Into<String>) -> NodeId {
		self.alloc(NumberLiteral { value: value.into() })
	}

	/// Mirrors upstream `luau.number()`: negatives become unary minus.
	pub fn num(&mut self, value: f64) -> NodeId {
		if value >= 0.0 {
			return self.number_literal(js_number_string(value));
		}
		let magnitude = self.num(value.abs());
		self.unary(UnaryOperator::Negate, magnitude)
	}

	/// A string literal.
	pub fn str(&mut self, value: impl Into<String>) -> NodeId {
		self.alloc(StringLiteral {
			value: value.into(),
			raw: None,
		})
	}

	/// A string literal of arbitrary bytes, which need not be valid UTF-8 (Go's byte strings).
	pub fn str_bytes(&mut self, bytes: Vec<u8>) -> NodeId {
		match String::from_utf8(bytes) {
			Ok(value) => self.str(value),
			Err(error) => {
				let raw = error.into_bytes().into_boxed_slice();
				let value = String::from_utf8_lossy(&raw).into_owned();
				self.alloc(StringLiteral { value, raw: Some(raw) })
			}
		}
	}

	/// `--text`
	pub fn comment(&mut self, text: impl Into<String>) -> NodeId {
		self.alloc(Comment { text: text.into() })
	}

	/// `function(params) ... end`
	pub fn function_expression(
		&mut self,
		params: List<element::AnyIdentifier>,
		has_dot_dot_dot: bool,
		statements: List<element::Statement>,
	) -> NodeId {
		let node = self.reserve();
		let parameters = self.adopt_list(node, params);
		let statements = self.adopt_list(node, statements);
		self.fill(
			node,
			FunctionExpression {
				return_type: None,
				generics: Vec::new(),
				var_args_type: None,
				parameters,
				has_dot_dot_dot,
				statements,
			},
		)
	}

	/// `left op right`
	pub fn binary(&mut self, left: NodeId, operator: BinaryOperator, right: NodeId) -> NodeId {
		let node = self.reserve();
		let left = self.adopt(node, left);
		let right = self.adopt(node, right);
		self.fill(node, BinaryExpression { left, operator, right })
	}

	/// `op expression`
	pub fn unary(&mut self, operator: UnaryOperator, expression: NodeId) -> NodeId {
		let node = self.reserve();
		let expression = self.adopt(node, expression);
		self.fill(node, UnaryExpression { operator, expression })
	}

	/// `if condition then expression else alternative`
	pub fn if_expression(&mut self, condition: NodeId, expression: NodeId, alternative: NodeId) -> NodeId {
		let node = self.reserve();
		let condition = self.adopt(node, condition);
		let expression = self.adopt(node, expression);
		let alternative = self.adopt(node, alternative);
		self.fill(
			node,
			IfExpression {
				condition,
				expression,
				alternative,
			},
		)
	}

	/// `` `parts` ``
	pub fn interpolated_string(&mut self, parts: List<element::Node>) -> NodeId {
		let node = self.reserve();
		let parts = self.adopt_list(node, parts);
		self.fill(node, InterpolatedString { parts })
	}

	/// The literal text of an interpolated string.
	pub fn interpolated_string_part(&mut self, text: impl Into<String>) -> NodeId {
		self.alloc(InterpolatedStringPart { text: text.into() })
	}

	/// `{ members }`
	pub fn array(&mut self, members: List<element::Expression>) -> NodeId {
		let node = self.reserve();
		let members = self.adopt_list(node, members);
		self.fill(node, Array { members })
	}

	/// `{ [member] = true }`
	pub fn set(&mut self, members: List<element::Expression>) -> NodeId {
		let node = self.reserve();
		let members = self.adopt_list(node, members);
		self.fill(node, Set { members })
	}

	/// `[index] = value`
	pub fn map_field(&mut self, index: NodeId, value: NodeId) -> NodeId {
		let node = self.reserve();
		let index = self.adopt(node, index);
		let value = self.adopt(node, value);
		self.fill(node, MapField { index, value })
	}

	/// `{ fields }`
	pub fn map(&mut self, fields: List<element::MapField>) -> NodeId {
		let node = self.reserve();
		let fields = self.adopt_list(node, fields);
		self.fill(node, Map { fields })
	}

	/// A table of positional members and fields.
	pub fn mixed_table(&mut self, fields: List<element::Node>) -> NodeId {
		let node = self.reserve();
		let fields = self.adopt_list(node, fields);
		self.fill(node, MixedTable { fields })
	}

	/// `left op right`
	pub fn assignment(
		&mut self,
		left: impl Into<NodeOrList>,
		operator: AssignmentOperator,
		right: impl Into<NodeOrList>,
	) -> NodeId {
		let node = self.reserve();
		let left = self.adopt_node_or_list(node, left.into());
		let right = self.adopt_node_or_list(node, right.into());
		self.fill(node, Assignment { left, operator, right })
	}

	/// `break`
	pub fn break_statement(&mut self) -> NodeId {
		self.alloc(NodeData::BreakStatement)
	}

	/// `continue`
	pub fn continue_statement(&mut self) -> NodeId {
		self.alloc(NodeData::ContinueStatement)
	}

	/// A call used as a statement.
	pub fn call_statement(&mut self, expression: NodeId) -> NodeId {
		let node = self.reserve();
		let expression = self.adopt(node, expression);
		self.fill(node, CallStatement { expression })
	}

	/// `do ... end`
	pub fn do_statement(&mut self, statements: List<element::Statement>) -> NodeId {
		let node = self.reserve();
		let statements = self.adopt_list(node, statements);
		self.fill(node, DoStatement { statements })
	}

	/// `while condition do ... end`
	pub fn while_statement(&mut self, condition: NodeId, statements: List<element::Statement>) -> NodeId {
		let node = self.reserve();
		let condition = self.adopt(node, condition);
		let statements = self.adopt_list(node, statements);
		self.fill(node, WhileStatement { condition, statements })
	}

	/// `repeat ... until condition`
	pub fn repeat_statement(&mut self, condition: NodeId, statements: List<element::Statement>) -> NodeId {
		let node = self.reserve();
		let statements = self.adopt_list(node, statements);
		let condition = self.adopt(node, condition);
		self.fill(node, RepeatStatement { condition, statements })
	}

	/// `if condition then ... else_body end`. `None` means no else and stores
	/// an empty statement list, matching how upstream transform code builds
	/// if statements.
	pub fn if_statement(
		&mut self,
		condition: NodeId,
		statements: List<element::Statement>,
		else_body: Option<NodeOrList>,
	) -> NodeId {
		let node = self.reserve();
		let condition = self.adopt(node, condition);
		let statements = self.adopt_list(node, statements);
		let else_body = else_body.unwrap_or_else(|| self.new_list::<element::Statement>().into());
		let else_body = self.adopt_node_or_list(node, else_body);
		self.fill(
			node,
			IfStatement {
				condition,
				statements,
				else_body,
			},
		)
	}

	/// `for id = start, end, step do ... end`
	pub fn numeric_for(
		&mut self,
		id: NodeId,
		start: NodeId,
		end: NodeId,
		step: Option<NodeId>,
		statements: List<element::Statement>,
	) -> NodeId {
		let node = self.reserve();
		let id = self.adopt(node, id);
		let start = self.adopt(node, start);
		let end = self.adopt(node, end);
		let step = step.map(|step| self.adopt(node, step));
		let statements = self.adopt_list(node, statements);
		self.fill(
			node,
			NumericForStatement {
				id,
				start,
				end,
				step,
				statements,
			},
		)
	}

	/// `for ids in expression do ... end`
	pub fn for_statement(
		&mut self,
		ids: List<element::AnyIdentifier>,
		expression: NodeId,
		statements: List<element::Statement>,
	) -> NodeId {
		let node = self.reserve();
		let ids = self.adopt_list(node, ids);
		let expression = self.adopt(node, expression);
		let statements = self.adopt_list(node, statements);
		self.fill(
			node,
			ForStatement {
				ids,
				expression,
				statements,
			},
		)
	}

	/// `[local] function name(params) ... end`
	pub fn function_declaration(
		&mut self,
		localize: bool,
		name: NodeId,
		params: List<element::AnyIdentifier>,
		has_dot_dot_dot: bool,
		statements: List<element::Statement>,
	) -> NodeId {
		let node = self.reserve();
		let name = self.adopt(node, name);
		let parameters = self.adopt_list(node, params);
		let statements = self.adopt_list(node, statements);
		self.fill(
			node,
			FunctionDeclaration {
				return_type: None,
				localize,
				name,
				generics: Vec::new(),
				var_args_type: None,
				parameters,
				has_dot_dot_dot,
				statements,
			},
		)
	}

	/// `function expression:name(params) ... end`
	pub fn method_declaration(
		&mut self,
		expression: NodeId,
		name: impl Into<String>,
		params: List<element::AnyIdentifier>,
		has_dot_dot_dot: bool,
		statements: List<element::Statement>,
	) -> NodeId {
		let node = self.reserve();
		let expression = self.adopt(node, expression);
		let parameters = self.adopt_list(node, params);
		let statements = self.adopt_list(node, statements);
		self.fill(
			node,
			MethodDeclaration {
				return_type: None,
				expression,
				name: name.into(),
				generics: Vec::new(),
				var_args_type: None,
				parameters,
				has_dot_dot_dot,
				statements,
			},
		)
	}

	/// `local left = right`, or `local left` when `right` is `None`.
	pub fn variable_declaration(&mut self, left: impl Into<NodeOrList>, right: Option<NodeOrList>) -> NodeId {
		let node = self.reserve();
		let left = self.adopt_node_or_list(node, left.into());
		let right = right.map(|right| self.adopt_node_or_list(node, right));
		self.fill(node, VariableDeclaration { left, right })
	}

	/// `return expression`
	pub fn return_statement(&mut self, expression: impl Into<NodeOrList>) -> NodeId {
		let node = self.reserve();
		let expression = self.adopt_node_or_list(node, expression.into());
		self.fill(node, ReturnStatement { expression })
	}
}

#[cfg(test)]
mod tests;
