//! `optionalchain.go`: the optional path of `nodes/transformOptionalChain.ts` (L192-348).

use rotor_tsrs::ast as tsast;

use crate::access::add_one_if_array_type;
use crate::call::{
	ChainItem, ChainItemKind, convert_to_indexable_expression, flatten_optional_chain, transform_chain_item,
	wrap_return_if_lua_tuple,
};
use crate::conditional::is_used_as_statement;
use crate::diagnostics;
use crate::dispatch::transform_expression;
use crate::ensureorder::ensure_transform_order;
use crate::ismethod::is_method;
use crate::macros;
use crate::prelude::*;
use crate::types::get_first_defined_symbol;

/// `createOrSetTempId` (L192-217): creates the chain temp on first use, named after the outermost
/// node's variable declaration (else `result`), or reassigns it, skipping self-assignment.
fn create_or_set_temp_id(state: &mut State<'_>, temp_id: Option<NodeId>, expression: NodeId, node: P<Node>) -> NodeId {
	match temp_id {
		None => {
			let mut name = "result";
			if let Some(parent) = node.parent()
				&& tsast::is_variable_declaration(parent)
			{
				let declaration_name = parent.as_variable_declaration().name;
				if declaration_name.kind() == Kind::Identifier {
					name = declaration_name.text();
				}
			}
			state.push_to_var(Some(expression), name)
		}
		Some(temp_id) => {
			if temp_id != expression {
				let assignment = state.ast.assignment(temp_id, AssignmentOperator::Assign, expression);
				state.prereq(assignment);
			}
			temp_id
		}
	}
}

/// `createNilCheck` (L219-225): `if tempId ~= nil then <statements> end`.
fn create_nil_check(state: &mut State<'_>, temp_id: NodeId, statements: Statements, token: Option<P<Node>>) -> NodeId {
	let nil = state.ast.nil();
	let condition = state.ast.binary(temp_id, BinaryOperator::NotEqual, nil);
	if let Some(token) = token {
		state.set_operator_source_position(condition, token);
	}
	let else_body = state.ast.new_list::<element::Statement>();
	state.ast.if_statement(condition, statements, Some(else_body.into()))
}

/// `isCompoundCall` (L227-229).
const fn is_compound_call(item: &ChainItem) -> bool {
	matches!(item.kind, ChainItemKind::PropertyCall | ChainItemKind::ElementCall)
}

