//! `classes.go`: `nodes/class/transformClassLikeDeclaration.ts`, `transformClassConstructor.ts`,
//! `transformPropertyDeclaration.ts`, `statements/transformClassDeclaration.ts`,
//! `expressions/transformClassExpression.ts`, `transformThisExpression.ts` and
//! `transformSuperKeyword.ts`.
//!
//! `getExtendsNode`, `findConstructor`, `createClassNameFunction` and `isClassHoisted` live in
//! [`crate::shared`]; `getAllSuperTypeNodes` and `validateHeritageClause` in
//! [`crate::methodassignment`].

use rotor_luau::{Identifier, TemporaryIdentifier};
use rotor_tsrs::ast::{self as tsast, ModifierFlags, SymbolFlags};

use crate::control::{transform_method_declaration_with_receiver, transform_parameters};
use crate::diagnostics::{self, kind_name};
use crate::dispatch::transform_expression;
use crate::extras::transform_decorators;
use crate::identifier::{is_global_this_symbol, transform_identifier_defined, validate_identifier};
use crate::literals::transform_property_name;
use crate::luau_types::{
	LuauClass, get_luau_vararg_type, luau_declaration_statements, new_luau_class, pop_type_generics,
	push_type_generics, signature_generics,
};
use crate::methodassignment::validate_method_assignment;
use crate::pointer::MapPointer;
use crate::prelude::*;
use crate::shared::{
	create_class_name_function, find_constructor, get_extends_node, is_class_hoisted,
	register_optimizable_var_args_for_function, type_generic_names,
};
use crate::statementlist::transform_statement_list;
use crate::statements::transform_block;

/// `magicToStringMethod`: `MAGIC_TO_STRING_METHOD` (`transformClassLikeDeclaration.ts` L23).
const MAGIC_TO_STRING_METHOD: &str = "toString";

/// `constructorMethodName`
const CONSTRUCTOR_METHOD_NAME: &str = "constructor";

