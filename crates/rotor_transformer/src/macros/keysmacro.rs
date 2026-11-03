//! `keysmacro.go`: rotor's `$keys` compile-time macro (no rbxtsc counterpart).
//!
//! ```ts
//! $keys<{ x: number; y: string }>() // { "x", "y" }
//! ```
//!
//! The type argument's apparent type's string-keyed properties, in declaration order, become an
//! array of string literals. Same interception point as `$env`.

use super::is_macro_node;
use crate::call::{ChainItem, ChainItemKind};
use crate::diagnostics;
use crate::optionalchain::transform_optional_chain_inner;
use crate::prelude::*;
use crate::util::nodes;

/// `keysMacroSymbol` (`keysmacro.go`): the global `$keys` symbol.
pub fn keys_macro_symbol(state: &mut State<'_>) -> Option<P<Symbol>> {
	state.ambient_symbol("$keys")
}

/// `keysOfType`: the string property keys of the type node's apparent type, in declaration
/// order.
fn keys_of_type(state: &mut State<'_>, type_node: P<Node>) -> Vec<&'static str> {
	let mut checker = state.ck();
	let t = checker.get_type_from_type_node(type_node);
	let apparent = checker.get_apparent_type(t);
	checker
		.get_properties_of_type(apparent)
		.iter()
		.map(|property| property.name())
		.filter(|name| is_string_key_name(name))
		.collect()
}

/// `isStringKeyName`: excludes the checker's `__@` computed and symbol member names and
/// all-digit (array index) names.
fn is_string_key_name(name: &str) -> bool {
	if name.is_empty() {
		return false;
	}
	if name.starts_with("__@") {
		// Computed/ES-symbol member sentinel (InternalSymbolName.* family).
		return false;
	}
	// Pure-digit names are numeric indices, not string object keys.
	!name.bytes().all(|byte| byte.is_ascii_digit())
}

/// `interceptKeysChain`: `None` when the chain's base is not `$keys`. Otherwise the leading call
/// folds to the array of the type argument's string keys and the rest of the chain continues
/// from it.
pub fn intercept_keys_chain(state: &mut State<'_>, chain: &[ChainItem], expression: P<Node>) -> Option<NodeId> {
	let item = chain.first()?;
	if !is_macro_node(state, expression, "$keys") {
		return None;
	}

	if item.kind != ChainItemKind::Call {
		// a bare property or element access off $keys has no such surface
		state.diags.add(diagnostics::rotor_keys_bad_usage(item.node));
		return Some(state.ast.none());
	}

	// The type argument lives on the CallExpression node, not in item.args.
	let &[type_argument] = nodes(item.node.as_call_expression().type_arguments()) else {
		state.diags.add(diagnostics::rotor_keys_no_type(item.node));
		return Some(state.ast.none());
	};

	let keys = keys_of_type(state, type_argument);
	let members = state.ast.new_list();
	for key in keys {
		let key = state.ast.str(key);
		state.ast.list_push(members, key);
	}
	let value = state.ast.array(members);

	// Continue any remaining chain links off the inlined array, e.g. `$keys<T>().size()`.
	Some(transform_optional_chain_inner(state, chain, value, None, 1))
}
