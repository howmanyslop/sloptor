//! `functions.go`: `statements/transformFunctionDeclaration.ts`,
//! `expressions/transformFunctionExpression.ts` and `nodes/transformMethodDeclaration.ts`.

use rotor_luau::{FunctionDeclaration, FunctionExpression, Map, MethodDeclaration, StringLiteral, TypeGeneric};
use rotor_tsrs::ast::{self as tsast, ModifierFlags};

use super::namedfunction::emit_named_function_expression;
use super::parameters::transform_parameters;
use crate::diagnostics;
use crate::extras::wrap_statements_as_generator;
use crate::identifier::{transform_identifier_defined, validate_identifier};
use crate::ismethod::is_method;
use crate::literals::transform_property_name;
use crate::luau_types::{
	get_luau_local_type, luau_typed_binding, pop_type_generics, push_type_generics, signature_generics,
	signature_return_type, signature_var_args_type,
};
use crate::pointer::{MapPointer, assign_to_map_pointer};
use crate::prelude::*;
use crate::shared::{register_optimizable_var_args_for_function, this_parameter, type_generic_names};
use crate::statementlist::transform_statement_list;
use crate::statements::transform_return_statement_inner;

/// `transformedFunctionBody`
pub struct TransformedFunctionBody {
	pub parameters: List<element::AnyIdentifier>,
	pub statements: Statements,
	pub has_dot_dot_dot: bool,
	/// The function's own type parameters (luauTypes or `//!native`), rendered as `<K, V>` on the
	/// emitted function.
	pub generics: Vec<TypeGeneric>,
	/// The return annotation, if any.
	pub return_type: Option<NodeId>,
	/// The `...: T` annotation of the rest parameter (luauTypes only), if any.
	pub var_args_type: Option<NodeId>,
}

/// Go's `fn.Generics = ...; fn.ReturnType = ...; fn.VarArgsType = ...` field writes on a
/// function expression or declaration (no adoption, as in Go).
pub fn set_function_signature(
	ast: &mut Ast,
	node: NodeId,
	generics: Vec<TypeGeneric>,
	return_type: Option<NodeId>,
	var_args_type: Option<NodeId>,
) {
	if let Some(function) = ast.get_mut::<FunctionExpression>(node) {
		function.generics = generics;
		function.return_type = return_type;
		function.var_args_type = var_args_type;
	} else if let Some(function) = ast.get_mut::<FunctionDeclaration>(node) {
		function.generics = generics;
		function.return_type = return_type;
		function.var_args_type = var_args_type;
	}
}