/// `createClassBoilerplate`: `transformClassLikeDeclaration.ts` `createBoilerplate` (L37-188).
///
/// QUIRKS (oracle-verified, do not "fix"): abstract no-extends classes are a bare
/// `className = {}`; abstract classes never get `.new` but still get `constructor` and methods;
/// anonymous classes stringify as "Anonymous"; `.new` returns `self:constructor(...) or self`.
fn create_class_boilerplate(
	state: &mut State<'_>,
	node: P<Node>,
	class_name: NodeId,
	is_class_expression: bool,
) -> Statements {
	let is_abstract = tsast::has_abstract_modifier(node);
	let statements = state.ast.new_list();

	// if a class is abstract and it does not extend any class, it can just be a plain table;
	// otherwise we can use the default boilerplate
	let extends_node = get_extends_node(node);
	if is_abstract && extends_node.is_none() {
		let fields = state.ast.new_list();
		let map = state.ast.map(fields);
		let assignment = state.ast.assignment(class_name, AssignmentOperator::Assign, map);
		state.ast.list_push(statements, assignment);
	} else {
		let metatable_fields = state.ast.new_list::<element::MapField>();
		let tostring_name = state
			.ast
			.get::<Identifier>(class_name)
			.map_or_else(|| "Anonymous".to_owned(), |id| id.name.clone());
		let tostring_key = state.ast.str("__tostring");
		let tostring_function = create_class_name_function(&mut state.ast, &tostring_name);
		let tostring_field = state.ast.map_field(tostring_key, tostring_function);
		state.ast.list_push(metatable_fields, tostring_field);

		if let Some(extends_node) = extends_node {
			let (extends_exp, extends_exp_prereqs) = state.capture(|state| {
				if let Some(expression) = extends_node.expression() {
					transform_expression(state, expression)
				} else {
					state.ice("transformer: extends heritage element has no expression");
					state.ast.none()
				}
			});
			let super_id = state.ast.global_id("super");
			state.ast.list_push_list(statements, extends_exp_prereqs);
			let declaration = state.ast.variable_declaration(super_id, Some(extends_exp.into()));
			state.ast.list_push(statements, declaration);
			let index_key = state.ast.str("__index");
			let index_field = state.ast.map_field(index_key, super_id);
			state.ast.list_push(metatable_fields, index_field);
		}

		let setmetatable = state.ast.global_id("setmetatable");
		let empty_fields = state.ast.new_list();
		let empty = state.ast.map(empty_fields);
		let metatable_map = state.ast.map(metatable_fields);
		let args = state.ast.list_from([empty, metatable_map]);
		let metatable = state.ast.call(setmetatable, args);

		if is_class_expression && let Some(name) = node.name() {
			// the ONLY local-declaration form: the named class expression's inner name
			let inner_name = transform_identifier_defined(state, name);
			let declaration = state.ast.variable_declaration(inner_name, Some(metatable.into()));
			state.ast.list_push(statements, declaration);
		} else {
			let assignment = state.ast.assignment(class_name, AssignmentOperator::Assign, metatable);
			state.ast.list_push(statements, assignment);
		}

		// className.__index = className
		let index = state.ast.property_access(class_name, "__index");
		let assignment = state.ast.assignment(index, AssignmentOperator::Assign, class_name);
		state.ast.list_push(statements, assignment);
	}

	// statements for className.new
	if !is_abstract {
		let statements_inner = state.ast.new_list();

		// local self = setmetatable({}, className)
		let self_id = state.ast.global_id("self");
		let setmetatable = state.ast.global_id("setmetatable");
		let empty_fields = state.ast.new_list();
		let empty = state.ast.map(empty_fields);
		let args = state.ast.list_from([empty, class_name]);
		let call = state.ast.call(setmetatable, args);
		let declaration = state.ast.variable_declaration(self_id, Some(call.into()));
		state.ast.list_push(statements_inner, declaration);

		// return self:constructor(...) or self
		let self_receiver = state.ast.global_id("self");
		let var_args = state.ast.var_args();
		let args = state.ast.list_from([var_args]);
		let constructor_call = state.ast.method_call(CONSTRUCTOR_METHOD_NAME, self_receiver, args);
		let self_fallback = state.ast.global_id("self");
		let binary = state.ast.binary(constructor_call, BinaryOperator::Or, self_fallback);
		let ret = state.ast.return_statement(binary);
		state.ast.list_push(statements_inner, ret);

		// function className.new(...)
		// end
		let new_name = state.ast.property_access(class_name, "new");
		let parameters = state.ast.new_list();
		let function = state.ast.function_declaration(
			false, // localize
			new_name,
			parameters,
			true, // hasDotDotDot
			statements_inner,
		);
		state.ast.list_push(statements, function);
	}

	statements
}

/// `transformPropertyInitializers`: `transformClassConstructor.ts` (L16-50). Per NON-static
/// `PropertyDeclaration` in member order, `self[<name>] = <initializer>`. QUIRK: a `#field`
/// private identifier raises `noPrivateIdentifier` spanning the WHOLE CLASS (upstream passes the
/// class node), ported verbatim.
pub fn transform_property_initializers(state: &mut State<'_>, node: P<Node>) -> Statements {
	let statements = state.ast.new_list();
	for &member in node.members() {
		if !tsast::is_property_declaration(member) {
			continue;
		}
		if tsast::has_static_modifier(member) {
			continue;
		}

		let Some(name) = member.name() else {
			state.ice("transformer: PropertyDeclaration has no name");
			continue;
		};
		if tsast::is_private_identifier(name) {
			state.diags.add(diagnostics::no_private_identifier(node));
			continue;
		}

		let Some(initializer) = member.initializer() else {
			continue;
		};

		let (index, index_prereqs) = state.capture(|state| transform_property_name(state, name));

		let (right, right_prereqs) = state.capture(|state| transform_expression(state, initializer));
		let self_id = state.ast.global_id("self");
		let target = state.ast.computed_index(self_id, index);
		let assignment = state.ast.assignment(target, AssignmentOperator::Assign, right);
		state.ast.list_push(right_prereqs, assignment);
		state.set_statement_list_source_positions(member, &[index_prereqs, right_prereqs]);
		state.ast.list_push_list(statements, index_prereqs);
		state.ast.list_push_list(statements, right_prereqs);
	}
	statements
}

