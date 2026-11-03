//! Stage 4b slice (i), expressions and operators: destructuring (`binding*.go`), logical
//! assignment (`logicalassignment.go`) and spreads (`spread.go`, `spreaddestruct.go`).
//!
//! Every function here is a stub with the signature core code calls; the slice port replaces the
//! bodies and may add submodules below this file. See `SLICES.md`.

use crate::pointer::MapPointer;
use crate::prelude::*;

/// Per-file state owned by this slice.
#[derive(Debug, Default)]
pub struct SliceState {}

/// Pass-wide state owned by this slice.
#[derive(Debug, Default)]
pub struct MultiSliceState {}

/// `addIterableToArrayBuilder` (`spread.go`): appends a spread's values to `array_id`, given the
/// `_length` temp, the inline elements since its last update, and whether to update it after.
pub type AddIterableToArrayBuilder = fn(&mut State<'_>, NodeId, NodeId, NodeId, usize, bool) -> Statements;

fn not_yet_ported_expression(state: &mut State<'_>, node: P<Node>, what: &str) -> NodeId {
	state.not_yet_ported(node, what);
	state.ast.none()
}

/// `transformLogicalOrCoalescingAssignmentExpression` (`logicalassignment.go`).
pub fn transform_logical_or_coalescing_assignment_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	not_yet_ported_expression(state, node, "logical assignment")
}

/// `transformLogicalOrCoalescingAssignmentExpressionStatement` (`logicalassignment.go`).
pub fn transform_logical_or_coalescing_assignment_expression_statement(
	state: &mut State<'_>,
	node: P<Node>,
) -> Statements {
	state.not_yet_ported(node, "logical assignment");
	state.ast.new_list()
}

/// `transformOptimizedArrayAssignmentPattern` (`bindingarray.go`).
pub fn transform_optimized_array_assignment_pattern(state: &mut State<'_>, pattern: P<Node>, _rhs: NodeOrList) {
	state.not_yet_ported(pattern, "array destructuring assignment");
}

/// `transformArrayAssignmentPattern` (`bindingarray.go`).
pub fn transform_array_assignment_pattern(state: &mut State<'_>, pattern: P<Node>, _parent_id: NodeId) {
	state.not_yet_ported(pattern, "array destructuring assignment");
}

/// `transformObjectAssignmentPattern` (`bindingobject.go`).
pub fn transform_object_assignment_pattern(state: &mut State<'_>, pattern: P<Node>, _parent_id: NodeId) {
	state.not_yet_ported(pattern, "object destructuring assignment");
}

/// `transformOptimizedArrayBindingPattern` (`bindingarray.go`).
pub fn transform_optimized_array_binding_pattern(
	state: &mut State<'_>,
	pattern: P<Node>,
	_rhs: NodeOrList,
) -> Statements {
	state.not_yet_ported(pattern, "array destructuring");
	state.ast.new_list()
}

/// `transformArrayBindingPattern` (`bindingarray.go`).
pub fn transform_array_binding_pattern(state: &mut State<'_>, pattern: P<Node>, _parent_id: NodeId) {
	state.not_yet_ported(pattern, "array destructuring");
}

/// `transformObjectBindingPattern` (`bindingobject.go`).
pub fn transform_object_binding_pattern(state: &mut State<'_>, pattern: P<Node>, _parent_id: NodeId) {
	state.not_yet_ported(pattern, "object destructuring");
}

/// `transformBindingName` (`binding.go`): an identifier, or a temp destructured into
/// `initializers`.
pub fn transform_binding_name(state: &mut State<'_>, name: P<Node>, _initializers: Statements) -> NodeId {
	not_yet_ported_expression(state, name, "binding names")
}

fn empty_builder(state: &mut State<'_>, _: NodeId, _: NodeId, _: NodeId, _: usize, _: bool) -> Statements {
	state.ast.new_list()
}

/// `getAddIterableToArrayBuilder` (`spread.go`).
pub fn get_add_iterable_to_array_builder(
	state: &mut State<'_>,
	node: P<Node>,
	_t: P<Type>,
) -> AddIterableToArrayBuilder {
	state.not_yet_ported(node, "array spread");
	empty_builder
}

/// `transformSpreadAssignment` (`spread.go`): `{ ...x }`.
pub fn transform_spread_assignment(state: &mut State<'_>, _pointer: &mut MapPointer, property: P<Node>) {
	state.not_yet_ported(property, "object spread");
}

/// `transformSpreadElement` (`spread.go`).
pub fn transform_spread_element(state: &mut State<'_>, node: P<Node>) -> NodeId {
	not_yet_ported_expression(state, node, "spread element")
}
