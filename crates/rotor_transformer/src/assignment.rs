//! `assignment.go`: `util/assignment.ts`, `util/getAssignableValue.ts` and
//! `nodes/transformWritable.ts`.

use rotor_tsrs::ast as tsast;
use rotor_tsrs::types::TypeCheck;

use crate::access::add_one_if_array_type;
use crate::binary::{create_binary_from_operator, tostring_call};
use crate::call::convert_to_indexable_expression;
use crate::diagnostics;
use crate::dispatch::{tag_expression_origin, transform_expression};
use crate::ensureorder::ensure_transform_order;
use crate::prelude::*;
use crate::types::is_definitely_type;

/// `compoundOperatorMap` (`assignment.ts` L7-21). The bitwise compound operators are absent and
/// fall back to read-modify-write.
const fn compound_operator(kind: Kind) -> Option<AssignmentOperator> {
	Some(match kind {
		// compound assignment
		Kind::MinusEqualsToken | Kind::MinusMinusToken => AssignmentOperator::Subtract,
		Kind::AsteriskEqualsToken => AssignmentOperator::Multiply,
		Kind::SlashEqualsToken => AssignmentOperator::Divide,
		Kind::AsteriskAsteriskEqualsToken => AssignmentOperator::Power,
		Kind::PercentEqualsToken => AssignmentOperator::Modulo,
		// unary
		Kind::PlusPlusToken => AssignmentOperator::Add,
		// normal assignment
		Kind::EqualsToken => AssignmentOperator::Assign,
		_ => return None,
	})
}

/// `getSimpleAssignmentOperator` (L23-34): `+=` is `..=` when either side is definitely a
/// string. `None` means no simple operator exists (upstream `undefined`).
pub fn get_simple_assignment_operator(
	state: &mut State<'_>,
	left_type: P<Type>,
	operator_kind: Kind,
	right_type: P<Type>,
) -> Option<AssignmentOperator> {
	// plus
	if operator_kind == Kind::PlusEqualsToken {
		if is_definitely_type(state, left_type, &[TypeCheck::String])
			|| is_definitely_type(state, right_type, &[TypeCheck::String])
		{
			return Some(AssignmentOperator::Concat);
		}
		return Some(AssignmentOperator::Add);
	}
	compound_operator(operator_kind)
}

/// `getAssignableValue` (`util/getAssignableValue.ts`): a `..=` value that is not definitely a
/// string is wrapped in `tostring()`.
pub fn get_assignable_value(
	state: &mut State<'_>,
	operator: AssignmentOperator,
	value: NodeId,
	value_type: P<Type>,
) -> NodeId {
	if operator == AssignmentOperator::Concat && !is_definitely_type(state, value_type, &[TypeCheck::String]) {
		return tostring_call(&mut state.ast, value);
	}
	value
}

/// `createAssignmentExpression` (L36-50): prereqs the assignment and returns the writable.
pub fn create_assignment_expression(
	state: &mut State<'_>,
	readable: NodeId,
	operator: AssignmentOperator,
	value: NodeId,
	token: Option<P<Node>>,
) -> NodeId {
	let statement = state.ast.assignment(readable, operator, value);
	if operator != AssignmentOperator::Assign
		&& let Some(token) = token
	{
		state.set_operator_source_position(statement, token);
	}
	state.prereq(statement);
	readable
}

/// `createCompoundAssignmentStatement` (L52-67): `writable = <readable op value>`.
pub fn create_compound_assignment_statement(
	state: &mut State<'_>,
	writable: NodeId,
	writable_type: P<Type>,
	readable: NodeId,
	operator_kind: Kind,
	value: NodeId,
	value_type: P<Type>,
) -> NodeId {
	let binary = create_binary_from_operator(state, readable, writable_type, operator_kind, value, value_type);
	state.ast.assignment(writable, AssignmentOperator::Assign, binary)
}