/// `transformImplicitClassConstructor`: `transformClassConstructor.ts` (L52-88). QUIRK: an
/// implicit derived constructor is variadic (`constructor(...)` plus
/// `super.constructor(self, ...)`) even when the base constructor takes no arguments.
fn transform_implicit_class_constructor(state: &mut State<'_>, node: P<Node>, name: NodeId) -> Statements {
	let statements = state.ast.new_list();

	let mut has_dot_dot_dot = false;

	// if extends + no constructor:
	// - add ... to params
	// - add super.constructor(self, ...)
	if get_extends_node(node).is_some() {
		has_dot_dot_dot = true;
		let super_id = state.ast.global_id("super");
		let callee = state.ast.property_access(super_id, CONSTRUCTOR_METHOD_NAME);
		let self_id = state.ast.global_id("self");
		let var_args = state.ast.var_args();
		let args = state.ast.list_from([self_id, var_args]);
		let call = state.ast.call(callee, args);
		let call_statement = state.ast.call_statement(call);
		state.ast.list_push(statements, call_statement);
	}

	let initializers = transform_property_initializers(state, node);
	state.ast.list_push_list(statements, initializers);

	let parameters = state.ast.new_list();
	let method = state
		.ast
		.method_declaration(name, CONSTRUCTOR_METHOD_NAME, parameters, has_dot_dot_dot, statements);
	state.ast.list_from([method])
}

/// `transformClassConstructor`: `transformClassConstructor.ts` (L90-130). `node` is a
/// `ConstructorDeclaration` with a body. Statement order inside `function X:constructor(...)`:
/// parameter defaults and destructuring, the body up to AND INCLUDING the first top-level
/// `super()` call, parameter-property assignments, property initializers, the rest of the body.
///
/// `transformParameters` does NOT inject `self` here: `isMethod` is false for constructors; the
/// `:` declaration sugar supplies it. In luauTypes mode (`class` set) the constructor is a
/// dot-form function with an explicit `self` re-declaring the class generics.
fn transform_class_constructor(
	state: &mut State<'_>,
	node: P<Node>,
	name: NodeId,
	class: Option<&LuauClass>,
) -> Statements {
	if class.is_some() {
		let generics = signature_generics(state, node);
		push_type_generics(state, type_generic_names(&generics), false);
	} else {
		// A constructor is its own function WITH NO generics: a class type parameter is not
		// declarable in Luau, so it must widen to `any` here instead of inheriting an enclosing
		// function's names.
		push_type_generics(state, Vec::new(), false);
	}

	let transformed = transform_parameters(state, node);
	let parameters = transformed.parameters;
	let statements = transformed.statements;
	let has_dot_dot_dot = transformed.has_dot_dot_dot;
	let rest_param = register_optimizable_var_args_for_function(state, node, transformed.var_args_data.as_ref());
	let Some(body) = node.body() else {
		state.ice("transformer: constructor has no body");
		pop_type_generics(state);
		return state.ast.new_list();
	};
	let body_statements = body.statements();

	// property parameters must come after the first super() call
	let split = body_statements
		.iter()
		.position(|&statement| {
			tsast::is_expression_statement(statement) && statement.expression().is_some_and(tsast::is_super_call)
		})
		.map_or(0, |super_index| super_index + 1);

	let leading = transform_statement_list(state, Some(body), &body_statements[..split], None);
	state.ast.list_push_list(statements, leading);

	for &parameter in node.parameters() {
		if tsast::is_parameter_property_declaration(parameter, node)
			&& let Some(parameter_name) = parameter.name()
		{
			let param_id = transform_identifier_defined(state, parameter_name);
			let self_id = state.ast.global_id("self");
			let property_name = any_identifier_name(state, param_id);
			let target = state.ast.property_access(self_id, property_name);
			let assignment = state.ast.assignment(target, AssignmentOperator::Assign, param_id);
			state.set_node_source_position(assignment, parameter);
			state.ast.list_push(statements, assignment);
		}
	}

	if let Some(class_node) = node.parent() {
		let initializers = transform_property_initializers(state, class_node);
		state.ast.list_push_list(statements, initializers);
	}

	let trailing = transform_statement_list(state, Some(body), &body_statements[split..], None);
	state.ast.list_push_list(statements, trailing);
	if let Some(rest_param) = rest_param {
		state.unregister_optimizable_var_args(rest_param);
	}

	let constructor = if let Some(class) = class {
		let var_args_type = get_luau_vararg_type(state, node);
		class.constructor_function(state, parameters, has_dot_dot_dot, statements, var_args_type)
	} else {
		state
			.ast
			.method_declaration(name, CONSTRUCTOR_METHOD_NAME, parameters, has_dot_dot_dot, statements)
	};
	state.set_node_source_position(constructor, node);
	pop_type_generics(state);
	state.ast.list_from([constructor])
}

