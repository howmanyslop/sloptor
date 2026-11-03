//! `decorators.go`: `nodes/class/transformDecorators.ts`, legacy experimental decorators only.
//!
//! ```text
//! -- method
//! local _descriptor = decorator(Class, KEY, {
//!     value = Class[KEY],
//! })
//! if _descriptor then
//!     Class[KEY] = _descriptor.value
//! end
//! -- property
//! decorator(Class, KEY)
//! -- parameter (0-based index; KEY is nil for constructor parameters)
//! decorator(Class, KEY, i)
//! -- class
//! Class = decorator(Class) or Class
//! ```
//!
//! Decorator expressions initialize first to last (a mutable one spills to `local _decorator`
//! unless it may be inlined); applications run last to first. Parameter decorators sandwich
//! between their member's initializers and finalizers.

use rotor_tsrs::ast::{self as tsast};

use crate::call::{convert_to_indexable_expression, expression_might_mutate};
use crate::dispatch::transform_expression;
use crate::literals::transform_property_name;
use crate::prelude::*;
use crate::shared::find_constructor;

/// `getDecorators`: the decorator nodes among the modifiers.
fn get_decorators(node: P<Node>) -> Vec<P<Node>> {
	node.modifier_nodes()
		.iter()
		.copied()
		.filter(|modifier| modifier.kind() == Kind::Decorator)
		.collect()
}

/// `countDecorators` (L13-15).
fn count_decorators(node: P<Node>) -> usize {
	get_decorators(node).len()
}

fn any_parameter_has_decorators(node: P<Node>) -> bool {
	node.parameters()
		.iter()
		.any(|&parameter| count_decorators(parameter) > 0)
}

/// `shouldInline` (L17-56).
fn should_inline_decorator(
	state: &mut State<'_>,
	is_last_decorator: bool,
	decorator: P<Node>,
	expression: NodeId,
) -> bool {
	// immutable expressions can be inlined
	if !expression_might_mutate(state, expression, decorator.expression()) {
		return true;
	}

	// if it's not the last decorator, we can't inline: all decorators initialize before running
	if !is_last_decorator {
		return false;
	}

	let Some(node) = decorator.parent() else {
		return true;
	};

	// parameter decorators run between initializing and running method decorators
	if tsast::is_method_declaration(node) && any_parameter_has_decorators(node) {
		return false;
	}

	// constructor parameter decorators run between initializing and running class decorators
	if tsast::is_class_like(node)
		&& let Some(constructor) = find_constructor(node)
		&& any_parameter_has_decorators(constructor)
	{
		return false;
	}

	// all of the parameters must be initialized before running any, including from sibling
	// parameters
	if tsast::is_parameter_declaration(node)
		&& let Some(owner) = node.parent()
	{
		let parameters = owner.parameters();
		let index = parameters.iter().position(|&parameter| parameter == node);
		let later = index.map_or(parameters, |index| &parameters[index + 1..]);
		if later.iter().any(|&parameter| count_decorators(parameter) > 0) {
			return false;
		}
	}

	true
}

