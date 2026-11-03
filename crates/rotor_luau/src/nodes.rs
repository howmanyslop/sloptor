//! Node payloads. Each kind with fields has a payload struct; [`NodeData`] is
//! the tagged union stored in the arena.

use crate::list::element;
use crate::types::{
	CastExpression, TypeAlias, TypeArray, TypeBooleanSingleton, TypeFunction, TypeFunctionDeclaration, TypeGeneric,
	TypeGenericPack, TypeIntersection, TypeOptional, TypePack, TypeReference, TypeStringSingleton, TypeTable,
	TypeTableIndexer, TypeTableProp, TypeTypeof, TypeUnion, TypeVariadic,
};
use crate::{AssignmentOperator, BinaryOperator, List, NodeId, NodeOrList, SyntaxKind, TempId, UnaryOperator};

/// `name`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identifier {
	/// The name.
	pub name: String,
	/// The binding annotation, rendered in parameter lists and on `local`
	/// left sides.
	pub annotation: Option<NodeId>,
}

/// A temporary whose name is solved at render time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemporaryIdentifier {
	/// The name hint.
	pub name: String,
	/// The rendering identity, shared by clones.
	pub id: TempId,
	/// Whether the hint is used as is, without the `_` prefix.
	pub exact_name: bool,
	/// The binding annotation.
	pub annotation: Option<NodeId>,
}

/// `expression[index]`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputedIndexExpression {
	/// The indexed expression.
	pub expression: NodeId,
	/// The index.
	pub index: NodeId,
}

/// `expression.name`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyAccessExpression {
	/// The indexed expression.
	pub expression: NodeId,
	/// The property name.
	pub name: String,
}

/// `expression(args)`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallExpression {
	/// The callee.
	pub expression: NodeId,
	/// The arguments.
	pub args: List<element::Expression>,
}

/// `expression:name(args)`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodCallExpression {
	/// The method name.
	pub name: String,
	/// The receiver.
	pub expression: NodeId,
	/// The arguments.
	pub args: List<element::Expression>,
}

/// `(expression)`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParenthesizedExpression {
	/// The wrapped expression.
	pub expression: NodeId,
}

/// A number literal, kept as source text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumberLiteral {
	/// The literal text.
	pub value: String,
}

/// A string literal.
///
/// Go's string literals are byte strings, and a `$file` text file that is not valid UTF-8 keeps
/// its bytes in the output. `raw` carries those bytes; `value` is then their lossy UTF-8 view for
/// the checks that read the text (an identifier-shaped key, for instance, is ASCII and so never
/// raw). The renderer writes [`StringLiteral::bytes`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringLiteral {
	/// The unescaped value.
	pub value: String,
	/// The exact bytes, set only when they are not valid UTF-8.
	pub raw: Option<Box<[u8]>>,
}

impl StringLiteral {
	/// The bytes the literal renders.
	#[must_use]
	pub fn bytes(&self) -> &[u8] {
		self.raw.as_deref().unwrap_or(self.value.as_bytes())
	}
}

/// `function<T>(params): R ... end`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionExpression {
	/// `: R` after the parameter list; `None` leaves the return unannotated.
	pub return_type: Option<NodeId>,
	/// `<K, V>` after `function`; defaults are not allowed.
	pub generics: Vec<TypeGeneric>,
	/// `...: T` on the rest parameter; `None` leaves it unannotated.
	pub var_args_type: Option<NodeId>,
	/// The parameters.
	pub parameters: List<element::AnyIdentifier>,
	/// Whether the function takes `...`.
	pub has_dot_dot_dot: bool,
	/// The body.
	pub statements: List<element::Statement>,
}

/// `left op right`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BinaryExpression {
	/// The left operand.
	pub left: NodeId,
	/// The operator.
	pub operator: BinaryOperator,
	/// The right operand.
	pub right: NodeId,
}

