//! The tsrs AST, re-exported, plus the typescript-go helpers rotor calls that tsrs does not export.

use tsrs_core::P;

pub use tsrs_ast::*;

/// Whether `kind` is a wrapper `skipUpwards` climbs through: a non-null, parenthesized, `as`,
/// type assertion or `satisfies` expression.
#[must_use]
pub const fn is_upwards_skip_kind(kind: Kind) -> bool {
	matches!(
		kind,
		Kind::NonNullExpression
			| Kind::ParenthesizedExpression
			| Kind::AsExpression
			| Kind::TypeAssertionExpression
			| Kind::SatisfiesExpression
	)
}

/// The outermost wrapper of `node` that [`is_upwards_skip_kind`] matches, or `node` itself
/// (rotor's `transformer.SkipUpwards`), so `(x as Foo)!` is queried as a whole.
#[must_use]
pub fn skip_upwards(mut node: P<Node>) -> P<Node> {
	while let Some(parent) = node.parent()
		&& is_upwards_skip_kind(parent.kind())
	{
		node = parent;
	}
	node
}

/// The innermost expression under the wrappers [`is_upwards_skip_kind`] matches (rotor's
/// `transformer.SkipDownwards`), so `(x as Foo)!` resolves to `x`.
#[must_use]
pub fn skip_downwards(mut node: P<Node>) -> P<Node> {
	while is_upwards_skip_kind(node.kind())
		&& let Some(expression) = node.expression()
	{
		node = expression;
	}
	node
}

/// Go `ast.IsAsExpression`.
#[must_use]
pub fn is_as_expression(node: P<Node>) -> bool {
	node.kind() == Kind::AsExpression
}

/// Go `ast.IsBreakStatement`.
#[must_use]
pub fn is_break_statement(node: P<Node>) -> bool {
	node.kind() == Kind::BreakStatement
}

/// Go `ast.IsDeleteExpression`.
#[must_use]
pub fn is_delete_expression(node: P<Node>) -> bool {
	node.kind() == Kind::DeleteExpression
}

/// Go `ast.IsSemicolonClassElement`.
#[must_use]
pub fn is_semicolon_class_element(node: P<Node>) -> bool {
	node.kind() == Kind::SemicolonClassElement
}

/// Go `ast.IsJSDocLink`.
#[must_use]
pub fn is_jsdoc_link(node: P<Node>) -> bool {
	node.kind() == Kind::JSDocLink
}

/// Go `ast.IsJSDocLinkCode`.
#[must_use]
pub fn is_jsdoc_link_code(node: P<Node>) -> bool {
	node.kind() == Kind::JSDocLinkCode
}

/// Go `ast.IsJSDocLinkPlain`.
#[must_use]
pub fn is_jsdoc_link_plain(node: P<Node>) -> bool {
	node.kind() == Kind::JSDocLinkPlain
}

/// Go `ast.IsJSDocLinkLike`.
#[must_use]
pub fn is_jsdoc_link_like(node: P<Node>) -> bool {
	matches!(
		node.kind(),
		Kind::JSDocLink | Kind::JSDocLinkCode | Kind::JSDocLinkPlain
	)
}

/// Go `ast.IsSuperCall`: a call whose callee is `super`.
#[must_use]
pub fn is_super_call(node: P<Node>) -> bool {
	is_call_expression(node)
		&& node
			.expression()
			.is_some_and(|callee| callee.kind() == Kind::SuperKeyword)
}

/// Go `Kind.String()`: the kind's name with typescript-go's `Kind` prefix.
#[must_use]
pub fn kind_string(kind: Kind) -> String {
	format!("Kind{kind:?}")
}
