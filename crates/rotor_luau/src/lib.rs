//! The Luau AST and renderer.
//!
//! An [`Ast`] is one file's arena. Nodes are [`NodeId`]s, lists are
//! [`List`] handles over arena-allocated links, and origins live in a shared
//! table addressed by [`OriginId`]. Constructors on [`Ast`] adopt their
//! children, cloning any child that already has a parent, so the tree is a
//! DAG while it is built, exactly as in the Go port. [`render`] turns a
//! statement list into Luau text.

mod ast;
mod create;
mod globals;
mod guards;
mod ids;
mod jsnumber;
mod kind;
mod list;
mod nodes;
mod origin;
pub mod render;
mod types;
pub mod utf8;
mod validate;

pub use ast::Ast;
pub use guards::ParamData;
pub use ids::{ListId, ListNodeId, NodeId, OriginId, TempId};
pub use jsnumber::{JsNumberParseError, js_number_parse, js_number_string};
pub use kind::{AssignmentOperator, BinaryOperator, SyntaxKind, UnaryOperator};
pub use list::{AnyList, List, ListElement, ListIter, ListKind, ListLinkIter, NodeOrList, element};
pub use nodes::{
	Array, Assignment, BinaryExpression, CallExpression, CallStatement, Comment, ComputedIndexExpression, DoStatement,
	ForStatement, FunctionDeclaration, FunctionExpression, Identifier, IfExpression, IfStatement, InterpolatedString,
	InterpolatedStringPart, Map, MapField, MethodCallExpression, MethodDeclaration, MixedTable, NodeData,
	NumberLiteral, NumericForStatement, ParenthesizedExpression, Payload, PropertyAccessExpression, RepeatStatement,
	ReturnStatement, Set, StringLiteral, TemporaryIdentifier, UnaryExpression, VariableDeclaration, WhileStatement,
};
pub use origin::{Origin, Position};
pub use types::{
	CastExpression, TypeAccess, TypeAlias, TypeArray, TypeBooleanSingleton, TypeFunction, TypeFunctionDeclaration,
	TypeFunctionParameter, TypeGeneric, TypeGenericPack, TypeIntersection, TypeOptional, TypePack, TypeReference,
	TypeStringSingleton, TypeTable, TypeTableIndexer, TypeTableProp, TypeTypeof, TypeUnion, TypeVariadic,
};
pub use validate::{
	is_metamethod, is_reserved_class_field, is_reserved_identifier, is_reserved_keyword, is_valid_identifier,
	is_valid_number_literal,
};
