//! `call.go`: `expressions/transformCallExpression.ts`, the flatten and dispatch half of
//! `nodes/transformOptionalChain.ts` (the optional path is in [`crate::optionalchain`]), and
//! `util/convertToIndexableExpression.ts`, `util/expressionMightMutate.ts`,
//! `util/wrapReturnIfLuaTuple.ts` and `util/arrayBindingPatternContainsHoists.ts`.

use rotor_luau::{Array, BinaryExpression, IfExpression, Map, ParenthesizedExpression, Set, UnaryExpression};
use rotor_tsrs::ast as tsast;
use rotor_tsrs::checker::SignatureKind;
use rotor_tsrs::types::TypeCheck;

use crate::access::{
	add_one_if_array_type, transform_element_access_expression_inner, transform_property_access_expression_inner,
	validate_not_any_type,
};
use crate::control;
use crate::ensureorder::ensure_transform_order;
use crate::exports::is_symbol_mutable;
use crate::ismethod::{is_method, is_method_from_type};
use crate::prelude::*;
use crate::spreadhelpers::array_like_expression_contains_spread;
use crate::statements::check_variable_hoist;
use crate::types::{check, get_first_defined_symbol, is_possibly_type};
use crate::util::{is_identifier, nodes};
use crate::{diagnostics, extras};

/// `convertToIndexableExpression`: parenthesizes expressions that cannot be indexed or called.
pub fn convert_to_indexable_expression(ast: &mut Ast, expression: NodeId) -> NodeId {
	if ast.kind(expression).is_indexable_expression() {
		return expression;
	}
	ast.parenthesized(expression)
}

/// `expressionMightMutate`: whether the rendered expression could change value if statements ran
/// between its computation and its use. `node` is the originating TS expression, if known: reads
/// of constant identifiers cannot change.
pub fn expression_might_mutate(state: &mut State<'_>, expression: NodeId, node: Option<P<Node>>) -> bool {
	let ast = &state.ast;
	match ast.kind(expression) {
		// Assume temps are never reassigned after being returned.
		LuauKind::TemporaryIdentifier | LuauKind::FunctionExpression | LuauKind::VarArgsLiteral => return false,
		LuauKind::ParenthesizedExpression => {
			if let Some(parenthesized) = ast.get::<ParenthesizedExpression>(expression) {
				let inner = parenthesized.expression;
				return expression_might_mutate(state, inner, None);
			}
		}
		LuauKind::IfExpression => {
			if let Some(if_expression) = ast.get::<IfExpression>(expression) {
				let parts = [
					if_expression.condition,
					if_expression.expression,
					if_expression.alternative,
				];
				return parts.into_iter().any(|part| expression_might_mutate(state, part, None));
			}
		}
		LuauKind::BinaryExpression => {
			if let Some(binary) = ast.get::<BinaryExpression>(expression) {
				let (left, right) = (binary.left, binary.right);
				return expression_might_mutate(state, left, None) || expression_might_mutate(state, right, None);
			}
		}
		LuauKind::UnaryExpression => {
			if let Some(unary) = ast.get::<UnaryExpression>(expression) {
				let inner = unary.expression;
				return expression_might_mutate(state, inner, None);
			}
		}
		LuauKind::Array => {
			if let Some(array) = ast.get::<Array>(expression) {
				let members = ast.list_to_vec(array.members);
				return members
					.into_iter()
					.any(|member| expression_might_mutate(state, member, None));
			}
		}
		LuauKind::Set => {
			if let Some(set) = ast.get::<Set>(expression) {
				let members = ast.list_to_vec(set.members);
				return members
					.into_iter()
					.any(|member| expression_might_mutate(state, member, None));
			}
		}
		LuauKind::Map => {
			if let Some(map) = ast.get::<Map>(expression) {
				let fields = ast.list_to_vec(map.fields);
				return fields.into_iter().any(|field| {
					let Some(field) = state.ast.get::<rotor_luau::MapField>(field) else {
						return false;
					};
					let (index, value) = (field.index, field.value);
					expression_might_mutate(state, index, None) || expression_might_mutate(state, value, None)
				});
			}
		}
		_ => {}
	}

	if state.ast.kind(expression).is_simple_primitive() {
		return false;
	}

	if let Some(node) = node {
		let node = tsast::skip_downwards(node);
		if is_identifier(node)
			&& let Some(symbol) = state.ck().get_symbol_at_location(node)
			&& !is_symbol_mutable(state, symbol)
		{
			return false;
		}
	}

	// Identifier / ComputedIndexExpression / PropertyAccessExpression / CallExpression /
	// MethodCallExpression
	true
}

