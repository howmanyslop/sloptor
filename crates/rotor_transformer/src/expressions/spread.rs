//! `spread.go`: object-literal spread (`transformObjectLiteralExpression.ts` L34-90), the
//! per-iterable "append to array" builders shared by array-literal and call-argument spread
//! (`util/getAddIterableToArrayBuilder.ts`), and call-argument spread
//! (`expressions/transformSpreadElement.ts`, `f(...exp)` to `f(unpack(exp))`).

use rotor_luau::Map;
use rotor_tsrs::ast as tsast;
use rotor_tsrs::types::TypeCheck;

use super::{AddIterableToArrayBuilder, num_from_index};
use crate::access::validate_not_any_type;
use crate::call::convert_to_indexable_expression;
use crate::diagnostics::{self, kind_name};
use crate::dispatch::transform_expression;
use crate::pointer::{MapPointer, disable_map_inline};
use crate::prelude::*;
use crate::truthiness::create_truthiness_checks;
use crate::types::{get_first_defined_symbol, is_definitely_type, is_shared_table_type};
use crate::valuetoidstr::value_to_id_str;

// -------------------------------------------------------------------------------------------------
// Object-literal spread: transformObjectLiteralExpression.ts L34-90
// -------------------------------------------------------------------------------------------------

/// `transformSpreadAssignment`. Fast path: a definitely-object spread while the pointer is still
/// an empty inline map (the spread is the first member) becomes `local _object =
/// table.clone(exp)` plus `setmetatable(_object, nil)` (classes can be spread). Otherwise the
/// inline map spills and a generalized-iteration copy loop runs, if-wrapped in truthiness checks
/// when the spread is not definitely an object (with the expression pinned to `_spread` when
/// complex).
pub fn transform_spread_assignment(state: &mut State<'_>, pointer: &mut MapPointer, property: P<Node>) {
	let expression = property.as_spread_assignment().expression;
	let spread_type = state.get_type(expression);
	let exp_type = state.ck().get_non_optional_type(spread_type);
	let symbol = get_first_defined_symbol(state, exp_type);
	if symbol.is_some_and(|symbol| state.macros().is_macro_only_class(symbol)) {
		state.diags.add(diagnostics::no_macro_object_spread(property));
	}

	let t = state.get_type(expression); // NOT the non-optional type!
	let definitely_object = is_definitely_type(state, t, &[TypeCheck::Object, TypeCheck::NonPrimitive]);

	let is_empty_inline_map = state
		.ast
		.get::<Map>(pointer.value)
		.map(|map| map.fields)
		.is_some_and(|fields| state.ast.list_is_empty(fields));
	if definitely_object && is_empty_inline_map {
		let clone = state.ast.global_property("table", "clone");
		let transformed = transform_expression(state, expression);
		let args = state.ast.list_from([transformed]);
		let call = state.ast.call(clone, args);
		pointer.value = state.push_to_var(Some(call), &pointer.name);
		// Explicitly remove metatable because things like classes can be spread
		let setmetatable = state.ast.global_id("setmetatable");
		let nil = state.ast.nil();
		let args = state.ast.list_from([pointer.value, nil]);
		let call = state.ast.call(setmetatable, args);
		let statement = state.ast.call_statement(call);
		state.prereq(statement);
		return;
	}

	disable_map_inline(state, pointer);
	let mut spread_exp = transform_expression(state, expression);
	if !definitely_object {
		spread_exp = state.push_to_var_if_complex(spread_exp, "spread");
	}

	let key_id = state.ast.temp_id("k");
	let value_id = state.ast.temp_id("v");
	let ids = state.ast.list_from([key_id, value_id]);
	let slot = state.ast.computed_index(pointer.value, key_id);
	let copy = state.ast.assignment(slot, AssignmentOperator::Assign, value_id);
	let body = state.ast.list_from([copy]);
	let mut statement = state.ast.for_statement(ids, spread_exp, body);

	if !definitely_object {
		let spread_type = state.get_type(expression);
		let condition = create_truthiness_checks(state, spread_exp, expression, spread_type);
		let statements = state.ast.list_from([statement]);
		let else_body = state.ast.new_list::<element::Statement>();
		statement = state.ast.if_statement(condition, statements, Some(else_body.into()));
	}

	state.prereq(statement);
}

// -------------------------------------------------------------------------------------------------
// addIterableToArrayBuilder: util/getAddIterableToArrayBuilder.ts
// -------------------------------------------------------------------------------------------------