/// `transformFunctionDeclaration` (`transformFunctionDeclaration.ts` L13-76).
///
/// - bodiless overload signatures emit nothing;
/// - anonymous functions are only legal as `export default` and emit under the literal name
///   `default`;
/// - localize: an export-default anonymous function is always localized; a named function is
///   localized unless its symbol was hoisted (a `local f` already emitted by
///   `createHoistDeclaration`, e.g. mutual recursion), in which case the non-local
///   `function f() end` assigns into it.
pub fn transform_function_declaration(state: &mut State<'_>, node: P<Node>) -> Statements {
	// overload signatures emit nothing
	if node.body().is_none() {
		return state.ast.new_list();
	}

	// NOTE ts.hasSyntacticModifier matches ANY selected flag, so a plain `export function f` also
	// sets isExportDefault: harmless upstream and here, because the named branch below overrides
	// localize and anonymous declarations require a real `export default`. Ported verbatim.
	let is_export_default = tsast::has_syntactic_modifier(node, ModifierFlags::ExportDefault);

	let name_node = node.name();
	if name_node.is_none() && !is_export_default {
		state.ice("transformer: anonymous FunctionDeclaration must be export default"); // upstream assert
		return state.ast.new_list();
	}

	let mut name = if let Some(name_node) = name_node {
		validate_identifier(state, name_node);
		transform_identifier_defined(state, name_node)
	} else {
		state.ast.id("default")
	};

	let mut body = transform_function_body(state, node);

	let localize = if let Some(name_node) = name_node {
		let Some(symbol) = state.ck().get_symbol_at_location(name_node) else {
			state.ice("transformer: FunctionDeclaration name has no symbol"); // upstream assert
			return state.ast.new_list();
		};
		!state.is_hoisted.get(&symbol).copied().unwrap_or(false)
	} else {
		is_export_default
	};

	let is_async = tsast::has_syntactic_modifier(node, ModifierFlags::Async);

	if node.as_function_declaration().asterisk_token().is_some() {
		if is_async {
			state.diags.add(diagnostics::no_async_generator_functions(node));
		}
		body.statements = wrap_statements_as_generator(state, node, body.statements);
	}

	// The async path replaces the FunctionDeclaration emit entirely: `local f = TS.async(function()
	// ... end)` when localized, `f = TS.async(...)` when hoisted (the hoist machinery emitted the
	// `local f` header at the premature use; async function declarations are hoist-sensitive). The
	// generator declarations stay real FunctionDeclarations whose body is the TS.generator return.
	// With luauTypes on, the local carries the full `(...) -> Promise<T>` type, since TS.async
	// itself is `any` to Luau.
	if is_async {
		let callback = state
			.ast
			.function_expression(body.parameters, body.has_dot_dot_dot, body.statements);
		set_function_signature(
			&mut state.ast,
			callback,
			body.generics,
			body.return_type,
			body.var_args_type,
		);
		state.set_node_source_position(callback, node);
		let runtime_async = state.runtime_lib(Some(node), "async");
		let args = state.ast.list_from([callback]);
		let right = state.ast.call(runtime_async, args);
		if localize {
			if state.is_luau_typed
				&& let Some(name_node) = name_node
				&& let Some(symbol) = state.ck().get_symbol_at_location(name_node)
			{
				let local_type = get_luau_local_type(state, name_node, Some(right));
				name = luau_typed_binding(state, symbol, name, local_type);
			}
			let declaration = state.ast.variable_declaration(name, Some(right.into()));
			return state.ast.list_from([declaration]);
		}
		let assignment = state.ast.assignment(name, AssignmentOperator::Assign, right);
		return state.ast.list_from([assignment]);
	}

	let declaration =
		state
			.ast
			.function_declaration(localize, name, body.parameters, body.has_dot_dot_dot, body.statements);
	set_function_signature(
		&mut state.ast,
		declaration,
		body.generics,
		body.return_type,
		body.var_args_type,
	);
	state.ast.list_from([declaration])
}

/// `transformFunctionBody`: a function's parameters and body. Its own type parameters are in
/// scope while both are transformed, so a nested annotation can name them (and the return type
/// can still use them).
pub fn transform_function_body(state: &mut State<'_>, node: P<Node>) -> TransformedFunctionBody {
	let generics = signature_generics(state, node);
	push_type_generics(state, type_generic_names(&generics), false);

	let parameters = transform_parameters(state, node);
	let statements = parameters.statements;
	let rest_param = register_optimizable_var_args_for_function(state, node, parameters.var_args_data.as_ref());

	match node.body() {
		Some(body) if tsast::is_block(body) => {
			let body_statements = transform_statement_list(state, Some(body), body.statements(), None);
			state.ast.list_push_list(statements, body_statements);
		}
		Some(body) => {
			let mut return_statements = None;
			let prereqs = state.capture_statements(|state| {
				return_statements = Some(transform_return_statement_inner(state, body));
			});
			let return_statements = return_statements.unwrap_or_else(|| state.ast.new_list());
			state.set_statement_list_source_positions(body, &[prereqs, return_statements]);
			state.ast.list_push_list(statements, prereqs);
			state.ast.list_push_list(statements, return_statements);
		}
		None => state.ice("transformer: function has no body"),
	}
	if let Some(rest_param) = rest_param {
		state.unregister_optimizable_var_args(rest_param);
	}

	let return_type = signature_return_type(state, node);
	let var_args_type = signature_var_args_type(state, node);
	pop_type_generics(state);

	TransformedFunctionBody {
		parameters: parameters.parameters,
		statements,
		has_dot_dot_dot: parameters.has_dot_dot_dot,
		generics,
		return_type,
		var_args_type,
	}
}

