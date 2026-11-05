//! The names nearly every transform file uses.

pub use rotor_luau::{
	AssignmentOperator, Ast, BinaryOperator, List, NodeId, NodeOrList, SyntaxKind as LuauKind, UnaryOperator, element,
};
pub use rotor_tsrs::P;
pub use rotor_tsrs::ast::{Kind, Node, SourceFile, Symbol};
pub use rotor_tsrs::checker::Type;

pub use crate::diagnostics::Diagnostic;
pub use crate::state::State;

/// `*luau.List[luau.Statement]`
pub type Statements = List<element::Statement>;
