//! `forof.go`: `statements/transformForOfStatement.ts`.

use rotor_luau::ForStatement;
use rotor_tsrs::ast::{self as tsast, skip_downwards};
use rotor_tsrs::checker::{ElementFlags, TupleType};
use rotor_tsrs::types::TypeCheck;

use crate::access::get_literal_number_value;
use crate::assignment::transform_writable_expression;
use crate::call::convert_to_indexable_expression;
use crate::diagnostics;
use crate::dispatch::transform_expression;
use crate::ensureorder::ensure_transform_order;
use crate::expressions::{
	transform_array_assignment_pattern, transform_binding_name, transform_object_assignment_pattern,
};
use crate::identifier::validate_identifier;
use crate::prelude::*;
use crate::shared::{is_omitted_binding_element, transform_initializer};
use crate::spreadhelpers::array_like_expression_contains_spread;
use crate::statementlist::transform_statement_list;
use crate::statements::get_statements;
use crate::types::{get_first_defined_symbol, is_definitely_type, is_shared_table_type};
use crate::util::is_identifier;
use crate::valuetoidstr::value_to_id_str;

/// `loopBuilder` (`type LoopBuilder`, L34-39): `statements` is the already-transformed body; the
/// builder wraps it in the loop shape. Go's closures are the variants.
#[derive(Clone, Copy)]
enum LoopBuilder {
	Array,
	Set,
	Map,
	String,
	IterableFunction,
	IterableFunctionLuaTuple(P<Type>),
	Generator,
	/// `emptyLoopBuilder`: upstream's `() => luau.list.make()` after a dispatch diagnostic.
	Empty,
}

/// `makeForLoopBuilder`'s callbacks: each fills `ids` (the generic-for binding list) and
/// `initializers` (statements to prepend to the body) and returns the for-expression.
#[derive(Clone, Copy)]
enum ForLoopCallback {
	Array,
	Set,
	Map,
	String,
	IterableFunction,
	Generator,
}

impl LoopBuilder {
	fn build(self, state: &mut State<'_>, statements: Statements, initializer: P<Node>, exp: NodeId) -> Statements {
		match self {
			Self::Array => make_for_loop(state, ForLoopCallback::Array, statements, initializer, exp),
			Self::Set => make_for_loop(state, ForLoopCallback::Set, statements, initializer, exp),
			Self::Map => make_for_loop(state, ForLoopCallback::Map, statements, initializer, exp),
			Self::String => make_for_loop(state, ForLoopCallback::String, statements, initializer, exp),
			Self::IterableFunction => {
				make_for_loop(state, ForLoopCallback::IterableFunction, statements, initializer, exp)
			}
			Self::IterableFunctionLuaTuple(t) => {
				build_iterable_function_lua_tuple_loop(state, t, statements, initializer, exp)
			}
			Self::Generator => make_for_loop(state, ForLoopCallback::Generator, statements, initializer, exp),
			Self::Empty => state.ast.new_list(),
		}
	}
}

/// `makeForLoopBuilder` (L41-57), applied.
fn make_for_loop(
	state: &mut State<'_>,
	callback: ForLoopCallback,
	statements: Statements,
	initializer: P<Node>,
	exp: NodeId,
) -> Statements {
	let ids = state.ast.new_list();
	let initializers = state.ast.new_list();
	let expression = match callback {
		ForLoopCallback::Array => build_array_loop(state, initializer, exp, ids, initializers),
		ForLoopCallback::Set | ForLoopCallback::IterableFunction => {
			build_single_binding_loop(state, initializer, exp, ids, initializers)
		}
		ForLoopCallback::Map => build_map_loop(state, initializer, exp, ids, initializers),
		ForLoopCallback::String => build_string_loop(state, initializer, exp, ids, initializers),
		ForLoopCallback::Generator => build_generator_loop(state, initializer, exp, ids, initializers),
	};
	state.ast.list_unshift_list(statements, initializers);
	let loop_statement = state.ast.for_statement(ids, expression, statements);
	state.ast.list_from([loop_statement])
}