/// `arrayBindingPatternContainsHoists`: whether any identifier the pattern binds needs hoisting.
pub fn array_binding_pattern_contains_hoists(state: &mut State<'_>, array_binding_pattern: P<Node>) -> bool {
	for &element in array_binding_pattern.as_binding_pattern().elements.nodes() {
		// Anything else is an OmittedExpression; nested patterns get temp ids.
		if tsast::is_binding_element(element)
			&& let Some(name) = element.name()
			&& is_identifier(name)
			&& let Some(symbol) = state.ck().get_symbol_at_location(name)
		{
			// isHoisted is marked inside checkVariableHoist
			check_variable_hoist(state, name, symbol);
			if state.is_hoisted.get(&symbol).copied().unwrap_or(false) {
				return true;
			}
		}
	}
	false
}

/// `shouldWrapLuaTuple` (`wrapReturnIfLuaTuple.ts` L8-56): a `LuaTuple` call is packed
/// `{ f() }` unless the syntactic context consumes the multiple values.
pub fn should_wrap_lua_tuple(state: &mut State<'_>, node: P<Node>, expression: NodeId) -> bool {
	if !state.ast.kind(expression).is_call() {
		return true;
	}

	let child = tsast::skip_upwards(node);
	let Some(parent) = child.parent() else {
		return true;
	};
	let question_dot = node.question_dot_token();

	// `foo();`
	if tsast::is_expression_statement(parent) {
		return false;
	}

	// part of a for statement definition, except as the condition
	if tsast::is_for_statement(parent) && parent.as_for_statement().condition != Some(child) {
		return false;
	}

	// `const [a] = foo()`
	if tsast::is_variable_declaration(parent) {
		let name = parent.as_variable_declaration().name;
		if tsast::is_array_binding_pattern(name)
			&& !array_binding_pattern_contains_hoists(state, name)
			&& !array_like_expression_contains_spread(name)
			&& question_dot.is_none()
		{
			return false;
		}
	}

	// `[a] = foo()`
	if tsast::is_assignment_expression(parent, false) {
		let left = parent.as_binary_expression().left;
		if tsast::is_array_literal_expression(left)
			&& !array_like_expression_contains_spread(left)
			&& question_dot.is_none()
		{
			return false;
		}
	}

	// `foo()[n]`
	if tsast::is_element_access_expression(parent) && question_dot.is_none() && parent.question_dot_token().is_none() {
		return false;
	}

	// `return foo()`
	if tsast::is_return_statement(parent) {
		return false;
	}

	// `void foo()`
	if tsast::is_void_expression(parent) {
		return false;
	}

	true
}

/// `wrapReturnIfLuaTuple` (L58-63).
pub fn wrap_return_if_lua_tuple(state: &mut State<'_>, node: P<Node>, expression: NodeId) -> NodeId {
	let t = state.get_type(node);
	let t = state.ck().get_non_nullable_type(t);
	if check(state, TypeCheck::LuaTuple, t) && should_wrap_lua_tuple(state, node, expression) {
		let members = state.ast.list_from([expression]);
		return state.ast.array(members);
	}
	expression
}

