//! `binding.go`: the shared destructuring plumbing (`nodes/binding/transformBindingName.ts`,
//! `util/binding/getAccessorForBindingType.ts` and `util/binding/objectAccessor.ts`). The pattern
//! transforms live in [`super::bindingarray`] and [`super::bindingobject`].
//! `isOmittedBindingElement` is in [`crate::shared`].

use rotor_tsrs::ast as tsast;
use rotor_tsrs::types::TypeCheck;

use super::bindingarray::transform_array_binding_pattern;
use super::bindingiterator::{
	BindingState, collection_binding_state, generator_binding_state, iterator_binding_accessor,
};
use super::bindingobject::transform_object_binding_pattern;
use super::num_from_index;
use crate::access::{add_index_diagnostics, add_one_if_array_type};
use crate::diagnostics::{self, kind_name};
use crate::dispatch::transform_expression;
use crate::identifier::transform_identifier_defined;
use crate::prelude::*;
use crate::types::{is_definitely_type, is_shared_table_type};

/// `transformBindingName` (`transformBindingName.ts` L8-30): identifiers directly, patterns
/// through a `_binding` temp whose destructure statements are appended to `initializers` (used
/// by for-of loop headers; variable declarations go through `transformVariableDeclaration`).
pub fn transform_binding_name(state: &mut State<'_>, name: P<Node>, initializers: Statements) -> NodeId {
	if tsast::is_identifier(name) {
		return transform_identifier_defined(state, name);
	}
	let id = state.ast.temp_id("binding");
	let statements = state.capture_statements(|state| {
		if tsast::is_array_binding_pattern(name) {
			transform_array_binding_pattern(state, name, id);
		} else {
			transform_object_binding_pattern(state, name, id);
		}
	});
	state.ast.list_push_list(initializers, statements);
	id
}