/// `transformForInitializerExpressionDirect` (L59-92): binds a known value expression through
/// the initializer (used by the Map and generator builders, whose loop bindings are protocol
/// temps rather than the user's binding). Array and object literal targets destructure a
/// `_binding` temp pushed to var from the value; any other writable target gets a direct
/// assignment.
fn transform_for_initializer_expression_direct(
	state: &mut State<'_>,
	initializer: P<Node>,
	initializers: Statements,
	value: NodeId,
) {
	if tsast::is_array_literal_expression(initializer) {
		let statements = state.capture_statements(|state| {
			let parent_id = state.push_to_var(Some(value), "binding");
			transform_array_assignment_pattern(state, initializer, parent_id);
		});
		state.ast.list_push_list(initializers, statements);
	} else if tsast::is_object_literal_expression(initializer) {
		let statements = state.capture_statements(|state| {
			let parent_id = state.push_to_var(Some(value), "binding");
			transform_object_assignment_pattern(state, initializer, parent_id);
		});
		state.ast.list_push_list(initializers, statements);
	} else {
		let expression = transform_writable_expression(state, initializer, false);
		let assignment = state.ast.assignment(expression, AssignmentOperator::Assign, value);
		state.ast.list_push(initializers, assignment);
	}
}

/// The first declaration's name of a `for (const x of ...)` initializer.
fn first_declaration_name(initializer: P<Node>) -> Option<P<Node>> {
	initializer
		.as_variable_declaration_list()
		.declarations
		.nodes()
		.first()
		.map(|declaration| declaration.as_variable_declaration().name)
}

/// `transformForInitializer` (L94-128): the single identifier bound by the loop, with any binding
/// statements prepended to the body through `initializers`.
fn transform_for_initializer(state: &mut State<'_>, initializer: P<Node>, initializers: Statements) -> NodeId {
	if tsast::is_variable_declaration_list(initializer) {
		let Some(name) = first_declaration_name(initializer) else {
			state.ice("transformer: for-of declaration list has no declaration");
			return state.ast.none();
		};
		return transform_binding_name(state, name, initializers);
	} else if tsast::is_array_literal_expression(initializer) {
		// `for ([a, b] of ...)`: destructure a `_binding` temp inside the body.
		let parent_id = state.ast.temp_id("binding");
		let statements =
			state.capture_statements(|state| transform_array_assignment_pattern(state, initializer, parent_id));
		state.ast.list_push_list(initializers, statements);
		return parent_id;
	} else if tsast::is_object_literal_expression(initializer) {
		let parent_id = state.ast.temp_id("binding");
		let statements =
			state.capture_statements(|state| transform_object_assignment_pattern(state, initializer, parent_id));
		state.ast.list_push_list(initializers, statements);
		return parent_id;
	}
	// `for (x of ...)`, `for (a.b of ...)`: a `_v` temp is the loop binding, assigned through the
	// writable target at the top of the body.
	let value_id = state.ast.temp_id("v");
	let expression = transform_writable_expression(state, initializer, false);
	let assignment = state.ast.assignment(expression, AssignmentOperator::Assign, value_id);
	state.ast.list_push(initializers, assignment);
	value_id
}

/// `buildArrayLoop` (L130-134): `for _, x in exp do`. The first binding is an unnamed temp
/// (rendered `_` when unreferenced); there is no ipairs in 3.0.
fn build_array_loop(
	state: &mut State<'_>,
	initializer: P<Node>,
	exp: NodeId,
	ids: List<element::AnyIdentifier>,
	initializers: Statements,
) -> NodeId {
	let discard = state.ast.temp_id("");
	state.ast.list_push(ids, discard);
	let id = transform_for_initializer(state, initializer, initializers);
	state.ast.list_push(ids, id);
	exp
}

/// `buildSetLoop` (L136-139) and `buildIterableFunctionLoop` (L264-267): `for x in exp do`.
fn build_single_binding_loop(
	state: &mut State<'_>,
	initializer: P<Node>,
	exp: NodeId,
	ids: List<element::AnyIdentifier>,
	initializers: Statements,
) -> NodeId {
	let id = transform_for_initializer(state, initializer, initializers);
	state.ast.list_push(ids, id);
	exp
}