/// `createCompoundAssignmentExpression` (L69-85).
pub fn create_compound_assignment_expression(
	state: &mut State<'_>,
	writable: NodeId,
	writable_type: P<Type>,
	readable: NodeId,
	operator_kind: Kind,
	value: NodeId,
	value_type: P<Type>,
) -> NodeId {
	let binary = create_binary_from_operator(state, readable, writable_type, operator_kind, value, value_type);
	create_assignment_expression(state, writable, AssignmentOperator::Assign, binary, None)
}

/// `transformWritableExpression` (`transformWritable.ts` L13-41): identifier, property access
/// and element access lvalues. `read_after_write` pins the base object (and the index) so re-reads
/// see the same target. The index goes through `addOneIfArrayType` with the raw object type.
pub fn transform_writable_expression(state: &mut State<'_>, node: P<Node>, read_after_write: bool) -> NodeId {
	let expression = transform_writable_expression_inner(state, node, read_after_write);
	if state.source_map_enabled
		&& (tsast::is_property_access_expression(node) || tsast::is_element_access_expression(node))
	{
		return tag_expression_origin(state, expression, node);
	}
	expression
}

fn transform_writable_expression_inner(state: &mut State<'_>, node: P<Node>, read_after_write: bool) -> NodeId {
	if tsast::is_prototype_access(node) {
		state.diags.add(diagnostics::no_prototype(node));
	}
	if tsast::is_property_access_expression(node) {
		let property_access = node.as_property_access_expression();
		let expression = transform_expression(state, property_access.expression);
		let base = if read_after_write {
			state.push_to_var_if_non_id(expression, "exp")
		} else {
			convert_to_indexable_expression(&mut state.ast, expression)
		};
		return state.ast.property_access(base, property_access.name.text());
	} else if tsast::is_element_access_expression(node) {
		let element_access = node.as_element_access_expression();
		let ordered = ensure_transform_order(state, &[element_access.expression, element_access.argument_expression]);
		let (expression, index) = (ordered[0], ordered[1]);
		let object_type = state.get_type(element_access.expression);
		let mut index_expression = add_one_if_array_type(state, object_type, index);
		let base = if read_after_write {
			let base = state.push_to_var_if_non_id(expression, "exp");
			index_expression = state.push_to_var_if_complex(index_expression, "index");
			base
		} else {
			convert_to_indexable_expression(&mut state.ast, expression)
		};
		return state.ast.computed_index(base, index_expression);
	}
	let transformed = transform_expression(state, tsast::skip_downwards(node));
	if !state.ast.kind(transformed).is_writable_expression() {
		state.ice("transformer: transformWritableExpression: lvalue is not writable");
	}
	transformed
}

/// `writableAssignment`: `readable` is a pre-value snapshot of the target for the compound
/// fallback, `writable` the target and `value` the transformed right side.
#[derive(Clone, Copy, Debug)]
pub struct WritableAssignment {
	/// The assignment target.
	pub writable: NodeId,
	/// The target as read before the value runs.
	pub readable: NodeId,
	/// The transformed value.
	pub value: NodeId,
}

/// `transformWritableAssignment` (L43-58): `readable` only becomes a `readable` temp when the
/// value had prereqs that could change the target and the caller reads before writing.
pub fn transform_writable_assignment(
	state: &mut State<'_>,
	write_node: P<Node>,
	value_node: P<Node>,
	read_after_write: bool,
	read_before_write: bool,
) -> WritableAssignment {
	let writable = transform_writable_expression(state, write_node, read_after_write);
	let (value, prereqs) = state.capture(|state| transform_expression(state, value_node));

	// without read_before_write, readable is never used
	let readable = if read_before_write && state.ast.list_is_non_empty(prereqs) {
		state.push_to_var(Some(writable), "readable")
	} else {
		writable
	};
	state.prereq_list(prereqs);

	WritableAssignment {
		writable,
		readable,
		value,
	}
}
