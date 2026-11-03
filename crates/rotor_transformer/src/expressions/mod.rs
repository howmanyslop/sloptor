//! Stage 4b slice (i), expressions and operators: destructuring (`binding*.go`), logical
//! assignment (`logicalassignment.go`) and spreads (`spread.go`, `spreaddestruct.go`). See
//! `SLICES.md`.
//!
//! The entry points other code calls are re-exported here with the signatures the stubs had:
//! [`transform_binding_name`], the binding and assignment pattern transforms, the logical
//! assignment transforms and the spread builders.

mod binding;
mod bindingarray;
mod bindingiterator;
mod bindingobject;
mod logicalassignment;
mod spread;
mod spreaddestruct;

pub use bindingarray::{
	transform_array_assignment_pattern, transform_array_binding_pattern, transform_optimized_array_assignment_pattern,
	transform_optimized_array_binding_pattern,
};
pub use bindingobject::{transform_object_assignment_pattern, transform_object_binding_pattern};
pub use logicalassignment::{
	transform_logical_or_coalescing_assignment_expression,
	transform_logical_or_coalescing_assignment_expression_statement,
};
pub use spread::{get_add_iterable_to_array_builder, transform_spread_assignment, transform_spread_element};

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

/// `transformBindingName` (`binding.go`): an identifier, or a temp destructured into
/// `initializers`.
pub fn transform_binding_name(state: &mut State<'_>, name: P<Node>, initializers: Statements) -> NodeId {
	binding::transform_binding_name(state, name, initializers)
}

/// `luau.Num(float64(value + offset))` for a pattern index or element count.
fn num_from_index(ast: &mut Ast, value: usize, offset: usize) -> NodeId {
	let value = u32::try_from(value.saturating_add(offset)).unwrap_or(u32::MAX);
	ast.num(f64::from(value))
}

#[cfg(test)]
mod tests;