/// `transformInLineArrayBindingPattern` (L141-160): the `for (const [k, v] of map)` fast path.
/// Each pattern element becomes a generic-for binding directly. NOTE upstream tests
/// `ts.isSpreadElement`, which never matches binding pattern elements: the rest check is dead
/// upstream and a rest element falls through to `transformBindingName`; ported verbatim.
fn transform_in_line_array_binding_pattern(
	state: &mut State<'_>,
	pattern: P<Node>,
	ids: List<element::AnyIdentifier>,
	initializers: Statements,
) {
	for &element in pattern.as_binding_pattern().elements.nodes() {
		if is_omitted_binding_element(element) {
			let discard = state.ast.temp_id("");
			state.ast.list_push(ids, discard);
		} else if tsast::is_spread_element(element) {
			state
				.diags
				.add(diagnostics::no_nested_spreads_in_assignment_patterns(element));
		} else {
			let Some(name) = element.name() else {
				continue;
			};
			let id = transform_binding_name(state, name, initializers);
			if let Some(element_initializer) = element.initializer() {
				let statement = transform_initializer(state, id, element_initializer);
				state.ast.list_push(initializers, statement);
			}
			state.ast.list_push(ids, id);
		}
	}
}

/// `transformInLineArrayAssignmentPattern` (L162-222): the `for ([a, b] of map)` fast path. Each
/// slot gets a `_binding` temp as the generic-for binding, with `target = _binding` assignments,
/// nested destructures and defaults captured into `initializers`.
fn transform_in_line_array_assignment_pattern(
	state: &mut State<'_>,
	assignment_pattern: P<Node>,
	ids: List<element::AnyIdentifier>,
	initializers: Statements,
) {
	let statements = state.capture_statements(|state| {
		for &original in assignment_pattern.as_array_literal_expression().elements.nodes() {
			let mut element = original;
			if tsast::is_omitted_expression(element) {
				let discard = state.ast.temp_id("");
				state.ast.list_push(ids, discard);
			} else if tsast::is_spread_element(element) {
				state
					.diags
					.add(diagnostics::no_nested_spreads_in_assignment_patterns(element));
			} else {
				let mut initializer = None;
				if tsast::is_binary_expression(element) {
					let binary = element.as_binary_expression();
					initializer = Some(skip_downwards(binary.right.get()));
					element = skip_downwards(binary.left);
				}

				let value_id = state.ast.temp_id("binding");
				if is_identifier(element)
					|| tsast::is_element_access_expression(element)
					|| tsast::is_property_access_expression(element)
				{
					let id = transform_writable_expression(state, element, initializer.is_some());
					let assignment = state.ast.assignment(id, AssignmentOperator::Assign, value_id);
					state.prereq(assignment);
					if let Some(initializer) = initializer {
						let statement = transform_initializer(state, id, initializer);
						state.prereq(statement);
					}
				} else if tsast::is_array_literal_expression(element) {
					if let Some(initializer) = initializer {
						let statement = transform_initializer(state, value_id, initializer);
						state.prereq(statement);
					}
					transform_array_assignment_pattern(state, element, value_id);
				} else if tsast::is_object_literal_expression(element) {
					if let Some(initializer) = initializer {
						let statement = transform_initializer(state, value_id, initializer);
						state.prereq(statement);
					}
					transform_object_assignment_pattern(state, element, value_id);
				} else {
					// upstream assert
					state.ice(format!(
						"transformer: transformInLineArrayAssignmentPattern invalid element: {}",
						diagnostics::kind_name(element.kind())
					));
				}

				state.ast.list_push(ids, value_id);
			}
		}
	});
	state.ast.list_push_list(initializers, statements);
}

/// `buildMapLoop` (L224-257): `for _k, _v in exp do`, with the inline `[k, v]` destructure fast
/// path. The fallback (a plain id or object pattern over a Map) binds `_k, _v` and reconstitutes
/// the entry: the declaration form is `local pair = { _k, _v }` plus pattern bindings; the
/// expression form assigns `{ _k, _v }` through the target.
fn build_map_loop(
	state: &mut State<'_>,
	initializer: P<Node>,
	exp: NodeId,
	ids: List<element::AnyIdentifier>,
	initializers: Statements,
) -> NodeId {
	if tsast::is_variable_declaration_list(initializer) {
		if let Some(name) = first_declaration_name(initializer)
			&& tsast::is_array_binding_pattern(name)
			&& !array_like_expression_contains_spread(name)
		{
			transform_in_line_array_binding_pattern(state, name, ids, initializers);
			return exp;
		}
	} else if tsast::is_array_literal_expression(initializer) && !array_like_expression_contains_spread(initializer) {
		transform_in_line_array_assignment_pattern(state, initializer, ids, initializers);
		return exp;
	}

	let key_id = state.ast.temp_id("k");
	let value_id = state.ast.temp_id("v");
	state.ast.list_push(ids, key_id);
	state.ast.list_push(ids, value_id);

	if tsast::is_variable_declaration_list(initializer) {
		let binding_list = state.ast.new_list();
		let id = transform_for_initializer(state, initializer, binding_list);
		let members = state.ast.list_from([key_id, value_id]);
		let entry = state.ast.array(members);
		let declaration = state.ast.variable_declaration(id, Some(entry.into()));
		state.ast.list_push(initializers, declaration);
		state.ast.list_push_list(initializers, binding_list);
	} else {
		let members = state.ast.list_from([key_id, value_id]);
		let entry = state.ast.array(members);
		transform_for_initializer_expression_direct(state, initializer, initializers, entry);
	}

	exp
}