/// `transformFunctionExpression` (`transformFunctionExpression.ts` L11-47): function expressions
/// and arrows share one transform. A named function expression is lifted to a local function
/// declaration in any expression position: the prereq machinery already places the declaration
/// where the expression is evaluated, so short-circuit operands and conditional arms stay
/// conditional. Async names keep the declaration (the Luau debug name) and assign
/// `name = TS.async(name)`. Generator names lift like a generator declaration. Arrow expression
/// bodies reuse the full return transform with prereqs captured into the function body.
pub fn transform_function_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	if tsast::is_function_expression(node)
		&& let Some(name) = node.name()
	{
		validate_identifier(state, name);
		let identifier = state.ast.exact_temp_id(name.text());
		let statements = emit_named_function_expression(state, node, identifier, true, Some(identifier));
		state.prereq_list(statements);
		return identifier;
	}

	let mut body = transform_function_body(state, node);

	let is_async = tsast::has_syntactic_modifier(node, ModifierFlags::Async);

	let asterisk_token = if tsast::is_function_expression(node) {
		node.as_function_expression().asterisk_token()
	} else {
		None
	};
	if asterisk_token.is_some() {
		if is_async {
			state.diags.add(diagnostics::no_async_generator_functions(node));
		}
		body.statements = wrap_statements_as_generator(state, node, body.statements);
	}

	let function_expression = state
		.ast
		.function_expression(body.parameters, body.has_dot_dot_dot, body.statements);
	set_function_signature(
		&mut state.ast,
		function_expression,
		body.generics,
		body.return_type,
		body.var_args_type,
	);
	let mut expression = function_expression;
	state.set_node_source_position(expression, node);

	if is_async {
		let runtime_async = state.runtime_lib(Some(node), "async");
		let args = state.ast.list_from([expression]);
		expression = state.ast.call(runtime_async, args);
	}

	expression
}