/// `transformOptionalChainInner` (L231-348). One temp threads the whole chain: created at the
/// first optional link and reassigned inside each nil-check block; deeper links nest inside the
/// current block through the inner capture. The statement order the two-level capture produces
/// is load-bearing.
pub fn transform_optional_chain_inner(
	state: &mut State<'_>,
	chain: &[ChainItem],
	mut base_expression: NodeId,
	mut temp_id: Option<NodeId>,
	index: usize,
) -> NodeId {
	let Some(item) = chain.get(index) else {
		return base_expression;
	};
	let Some(last) = chain.last() else {
		return base_expression;
	};
	let last_node = last.node;
	if !(item.optional || (is_compound_call(item) && item.call_optional)) {
		let next = transform_chain_item(state, base_expression, item);
		return transform_optional_chain_inner(state, chain, next, temp_id, index + 1);
	}

	let mut is_method_call = false;
	let mut is_super_call = false;
	let mut self_param = None;
	let inner_expression = item.expression.unwrap_or(item.node);

	if is_compound_call(item) {
		is_method_call = is_method(state, inner_expression);
		is_super_call = tsast::is_super_property(inner_expression);

		// a.b?.() on a real method binds the receiver to `_self` before any nil check; super
		// properties use the `self` global instead.
		if item.call_optional && is_method_call && !is_super_call {
			let param = state.push_to_var(Some(base_expression), "self");
			self_param = Some(param);
			base_expression = param;
		}

		if item.optional {
			let id = create_or_set_temp_id(state, temp_id, base_expression, last_node);
			temp_id = Some(id);
			base_expression = id;
		}

		if item.call_optional {
			if item.kind == ChainItemKind::PropertyCall {
				let base = convert_to_indexable_expression(&mut state.ast, base_expression);
				base_expression = state.ast.property_access(base, item.name);
			} else {
				let object = inner_expression.as_element_access_expression().expression;
				let object_type = state.get_type(object);
				let base = convert_to_indexable_expression(&mut state.ast, base_expression);
				let argument = item.argument_expression.unwrap_or(item.node);
				let index_expression = transform_expression(state, argument);
				let index_expression = add_one_if_array_type(state, object_type, index_expression);
				base_expression = state.ast.computed_index(base, index_expression);
			}
		}
	}

	// capture so we can wrap later if necessary
	let mut final_temp = temp_id;
	let (result, prereq_statements) = state.capture(|state| {
		let id = create_or_set_temp_id(state, final_temp, base_expression, last_node);
		final_temp = Some(id);

		let (new_value, if_statements) = state.capture(|state| {
			let new_expression = if is_compound_call(item) && item.call_optional {
				let callee_type = state.get_type(item.node.as_call_expression().expression);
				let expression_type = state.ck().get_non_optional_type(callee_type);
				// registration is what matters: a registered macro method cannot be optionally
				// called
				if let Some(symbol) = get_first_defined_symbol(state, expression_type)
					&& state.get_property_call_macro(symbol).is_some()
				{
					state.diags.add(diagnostics::no_optional_macro_call(item.node));
					return state.ast.none();
				}

				let mut args = ensure_transform_order(state, item.args);
				if is_method_call {
					let receiver = if is_super_call {
						state.ast.global_id("self")
					} else {
						self_param.unwrap_or(id)
					};
					args.insert(0, receiver);
				}
				let args = state.ast.list_from(args);
				let call = state.ast.call(id, args);
				wrap_return_if_lua_tuple(state, item.node, call)
			} else {
				transform_chain_item(state, id, item)
			};
			transform_optional_chain_inner(state, chain, new_expression, Some(id), index + 1)
		});

		let is_used = !state.ast.kind(new_value).is_none() && !is_used_as_statement(item.node);

		if id != new_value && is_used {
			let assignment = state.ast.assignment(id, AssignmentOperator::Assign, new_value);
			state.ast.list_push(if_statements, assignment);
		} else if state.ast.kind(new_value).is_call() {
			let statement = state.ast.call_statement(new_value);
			state.ast.list_push(if_statements, statement);
		}

		let token = if is_compound_call(item) && !item.call_optional {
			inner_expression.question_dot_token()
		} else {
			item.node.question_dot_token()
		};
		let nil_check = create_nil_check(state, id, if_statements, token);
		state.prereq(nil_check);

		if is_used { id } else { state.ast.none() }
	});

	if is_compound_call(item) && item.optional && item.call_optional {
		// a?.b?.(): a second nil check wraps everything captured above, the inner nil check for
		// the call included.
		let id = final_temp.unwrap_or(result);
		let nil_check = create_nil_check(state, id, prereq_statements, inner_expression.question_dot_token());
		state.prereq(nil_check);
	} else {
		state.prereq_list(prereq_statements);
	}

	result
}

/// `transformOptionalChain` (L350-356): every property access, element access and call routes
/// here; fully non-optional chains degenerate to an inner-to-outer fold.
pub fn transform_optional_chain(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let (chain, expression) = flatten_optional_chain(node);
	// rotor extensions: the compile-time macros consume their base identifier before the
	// expression transform sees it ($env, then $nameof, $keys and $file).
	if let Some(result) = macros::intercept_env_chain(state, &chain, expression) {
		return result;
	}
	if let Some(result) = macros::intercept_nameof_chain(state, &chain, expression) {
		return result;
	}
	if let Some(result) = macros::intercept_keys_chain(state, &chain, expression) {
		return result;
	}
	if let Some(result) = macros::intercept_file_chain(state, &chain, expression) {
		return result;
	}
	let base = transform_expression(state, expression);
	transform_optional_chain_inner(state, &chain, base, None, 0)
}