/// `addArray`: `table.move(input, 1, #input, lengthID + amt + 1, arrayID)`. The input is pinned
/// when not an identifier; `#input` is pinned (as `<input>Length`) only when the length must be
/// updated afterwards.
fn add_array_to_array(
	state: &mut State<'_>,
	expression: NodeId,
	array_id: NodeId,
	length_id: NodeId,
	amt_elements_since_update: usize,
	should_update_length_id: bool,
) -> Statements {
	let result = state.ast.new_list();

	let input_array = state.push_to_var_if_non_id(expression, "array");
	let mut input_length = state.ast.unary(UnaryOperator::Length, input_array);
	if should_update_length_id {
		let name = format!("{}Length", value_to_id_str(&state.ast, input_array));
		input_length = state.push_to_var(Some(input_length), &name);
	}

	let table_move = state.ast.global_property("table", "move");
	let one = state.ast.num(1.0);
	let offset = num_from_index(&mut state.ast, amt_elements_since_update, 1);
	let position = state.ast.binary(length_id, BinaryOperator::Add, offset);
	let args = state
		.ast
		.list_from([input_array, one, input_length, position, array_id]);
	let call = state.ast.call(table_move, args);
	let statement = state.ast.call_statement(call);
	state.ast.list_push(result, statement);

	if should_update_length_id {
		let update = state.ast.assignment(length_id, AssignmentOperator::Add, input_length);
		state.ast.list_push(result, update);
	}

	result
}

/// `catchUpLengthID`: the shared `lengthID += amt` prefix of the non-array builders when inline
/// elements were pushed since the last update.
fn catch_up_length_id(state: &mut State<'_>, result: Statements, length_id: NodeId, amt_elements_since_update: usize) {
	if amt_elements_since_update > 0 {
		let amount = num_from_index(&mut state.ast, amt_elements_since_update, 0);
		let update = state.ast.assignment(length_id, AssignmentOperator::Add, amount);
		state.ast.list_push(result, update);
	}
}

/// `appendValueLoop`: the loop body `lengthID += 1; arrayID[lengthID] = value`.
fn append_value_loop(state: &mut State<'_>, array_id: NodeId, length_id: NodeId, value: NodeId) -> [NodeId; 2] {
	let one = state.ast.num(1.0);
	let increment = state.ast.assignment(length_id, AssignmentOperator::Add, one);
	let slot = state.ast.computed_index(array_id, length_id);
	let assign = state.ast.assignment(slot, AssignmentOperator::Assign, value);
	[increment, assign]
}

/// `addString`: `for _char in string.gmatch(exp, utf8.charpattern) do`.
fn add_string_to_array(
	state: &mut State<'_>,
	expression: NodeId,
	array_id: NodeId,
	length_id: NodeId,
	amt_elements_since_update: usize,
	_: bool,
) -> Statements {
	let result = state.ast.new_list();
	catch_up_length_id(state, result, length_id, amt_elements_since_update);

	let value_id = state.ast.temp_id("char");
	let loop_body = append_value_loop(state, array_id, length_id, value_id);
	let ids = state.ast.list_from([value_id]);
	let gmatch = state.ast.global_property("string", "gmatch");
	let charpattern = state.ast.global_property("utf8", "charpattern");
	let args = state.ast.list_from([expression, charpattern]);
	let call = state.ast.call(gmatch, args);
	let body = state.ast.list_from(loop_body);
	let loop_statement = state.ast.for_statement(ids, call, body);
	state.ast.list_push(result, loop_statement);

	result
}

/// `addSet`: `for _v in exp do`.
fn add_set_to_array(
	state: &mut State<'_>,
	expression: NodeId,
	array_id: NodeId,
	length_id: NodeId,
	amt_elements_since_update: usize,
	_: bool,
) -> Statements {
	let result = state.ast.new_list();
	catch_up_length_id(state, result, length_id, amt_elements_since_update);

	let value_id = state.ast.temp_id("v");
	let loop_body = append_value_loop(state, array_id, length_id, value_id);
	let ids = state.ast.list_from([value_id]);
	let body = state.ast.list_from(loop_body);
	let loop_statement = state.ast.for_statement(ids, expression, body);
	state.ast.list_push(result, loop_statement);

	result
}

