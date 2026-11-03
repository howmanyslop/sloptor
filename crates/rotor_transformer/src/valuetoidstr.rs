//! `valuetoidstr.go`: temp name hints derived from expressions (`util/valueToIdStr.ts`).

use rotor_luau::{CallExpression, Identifier, PropertyAccessExpression, is_valid_identifier};

use crate::prelude::*;

/// `expressionToStr`: `None` mirrors upstream's `undefined`.
fn expression_to_str(ast: &Ast, expression: NodeId) -> Option<String> {
	if let Some(identifier) = ast.get::<Identifier>(expression) {
		// X -> "X" (a TemporaryIdentifier is not luau.isIdentifier upstream)
		return Some(identifier.name.clone());
	}
	if let Some(access) = ast.get::<PropertyAccessExpression>(expression) {
		// A.B -> "B"
		return Some(access.name.clone());
	}
	if let Some(call) = ast.get::<CallExpression>(expression) {
		// X.new() -> "X"
		if let Some(access) = ast.get::<PropertyAccessExpression>(call.expression)
			&& access.name == "new"
		{
			return expression_to_str(ast, access.expression);
		}
	}
	None
}

/// `uncapitalizeFirstLetter`: the input passed `is_valid_identifier`, so its first character is
/// ASCII and byte-wise lowering matches JS.
fn uncapitalize_first_letter(text: &str) -> String {
	let mut characters = text.chars();
	characters.next().map_or_else(String::new, |first| {
		let mut result = first.to_ascii_lowercase().to_string();
		result.push_str(characters.as_str());
		result
	})
}

/// `ValueToIdStr`: a temp name hint from an expression, or `""` for an anonymous temp.
#[must_use]
pub fn value_to_id_str(ast: &Ast, value: NodeId) -> String {
	match expression_to_str(ast, value) {
		Some(text) if is_valid_identifier(&text) => uncapitalize_first_letter(&text),
		_ => String::new(),
	}
}