/// `buildStringLoop` (L259-262): `for c in string.gmatch(exp, utf8.charpattern) do`.
fn build_string_loop(
	state: &mut State<'_>,
	initializer: P<Node>,
	exp: NodeId,
	ids: List<element::AnyIdentifier>,
	initializers: Statements,
) -> NodeId {
	let id = transform_for_initializer(state, initializer, initializers);
	state.ast.list_push(ids, id);
	let gmatch = state.ast.global_property("string", "gmatch");
	let charpattern = state.ast.global_property("utf8", "charpattern");
	let args = state.ast.list_from([exp, charpattern]);
	state.ast.call(gmatch, args)
}

/// `makeIterableFunctionLuaTupleShorthand` (L269-284): the array-pattern fast path for
/// `IterableFunction<LuaTuple<T>>`. Inline bindings become the generic-for ids
/// (`for a, b in exp do`); no tuple table is built.
fn make_iterable_function_lua_tuple_shorthand(
	state: &mut State<'_>,
	array: P<Node>,
	statements: Statements,
	expression: NodeId,
) -> Statements {
	let ids = state.ast.new_list();
	let initializers = state.ast.new_list();
	if tsast::is_array_binding_pattern(array) {
		transform_in_line_array_binding_pattern(state, array, ids, initializers);
	} else {
		transform_in_line_array_assignment_pattern(state, array, ids, initializers);
	}
	state.ast.list_unshift_list(statements, initializers);
	let loop_statement = state.ast.for_statement(ids, expression, statements);
	state.ast.list_from([loop_statement])
}

/// `tupleCombinedFlags`: upstream's `target.combinedFlags`, the bitwise OR of every element's
/// flags.
fn tuple_combined_flags(target: &TupleType) -> ElementFlags {
	target
		.element_infos()
		.iter()
		.fold(ElementFlags::empty(), |combined, info| combined | info.flags)
}