/// `transformMethodDeclarationWithReceiver`: `transformMethodDeclaration.ts` (L14-106), for
/// object-literal methods (inline-map pointer) and class methods (identifier pointer), with the
/// luauTypes type of `self` supplied by the caller. Only a written `this` parameter beats the
/// receiver.
///
/// With luauTypes on, a method that would be `function obj:m(...)` is emitted in dot form with an
/// explicit leading `self`, `function obj.m(self: T, ...)`.
#[expect(
	clippy::too_many_lines,
	reason = "one block per upstream section, kept in upstream order"
)]
pub fn transform_method_declaration_with_receiver(
	state: &mut State<'_>,
	node: P<Node>,
	pointer: &MapPointer,
	receiver: Option<NodeId>,
) -> Statements {
	let result = state.ast.new_list();

	let Some(body) = node.body() else {
		return state.ast.new_list();
	};

	let Some(name_node) = node.name() else {
		state.ice("transformer: MethodDeclaration has no name"); // upstream assert
		return state.ast.new_list();
	};
	if name_node.kind() == Kind::PrivateIdentifier {
		state.diags.add(diagnostics::no_private_identifier(name_node));
		return state.ast.new_list();
	}

	// A method is its own function: its type parameters are in scope for its parameters, body,
	// and return type.
	let luau_generics = signature_generics(state, node);
	push_type_generics(state, type_generic_names(&luau_generics), false);

	let transformed = transform_parameters(state, node);
	let parameters = transformed.parameters;
	let mut statements = transformed.statements;
	let has_dot_dot_dot = transformed.has_dot_dot_dot;
	if let Some(receiver) = receiver
		&& state.luau_typed_signatures()
		&& let Some(head) = state.ast.list_head(parameters)
		&& this_parameter(node).is_none()
		&& is_method(state, node)
	{
		let self_id = state.ast.link_value(head);
		state.ast.set_binding_type(self_id, Some(receiver));
	}
	let rest_param = register_optimizable_var_args_for_function(state, node, transformed.var_args_data.as_ref());
	let body_statements = transform_statement_list(state, Some(body), body.statements(), None);
	state.ast.list_push_list(statements, body_statements);
	if let Some(rest_param) = rest_param {
		state.unregister_optimizable_var_args(rest_param);
	}
	let return_type = signature_return_type(state, node);
	let var_args_type = signature_var_args_type(state, node);

	let mut name = transform_property_name(state, name_node);

	// Decorator key pinning (upstream L36-49): a decorated method (or a method with decorated
	// parameters) records the object key the decorator transforms re-read; a computed non-literal
	// key is pinned to a temp first so it only evaluates once.
	let has_parameter_decorators = node
		.parameters()
		.iter()
		.any(|&parameter| tsast::has_decorators(parameter));
	if tsast::has_decorators(node) || has_parameter_decorators {
		if !state.ast.kind(name).is_simple_primitive() {
			let temp_id = state.ast.temp_id("key");
			let declaration = state.ast.variable_declaration(temp_id, Some(name.into()));
			state.ast.list_push(result, declaration);
			name = temp_id;
		}
		state.set_class_element_object_key(node, name);
	}

	let is_async = tsast::has_syntactic_modifier(node, ModifierFlags::Async);

	// Generator wrap happens first and does not exclude the method shape: a generator method may
	// still emit as `function Class:m()` whose body is `return TS.generator(...)`.
	if node.as_method_declaration().asterisk_token().is_some() {
		if is_async {
			state.diags.add(diagnostics::no_async_generator_functions(node));
		}
		statements = wrap_statements_as_generator(state, node, statements);
	}

	// can we use `function class:name() end`? Only when the pointer was already spilled to a temp
	// id (an inline map field can't hold a function statement). Async methods are excluded: they
	// fall to the map-pointer path with `self` kept in parameters.
	let name_str = state
		.ast
		.get::<StringLiteral>(name)
		.map(|literal| literal.value.clone());
	let pointer_is_map = state.ast.get::<Map>(pointer.value).is_some();
	if !is_async
		&& let Some(name_str) = name_str
		&& !pointer_is_map
		&& rotor_luau::is_valid_identifier(&name_str)
	{
		let statement = if is_method(state, node) && !state.luau_typed_signatures() {
			state.ast.list_shift(parameters); // remove `self`
			let method = state
				.ast
				.method_declaration(pointer.value, name_str, parameters, has_dot_dot_dot, statements);
			if let Some(method_declaration) = state.ast.get_mut::<MethodDeclaration>(method) {
				method_declaration.generics = luau_generics;
				method_declaration.return_type = return_type;
			}
			method
		} else {
			let function_name = state.ast.property_access(pointer.value, name_str);
			let function = state.ast.function_declaration(
				false, // localize
				function_name,
				parameters,
				has_dot_dot_dot,
				statements,
			);
			set_function_signature(&mut state.ast, function, luau_generics, return_type, var_args_type);
			function
		};
		state.ast.list_push(result, statement);
		state.set_node_source_position(statement, node);
		pop_type_generics(state);
		return result;
	}

	let method_expression = state.ast.function_expression(parameters, has_dot_dot_dot, statements);
	set_function_signature(
		&mut state.ast,
		method_expression,
		luau_generics,
		return_type,
		var_args_type,
	);
	let mut expression = method_expression;
	state.set_node_source_position(expression, node);

	if is_async {
		let runtime_async = state.runtime_lib(Some(node), "async");
		let args = state.ast.list_from([expression]);
		expression = state.ast.call(runtime_async, args);
	}

	// we have to use `class[name] = function()`
	let assignments = state.capture_statements(|state| assign_to_map_pointer(state, pointer, name, expression));
	state.ast.list_push_list(result, assignments);

	pop_type_generics(state);
	result
}