/// `anyIdentifierName`: the name of an `Identifier` or `TemporaryIdentifier` (upstream reads
/// `.name` off the `AnyIdentifier` union).
fn any_identifier_name(state: &mut State<'_>, id: NodeId) -> String {
	if let Some(identifier) = state.ast.get::<Identifier>(id) {
		return identifier.name.clone();
	}
	if let Some(identifier) = state.ast.get::<TemporaryIdentifier>(id) {
		return identifier.name.clone();
	}
	state.ice("transformer: anyIdentifierName: not an identifier");
	String::new()
}

/// `transformClassPropertyDeclaration`: `nodes/class/transformPropertyDeclaration.ts` (L9-37). One
/// `name[<key>] = <value>` assignment per STATIC `PropertyDeclaration` with an initializer; both
/// transforms run inline so their prereqs flow to the caller's capture (index prereqs before value
/// prereqs).
fn transform_class_property_declaration(state: &mut State<'_>, node: P<Node>, name: NodeId) -> Statements {
	if !tsast::has_static_modifier(node) {
		return state.ast.new_list();
	}

	let Some(property_name) = node.name() else {
		state.ice("transformer: PropertyDeclaration has no name");
		return state.ast.new_list();
	};
	if tsast::is_private_identifier(property_name) {
		state.diags.add(diagnostics::no_private_identifier(node));
		return state.ast.new_list();
	}

	let Some(initializer) = node.initializer() else {
		return state.ast.new_list();
	};

	let index = transform_property_name(state, property_name);
	let target = state.ast.computed_index(name, index);
	let value = transform_expression(state, initializer);
	let assignment = state.ast.assignment(target, AssignmentOperator::Assign, value);
	state.ast.list_from([assignment])
}

