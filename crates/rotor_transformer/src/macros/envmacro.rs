//! `envmacro.go`: rotor's `$env` compile-time environment macro (no rbxtsc counterpart).
//!
//! ```ts
//! $env("GAME_NAME")             // "value" or nil
//! $env("GAME_NAME", "fallback") // "value" or "fallback"
//! $env.GAME_NAME                // "value" or nil
//! $env["GAME_NAME"]             // "value" or nil
//! ```
//!
//! Values resolve through the pass's `Env` (process environment, then `.env.<NODE_ENV>`, then
//! `.env`). Names and fallbacks must be string literals. The hook sits in
//! `transformOptionalChain`, before the base identifier is transformed, and keys on the
//! identifier's symbol (the global `$env`), so a local `$env` compiles as plain code.

use rotor_config::dotenv::Env;

use super::{is_macro_node, string_literal_text};
use crate::call::{ChainItem, ChainItemKind};
use crate::diagnostics;
use crate::optionalchain::transform_optional_chain_inner;
use crate::prelude::*;

/// `envMacroSymbol` (`envmacro.go`): the global `$env` symbol.
pub fn env_macro_symbol(state: &mut State<'_>) -> Option<P<Symbol>> {
	state.ambient_symbol("$env")
}

/// `envValueExpression`: the value as a string literal, or `nil` when unset.
fn env_value_expression(state: &mut State<'_>, name: &str) -> NodeId {
	match Env::lookup_in(state.env, name) {
		Some(value) => state.ast.str(value),
		None => state.ast.nil(),
	}
}

/// `interceptEnvChain`: `None` when the chain's base is not `$env`. Otherwise the first link
/// folds to its compile-time value and the rest of the chain continues from that literal.
/// Optionality markers (`$env?.X`) are accepted and ignored: `$env` is never nil.
pub fn intercept_env_chain(state: &mut State<'_>, chain: &[ChainItem], expression: P<Node>) -> Option<NodeId> {
	let item = chain.first()?;
	if !is_macro_node(state, expression, "$env") {
		return None;
	}

	let value = match item.kind {
		ChainItemKind::Call => {
			// $env("NAME") / $env("NAME", "fallback")
			let &[name_arg, ref rest @ ..] = item.args else {
				// unreachable after typecheck (the ambient signatures take 1-2 args)
				state.diags.add(diagnostics::rotor_env_bad_usage(item.node));
				return Some(state.ast.none());
			};
			if rest.len() > 1 {
				state.diags.add(diagnostics::rotor_env_bad_usage(item.node));
				return Some(state.ast.none());
			}
			let Some(name) = string_literal_text(name_arg) else {
				state.diags.add(diagnostics::rotor_env_non_literal_arg(name_arg));
				return Some(state.ast.none());
			};
			if let Some(env_value) = Env::lookup_in(state.env, name) {
				state.ast.str(env_value)
			} else if let Some(&fallback) = rest.first() {
				let Some(fallback_text) = string_literal_text(fallback) else {
					state.diags.add(diagnostics::rotor_env_non_literal_arg(fallback));
					return Some(state.ast.none());
				};
				state.ast.str(fallback_text)
			} else {
				state.ast.nil()
			}
		}
		// $env.NAME
		ChainItemKind::PropertyAccess => env_value_expression(state, item.name),
		// $env["NAME"]
		ChainItemKind::ElementAccess => {
			let Some(argument) = item.argument_expression else {
				state.ice("transformer: $env element access without an argument expression");
				return Some(state.ast.none());
			};
			let Some(name) = string_literal_text(argument) else {
				state.diags.add(diagnostics::rotor_env_non_literal_arg(argument));
				return Some(state.ast.none());
			};
			env_value_expression(state, name)
		}
		// $env.X(...) / $env["X"](...): the checker already rejects these
		ChainItemKind::PropertyCall | ChainItemKind::ElementCall => {
			state.diags.add(diagnostics::rotor_env_bad_usage(item.node));
			return Some(state.ast.none());
		}
	};

	// Continue any remaining chain links off the inlined literal, e.g. `$env("A")!.size()`.
	Some(transform_optional_chain_inner(state, chain, value, None, 1))
}