/// `addMap`: `for _k, _v in exp do`, appending `{ _k, _v }` entry pairs.
fn add_map_to_array(
	state: &mut State<'_>,
	expression: NodeId,
	array_id: NodeId,
	length_id: NodeId,
	amt_elements_since_update: usize,
	_: bool,
) -> Statements {
	let result = state.ast.new_list();
	catch_up_length_id(state, result, length_id, amt_elements_since_update);

	let key_id = state.ast.temp_id("k");
	let value_id = state.ast.temp_id("v");
	let members = state.ast.list_from([key_id, value_id]);
	let entry = state.ast.array(members);
	let loop_body = append_value_loop(state, array_id, length_id, entry);
	let ids = state.ast.list_from([key_id, value_id]);
	let body = state.ast.list_from(loop_body);
	let loop_statement = state.ast.for_statement(ids, expression, body);
	state.ast.list_push(result, loop_statement);

	result
}

/// `addIterableFunction`: `for _result in exp do`.
fn add_iterable_function_to_array(
	state: &mut State<'_>,
	expression: NodeId,
	array_id: NodeId,
	length_id: NodeId,
	amt_elements_since_update: usize,
	_: bool,
) -> Statements {
	let result = state.ast.new_list();
	catch_up_length_id(state, result, length_id, amt_elements_since_update);

	let value_id = state.ast.temp_id("result");
	let loop_body = append_value_loop(state, array_id, length_id, value_id);
	let ids = state.ast.list_from([value_id]);
	let body = state.ast.list_from(loop_body);
	let loop_statement = state.ast.for_statement(ids, expression, body);
	state.ast.list_push(result, loop_statement);

	result
}

/// `addIterableFunctionLuaTuple`: the `while true` protocol collecting each `{ iterFunc() }`
/// tuple until it is empty.
fn add_iterable_function_lua_tuple_to_array(
	state: &mut State<'_>,
	expression: NodeId,
	array_id: NodeId,
	length_id: NodeId,
	amt_elements_since_update: usize,
	_: bool,
) -> Statements {
	let result = state.ast.new_list();
	catch_up_length_id(state, result, length_id, amt_elements_since_update);

	let iter_func_id = state.push_to_var(Some(expression), "iterFunc");
	let value_id = state.ast.temp_id("results");
	let [increment, assign] = append_value_loop(state, array_id, length_id, value_id);
	let args = state.ast.new_list();
	let call = state.ast.call(iter_func_id, args);
	let members = state.ast.list_from([call]);
	let packed = state.ast.array(members);
	let declaration = state.ast.variable_declaration(value_id, Some(packed.into()));
	let size = state.ast.unary(UnaryOperator::Length, value_id);
	let zero = state.ast.num(0.0);
	let is_empty = state.ast.binary(size, BinaryOperator::Equal, zero);
	let break_statement = state.ast.break_statement();
	let break_list = state.ast.list_from([break_statement]);
	let else_body = state.ast.new_list::<element::Statement>();
	let check = state.ast.if_statement(is_empty, break_list, Some(else_body.into()));
	let condition = state.ast.bool(true);
	let body = state.ast.list_from([declaration, check, increment, assign]);
	let loop_statement = state.ast.while_statement(condition, body);
	state.ast.list_push(result, loop_statement);

	result
}

/// `addGenerator`: the `.next()`/`.done`/`.value` protocol.
fn add_generator_to_array(
	state: &mut State<'_>,
	expression: NodeId,
	array_id: NodeId,
	length_id: NodeId,
	amt_elements_since_update: usize,
	_: bool,
) -> Statements {
	let result = state.ast.new_list();
	catch_up_length_id(state, result, length_id, amt_elements_since_update);

	let iter_id = state.ast.temp_id("result");
	let value = state.ast.property_access(iter_id, "value");
	let [increment, assign] = append_value_loop(state, array_id, length_id, value);
	let ids = state.ast.list_from([iter_id]);
	let indexable = convert_to_indexable_expression(&mut state.ast, expression);
	let next = state.ast.property_access(indexable, "next");
	let done = state.ast.property_access(iter_id, "done");
	let break_statement = state.ast.break_statement();
	let break_list = state.ast.list_from([break_statement]);
	let else_body = state.ast.new_list::<element::Statement>();
	let check = state.ast.if_statement(done, break_list, Some(else_body.into()));
	let body = state.ast.list_from([check, increment, assign]);
	let loop_statement = state.ast.for_statement(ids, next, body);
	state.ast.list_push(result, loop_statement);

	result
}

/// `emptyAddIterableToArrayBuilder`: upstream's `() => luau.list.make()` after a diagnostic.
fn empty_add_iterable_to_array_builder(
	state: &mut State<'_>,
	_: NodeId,
	_: NodeId,
	_: NodeId,
	_: usize,
	_: bool,
) -> Statements {
	state.ast.new_list()
}