/// `transformClassLikeDeclaration`: `nodes/class/transformClassLikeDeclaration.ts` (L199-384).
/// Returns the statements and the Luau identifier holding the class (the temp `_class` for a
/// named class expression, the class name otherwise, `default` for an unnamed export-default
/// class).
#[expect(
	clippy::too_many_lines,
	reason = "one block per upstream section, kept in upstream order"
)]
fn transform_class_like_declaration(state: &mut State<'_>, node: P<Node>) -> (Statements, NodeId) {
	let is_class_expression = tsast::is_class_expression(node);
	let statements = state.ast.new_list();

	let is_export_default = tsast::has_syntactic_modifier(node, ModifierFlags::ExportDefault);

	if let Some(name) = node.name() {
		validate_identifier(state, name);
	}

	/*
		local className
		do
			OOP boilerplate
			class functions
		end
	*/

	let should_use_internal_name = is_class_expression && node.name().is_some();

	let return_var = match node.name() {
		_ if should_use_internal_name => state.ast.temp_id("class"),
		Some(name) => transform_identifier_defined(state, name),
		None if is_export_default => state.ast.id("default"),
		None => state.ast.temp_id("class"),
	};

	let internal_name = match node.name() {
		Some(name) if should_use_internal_name => transform_identifier_defined(state, name),
		_ => return_var,
	};
	state.class_identifier_map.insert(node, internal_name);

	// luauTypes: the instance alias at the declaration, the class generics in scope for the whole
	// body, and the typed boilerplate, whose declaration line replaces the bare `local className`.
	let mut class = None;
	if state.luau_typed_signatures() {
		let luau_class = new_luau_class(state, node, internal_name);
		if !is_class_expression {
			let declarations = luau_declaration_statements(state, node);
			state.ast.list_push_list(statements, declarations);
		}
		push_type_generics(state, luau_class.generic_names(), false);
		class = Some(luau_class);
	} else if !is_class_hoisted(state, node) {
		let declaration = state.ast.variable_declaration(return_var, None);
		state.ast.list_push(statements, declaration);
	}

	// OOP boilerplate + class functions
	let statements_inner = state.ast.new_list();
	if let Some(class) = &class {
		if should_use_internal_name && !is_class_hoisted(state, node) {
			let declaration = state.ast.variable_declaration(return_var, None);
			state.ast.list_push(statements, declaration);
		}
		let (outer, inner) = class.boilerplate(state, is_class_expression);
		state.ast.list_push_list(statements, outer);
		state.ast.list_push_list(statements_inner, inner);
	} else {
		let boilerplate = create_class_boilerplate(state, node, internal_name, is_class_expression);
		state.ast.list_push_list(statements_inner, boilerplate);
	}

	let constructor_statements = match (find_constructor(node), &class) {
		(Some(constructor), _) => transform_class_constructor(state, constructor, internal_name, class.as_ref()),
		(None, Some(class)) => class.implicit_constructor(state),
		(None, None) => transform_implicit_class_constructor(state, node, internal_name),
	};
	state.ast.list_push_list(statements_inner, constructor_statements);

	for &member in node.members() {
		if (tsast::is_property_declaration(member) || tsast::is_method_declaration(member))
			&& let Some(name) = member.name()
			&& (tsast::is_identifier(name) || tsast::is_string_literal(name))
			&& rotor_luau::is_reserved_class_field(name.text())
		{
			state.diags.add(diagnostics::no_reserved_class_fields(name));
		}
		if tsast::is_auto_accessor_property_declaration(member) {
			// member must have AccessorKeyword to be AutoAccessorPropertyDeclaration
			let keyword = member
				.modifier_nodes()
				.iter()
				.copied()
				.find(|modifier| modifier.kind() == Kind::AccessorKeyword);
			state.diags.add(diagnostics::no_auto_accessor_modifiers(keyword));
		}
	}

	let mut methods = Vec::new();
	let mut static_declarations = Vec::new();

	for &member in node.members() {
		validate_method_assignment(state, member);
		if tsast::is_constructor_declaration(member)
			|| tsast::is_index_signature_declaration(member)
			|| tsast::is_semicolon_class_element(member)
		{
			continue;
		}
		if tsast::is_method_declaration(member) {
			methods.push(member);
		} else if tsast::is_property_declaration(member) {
			// non-static properties are done in transformClassConstructor
			if !tsast::has_static_modifier(member) {
				continue;
			}
			static_declarations.push(member);
		} else if tsast::is_accessor(member) {
			state.diags.add(diagnostics::no_getter_setter(member));
		} else if tsast::is_class_static_block_declaration(member) {
			static_declarations.push(member);
		} else {
			state.ice(format!(
				"transformer: ClassMember kind not implemented: {}",
				kind_name(member.kind())
			)); // upstream assert
		}
	}

	let Some(class_symbol) = node.symbol() else {
		state.ice("transformer: class has no symbol");
		return (statements, return_var);
	};
	let Some(class_type) = state.ck().get_type_of_symbol_at_location(class_symbol, Some(node)) else {
		state.ice("transformer: class has no type");
		return (statements, return_var);
	};
	let instance_type = state.ck().get_declared_type_of_symbol(class_symbol);

	for &method in &methods {
		if let Some(name) = method.name()
			&& (tsast::is_identifier(name) || tsast::is_string_literal(name))
		{
			if rotor_luau::is_metamethod(name.text()) {
				state.diags.add(diagnostics::no_class_metamethods(name));
			}

			if tsast::has_static_modifier(method) {
				if state.ck().get_property_of_type(instance_type, name.text()).is_some() {
					state.diags.add(diagnostics::no_instance_method_collisions(method));
				}
			} else if state.ck().get_property_of_type(class_type, name.text()).is_some() {
				state.diags.add(diagnostics::no_static_method_collisions(method));
			}
		}

		let method_statements = state.capture_statements(|state| {
			// statements and prereqs are concatenated in order, so one capture covers upstream's
			// [statements, prereqs] split.
			let receiver = class.as_ref().and_then(|class| class.receiver_for(state, method));
			let mut pointer = MapPointer {
				name: "name".to_owned(),
				value: internal_name,
			};
			let method_declaration = transform_method_declaration_with_receiver(state, method, &mut pointer, receiver);
			state.prereq_list(method_declaration);
		});
		state.ast.list_push_list(statements_inner, method_statements);
	}

	// QUIRK: getPropertyOfType sees INHERITED members, so every subclass whose instance type
	// carries a toString METHOD (symbol flag check excludes function-typed properties) re-emits
	// the __tostring wrapper.
	let has_to_string_method = state
		.ck()
		.get_property_of_type(instance_type, MAGIC_TO_STRING_METHOD)
		.is_some_and(|property| property.flags().intersects(SymbolFlags::Method));
	if has_to_string_method && let Some(class) = &class {
		let function = class.to_string_function(state);
		state.ast.list_push(statements_inner, function);
	} else if has_to_string_method {
		let self_id = state.ast.global_id("self");
		let args = state.ast.new_list();
		let call = state.ast.method_call(MAGIC_TO_STRING_METHOD, self_id, args);
		let ret = state.ast.return_statement(call);
		let body = state.ast.list_from([ret]);
		let parameters = state.ast.new_list();
		let method = state
			.ast
			.method_declaration(internal_name, "__tostring", parameters, false, body);
		state.ast.list_push(statements_inner, method);
	}

	for &declaration in &static_declarations {
		let declaration_statements = if tsast::is_class_static_block_declaration(declaration) {
			transform_block(state, declaration.as_class_static_block_declaration().body())
		} else {
			state.capture_statements(|state| {
				let property = transform_class_property_declaration(state, declaration, internal_name);
				state.prereq_list(property);
			})
		};
		state.set_statement_list_source_positions(declaration, &[declaration_statements]);
		state.ast.list_push_list(statements_inner, declaration_statements);
	}

	// if using internal name, assign to return var
	if should_use_internal_name {
		let assignment = state
			.ast
			.assignment(return_var, AssignmentOperator::Assign, internal_name);
		state.ast.list_push(statements_inner, assignment);
	}

	let decorators = transform_decorators(state, node, return_var);
	state.ast.list_push_list(statements_inner, decorators);

	let block = state.ast.do_statement(statements_inner);
	state.ast.list_push(statements, block);

	if class.is_some() {
		pop_type_generics(state);
	}

	(statements, return_var)
}