/// `op expression`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnaryExpression {
	/// The operator.
	pub operator: UnaryOperator,
	/// The operand.
	pub expression: NodeId,
}

/// `if condition then expression else alternative`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IfExpression {
	/// The condition.
	pub condition: NodeId,
	/// The value when the condition holds.
	pub expression: NodeId,
	/// The value otherwise.
	pub alternative: NodeId,
}

/// `` `text {expression}` ``
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterpolatedString {
	/// Literal parts and interpolated expressions.
	pub parts: List<element::Node>,
}

/// `{ a, b }`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Array {
	/// The members.
	pub members: List<element::Expression>,
}

/// `{ [k] = v }`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Map {
	/// The fields.
	pub fields: List<element::MapField>,
}

/// `{ [k] = true }`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Set {
	/// The members.
	pub members: List<element::Expression>,
}

/// A table of positional members and fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MixedTable {
	/// Map fields and expressions.
	pub fields: List<element::Node>,
}

/// `left op right`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assignment {
	/// A writable expression or a list of them.
	pub left: NodeOrList,
	/// The operator.
	pub operator: AssignmentOperator,
	/// An expression or a list of them.
	pub right: NodeOrList,
}

/// A call used as a statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallStatement {
	/// A call or method call.
	pub expression: NodeId,
}

/// `do ... end`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoStatement {
	/// The body.
	pub statements: List<element::Statement>,
}

/// `while condition do ... end`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhileStatement {
	/// The condition.
	pub condition: NodeId,
	/// The body.
	pub statements: List<element::Statement>,
}

/// `repeat ... until condition`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepeatStatement {
	/// The condition.
	pub condition: NodeId,
	/// The body.
	pub statements: List<element::Statement>,
}

/// `if condition then ... else ... end`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IfStatement {
	/// The condition.
	pub condition: NodeId,
	/// The body.
	pub statements: List<element::Statement>,
	/// An `elseif` link or the else statements (empty for no else).
	pub else_body: NodeOrList,
}

/// `for id = start, end, step do ... end`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumericForStatement {
	/// The loop variable.
	pub id: NodeId,
	/// The start value.
	pub start: NodeId,
	/// The end value.
	pub end: NodeId,
	/// The step, when given.
	pub step: Option<NodeId>,
	/// The body.
	pub statements: List<element::Statement>,
}

/// `for ids in expression do ... end`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForStatement {
	/// The loop variables.
	pub ids: List<element::AnyIdentifier>,
	/// The iterator expression.
	pub expression: NodeId,
	/// The body.
	pub statements: List<element::Statement>,
}

/// `[local] function name<T>(params): R ... end`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionDeclaration {
	/// `: R` after the parameter list.
	pub return_type: Option<NodeId>,
	/// Whether this is `local function`.
	pub localize: bool,
	/// An identifier or a property access.
	pub name: NodeId,
	/// `<K, V>` after the name.
	pub generics: Vec<TypeGeneric>,
	/// `...: T` on the rest parameter.
	pub var_args_type: Option<NodeId>,
	/// The parameters.
	pub parameters: List<element::AnyIdentifier>,
	/// Whether the function takes `...`.
	pub has_dot_dot_dot: bool,
	/// The body.
	pub statements: List<element::Statement>,
}

/// `function expression:name<T>(params): R ... end`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodDeclaration {
	/// `: R` after the parameter list.
	pub return_type: Option<NodeId>,
	/// The receiver.
	pub expression: NodeId,
	/// The method name.
	pub name: String,
	/// `<K, V>` after `:name`.
	pub generics: Vec<TypeGeneric>,
	/// `...: T` on the rest parameter.
	pub var_args_type: Option<NodeId>,
	/// The parameters.
	pub parameters: List<element::AnyIdentifier>,
	/// Whether the method takes `...`.
	pub has_dot_dot_dot: bool,
	/// The body.
	pub statements: List<element::Statement>,
}

