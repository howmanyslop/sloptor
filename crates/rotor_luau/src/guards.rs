//! Kind guards, the category tests behind the Go marker interfaces.

use crate::list::element;
use crate::nodes::{
	DoStatement, ForStatement, FunctionDeclaration, FunctionExpression, IfStatement, MethodDeclaration,
	NumericForStatement, RepeatStatement, WhileStatement,
};
use crate::types::TypeFunctionDeclaration;
use crate::{Ast, List, NodeData, NodeId, SyntaxKind};

const fn in_range(kind: SyntaxKind, first: SyntaxKind, last: SyntaxKind) -> bool {
	kind as u8 >= first as u8 && kind as u8 <= last as u8
}

impl SyntaxKind {
	/// `luau.IndexableExpression`
	#[must_use]
	pub const fn is_indexable_expression(self) -> bool {
		in_range(self, Self::FIRST_INDEXABLE_EXPRESSION, Self::LAST_INDEXABLE_EXPRESSION)
	}

	/// `luau.Expression`
	#[must_use]
	pub const fn is_expression(self) -> bool {
		in_range(self, Self::FIRST_EXPRESSION, Self::LAST_EXPRESSION)
	}

	/// `luau.Statement`
	#[must_use]
	pub const fn is_statement(self) -> bool {
		in_range(self, Self::FIRST_STATEMENT, Self::LAST_STATEMENT)
	}

	/// `luau.FieldNode`
	#[must_use]
	pub const fn is_field(self) -> bool {
		in_range(self, Self::FIRST_FIELD, Self::LAST_FIELD)
	}

	/// `luau.TypeNode`
	#[must_use]
	pub const fn is_type_node(self) -> bool {
		in_range(self, Self::FIRST_TYPE, Self::LAST_TYPE)
	}

	/// `luau.TypeTableField`
	#[must_use]
	pub const fn is_type_table_field(self) -> bool {
		in_range(self, Self::FIRST_TYPE_FIELD, Self::LAST_TYPE_FIELD)
	}

	/// `luau.AnyIdentifier`: an identifier or a temporary.
	#[must_use]
	pub const fn is_any_identifier(self) -> bool {
		matches!(self, Self::Identifier | Self::TemporaryIdentifier)
	}

	/// `luau.WritableExpression`: an identifier, a temporary, a property
	/// access, or a computed index.
	#[must_use]
	pub const fn is_writable_expression(self) -> bool {
		matches!(
			self,
			Self::Identifier
				| Self::TemporaryIdentifier
				| Self::PropertyAccessExpression
				| Self::ComputedIndexExpression
		)
	}

	/// A pack-shaped type: `(A, B)`, `...T`, or `T...`. Packs are legal only
	/// as the last type argument, the last pack element, a function type's
	/// varargs, and a return.
	#[must_use]
	pub const fn is_type_pack(self) -> bool {
		matches!(self, Self::TypePack | Self::TypeVariadic | Self::TypeGenericPack)
	}

	/// Identifiers and primitive literals.
	#[must_use]
	pub const fn is_simple(self) -> bool {
		matches!(
			self,
			Self::Identifier
				| Self::TemporaryIdentifier
				| Self::NilLiteral
				| Self::TrueLiteral
				| Self::FalseLiteral
				| Self::NumberLiteral
				| Self::StringLiteral
		)
	}

	/// Primitive literals.
	#[must_use]
	pub const fn is_simple_primitive(self) -> bool {
		matches!(
			self,
			Self::NilLiteral | Self::TrueLiteral | Self::FalseLiteral | Self::NumberLiteral | Self::StringLiteral
		)
	}

	/// Table constructors.
	#[must_use]
	pub const fn is_table(self) -> bool {
		matches!(self, Self::Array | Self::Set | Self::Map | Self::MixedTable)
	}

	/// `break`, `return`, and `continue`.
	#[must_use]
	pub const fn is_final_statement(self) -> bool {
		matches!(
			self,
			Self::BreakStatement | Self::ReturnStatement | Self::ContinueStatement
		)
	}