/// `transformClassDeclaration`: `statements/transformClassDeclaration.ts`.
pub fn transform_class_declaration(state: &mut State<'_>, node: P<Node>) -> Statements {
	transform_class_like_declaration(state, node).0
}

/// `transformClassExpression`: `expressions/transformClassExpression.ts`.
pub fn transform_class_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let (statements, name) = transform_class_like_declaration(state, node);
	state.prereq_list(statements);
	name
}

/// `transformThisExpression`: `expressions/transformThisExpression.ts` (L7-29). `this` is `self`,
/// EXCEPT inside static blocks and static property initializers where it is the class's internal
/// identifier (static METHODS still use `self`: the `:` declaration form binds it).
pub fn transform_this_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let symbol = state.ck().get_symbol_at_location(node);
	if let Some(symbol) = symbol
		&& is_global_this_symbol(symbol)
	{
		state.diags.add(diagnostics::no_global_this(node));
	}

	if symbol.is_some() {
		let container = tsast::get_this_container(node, false, false);

		// ts.hasStaticModifier doesn't work on static blocks
		let is_static = tsast::has_static_modifier(container) || tsast::is_class_static_block_declaration(container);

		// MethodDeclaration creates it's own implicit this
		if is_static
			&& !tsast::is_method_declaration(container)
			&& let Some(parent) = container.parent()
			&& tsast::is_class_like(parent)
			&& let Some(&identifier) = state.class_identifier_map.get(&parent)
		{
			return identifier;
		}
	}

	state.ast.global_id("self")
}