/// `local left = right`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariableDeclaration {
	/// An identifier or a list of them.
	pub left: NodeOrList,
	/// An expression, a list of them, or nothing.
	pub right: Option<NodeOrList>,
}

/// `return expression`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReturnStatement {
	/// An expression or a list of them.
	pub expression: NodeOrList,
}

/// `--text`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comment {
	/// The comment text.
	pub text: String,
}

/// `[index] = value`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapField {
	/// The key.
	pub index: NodeId,
	/// The value.
	pub value: NodeId,
}

/// The literal text of an interpolated string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterpolatedStringPart {
	/// The text, unescaped.
	pub text: String,
}

macro_rules! node_data {
	(
		units { $($(#[$unit_meta:meta])* $unit:ident),* $(,)? }
		payloads { $($payload:ident),* $(,)? }
	) => {
		/// A node's kind and fields. Variants are named after their
		/// [`SyntaxKind`].
		#[derive(Clone, Debug, PartialEq, Eq)]
		pub enum NodeData {
			$($(#[$unit_meta])* $unit,)*
			$(
				#[doc = concat!("A [`", stringify!($payload), "`].")]
				$payload($payload),
			)*
		}

		impl NodeData {
			/// The node's kind.
			#[must_use]
			pub const fn kind(&self) -> SyntaxKind {
				match self {
					$(Self::$unit => SyntaxKind::$unit,)*
					$(Self::$payload(_) => SyntaxKind::$payload,)*
				}
			}
		}

		$(
			impl Payload for $payload {
				fn from_data(data: &NodeData) -> Option<&Self> {
					match data {
						NodeData::$payload(payload) => Some(payload),
						_ => None,
					}
				}

				fn from_data_mut(data: &mut NodeData) -> Option<&mut Self> {
					match data {
						NodeData::$payload(payload) => Some(payload),
						_ => None,
					}
				}
			}

			impl From<$payload> for NodeData {
				fn from(payload: $payload) -> Self {
					Self::$payload(payload)
				}
			}
		)*
	};
}

/// A payload struct stored in a [`NodeData`] variant.
pub trait Payload: Sized {
	/// The payload, when `data` holds this kind.
	fn from_data(data: &NodeData) -> Option<&Self>;
	/// The payload, mutably, when `data` holds this kind.
	fn from_data_mut(data: &mut NodeData) -> Option<&mut Self>;
}

node_data! {
	units {
		/// `None`, which must never be rendered.
		None,
		/// `nil`
		NilLiteral,
		/// `false`
		FalseLiteral,
		/// `true`
		TrueLiteral,
		/// `...`
		VarArgsLiteral,
		/// `break`
		BreakStatement,
		/// `continue`
		ContinueStatement,
	}
	payloads {
		Identifier,
		TemporaryIdentifier,
		ComputedIndexExpression,
		PropertyAccessExpression,
		CallExpression,
		MethodCallExpression,
		ParenthesizedExpression,
		NumberLiteral,
		StringLiteral,
		FunctionExpression,
		BinaryExpression,
		UnaryExpression,
		IfExpression,
		InterpolatedString,
		Array,
		Map,
		Set,
		MixedTable,
		CastExpression,
		Assignment,
		CallStatement,
		DoStatement,
		WhileStatement,
		RepeatStatement,
		IfStatement,
		NumericForStatement,
		ForStatement,
		FunctionDeclaration,
		MethodDeclaration,
		VariableDeclaration,
		ReturnStatement,
		Comment,
		TypeAlias,
		TypeFunctionDeclaration,
		MapField,
		InterpolatedStringPart,
		TypeReference,
		TypeStringSingleton,
		TypeBooleanSingleton,
		TypeOptional,
		TypeUnion,
		TypeIntersection,
		TypeTable,
		TypeArray,
		TypeFunction,
		TypeTypeof,
		TypePack,
		TypeVariadic,
		TypeGenericPack,
		TypeTableProp,
		TypeTableIndexer,
	}
}

#[cfg(test)]
mod tests;