/// `getAddIterableToArrayBuilder` (L348-376): the `isDefinitelyType` dispatch in upstream order
/// (string before set and map here, unlike `getLoopBuilder`). `Iterable<T>` and unions keep
/// upstream's own `noIterableIteration` and `noMacroUnion` errors, both returning a builder that
/// emits nothing; the fallthrough is upstream's `assert(false, ...)`.
pub fn get_add_iterable_to_array_builder(
	state: &mut State<'_>,
	node: P<Node>,
	t: P<Type>,
) -> AddIterableToArrayBuilder {
	if is_definitely_type(state, t, &[TypeCheck::Array]) {
		return add_array_to_array;
	} else if is_definitely_type(state, t, &[TypeCheck::String]) {
		return add_string_to_array;
	} else if is_definitely_type(state, t, &[TypeCheck::Set]) {
		return add_set_to_array;
	} else if is_definitely_type(state, t, &[TypeCheck::Map]) || is_shared_table_type(state, t) {
		return add_map_to_array;
	} else if is_definitely_type(state, t, &[TypeCheck::IterableFunctionLuaTuple]) {
		return add_iterable_function_lua_tuple_to_array;
	} else if is_definitely_type(state, t, &[TypeCheck::IterableFunction]) {
		return add_iterable_function_to_array;
	} else if is_definitely_type(state, t, &[TypeCheck::Generator]) {
		return add_generator_to_array;
	} else if is_definitely_type(state, t, &[TypeCheck::Iterable]) {
		state.diags.add(diagnostics::no_iterable_iteration(node));
		return empty_add_iterable_to_array_builder;
	} else if t.is_union() {
		state.diags.add(diagnostics::no_macro_union(node));
		return empty_add_iterable_to_array_builder;
	}
	// upstream assert(false)
	let type_string = state.ck().type_to_string(t);
	state.ice(format!("transformer: Iteration type not implemented: {type_string}"));
	empty_add_iterable_to_array_builder
}

// -------------------------------------------------------------------------------------------------
// Call-argument spread: expressions/transformSpreadElement.ts
// -------------------------------------------------------------------------------------------------

/// `transformSpreadElement`: `f(...exp)`. A definitely-array spread is `unpack(exp)` directly;
/// any other iterable is collected into a fresh `_array`/`_length` pair through the shared
/// builders and then unpacked. A spread that is not the last argument reports
/// `noPrecedingSpreadElement`.
pub fn transform_spread_element(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let spread_expression = node.as_spread_element().expression;
	validate_not_any_type(state, spread_expression);

	// array literal is caught and handled separately in transformArrayLiteralExpression.ts
	let Some(parent) = node
		.parent()
		.filter(|&parent| tsast::is_call_expression(parent) || tsast::is_new_expression(parent))
	else {
		// upstream assert
		let parent_kind = node
			.parent()
			.map_or_else(String::new, |parent| kind_name(parent.kind()));
		state.ice(format!(
			"transformer: transformSpreadElement: unexpected parent {parent_kind}"
		));
		return state.ast.none();
	};
	let arguments = parent.arguments();
	let Some(&last) = arguments.last() else {
		// node is itself one of parent's arguments, so this is unreachable.
		state.ice("transformer: transformSpreadElement: spread has no parent arguments");
		return state.ast.none();
	};
	if last != node {
		state.diags.add(diagnostics::no_preceding_spread_element(node));
	}
	if state
		.get_optimizable_var_args_data(spread_expression)
		.is_some_and(|data| data.is_optimizable)
	{
		return state.ast.var_args();
	}

	let expression = transform_expression(state, spread_expression);

	let t = state.get_type(spread_expression);
	if is_definitely_type(state, t, &[TypeCheck::Array]) {
		let unpack = state.ast.global_id("unpack");
		let args = state.ast.list_from([expression]);
		return state.ast.call(unpack, args);
	}
	let builder = get_add_iterable_to_array_builder(state, spread_expression, t);
	let members = state.ast.new_list();
	let empty = state.ast.array(members);
	let array_id = state.push_to_var(Some(empty), "array");
	let zero = state.ast.num(0.0);
	let length_id = state.push_to_var(Some(zero), "length");
	let statements = builder(state, expression, array_id, length_id, 0, false);
	state.prereq_list(statements);
	let unpack = state.ast.global_id("unpack");
	let args = state.ast.list_from([array_id]);
	state.ast.call(unpack, args)
}