/// `buildIterableFunctionLuaTupleLoop` (L286-382). Three shapes, in order:
///
/// 1. an array-pattern initializer (binding or assignment form): the inline fast path;
/// 2. a declaration-list initializer whose `LuaTuple<T>` argument is a tuple type without rest
///    elements: one generic-for temp per tuple slot (named from the tuple labels when valid
///    identifiers, else `element`), reassembled into `local t = { _el1, _el2 }`;
/// 3. otherwise (unknown arity or an expression initializer): the while-true protocol,
///    `local _v = { _iterFunc() } if #_v == 0 then break end`.
fn build_iterable_function_lua_tuple_loop(
	state: &mut State<'_>,
	t: P<Type>,
	statements: Statements,
	initializer: P<Node>,
	exp: NodeId,
) -> Statements {
	if tsast::is_variable_declaration_list(initializer) {
		// for (const [a, b] of iter())
		if let Some(name) = first_declaration_name(initializer)
			&& tsast::is_array_binding_pattern(name)
			&& !array_like_expression_contains_spread(name)
		{
			return make_iterable_function_lua_tuple_shorthand(state, name, statements, exp);
		}
	} else if tsast::is_array_literal_expression(initializer) && !array_like_expression_contains_spread(initializer) {
		// for ([a, b] of iter())
		return make_iterable_function_lua_tuple_shorthand(state, initializer, statements, exp);
	}

	let mut iterator_return_ids = Vec::new();

	// get call signature of IterableFunction<T> which is `(): T`
	// and get return type of call signature which is `T`
	let Some(&signature) = state.ck().get_call_signatures(t).first() else {
		state.ice("transformer: IterableFunction has no call signature");
		return state.ast.new_list();
	};
	let lua_tuple_type = state.ck().get_return_type_of_signature(signature);
	let Some(&[tuple_arg_type]) = lua_tuple_type.alias().map(|alias| alias.type_arguments()) else {
		state.ice("transformer: Incorrect LuaTuple<T> type arguments"); // upstream assert
		return state.ast.new_list();
	};
	// if the initializer is a variable declaration `for (const a of iter())` and LuaTuple has a
	// fixed element count, then use a Luau for-in loop, specifying all elements and putting them
	// in a table
	if tsast::is_variable_declaration_list(initializer)
		&& tuple_arg_type.is_tuple_type()
		&& !tuple_combined_flags(tuple_arg_type.target_tuple_type()).intersects(ElementFlags::Variable)
	{
		for info in tuple_arg_type.target_tuple_type().element_infos() {
			let mut name = "element".to_owned();
			if let Some(label) = info.labeled_declaration()
				&& let Some(label_name) = label.name()
				&& is_identifier(label_name)
				&& rotor_luau::is_valid_identifier(label_name.text())
			{
				label_name.text().clone_into(&mut name);
			}
			iterator_return_ids.push(state.ast.temp_id(name));
		}
	} else {
		let mut name_hint = value_to_id_str(&state.ast, exp);
		if name_hint.is_empty() {
			"iterFunc".clone_into(&mut name_hint);
		}
		let iter_func_id = state.push_to_var(Some(exp), &name_hint);
		let loop_statements = state.ast.new_list();

		let initializer_statements = state.ast.new_list();
		let value_id = transform_for_initializer(state, initializer, initializer_statements);

		let no_args = state.ast.new_list();
		let call = state.ast.call(iter_func_id, no_args);
		let members = state.ast.list_from([call]);
		let packed = state.ast.array(members);
		let declaration = state.ast.variable_declaration(value_id, Some(packed.into()));
		state.ast.list_push(loop_statements, declaration);

		let length = state.ast.unary(UnaryOperator::Length, value_id);
		let zero = state.ast.num(0.0);
		let is_empty = state.ast.binary(length, BinaryOperator::Equal, zero);
		let break_statement = state.ast.break_statement();
		let break_body = state.ast.list_from([break_statement]);
		let check = state.ast.if_statement(is_empty, break_body, None);
		state.ast.list_push(loop_statements, check);

		state.ast.list_push_list(loop_statements, initializer_statements);

		state.ast.list_push_list(loop_statements, statements);

		let condition = state.ast.bool(true);
		let loop_statement = state.ast.while_statement(condition, loop_statements);
		return state.ast.list_from([loop_statement]);
	}

	let tuple_id = transform_for_initializer(state, initializer, statements);

	// makeForLoopBuilder with the tuple callback, applied
	let ids = state.ast.new_list();
	let initializers = state.ast.new_list();
	let members = state.ast.new_list();
	for &id in &iterator_return_ids {
		state.ast.list_push(ids, id);
		state.ast.list_push(members, id);
	}
	let tuple = state.ast.array(members);
	let declaration = state.ast.variable_declaration(tuple_id, Some(tuple.into()));
	state.ast.list_push(initializers, declaration);
	state.ast.list_unshift_list(statements, initializers);
	let loop_statement = state.ast.for_statement(ids, exp, statements);
	state.ast.list_from([loop_statement])
}

/// `buildGeneratorLoop` (L384-412): the `.next()`/`.done`/`.value` protocol.
///
/// ```text
/// for _result in exp.next do
///     if _result.done then break end
///     local x = _result.value
///     <body>
/// end
/// ```
fn build_generator_loop(
	state: &mut State<'_>,
	initializer: P<Node>,
	exp: NodeId,
	ids: List<element::AnyIdentifier>,
	initializers: Statements,
) -> NodeId {
	let loop_id = state.ast.temp_id("result");
	state.ast.list_push(ids, loop_id);

	let done = state.ast.property_access(loop_id, "done");
	let break_statement = state.ast.break_statement();
	let break_body = state.ast.list_from([break_statement]);
	let check = state.ast.if_statement(done, break_body, None);
	state.ast.list_push(initializers, check);

	if tsast::is_variable_declaration_list(initializer) {
		let binding_list = state.ast.new_list();
		let id = transform_for_initializer(state, initializer, binding_list);
		let value = state.ast.property_access(loop_id, "value");
		let declaration = state.ast.variable_declaration(id, Some(value.into()));
		state.ast.list_push(initializers, declaration);
		state.ast.list_push_list(initializers, binding_list);
	} else {
		let value = state.ast.property_access(loop_id, "value");
		transform_for_initializer_expression_direct(state, initializer, initializers, value);
	}

	let indexable = convert_to_indexable_expression(&mut state.ast, exp);
	state.ast.property_access(indexable, "next")
}

