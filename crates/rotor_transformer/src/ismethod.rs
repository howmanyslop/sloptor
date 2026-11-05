//! `ismethod.go`: `util/isMethod.ts`.

use rotor_tsrs::ast as tsast;
use rotor_tsrs::checker::{ContextFlags, TypeFlags};

use crate::diagnostics;
use crate::prelude::*;
use crate::receivertype::can_change_receiver_convention;
use crate::types::walk_types;

fn is_method_declaration_node(node: P<Node>) -> bool {
	if tsast::is_method_declaration(node) || tsast::is_method_signature_declaration(node) {
		return true;
	}
	if tsast::is_function_expression(node) {
		return tsast::skip_upwards(node)
			.parent()
			.is_some_and(tsast::is_property_assignment);
	}
	false
}

/// `isMethodInner` (`isMethod.ts` L51-77): an explicit `this` typed `void` marks a callback, any
/// other `this` type a method; signatures without one fall back to their declaration's shape.
/// Mixing both is `noMixedTypeCall`.
fn is_method_inner(state: &mut State<'_>, node: P<Node>, t: P<Type>) -> bool {
	let mut has_method_definition = false;
	let mut has_callback_definition = false;

	for &call_signature in state.ck().get_call_signatures(t) {
		if let Some(this_parameter) = call_signature.this_parameter() {
			let Some(this_type) = state.ck().get_type_of_symbol_at_location(this_parameter, Some(node)) else {
				// Go reads `thisType.Flags()` and nil-derefs here
				state.ice("transformer: isMethod: this parameter has no type");
				return false;
			};
			if can_change_receiver_convention(state, this_type) {
				let mut cache = std::mem::take(&mut state.multi.is_reported_by_no_unstable_this_type);
				state
					.diags
					.add_with_cache(node, diagnostics::no_unstable_this_type(node), &mut cache);
				state.multi.is_reported_by_no_unstable_this_type = cache;
			}
			if this_type.flags().intersects(TypeFlags::Void) {
				has_callback_definition = true;
			} else {
				has_method_definition = true;
			}
		} else if let Some(declaration) = call_signature.declaration() {
			if is_method_declaration_node(declaration) {
				has_method_definition = true;
			} else {
				has_callback_definition = true;
			}
		}
	}

	if has_method_definition && has_callback_definition {
		state.diags.add(diagnostics::no_mixed_type_call(node));
	}

	has_method_definition
}

/// `isMethodFromType` (`isMethod.ts` L79-91): each leaf with a symbol consults the pass-wide
/// cache. Upstream's `result ||= ...` short-circuits: once true, later leaves are neither checked
/// nor cached.
pub fn is_method_from_type(state: &mut State<'_>, node: P<Node>, t: P<Type>) -> bool {
	let mut result = false;
	walk_types(state, t, &mut |state, leaf| {
		if result {
			return;
		}
		if leaf.symbol().is_some() {
			let cached = if let Some(&cached) = state.multi.is_method_cache.get(&leaf) {
				cached
			} else {
				let computed = is_method_inner(state, node, leaf);
				state.multi.is_method_cache.insert(leaf, computed);
				computed
			};
			result = result || cached;
		}
	});
	result
}

/// `isMethod` (`isMethod.ts` L93-98).
pub fn is_method(state: &mut State<'_>, node: P<Node>) -> bool {
	let t = state.get_type(node);
	if is_method_from_type(state, node, t) {
		return true;
	}
	if tsast::is_function_expression(node) {
		let parameters = node.parameters();
		if let Some(&first) = parameters.first()
			&& first.name().is_some_and(tsast::is_this_identifier)
		{
			return false;
		}
		if let Some(contextual_type) = state.ck().get_contextual_type(node, ContextFlags::None)
			&& contextual_type != t
		{
			return is_method_from_type(state, node, contextual_type);
		}
	}
	false
}