/// `bindingAccessor`: reads one pattern position; the [`BindingState`] keeps iterator progress.
pub type BindingAccessor = fn(&mut State<'_>, NodeId, usize, &mut BindingState, bool) -> NodeId;

/// `arrayAccessor` (L32-37): `parentId[index + 1]` with the `+1` folded into the literal; an
/// omitted element emits nothing.
fn array_accessor(state: &mut State<'_>, parent_id: NodeId, index: usize, _: &mut BindingState, _: bool) -> NodeId {
	let index = num_from_index(&mut state.ast, index, 1);
	state.ast.computed_index(parent_id, index)
}

/// `stringAccessor`: one `string.gmatch(parentId, utf8.charpattern)` matcher per pattern.
fn string_accessor(
	state: &mut State<'_>,
	parent_id: NodeId,
	_: usize,
	binding_state: &mut BindingState,
	is_omitted: bool,
) -> NodeId {
	let id = if let Some(matcher) = binding_state.matcher {
		matcher
	} else {
		let gmatch = state.ast.global_property("string", "gmatch");
		let charpattern = state.ast.global_property("utf8", "charpattern");
		let args = state.ast.list_from([parent_id, charpattern]);
		let call = state.ast.call(gmatch, args);
		let id = state.push_to_var(Some(call), "matcher");
		binding_state.matcher = Some(id);
		id
	};

	let args = state.ast.new_list();
	let call_exp = state.ast.call(id, args);

	if is_omitted {
		let statement = state.ast.call_statement(call_exp);
		state.prereq(statement);
		return state.ast.none();
	}
	call_exp
}

/// `iterableFunctionLuaTupleAccessor` (L105-117): `{ parentId() }`, the call's returns packed;
/// an omitted element calls the function as a statement to advance.
fn iterable_function_lua_tuple_accessor(
	state: &mut State<'_>,
	parent_id: NodeId,
	_: usize,
	_: &mut BindingState,
	is_omitted: bool,
) -> NodeId {
	let args = state.ast.new_list();
	let call_exp = state.ast.call(parent_id, args);
	if is_omitted {
		let statement = state.ast.call_statement(call_exp);
		state.prereq(statement);
		return state.ast.none();
	}
	let members = state.ast.list_from([call_exp]);
	state.ast.array(members)
}

/// `iterableFunctionAccessor` (L119-131): `parentId()`; an omitted element calls as a statement
/// to advance.
fn iterable_function_accessor(
	state: &mut State<'_>,
	parent_id: NodeId,
	_: usize,
	_: &mut BindingState,
	is_omitted: bool,
) -> NodeId {
	let args = state.ast.new_list();
	let call_exp = state.ast.call(parent_id, args);
	if is_omitted {
		let statement = state.ast.call_statement(call_exp);
		state.prereq(statement);
		return state.ast.none();
	}
	call_exp
}

/// `noneAccessor`: upstream's `() => luau.none()` after the `noIterableIteration` diagnostic.
fn none_accessor(state: &mut State<'_>, _: NodeId, _: usize, _: &mut BindingState, _: bool) -> NodeId {
	state.ast.none()
}

/// `getAccessorForBindingType` (L143-167): the eight-entry `isDefinitelyType` dispatch, in
/// upstream order. `Iterable<T>` keeps upstream's own `noIterableIteration` error. The
/// fallthrough is upstream's `assert(false, ...)`.
pub fn get_accessor_for_binding_type(
	state: &mut State<'_>,
	node: P<Node>,
	t: P<Type>,
	parent_id: NodeId,
) -> (BindingAccessor, BindingState) {
	if is_definitely_type(state, t, &[TypeCheck::Array]) {
		return (array_accessor, BindingState::default());
	} else if is_definitely_type(state, t, &[TypeCheck::String]) {
		return (string_accessor, BindingState::default());
	} else if is_definitely_type(state, t, &[TypeCheck::Set]) {
		return (
			iterator_binding_accessor,
			collection_binding_state(state, parent_id, false),
		);
	} else if is_definitely_type(state, t, &[TypeCheck::Map]) || is_shared_table_type(state, t) {
		return (
			iterator_binding_accessor,
			collection_binding_state(state, parent_id, true),
		);
	} else if is_definitely_type(state, t, &[TypeCheck::IterableFunctionLuaTuple]) {
		return (iterable_function_lua_tuple_accessor, BindingState::default());
	} else if is_definitely_type(state, t, &[TypeCheck::IterableFunction]) {
		return (iterable_function_accessor, BindingState::default());
	} else if is_definitely_type(state, t, &[TypeCheck::Iterable]) {
		state.diags.add(diagnostics::no_iterable_iteration(node));
		return (none_accessor, BindingState::default());
	} else if is_definitely_type(state, t, &[TypeCheck::Generator])
		|| is_definitely_type(state, t, &[TypeCheck::Object])
		|| node.kind() == Kind::ThisKeyword
	{
		return (iterator_binding_accessor, generator_binding_state(state, parent_id));
	}
	// upstream assert(false)
	let type_string = state.ck().type_to_string(t);
	state.ice(format!(
		"transformer: Destructuring not supported for type: {type_string}"
	));
	(none_accessor, BindingState::default())
}

/// `objectAccessor` (`objectAccessor.ts` L11-36): the read expression for one object-pattern
/// property. `t` is the parent pattern's type.
///
/// Quirk (ported verbatim): computed names get the `+1` array adjustment, but literal numeric
/// names do not, so `{ 0: x }` over a tuple emits `parent[0]` while `{ [0]: x }` emits
/// `parent[1]`.
pub fn object_accessor(state: &mut State<'_>, parent_id: NodeId, t: P<Type>, name: P<Node>) -> NodeId {
	if name.kind() == Kind::BigIntLiteral {
		state.diags.add(diagnostics::no_big_int(name));
		return state.ast.none();
	}

	let name_type = state.get_type(name);
	add_index_diagnostics(state, name, name_type);

	if tsast::is_identifier(name) {
		return state.ast.property_access(parent_id, name.text());
	} else if tsast::is_computed_property_name(name) {
		let expression = transform_expression(state, name.as_computed_property_name().expression);
		let index = add_one_if_array_type(state, t, expression);
		return state.ast.computed_index(parent_id, index);
	} else if tsast::is_numeric_literal(name)
		|| tsast::is_string_literal(name)
		|| tsast::is_no_substitution_template_literal(name)
	{
		let index = transform_expression(state, name);
		return state.ast.computed_index(parent_id, index);
	} else if tsast::is_private_identifier(name) {
		state.diags.add(diagnostics::no_private_identifier(name));
		return state.ast.none();
	}
	// upstream assertNever
	state.ice(format!(
		"transformer: objectAccessor unexpected name kind: {}",
		kind_name(name.kind())
	));
	state.ast.none()
}