/// `transformMemberDecorators` (L58-92), the shared driver. Per decorator in source order: the
/// transformed expression's prereqs join the initializers, it spills to `local _decorator` unless
/// inlinable, and the callback's statements are unshifted into the finalizers.
fn transform_member_decorators(
	state: &mut State<'_>,
	node: P<Node>,
	mut callback: impl FnMut(&mut State<'_>, NodeId) -> Statements,
) -> (Statements, Statements) {
	let initializers = state.ast.new_list();
	let finalizers = state.ast.new_list();

	let decorators = get_decorators(node);

	for (index, &decorator) in decorators.iter().enumerate() {
		let (mut expression, prereqs) = state.capture(|state| match decorator.expression() {
			Some(expression) => transform_expression(state, expression),
			None => state.ast.none(),
		});

		state.ast.list_push_list(initializers, prereqs);

		let is_last_decorator = index == decorators.len() - 1;

		if !should_inline_decorator(state, is_last_decorator, decorator, expression) {
			let temp_id = state.ast.temp_id("decorator");
			let declaration = state.ast.variable_declaration(temp_id, Some(expression.into()));
			state.ast.list_push(initializers, declaration);
			expression = temp_id;
		}

		let expression = convert_to_indexable_expression(&mut state.ast, expression);
		let statements = callback(state, expression);
		state.ast.list_unshift_list(finalizers, statements);
	}

	(initializers, finalizers)
}

/// The class element key of `member`, or an internal error.
fn class_element_key(state: &mut State<'_>, member: P<Node>, message: &str) -> NodeId {
	state.get_class_element_object_key(member).unwrap_or_else(|| {
		state.ice(message);
		state.ast.none()
	})
}

/// `transformMethodDecorators` (L94-157). The member's parameter decorators run between the
/// method decorators' initializers and finalizers.
fn transform_method_decorators(state: &mut State<'_>, member: P<Node>, class_id: NodeId) -> Statements {
	let (initializers, finalizers) = transform_member_decorators(state, member, |state, expression| {
		// local _descriptor = decorator(Class, "name", { value = Class.name })
		// if _descriptor then
		// 	Class.name = _descriptor.value
		// end
		let descriptor_id = state.ast.temp_id("descriptor");
		let key = class_element_key(
			state,
			member,
			"transformer: did not find method key for method decorator",
		);

		let value_key = state.ast.str("value");
		let value = state.ast.computed_index(class_id, key);
		let field = state.ast.map_field(value_key, value);
		let fields = state.ast.list_from([field]);
		let descriptor = state.ast.map(fields);
		let args = state.ast.list_from([class_id, key, descriptor]);
		let call = state.ast.call(expression, args);
		let declaration = state.ast.variable_declaration(descriptor_id, Some(call.into()));

		let target = state.ast.computed_index(class_id, key);
		let descriptor_value = state.ast.property_access(descriptor_id, "value");
		let assignment = state
			.ast
			.assignment(target, AssignmentOperator::Assign, descriptor_value);
		let body = state.ast.list_from([assignment]);
		let check = state.ast.if_statement(descriptor_id, body, None);

		state.ast.list_from([declaration, check])
	});

	let result = state.ast.new_list();
	state.ast.list_push_list(result, initializers);
	let parameters = transform_parameter_decorators(state, member, class_id);
	state.ast.list_push_list(result, parameters);
	state.ast.list_push_list(result, finalizers);
	result
}

/// `transformPropertyDecorators` (L159-180).
fn transform_property_decorators(state: &mut State<'_>, member: P<Node>, class_id: NodeId) -> Statements {
	let (initializers, finalizers) = transform_member_decorators(state, member, |state, expression| {
		// typescript enforces that property keys are static, so they shouldn't have prereqs
		let key = state.no_prereqs(|state| match member.name() {
			Some(name) => transform_property_name(state, name),
			None => state.ast.none(),
		});

		// decorator(Class, "name")
		let args = state.ast.list_from([class_id, key]);
		let call = state.ast.call(expression, args);
		let statement = state.ast.call_statement(call);
		state.ast.list_from([statement])
	});

	let result = state.ast.new_list();
	state.ast.list_push_list(result, initializers);
	state.ast.list_push_list(result, finalizers);
	result
}

/// A parameter index as a Luau number.
fn index_number(ast: &mut Ast, index: usize) -> NodeId {
	ast.num(f64::from(u32::try_from(index).unwrap_or(u32::MAX)))
}

/// `transformParameterDecorators` (L182-212). `member` is a method or constructor; parameter
/// finalizers also unshift, so the last parameter's decorators apply first.
fn transform_parameter_decorators(state: &mut State<'_>, member: P<Node>, class_id: NodeId) -> Statements {
	let initializers = state.ast.new_list();
	let finalizers = state.ast.new_list();

	for (index, &parameter) in member.parameters().iter().enumerate() {
		let (parameter_initializers, parameter_finalizers) =
			transform_member_decorators(state, parameter, |state, expression| {
				// No member.name means it's the constructor, so the name argument should be nil
				let key = if member.name().is_some() {
					class_element_key(
						state,
						member,
						"transformer: did not find method key for parameter decorator",
					)
				} else {
					state.ast.nil()
				};

				// decorator(Class, "name", 0)
				let position = index_number(&mut state.ast, index);
				let args = state.ast.list_from([class_id, key, position]);
				let call = state.ast.call(expression, args);
				let statement = state.ast.call_statement(call);
				state.ast.list_from([statement])
			});
		state.ast.list_push_list(initializers, parameter_initializers);
		state.ast.list_unshift_list(finalizers, parameter_finalizers);
	}

	let result = state.ast.new_list();
	state.ast.list_push_list(result, initializers);
	state.ast.list_push_list(result, finalizers);
	result
}

/// `transformClassDecorators` (L214-240). Constructor parameter decorators run between the class
/// decorators' initializers and finalizers.
fn transform_class_decorators(state: &mut State<'_>, node: P<Node>, class_id: NodeId) -> Statements {
	let (initializers, finalizers) = transform_member_decorators(state, node, |state, expression| {
		// Class = decorator(Class) or Class
		let args = state.ast.list_from([class_id]);
		let call = state.ast.call(expression, args);
		let value = state.ast.binary(call, BinaryOperator::Or, class_id);
		let assignment = state.ast.assignment(class_id, AssignmentOperator::Assign, value);
		state.ast.list_from([assignment])
	});

	let result = state.ast.new_list();
	state.ast.list_push_list(result, initializers);

	if let Some(constructor) = find_constructor(node) {
		let parameters = transform_parameter_decorators(state, constructor, class_id);
		state.ast.list_push_list(result, parameters);
	}

	state.ast.list_push_list(result, finalizers);
	result
}

fn transform_member_decorators_of(state: &mut State<'_>, member: P<Node>, class_id: NodeId, result: Statements) {
	if tsast::is_method_declaration(member) && member.body().is_some() {
		let statements = transform_method_decorators(state, member, class_id);
		state.ast.list_push_list(result, statements);
	} else if tsast::is_property_declaration(member) {
		let statements = transform_property_decorators(state, member, class_id);
		state.ast.list_push_list(result, statements);
	}
}

/// `transformDecorators` (L242-279), called as the last thing inside the class do-block with
/// the return var. Instance members (declaration order), then static members, then class
/// decorators (with constructor parameter decorators sandwiched). Methods without bodies are
/// overload signatures.
pub fn transform_decorators(state: &mut State<'_>, node: P<Node>, class_id: NodeId) -> Statements {
	let result = state.ast.new_list();

	// Instance Decorators
	for &member in node.members() {
		if !tsast::has_static_modifier(member) {
			transform_member_decorators_of(state, member, class_id, result);
		}
	}

	// Static Decorators
	for &member in node.members() {
		if tsast::has_static_modifier(member) {
			transform_member_decorators_of(state, member, class_id, result);
		}
	}

	// Class Decorators
	let class_decorators = transform_class_decorators(state, node, class_id);
	state.ast.list_push_list(result, class_decorators);

	result
}