	/// Calls and method calls.
	#[must_use]
	pub const fn is_call(self) -> bool {
		matches!(self, Self::CallExpression | Self::MethodCallExpression)
	}

	/// The `None` placeholder.
	#[must_use]
	pub const fn is_none(self) -> bool {
		matches!(self, Self::None)
	}

	/// Kinds with parameters (`luau.HasParameters`).
	#[must_use]
	pub const fn is_function_like(self) -> bool {
		matches!(
			self,
			Self::FunctionDeclaration
				| Self::FunctionExpression
				| Self::MethodDeclaration
				| Self::TypeFunctionDeclaration
		)
	}

	/// Kinds with a statement list.
	#[must_use]
	pub const fn has_statements(self) -> bool {
		matches!(
			self,
			Self::ForStatement
				| Self::NumericForStatement
				| Self::FunctionExpression
				| Self::DoStatement
				| Self::FunctionDeclaration
				| Self::IfStatement
				| Self::MethodDeclaration
				| Self::RepeatStatement
				| Self::WhileStatement
				| Self::TypeFunctionDeclaration
		)
	}

	/// Kinds that take part in operator precedence.
	#[must_use]
	pub const fn is_expression_with_precedence(self) -> bool {
		matches!(
			self,
			Self::IfExpression | Self::UnaryExpression | Self::BinaryExpression
		)
	}
}

/// The parameter data of a function-like node (`luau.HasParameters`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParamData {
	/// The parameters.
	pub params: List<element::AnyIdentifier>,
	/// Whether the function takes `...`.
	pub has_dot_dot_dot: bool,
	/// The `...: T` annotation, when given.
	pub var_args_type: Option<NodeId>,
}

impl Ast {
	/// The statement list of any [`SyntaxKind::has_statements`] node.
	#[must_use]
	pub fn statements_of(&self, node: NodeId) -> Option<List<element::Statement>> {
		match self.data(node) {
			NodeData::ForStatement(ForStatement { statements, .. })
			| NodeData::NumericForStatement(NumericForStatement { statements, .. })
			| NodeData::FunctionExpression(FunctionExpression { statements, .. })
			| NodeData::DoStatement(DoStatement { statements })
			| NodeData::FunctionDeclaration(FunctionDeclaration { statements, .. })
			| NodeData::IfStatement(IfStatement { statements, .. })
			| NodeData::MethodDeclaration(MethodDeclaration { statements, .. })
			| NodeData::RepeatStatement(RepeatStatement { statements, .. })
			| NodeData::WhileStatement(WhileStatement { statements, .. })
			| NodeData::TypeFunctionDeclaration(TypeFunctionDeclaration { statements, .. }) => Some(*statements),
			_ => None,
		}
	}

	/// The parameters of a function-like node.
	#[must_use]
	pub fn param_data(&self, node: NodeId) -> Option<ParamData> {
		match self.data(node) {
			NodeData::FunctionExpression(function) => Some(ParamData {
				params: function.parameters,
				has_dot_dot_dot: function.has_dot_dot_dot,
				var_args_type: function.var_args_type,
			}),
			NodeData::FunctionDeclaration(function) => Some(ParamData {
				params: function.parameters,
				has_dot_dot_dot: function.has_dot_dot_dot,
				var_args_type: function.var_args_type,
			}),
			NodeData::MethodDeclaration(method) => Some(ParamData {
				params: method.parameters,
				has_dot_dot_dot: method.has_dot_dot_dot,
				var_args_type: method.var_args_type,
			}),
			NodeData::TypeFunctionDeclaration(function) => Some(ParamData {
				params: function.parameters,
				has_dot_dot_dot: function.has_dot_dot_dot,
				var_args_type: None,
			}),
			_ => None,
		}
	}

	/// The return annotation of a function-like node.
	#[must_use]
	pub fn return_type_data(&self, node: NodeId) -> Option<NodeId> {
		match self.data(node) {
			NodeData::FunctionExpression(function) => function.return_type,
			NodeData::FunctionDeclaration(function) => function.return_type,
			NodeData::MethodDeclaration(method) => method.return_type,
			_ => None,
		}
	}
}

#[cfg(test)]
mod tests;