/// `getLoopBuilder` (L414-438): the `isDefinitelyType` dispatch, in upstream order. `Iterable<T>`
/// and unions keep upstream's own `noIterableIteration` / `noMacroUnion` errors. The fallthrough
/// is upstream's `assert(false, ...)`.
fn get_loop_builder(state: &mut State<'_>, node: P<Node>, t: P<Type>) -> LoopBuilder {
	if is_definitely_type(state, t, &[TypeCheck::Array]) {
		LoopBuilder::Array
	} else if is_definitely_type(state, t, &[TypeCheck::Set]) {
		LoopBuilder::Set
	} else if is_definitely_type(state, t, &[TypeCheck::Map]) || is_shared_table_type(state, t) {
		LoopBuilder::Map
	} else if is_definitely_type(state, t, &[TypeCheck::String]) {
		LoopBuilder::String
	} else if is_definitely_type(state, t, &[TypeCheck::IterableFunctionLuaTuple]) {
		LoopBuilder::IterableFunctionLuaTuple(t)
	} else if is_definitely_type(state, t, &[TypeCheck::IterableFunction]) {
		LoopBuilder::IterableFunction
	} else if is_definitely_type(state, t, &[TypeCheck::Generator]) {
		LoopBuilder::Generator
	} else if is_definitely_type(state, t, &[TypeCheck::Iterable]) {
		state.diags.add(diagnostics::no_iterable_iteration(node));
		LoopBuilder::Empty
	} else if t.is_union() {
		state.diags.add(diagnostics::no_macro_union(node));
		LoopBuilder::Empty
	} else {
		let text = state.ck().type_to_string(t);
		state.ice(format!("transformer: ForOf iteration type not implemented: {text}")); // upstream assert(false)
		LoopBuilder::Empty
	}
}

/// `findRangeMacro` (L440-448): the loop expression (skipped downwards) is a call whose callee
/// resolves to the `$range` macro. Checked before the expression is transformed, matching
/// upstream's position.
fn find_range_macro(state: &mut State<'_>, node: P<Node>) -> Option<P<Node>> {
	let expression = skip_downwards(node.as_for_in_or_of_statement().expression);
	if !tsast::is_call_expression(expression) {
		return None;
	}
	let callee_type = state.get_type(expression.as_call_expression().expression);
	let symbol = get_first_defined_symbol(state, callee_type)?;
	let entry = state.macros().get_call_macro(symbol)?;
	(entry.name == "$range").then_some(expression)
}

/// `transformForOfRangeMacro` (L450-477): `for (const i of $range(start, end[, step]))` becomes a
/// Luau numeric for. The macro arguments transform via `ensureTransformOrder` (prereqs land
/// before the loop); a non-literal step expression gets `or 1` appended to match `Number()`
/// defaulting.
fn transform_for_of_range_macro(state: &mut State<'_>, node: P<Node>, macro_call: P<Node>) -> Statements {
	let for_of = node.as_for_in_or_of_statement();
	let result = state.ast.new_list();

	let statements = state.ast.new_list();
	let id = transform_for_initializer(state, for_of.initializer, statements);

	let mut args = Vec::new();
	let prereqs = state.capture_statements(|state| {
		args = ensure_transform_order(state, macro_call.as_call_expression().arguments.nodes());
	});
	state.ast.list_push_list(result, prereqs);
	// [start, end, step] destructure: missing slots are nil (upstream undefined); the checker
	// enforces the two-argument minimum.
	let start = args.first().copied();
	let end = args.get(1).copied();
	let mut step = args.get(2).copied();

	let body = for_of.statement();
	let body_statements = transform_statement_list(state, Some(body), &get_statements(body), None);
	state.ast.list_push_list(statements, body_statements);
	let resets = state.label_resets();
	state.ast.list_unshift_list(statements, resets);

	if let Some(step_value) = step
		&& get_literal_number_value(&state.ast, step_value).is_none()
	{
		let one = state.ast.num(1.0);
		step = Some(state.ast.binary(step_value, BinaryOperator::Or, one));
	}
	let (Some(start), Some(end)) = (start, end) else {
		state.ice("transformer: $range needs a start and an end");
		return result;
	};
	let loop_statement = state.ast.numeric_for(id, start, end, step, statements);
	state.ast.list_push(result, loop_statement);

	result
}

