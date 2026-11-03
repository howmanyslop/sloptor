//! `nameofmacro.go`: rotor's `$nameof` compile-time macro (no rbxtsc counterpart).
//!
//! ```ts
//! $nameof(player.Humanoid.Health) // "Health"
//! $nameof(foo)                    // "foo"
//! ```
//!
//! The argument's raw AST is inspected, never transformed. Same interception point as `$env`.

use rotor_tsrs::ast as tsast;

use super::is_macro_node;
use crate::call::{ChainItem, ChainItemKind};
use crate::diagnostics;
use crate::optionalchain::transform_optional_chain_inner;
use crate::prelude::*;

/// `nameofMacroSymbol` (`nameofmacro.go`): the global `$nameof` symbol.
pub fn nameof_macro_symbol(state: &mut State<'_>) -> Option<P<Symbol>> {
	state.ambient_symbol("$nameof")
}

/// `nameofTrailingName`: the trailing identifier or property name of the argument, wrappers
/// skipped; `None` for forms without a statically known name.
fn nameof_trailing_name(node: P<Node>) -> Option<&'static str> {
	let node = tsast::skip_downwards(node);
	match node.kind() {
		Kind::Identifier => Some(node.text()),
		Kind::PropertyAccessExpression => Some(node.as_property_access_expression().name.text()),
		_ => None,
	}
}

/// `interceptNameofChain`: `None` when the chain's base is not `$nameof`. Otherwise the leading
/// call folds to the trailing name string and the rest of the chain continues from it.
pub fn intercept_nameof_chain(state: &mut State<'_>, chain: &[ChainItem], expression: P<Node>) -> Option<NodeId> {
	let item = chain.first()?;
	if !is_macro_node(state, expression, "$nameof") {
		return None;
	}

	// a bare property or element access off $nameof has no such surface; any other argument
	// count is unreachable after typecheck
	let (ChainItemKind::Call, &[argument]) = (item.kind, item.args) else {
		state.diags.add(diagnostics::rotor_nameof_bad_usage(item.node));
		return Some(state.ast.none());
	};

	let Some(name) = nameof_trailing_name(argument) else {
		state.diags.add(diagnostics::rotor_nameof_invalid(argument));
		return Some(state.ast.none());
	};
	let value = state.ast.str(name);

	// Continue any remaining chain links off the inlined literal, e.g. `$nameof(a.b).upper()`.
	Some(transform_optional_chain_inner(state, chain, value, None, 1))
}
