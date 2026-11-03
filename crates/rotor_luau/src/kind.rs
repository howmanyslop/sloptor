//! Syntax kinds and operators.

use core::fmt;

macro_rules! syntax_kinds {
	($($(#[$meta:meta])* $kind:ident),* $(,)?) => {
		/// The kind of a Luau AST node. Category ranges mirror upstream
		/// `enums.ts`, with rotor's extensions appended after each upstream
		/// category's last kind.
		#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
		#[repr(u8)]
		pub enum SyntaxKind {
			$($(#[$meta])* $kind,)*
		}

		impl SyntaxKind {
			/// Every kind, in declaration order.
			pub const ALL: &'static [Self] = &[$(Self::$kind,)*];

			/// The kind's name, as Go's `SyntaxKind.String()` prints it.
			#[must_use]
			pub const fn name(self) -> &'static str {
				match self {
					$(Self::$kind => stringify!($kind),)*
				}
			}
		}
	};
}

syntax_kinds! {
	/// `name`
	Identifier,
	/// A temporary whose name is solved at render time.
	TemporaryIdentifier,
	/// `expression[index]`
	ComputedIndexExpression,
	/// `expression.name`
	PropertyAccessExpression,
	/// `expression(args)`
	CallExpression,
	/// `expression:name(args)`
	MethodCallExpression,
	/// `(expression)`
	ParenthesizedExpression,
	/// A placeholder that must never be rendered.
	None,
	/// `nil`
	NilLiteral,
	/// `false`
	FalseLiteral,
	/// `true`
	TrueLiteral,
	/// A number literal.
	NumberLiteral,
	/// A string literal.
	StringLiteral,
	/// `...`
	VarArgsLiteral,
	/// `function(params) ... end`
	FunctionExpression,
	/// `left op right`
	BinaryExpression,
	/// `op expression`
	UnaryExpression,
	/// `if c then a else b`
	IfExpression,
	/// `` `text {expression}` ``
	InterpolatedString,
	/// `{ a, b }`
	Array,
	/// `{ [k] = v }`
	Map,
	/// `{ [k] = true }`
	Set,
	/// A table with both positional members and fields.
	MixedTable,
	/// `expression :: Type`, a rotor extension (roblox-ts never emits casts).
	CastExpression,
	/// `left op right`
	Assignment,
	/// `break`
	BreakStatement,
	/// A call used as a statement.
	CallStatement,
	/// `continue`
	ContinueStatement,
	/// `do ... end`
	DoStatement,
	/// `while c do ... end`
	WhileStatement,
	/// `repeat ... until c`
	RepeatStatement,
	/// `if c then ... end`
	IfStatement,
	/// `for i = a, b, c do ... end`
	NumericForStatement,
	/// `for ids in expression do ... end`
	ForStatement,
	/// `[local] function name(params) ... end`
	FunctionDeclaration,
	/// `function expression:name(params) ... end`
	MethodDeclaration,
	/// `local left = right`
	VariableDeclaration,
	/// `return expression`
	ReturnStatement,
	/// `--text`
	Comment,
	/// `type Name<K, V> = body`, a rotor extension.
	TypeAlias,
	/// `type function Name(params) ... end`, a rotor extension.
	TypeFunctionDeclaration,
	/// `[index] = value`
	MapField,
	/// The literal text of an interpolated string.
	InterpolatedStringPart,
	/// `Name`, `Module.Name`, or `Name<A, B>`.
	TypeReference,
	/// `"value"`
	TypeStringSingleton,
	/// `true` or `false`
	TypeBooleanSingleton,
	/// `T?`
	TypeOptional,
	/// `A | B`
	TypeUnion,
	/// `A & B`
	TypeIntersection,
	/// `{ a: T, [K]: V }`
	TypeTable,
	/// `{ T }`
	TypeArray,
	/// `<T>(a: A) -> R`
	TypeFunction,
	/// `typeof(expression)`
	TypeTypeof,
	/// `(A, B)`
	TypePack,
	/// `...T`
	TypeVariadic,
	/// `T...`
	TypeGenericPack,
	/// `[read|write] name: T`
	TypeTableProp,
	/// `[read|write] [K]: V`
	TypeTableIndexer,
}

impl SyntaxKind {
	/// The first indexable expression kind.
	pub const FIRST_INDEXABLE_EXPRESSION: Self = Self::Identifier;
	/// The last indexable expression kind.
	pub const LAST_INDEXABLE_EXPRESSION: Self = Self::ParenthesizedExpression;
	/// The first expression kind.
	pub const FIRST_EXPRESSION: Self = Self::Identifier;
	/// The last expression kind.
	pub const LAST_EXPRESSION: Self = Self::CastExpression;
	/// The first statement kind.
	pub const FIRST_STATEMENT: Self = Self::Assignment;
	/// The last statement kind.
	pub const LAST_STATEMENT: Self = Self::TypeFunctionDeclaration;
	/// The first field kind.
	pub const FIRST_FIELD: Self = Self::MapField;
	/// The last field kind.
	pub const LAST_FIELD: Self = Self::InterpolatedStringPart;
	/// The first type kind.
	pub const FIRST_TYPE: Self = Self::TypeReference;
	/// The last type kind.
	pub const LAST_TYPE: Self = Self::TypeGenericPack;
	/// The first type table field kind.
	pub const FIRST_TYPE_FIELD: Self = Self::TypeTableProp;
	/// The last type table field kind.
	pub const LAST_TYPE_FIELD: Self = Self::TypeTableIndexer;
}

impl fmt::Display for SyntaxKind {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(self.name())
	}
}

macro_rules! operators {
	($(#[$meta:meta])* $name:ident { $($(#[$variant_meta:meta])* $variant:ident => $text:literal),* $(,)? }) => {
		$(#[$meta])*
		#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
		pub enum $name {
			$($(#[$variant_meta])* $variant,)*
		}

		impl $name {
			/// The operator's Luau spelling.
			#[must_use]
			pub const fn as_str(self) -> &'static str {
				match self {
					$(Self::$variant => $text,)*
				}
			}

			/// Parses the operator's Luau spelling.
			#[must_use]
			pub fn parse(text: &str) -> Option<Self> {
				match text {
					$($text => Some(Self::$variant),)*
					_ => None,
				}
			}
		}

		impl fmt::Display for $name {
			fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
				formatter.write_str(self.as_str())
			}
		}
	};
}

operators! {
	/// A binary operator.
	BinaryOperator {
		/// `+`
		Add => "+",
		/// `-`
		Subtract => "-",
		/// `*`
		Multiply => "*",
		/// `/`
		Divide => "/",
		/// `//`
		FloorDivide => "//",
		/// `^`
		Power => "^",
		/// `%`
		Modulo => "%",
		/// `..`
		Concat => "..",
		/// `<`
		Less => "<",
		/// `<=`
		LessEqual => "<=",
		/// `>`
		Greater => ">",
		/// `>=`
		GreaterEqual => ">=",
		/// `==`
		Equal => "==",
		/// `~=`
		NotEqual => "~=",
		/// `and`
		And => "and",
		/// `or`
		Or => "or",
	}
}

operators! {
	/// A unary operator.
	UnaryOperator {
		/// `-`
		Negate => "-",
		/// `not`
		Not => "not",
		/// `#`
		Length => "#",
	}
}

operators! {
	/// An assignment operator.
	AssignmentOperator {
		/// `=`
		Assign => "=",
		/// `+=`
		Add => "+=",
		/// `-=`
		Subtract => "-=",
		/// `*=`
		Multiply => "*=",
		/// `/=`
		Divide => "/=",
		/// `//=`
		FloorDivide => "//=",
		/// `%=`
		Modulo => "%=",
		/// `^=`
		Power => "^=",
		/// `..=`
		Concat => "..=",
	}
}

#[cfg(test)]
mod tests;