/// `runCallMacro` (`transformCallExpression.ts` L21-83): transforms the arguments (expanding a
/// trailing tuple spread into temps; vararg macros reject spreads), pins mutable arguments and
/// the base expression when prereqs intervene, then runs the macro.
pub fn run_call_macro(
	macro_fn: impl FnOnce(&mut State<'_>, P<Node>, NodeId, Vec<NodeId>) -> NodeId,
	state: &mut State<'_>,
	node: P<Node>,
	mut expression: NodeId,
	node_arguments: &[P<Node>],
) -> NodeId {
	let call = node.as_call_expression();

	let mut args: Vec<NodeId> = Vec::new();
	let prereqs = state.capture_statements(|state| {
		args.extend(ensure_transform_order(state, node_arguments));
		if let Some(&last_arg) = node_arguments.last()
			&& tsast::is_spread_element(last_arg)
		{
			let callee_type = state.get_type(call.expression);
			let signatures = state.ck().get_signatures_of_type(callee_type, SignatureKind::Call);
			let Some(&signature) = signatures.first() else {
				state.ice("transformer: runCallMacro: macro has no call signature");
				return;
			};
			let parameters = signature.parameters();
			let Some(&last_parameter) = parameters.last() else {
				// Go indexes `parameters[len-1]` and panics on an empty list
				state.ice("transformer: runCallMacro: macro signature has no parameters");
				return;
			};
			let last_parameter = last_parameter.value_declaration();
			if let Some(last_parameter) = last_parameter
				&& tsast::is_parameter_declaration(last_parameter)
				&& last_parameter.as_parameter_declaration().dot_dot_dot_token().is_some()
			{
				state.diags.add(diagnostics::no_var_args_macro_spread(last_arg));
				return;
			}

			// .expression for the tuple type; `lastArg` would give the element type
			let Some(spread_expression) = last_arg.expression() else {
				return;
			};
			let tuple_arg_type = state.get_type(spread_expression);
			// Excluding vararg macros, TypeScript ensured the spread is from a tuple type.
			if !tuple_arg_type.is_tuple_type() {
				state.ice("transformer: runCallMacro spread argument is not a tuple type");
				return;
			}
			let argument_count = tuple_arg_type.target_tuple_type().element_flags().len();

			let Some(spread) = args.pop() else {
				return;
			};
			let temp_ids = state.ast.new_list::<element::AnyIdentifier>();
			for index in args.len()..argument_count {
				let temp_id = state.ast.temp_id(format!("spread{index}"));
				args.push(temp_id);
				state.ast.list_push(temp_ids, temp_id);
			}
			let declaration = state.ast.variable_declaration(temp_ids, Some(spread.into()));
			state.prereq(declaration);
		}

		for index in 0..args.len() {
			// spread-expanded args extend past the nodes; upstream indexes past the end
			let node_arg = node_arguments.get(index).copied();
			if expression_might_mutate(state, args[index], node_arg) {
				let mut name = crate::valuetoidstr::value_to_id_str(&state.ast, args[index]);
				if name.is_empty() {
					name = format!("arg{index}");
				}
				args[index] = state.push_to_var(Some(args[index]), &name);
			}
		}
	});

	let mut node_expression = call.expression;
	if tsast::is_property_access_expression(node_expression) || tsast::is_element_access_expression(node_expression) {
		node_expression = node_expression.expression().unwrap_or(node_expression);
	}

	if state.ast.list_is_non_empty(prereqs) && expression_might_mutate(state, expression, Some(node_expression)) {
		let mut name = crate::valuetoidstr::value_to_id_str(&state.ast, expression);
		if name.is_empty() {
			"exp".clone_into(&mut name);
		}
		expression = state.push_to_var(Some(expression), &name);
	}
	state.prereq_list(prereqs);

	let result = macro_fn(state, node, expression, args);
	wrap_return_if_lua_tuple(state, node, result)
}

/// `fixVoidArgumentsForRobloxFunctions` (`transformCallExpression.ts` L96-113): parenthesizes
/// possibly-undefined call arguments of Roblox API functions, truncating multiple returns so C
/// functions like `tonumber()` do not error on zero values.
fn fix_void_arguments_for_roblox_functions(
	state: &mut State<'_>,
	expression_type: P<Type>,
	args: &mut [NodeId],
	node_arguments: &[P<Node>],
) {
	if !is_possibly_type(state, expression_type, &[TypeCheck::Roblox]) {
		return;
	}
	for (index, &node_arg) in node_arguments.iter().enumerate() {
		if tsast::is_call_expression(node_arg) {
			let t = state.get_type(node_arg);
			if is_possibly_type(state, t, &[TypeCheck::Undefined]) {
				args[index] = state.ast.parenthesized(args[index]);
			}
		}
	}
}

fn try_transform_optimizable_var_args_size_call(state: &mut State<'_>, node: P<Node>) -> Option<NodeId> {
	let call = node.as_call_expression();
	if !tsast::is_property_access_expression(call.expression) {
		return None;
	}
	let property_access = call.expression.as_property_access_expression();
	if property_access.name.text() != "size" {
		return None;
	}
	let data = state.get_optimizable_var_args_data(property_access.expression)?;
	if !data.is_optimizable {
		return None;
	}
	if data.uses_length_cache() {
		return data.length_id;
	}
	Some(control::create_var_args_length_select(&mut state.ast))
}

/// `transformCallExpressionInner` (L115-155): `f(...)`.
pub fn transform_call_expression_inner(
	state: &mut State<'_>,
	node: P<Node>,
	mut expression: NodeId,
	node_arguments: &[P<Node>],
) -> NodeId {
	if tsast::is_import_call(node) {
		return extras::transform_import_expression(state, node);
	}

	let call = node.as_call_expression();

	// a in a()
	validate_not_any_type(state, call.expression);

	if rotor_tsrs::ast::is_super_call(node) {
		// super(...) -> super.constructor(self, <args>)
		let callee = convert_to_indexable_expression(&mut state.ast, expression);
		let constructor = state.ast.property_access(callee, "constructor");
		let self_id = state.ast.global_id("self");
		let mut args = vec![self_id];
		args.extend(ensure_transform_order(state, call.arguments.nodes()));
		let args = state.ast.list_from(args);
		return state.ast.call(constructor, args);
	}
	if let Some(optimized) = try_transform_optimizable_var_args_size_call(state, node) {
		return optimized;
	}

	let callee_type = state.get_type(call.expression);
	let expression_type = state.ck().get_non_optional_type(callee_type);
	if let Some(symbol) = get_first_defined_symbol(state, expression_type)
		&& let Some(entry) = state.macros().get_call_macro(symbol)
	{
		return run_call_macro(entry.macro_fn, state, node, expression, node_arguments);
	}

	let mut args = Vec::new();
	let prereqs = state.capture_statements(|state| args.extend(ensure_transform_order(state, node_arguments)));
	fix_void_arguments_for_roblox_functions(state, expression_type, &mut args, node_arguments);

	if state.ast.list_is_non_empty(prereqs) && expression_might_mutate(state, expression, Some(call.expression)) {
		expression = state.push_to_var(Some(expression), "fn");
	}
	state.prereq_list(prereqs);

	if is_method_from_type(state, call.expression, expression_type) {
		let nil = state.ast.nil();
		args.insert(0, nil);
	}

	let callee = convert_to_indexable_expression(&mut state.ast, expression);
	let args = state.ast.list_from(args);
	let call_expression = state.ast.call(callee, args);

	wrap_return_if_lua_tuple(state, node, call_expression)
}

/// `transformPropertyCallExpressionInner` (L157-215): `a.b(...)`. `expression` is the property
/// access; `base_expression` the transformed `a`.
pub fn transform_property_call_expression_inner(
	state: &mut State<'_>,
	node: P<Node>,
	expression: P<Node>,
	mut base_expression: NodeId,
	name: &str,
	node_arguments: &[P<Node>],
) -> NodeId {
	let property_access = expression.as_property_access_expression();
	let call = node.as_call_expression();

	// a in a.b()
	validate_not_any_type(state, property_access.expression);
	// a.b in a.b()
	validate_not_any_type(state, call.expression);

	if tsast::is_super_property(expression) {
		let mut args = ensure_transform_order(state, call.arguments.nodes());
		if is_method(state, expression) {
			let self_id = state.ast.global_id("self");
			args.insert(0, self_id);
		}
		let base = convert_to_indexable_expression(&mut state.ast, base_expression);
		let callee = state.ast.property_access(base, property_access.name.text());
		let args = state.ast.list_from(args);
		return state.ast.call(callee, args);
	}
	if let Some(optimized) = try_transform_optimizable_var_args_size_call(state, node) {
		return optimized;
	}

	let callee_type = state.get_type(call.expression);
	let expression_type = state.ck().get_non_optional_type(callee_type);
	if let Some(symbol) = get_first_defined_symbol(state, expression_type)
		&& let Some(entry) = state.get_property_call_macro(symbol)
	{
		return crate::macros::run_property_call_macro(entry, state, node, base_expression, node_arguments);
	}

	let mut args = Vec::new();
	let prereqs = state.capture_statements(|state| args.extend(ensure_transform_order(state, node_arguments)));
	fix_void_arguments_for_roblox_functions(state, expression_type, &mut args, node_arguments);

	if state.ast.list_is_non_empty(prereqs)
		&& expression_might_mutate(state, base_expression, Some(property_access.expression))
	{
		base_expression = state.push_to_var(Some(base_expression), "");
	}
	state.prereq_list(prereqs);

	let call_expression = if is_method(state, expression) {
		// a Luau keyword name needs a property access with an explicit self argument
		if rotor_luau::is_valid_identifier(name) {
			let base = convert_to_indexable_expression(&mut state.ast, base_expression);
			let args = state.ast.list_from(args);
			state.ast.method_call(name, base, args)
		} else {
			base_expression = state.push_to_var_if_complex(base_expression, "");
			args.insert(0, base_expression);
			let base = convert_to_indexable_expression(&mut state.ast, base_expression);
			let callee = state.ast.property_access(base, name);
			let args = state.ast.list_from(args);
			state.ast.call(callee, args)
		}
	} else {
		// the property access wraps the identifier if necessary
		let base = convert_to_indexable_expression(&mut state.ast, base_expression);
		let callee = state.ast.property_access(base, name);
		let args = state.ast.list_from(args);
		state.ast.call(callee, args)
	};

	wrap_return_if_lua_tuple(state, node, call_expression)
}

/// `transformElementCallExpressionInner` (L217-280): `a[b](...)`. The index is ordered with the
/// arguments, and `a[b]` never uses `:` sugar.
pub fn transform_element_call_expression_inner(
	state: &mut State<'_>,
	node: P<Node>,
	expression: P<Node>,
	mut base_expression: NodeId,
	argument_expression: P<Node>,
	node_arguments: &[P<Node>],
) -> NodeId {
	let element_access = expression.as_element_access_expression();
	let call = node.as_call_expression();

	// a in a[b]()
	validate_not_any_type(state, element_access.expression);
	// b in a[b]()
	validate_not_any_type(state, element_access.argument_expression);
	// a[b] in a[b]()
	validate_not_any_type(state, call.expression);

	if tsast::is_super_property(expression) {
		let index = crate::dispatch::transform_expression(state, element_access.argument_expression);
		let mut args = ensure_transform_order(state, call.arguments.nodes());
		if is_method(state, expression) {
			let self_id = state.ast.global_id("self");
			args.insert(0, self_id);
		}
		let base = convert_to_indexable_expression(&mut state.ast, base_expression);
		let callee = state.ast.computed_index(base, index);
		let args = state.ast.list_from(args);
		return state.ast.call(callee, args);
	}
	if let Some(optimized) = try_transform_optimizable_var_args_size_call(state, node) {
		return optimized;
	}

	let callee_type = state.get_type(call.expression);
	let expression_type = state.ck().get_non_optional_type(callee_type);
	if let Some(symbol) = get_first_defined_symbol(state, expression_type)
		&& let Some(entry) = state.get_property_call_macro(symbol)
	{
		return crate::macros::run_property_call_macro(entry, state, node, base_expression, node_arguments);
	}

	let mut ordered = Vec::new();
	let prereqs = state.capture_statements(|state| {
		let mut all = vec![argument_expression];
		all.extend_from_slice(node_arguments);
		ordered.extend(ensure_transform_order(state, &all));
	});
	let Some((&argument, rest)) = ordered.split_first() else {
		state.ice("transformer: transformElementCallExpressionInner: no index expression");
		return state.ast.none();
	};
	let mut args = rest.to_vec();

	fix_void_arguments_for_roblox_functions(state, expression_type, &mut args, node_arguments);

	if state.ast.list_is_non_empty(prereqs)
		&& expression_might_mutate(state, base_expression, Some(element_access.expression))
	{
		base_expression = state.push_to_var(Some(base_expression), "");
	}
	state.prereq_list(prereqs);

	if is_method(state, expression) {
		base_expression = state.push_to_var_if_complex(base_expression, "");
		args.insert(0, base_expression);
	}

	let base = convert_to_indexable_expression(&mut state.ast, base_expression);
	let object_type = state.get_type(element_access.expression);
	let object_type = state.ck().get_non_optional_type(object_type);
	let index = add_one_if_array_type(state, object_type, argument);
	let callee = state.ast.computed_index(base, index);
	let args = state.ast.list_from(args);
	let call_expression = state.ast.call(callee, args);

	wrap_return_if_lua_tuple(state, node, call_expression)
}

/// `transformCallExpression` (L282-284): every call routes through the optional chain walker.
pub fn transform_call_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	crate::optionalchain::transform_optional_chain(state, node)
}