/// `transformOptimizableVarArgsForOf`: `for (const x of args)` over an optimizable rest parameter
/// becomes `for _i = 1, select("#", ...) do local x = (select(_i, ...)) ... end`.
fn transform_optimizable_var_args_for_of(state: &mut State<'_>, node: P<Node>, result: Statements) -> Statements {
	let for_of = node.as_for_in_or_of_statement();
	let Some(data) = state
		.get_optimizable_var_args_data(for_of.expression)
		.filter(|data| data.is_optimizable)
	else {
		return result;
	};

	let output = state.ast.new_list();
	let mut found = None;
	for statement in state.ast.list_to_vec(result) {
		if let Some(for_statement) = state.ast.get::<ForStatement>(statement) {
			found = Some(for_statement.clone());
			continue;
		}
		state.ast.list_push(output, statement);
	}
	let Some(loop_statement) = found else {
		state.ice("transformer: optimizable varargs for-of has no array loop");
		return result;
	};

	let value_id = if tsast::is_variable_declaration_list(for_of.initializer) {
		match first_declaration_name(for_of.initializer) {
			Some(name) if is_identifier(name) => state.ast.id(name.text()),
			_ => state.ast.temp_id("v"),
		}
	} else {
		let loop_ids = state.ast.list_to_vec(loop_statement.ids);
		let [_, value_id] = loop_ids[..] else {
			state.ice("transformer: optimizable varargs for-of has invalid array loop ids");
			return result;
		};
		value_id
	};

	let index_id = state.ast.temp_id("i");
	let select = state.ast.global_id("select");
	let var_args = state.ast.var_args();
	let args = state.ast.list_from([index_id, var_args]);
	let call = state.ast.call(select, args);
	let value = state.ast.parenthesized(call);
	let declaration = state.ast.variable_declaration(value_id, Some(value.into()));
	let loop_statements = state.ast.list_from([declaration]);
	state.ast.list_push_list(loop_statements, loop_statement.statements);

	let length = match data.length_id {
		Some(length_id) if data.uses_length_cache() => length_id,
		_ => super::create_var_args_length_select(&mut state.ast),
	};
	let one = state.ast.num(1.0);
	let numeric_for = state.ast.numeric_for(index_id, one, length, None, loop_statements);
	state.ast.list_push(output, numeric_for);
	output
}

/// `transformForOfStatement` (L479-508). The body is transformed before the loop shape is built;
/// builders prepend initializer statements.
pub fn transform_for_of_statement(state: &mut State<'_>, node: P<Node>) -> Statements {
	let for_of = node.as_for_in_or_of_statement();

	if for_of.await_modifier().is_some() {
		state.diags.add(diagnostics::no_await_for_of(node));
	}

	if tsast::is_variable_declaration_list(for_of.initializer)
		&& let Some(name) = first_declaration_name(for_of.initializer)
		&& is_identifier(name)
	{
		validate_identifier(state, name);
	}

	// One break scope covers the range-macro numeric for and every builder shape (rotor
	// extension).
	state.enter_break_scope();

	let mut result;
	if let Some(range_macro_call) = find_range_macro(state, node) {
		result = transform_for_of_range_macro(state, node, range_macro_call);
	} else {
		result = state.ast.new_list();

		let (exp, exp_prereqs) = state.capture(|state| transform_expression(state, for_of.expression));
		state.ast.list_push_list(result, exp_prereqs);

		let exp_type = state.get_type(for_of.expression);
		let body = for_of.statement();
		let statements = transform_statement_list(state, Some(body), &get_statements(body), None);
		// Unshift into the body list here: it is still writable, and every builder either
		// unshifts its own initializers above these or splices the list into a larger body.
		let resets = state.label_resets();
		state.ast.list_unshift_list(statements, resets);

		let loop_builder = get_loop_builder(state, for_of.expression, exp_type);
		let loop_statements = loop_builder.build(state, statements, for_of.initializer, exp);
		state.ast.list_push_list(result, loop_statements);

		// ORDER-CRITICAL: the varargs pass rebuilds the result list and re-appends the loop last,
		// so the checks must go on afterwards.
		result = transform_optimizable_var_args_for_of(state, node, result);
	}

	let checks = state.label_checks();
	state.ast.list_push_list(result, checks);
	state.exit_break_scope();
	result
}
