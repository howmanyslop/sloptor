//! `ensureorder.go`: `util/ensureTransformOrder.ts`.

use crate::dispatch::transform_expression;
use crate::exports::is_symbol_mutable;
use crate::prelude::*;
use crate::util::is_identifier;

/// `ensureTransformOrder`: transforms sibling expressions so they run in source order. When a
/// later sibling produced prerequisite statements, every earlier expression those statements
/// could change is pinned into an `exp` temp. Primitives, temps and reads of constant
/// identifiers stay inline.
pub fn ensure_transform_order(state: &mut State<'_>, nodes: &[P<Node>]) -> Vec<NodeId> {
	ensure_transform_order_with(state, nodes, transform_expression)
}

/// `ensureTransformOrderWith`: [`ensure_transform_order`] with an explicit per-node transform.
pub fn ensure_transform_order_with(
	state: &mut State<'_>,
	nodes: &[P<Node>],
	mut transformer: impl FnMut(&mut State<'_>, P<Node>) -> NodeId,
) -> Vec<NodeId> {
	let mut result = Vec::with_capacity(nodes.len());
	let mut prereqs = Vec::with_capacity(nodes.len());
	for &node in nodes {
		let (expression, prereq_list) = state.capture(|state| transformer(state, node));
		result.push(expression);
		prereqs.push(prereq_list);
	}

	// findLastIndex(expressionInfoList, info => !isEmpty(info.prereqs))
	let last_index_with_prereqs = prereqs.iter().rposition(|&list| state.ast.list_is_non_empty(list));

	for (index, &list) in prereqs.iter().enumerate() {
		state.prereq_list(list);

		let expression = result[index];
		let node = nodes[index];
		let is_const_var = is_identifier(node)
			&& state
				.ck()
				.get_symbol_at_location(node)
				.is_some_and(|symbol| !is_symbol_mutable(state, symbol));

		let kind = state.ast.kind(expression);
		if last_index_with_prereqs.is_some_and(|last| index < last)
			&& !kind.is_simple_primitive()
			&& kind != LuauKind::TemporaryIdentifier
			&& !is_const_var
		{
			result[index] = state.push_to_var(Some(expression), "exp");
		}
	}
	result
}