/// The kind of an optional chain link (`OptionalChainItem`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainItemKind {
	PropertyAccess,
	ElementAccess,
	Call,
	PropertyCall,
	ElementCall,
}

/// `chainItem` (`OptionalChainItem`, L24-66). Upstream's eager type snapshots are never read and
/// are omitted.
#[derive(Clone, Debug)]
pub struct ChainItem {
	pub(crate) kind: ChainItemKind,
	/// The property access, element access or call expression.
	pub(crate) node: P<Node>,
	pub(crate) optional: bool,
	/// `PropertyAccess` / `PropertyCall`
	pub(crate) name: &'static str,
	/// `ElementAccess` / `ElementCall`
	pub(crate) argument_expression: Option<P<Node>>,
	/// The inner access of a compound call (`PropertyCall` / `ElementCall`).
	pub(crate) expression: Option<P<Node>>,
	pub(crate) call_optional: bool,
	pub(crate) args: &'static [P<Node>],
}

impl ChainItem {
	const fn new(kind: ChainItemKind, node: P<Node>, optional: bool) -> Self {
		Self {
			kind,
			node,
			optional,
			name: "",
			argument_expression: None,
			expression: None,
			call_optional: false,
			args: &[],
		}
	}
}

/// `flattenOptionalChain` (L135-162): walks down `.expression` collecting chain items; a call
/// whose callee (skipped downwards) is an access becomes a compound item.
pub fn flatten_optional_chain(mut expression: P<Node>) -> (Vec<ChainItem>, P<Node>) {
	let mut chain = Vec::new(); // built outer to inner, reversed below (upstream unshifts)
	loop {
		if tsast::is_property_access_expression(expression) {
			let property_access = expression.as_property_access_expression();
			let mut item = ChainItem::new(
				ChainItemKind::PropertyAccess,
				expression,
				expression.question_dot_token().is_some(),
			);
			item.name = property_access.name.text();
			chain.push(item);
			expression = property_access.expression;
		} else if tsast::is_element_access_expression(expression) {
			let element_access = expression.as_element_access_expression();
			let mut item = ChainItem::new(
				ChainItemKind::ElementAccess,
				expression,
				expression.question_dot_token().is_some(),
			);
			item.argument_expression = Some(element_access.argument_expression);
			chain.push(item);
			expression = element_access.expression;
		} else if tsast::is_call_expression(expression) {
			// this is a bit of a mess..
			let call = expression.as_call_expression();
			let call_optional = expression.question_dot_token().is_some();
			let sub_expression = tsast::skip_downwards(call.expression);
			if tsast::is_property_access_expression(sub_expression) {
				let property_access = sub_expression.as_property_access_expression();
				let mut item = ChainItem::new(
					ChainItemKind::PropertyCall,
					expression,
					sub_expression.question_dot_token().is_some(),
				);
				item.expression = Some(sub_expression);
				item.name = property_access.name.text();
				item.call_optional = call_optional;
				item.args = call.arguments.nodes();
				chain.push(item);
				expression = property_access.expression;
			} else if tsast::is_element_access_expression(sub_expression) {
				let element_access = sub_expression.as_element_access_expression();
				let mut item = ChainItem::new(
					ChainItemKind::ElementCall,
					expression,
					sub_expression.question_dot_token().is_some(),
				);
				item.expression = Some(sub_expression);
				item.argument_expression = Some(element_access.argument_expression);
				item.call_optional = call_optional;
				item.args = call.arguments.nodes();
				chain.push(item);
				expression = element_access.expression;
			} else {
				let mut item = ChainItem::new(ChainItemKind::Call, expression, call_optional);
				item.args = call.arguments.nodes();
				chain.push(item);
				expression = sub_expression;
			}
		} else {
			break;
		}
	}
	chain.reverse();
	(chain, expression)
}

/// `transformChainItem` (L164-190).
pub fn transform_chain_item(state: &mut State<'_>, base_expression: NodeId, item: &ChainItem) -> NodeId {
	let argument = || item.argument_expression.unwrap_or(item.node);
	let inner = || item.expression.unwrap_or(item.node);
	match item.kind {
		ChainItemKind::PropertyAccess => {
			transform_property_access_expression_inner(state, item.node, base_expression, item.name)
		}
		ChainItemKind::ElementAccess => {
			transform_element_access_expression_inner(state, item.node, base_expression, argument())
		}
		ChainItemKind::Call => transform_call_expression_inner(state, item.node, base_expression, item.args),
		ChainItemKind::PropertyCall => {
			transform_property_call_expression_inner(state, item.node, inner(), base_expression, item.name, item.args)
		}
		ChainItemKind::ElementCall => {
			transform_element_call_expression_inner(state, item.node, inner(), base_expression, argument(), item.args)
		}
	}
}

/// The arguments of a call node, empty when absent.
pub fn call_arguments(node: P<Node>) -> &'static [P<Node>] {
	nodes(node.argument_list())
}
