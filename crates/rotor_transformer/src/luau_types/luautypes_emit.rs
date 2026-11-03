//! `luautypes_emit.go`: the emission sites of the experimental `luauTypes` mode: what a
//! function's parameters, `...`, `self`, generics and return get annotated with, which locals
//! get a `: T`, and how a defaulted parameter is refined.
//!
//! Two shapes were settled by probing luau-lsp 1.70.1 and luau-analyze 0.730:
//! - a void function with an explicit return is spelled `...nil`, the one pack both `return nil`
//!   and falling off the end satisfy;
//! - neither solver refines `c: number?` through rbxtsc's `if c == nil then c = 1 end` default,
//!   nor a `_condition` temp through the chain of `a ?? b`, so both are re-bound right after as
//!   `local c = c :: number`.

use rotor_luau::{
	CallExpression, Identifier, PropertyAccessExpression, TypeGeneric, TypePack, TypeReference, VariableDeclaration,
};
use rotor_tsrs::ast::{self as tsast, ModifierFlags, SymbolFlags};
use rotor_tsrs::checker::{ContextFlags, TypeFlags};

use super::luautypes::{FidelityLossKind, TypeContext, TypePosition, any_type, clone_type, types};
use super::luautypes_class::luau_hoisted_declaration_type;
use super::nativetypes::{NATIVE_TYPE_ANY, NATIVE_TYPE_NIL, get_native_function_generics, get_native_return_type};
use crate::extras::is_return_blocked_by_try_statement;
use crate::prelude::*;
use crate::shared::this_parameter;
use crate::spreadhelpers::array_like_expression_contains_spread;

// -----------------------------------------------------------------------------------------
// Signatures
// -----------------------------------------------------------------------------------------

/// `getLuauParameterType`: the annotation of one parameter. A written type is spelled from its
/// node; otherwise the checker's type. `?` and an initializer make it optional.
pub fn get_luau_parameter_type(state: &mut State<'_>, parameter_node: P<Node>) -> NodeId {
	let parameter = parameter_node.as_parameter_declaration();
	let ctx = TypeContext::at(TypePosition::Parameter, Some(parameter_node));
	let mut node = luau_declared_type(state, parameter.type_(), parameter.name, ctx);
	if parameter.question_token().is_some() || parameter_node.initializer().is_some() {
		node = state.ast.optional_type(node);
	}
	node
}

/// `getLuauBindingElementType`: the annotation of a parameter flattened out of a `...[a, b]`
/// rest pattern: the element's type, optional when it has a default.
pub fn get_luau_binding_element_type(state: &mut State<'_>, element: P<Node>) -> NodeId {
	let binding_element = element.as_binding_element();
	let ctx = TypeContext::at(TypePosition::Parameter, Some(element));
	let t = binding_element.name.map(|name| state.ck().get_type_at_location(name));
	let mut node = types(state).build_opt(t, ctx).node;
	if element.initializer().is_some() {
		node = state.ast.optional_type(node);
	}
	node
}

/// `getLuauVarargType`: the `...: T` annotation of a function's rest parameter, or `None` when
/// the function emits no `...`.
pub fn get_luau_vararg_type(state: &mut State<'_>, node: P<Node>) -> Option<NodeId> {
	for &parameter_node in node.parameters() {
		let parameter = parameter_node.as_parameter_declaration();
		if parameter.dot_dot_dot_token().is_none() {
			continue;
		}
		let name = parameter.name;
		if tsast::is_array_binding_pattern(name) && !array_like_expression_contains_spread(name) {
			return None;
		}
		let ctx = TypeContext::at(TypePosition::Parameter, Some(parameter_node));
		let rest_type = state.ck().get_type_at_location(name);
		if let Some(element) = state.ck().get_element_type_of_array_type(rest_type) {
			return Some(types(state).build(element, ctx).node);
		}
		let mut builder = types(state);
		let any = any_type(builder.ast());
		return Some(builder.lose(
			FidelityLossKind::Unspellable,
			Some(rest_type),
			any,
			ctx,
			"a rest parameter that is not an array has no Luau spelling",
		));
	}
	None
}

/// `getLuauReturnType`: the annotation of a function's return.
fn get_luau_return_type(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let signature = state.ck().get_signature_from_declaration(node);
	let mut return_type = state.ck().get_return_type_of_signature(signature);
	if tsast::has_syntactic_modifier(node, ModifierFlags::Async)
		&& let Some(promised) = state.ck().get_promised_type_of_promise(return_type)
	{
		return_type = promised;
	}
	let ctx = TypeContext::at(TypePosition::Return, Some(node));
	let pack = types(state).build_return_pack(Some(return_type), ctx);
	if is_empty_type_pack(&state.ast, pack) && function_returns_explicitly(node) {
		let nil = state.ast.type_name(NATIVE_TYPE_NIL);
		return state.ast.type_variadic(nil);
	}
	pack
}

fn is_empty_type_pack(ast: &Ast, node: NodeId) -> bool {
	ast.get::<TypePack>(node)
		.is_some_and(|pack| ast.list_is_empty(pack.elements))
}

/// `functionReturnsExplicitly`: the function's emitted body has a `return` of its own.
fn function_returns_explicitly(node: P<Node>) -> bool {
	fn visit(child: P<Node>) -> bool {
		if tsast::is_return_statement(child) {
			return !is_return_blocked_by_try_statement(child);
		}
		if tsast::is_function_like(child) || tsast::is_class_like(child) {
			return false;
		}
		child.for_each_child(&mut visit)
	}
	let Some(body) = node.body() else {
		return false;
	};
	if !tsast::is_block(body) {
		return true;
	}
	body.for_each_child(&mut visit)
}

/// `getLuauFunctionGenerics`: the type parameters a function declares. An instance member of a
/// class re-declares the class's type parameters first.
fn get_luau_function_generics(state: &mut State<'_>, node: P<Node>) -> Vec<TypeGeneric> {
	let own = types(state).generics(node);
	let Some(class) = enclosing_class_of_instance_member(node) else {
		return own;
	};
	let class_generics = types(state).generics(class);
	let mut generics = Vec::with_capacity(class_generics.len() + own.len());
	for generic in class_generics {
		if !own.iter().any(|existing| existing.name == generic.name) {
			generics.push(generic);
		}
	}
	generics.extend(own);
	generics
}

/// `enclosingClassOfInstanceMember`: the class a constructor, method or accessor declaration
/// is an instance member of.
fn enclosing_class_of_instance_member(node: P<Node>) -> Option<P<Node>> {
	let parent = node.parent()?;
	if !tsast::is_class_like(parent) || tsast::has_static_modifier(node) {
		return None;
	}
	if tsast::is_constructor_declaration(node) || tsast::is_method_declaration(node) || tsast::is_accessor(node) {
		return Some(parent);
	}
	None
}

/// `getLuauSelfType`: the annotation of the leading `self` parameter a method takes, or `None`
/// when this site does not decide it.
pub fn get_luau_self_type(state: &mut State<'_>, node: P<Node>) -> Option<NodeId> {
	let ctx = TypeContext::at(TypePosition::Parameter, Some(node));
	if let Some(this) = this_parameter(node) {
		let declaration = this.as_parameter_declaration();
		return Some(luau_declared_type(state, declaration.type_(), declaration.name, ctx));
	}
	if let Some(this_type) = contextual_this_type(state, node) {
		return Some(types(state).build(this_type, ctx).node);
	}
	let literal = object_literal_of_method(node)?;
	if let Some(contextual) = state.ck().get_contextual_type(literal, ContextFlags::None) {
		let contextual = state.ck().get_non_nullable_type(contextual);
		if let Some(reference) = types(state).build_alias_reference(contextual, ctx) {
			return Some(reference.node);
		}
		if let Some(symbol) = contextual.symbol()
			&& symbol.flags().intersects(SymbolFlags::Class)
			&& contextual.flags().intersects(TypeFlags::Object)
		{
			return Some(types(state).build(contextual, ctx).node);
		}
	}
	let mut builder = types(state);
	let any = any_type(builder.ast());
	Some(builder.lose_text(
		FidelityLossKind::ObjectLiteralSelf,
		"this",
		any,
		ctx,
		"an object literal method's receiver has no name",
	))
}

/// `contextualThisType`: the `this` type of the signature a function expression is
/// contextually typed by.
fn contextual_this_type(state: &mut State<'_>, node: P<Node>) -> Option<P<Type>> {
	if !tsast::is_function_expression(node) {
		return None;
	}
	let contextual = state.ck().get_contextual_type(node, ContextFlags::None)?;
	let signatures = state.ck().get_call_signatures(contextual);
	for &signature in signatures {
		if let Some(this) = signature.this_parameter() {
			return Some(state.ck().get_type_of_symbol(this));
		}
	}
	None
}

/// `objectLiteralOfMethod`: the object literal a method declaration, or a function expression
/// assigned to a property, belongs to.
pub fn object_literal_of_method(node: P<Node>) -> Option<P<Node>> {
	let parent = node.parent()?;
	if tsast::is_method_declaration(node) && tsast::is_object_literal_expression(parent) {
		return Some(parent);
	}
	if tsast::is_function_expression(node)
		&& let Some(assignment) = tsast::skip_upwards(node).parent()
		&& tsast::is_property_assignment(assignment)
		&& let Some(literal) = assignment.parent()
		&& tsast::is_object_literal_expression(literal)
	{
		return Some(literal);
	}
	None
}

/// `luauDeclaredType`: a declaration's type, from its written type node when there is one, else
/// from the checker's type at the name.
fn luau_declared_type(state: &mut State<'_>, type_node: Option<P<Node>>, name: P<Node>, ctx: TypeContext) -> NodeId {
	if let Some(type_node) = type_node {
		return types(state).build_type_node(type_node, ctx).node;
	}
	let t = state.ck().get_type_at_location(name);
	types(state).build(t, ctx).node
}

/// `luauDefaultRefinement`: `local c = c :: T` after a parameter's default, or `None` when the
/// parameter's type admits nil anyway.
pub fn luau_default_refinement(state: &mut State<'_>, name: P<Node>, owner: P<Node>, id: NodeId) -> Option<NodeId> {
	let t = state.ck().get_type_at_location(name);
	let refined = types(state)
		.build(t, TypeContext::at(TypePosition::Local, Some(owner)))
		.node;
	if type_admits_nil(&state.ast, refined) {
		return None;
	}
	Some(luau_refinement_shadow(state, id, refined))
}

/// `luauRefinedCondition`: re-binds the `_condition` temp of a logical chain to the
/// expression's TS type once the chain has assigned it, and returns the identifier to use.
pub fn luau_refined_condition(state: &mut State<'_>, node: P<Node>, condition_id: NodeId) -> NodeId {
	if !state.is_luau_typed {
		return condition_id;
	}
	let t = state.get_type(node);
	let refined = types(state)
		.build(t, TypeContext::at(TypePosition::Local, Some(node)))
		.node;
	if type_admits_nil(&state.ast, refined) {
		return condition_id;
	}
	let shadow = luau_refinement_shadow(state, condition_id, refined);
	state.prereq(shadow);
	match state
		.ast
		.get::<VariableDeclaration>(shadow)
		.map(|declaration| declaration.left)
	{
		Some(NodeOrList::Node(left)) => left,
		_ => condition_id,
	}
}

/// `luauRefinementShadow`: `local id = id :: refined`.
fn luau_refinement_shadow(state: &mut State<'_>, id: NodeId, refined: NodeId) -> NodeId {
	let shadow = clone_type(&mut state.ast, id);
	state.ast.set_binding_type(shadow, None);
	let cast = state.ast.cast(id, refined);
	state.ast.variable_declaration(shadow, Some(cast.into()))
}

/// `typeAdmitsNil`: a spelling nil already satisfies: `T?`, `nil`, `any` and `unknown`.
fn type_admits_nil(ast: &Ast, node: NodeId) -> bool {
	if ast.kind(node) == LuauKind::TypeOptional {
		return true;
	}
	let Some(reference) = ast.get::<TypeReference>(node) else {
		return false;
	};
	if !reference.module.is_empty()
		|| reference
			.arguments
			.is_some_and(|arguments| !ast.list_is_empty(arguments))
	{
		return false;
	}
	matches!(reference.name.as_str(), NATIVE_TYPE_NIL | NATIVE_TYPE_ANY | "unknown")
}

/// `signatureGenerics`: a function's own generics, from the `luauTypes` builder or the native
/// emitter (none outside a `//!native` file).
pub fn signature_generics(state: &mut State<'_>, node: P<Node>) -> Vec<TypeGeneric> {
	if state.luau_typed_signatures() {
		return get_luau_function_generics(state, node);
	}
	get_native_function_generics(state, node)
}

/// `signatureReturnType`: `None` leaves the return unannotated.
pub fn signature_return_type(state: &mut State<'_>, node: P<Node>) -> Option<NodeId> {
	if state.luau_typed_signatures() {
		return Some(get_luau_return_type(state, node));
	}
	get_native_return_type(state, node)
}

/// `signatureVarArgsType`: `None` leaves `...` unannotated.
pub fn signature_var_args_type(state: &mut State<'_>, node: P<Node>) -> Option<NodeId> {
	if state.luau_typed_signatures() {
		return get_luau_vararg_type(state, node);
	}
	None
}

// -----------------------------------------------------------------------------------------
// Locals
// -----------------------------------------------------------------------------------------

/// `getLuauLocalType`: the annotation of a local bound to `identifier` with `right` as its
/// value (`None` for a hoisted or uninitialized local), or `None` to let Luau infer it.
pub fn get_luau_local_type(state: &mut State<'_>, identifier: P<Node>, right: Option<NodeId>) -> Option<NodeId> {
	let symbol = state.ck().get_symbol_at_location(identifier)?;
	if right.is_none()
		&& let Some(hoisted) = luau_hoisted_declaration_type(state, symbol, identifier.parent())
	{
		// A hoisted class or enum local carries the shape its own transform fills in later.
		return Some(hoisted);
	}
	if !should_annotate_local(state, symbol, right) {
		return None;
	}
	let ctx = TypeContext::at(TypePosition::Local, identifier.parent());
	if let Some(declaration) = symbol.value_declaration()
		&& tsast::is_variable_declaration(declaration)
		&& let Some(type_node) = declaration.type_node()
	{
		return Some(types(state).build_type_node(type_node, ctx).node);
	}
	let t = state.ck().get_type_of_symbol(symbol);
	Some(types(state).build(t, ctx).node)
}

/// `shouldAnnotateLocal`: a local is annotated only where inference would lose or change its
/// type: no initializer, a written type that is not the initializer's own (widened) type, or a
/// value wrapped by `TS.async`.
fn should_annotate_local(state: &mut State<'_>, symbol: P<Symbol>, right: Option<NodeId>) -> bool {
	if !state.is_luau_typed
		|| symbol
			.flags()
			.intersects(SymbolFlags::Class | SymbolFlags::Enum | SymbolFlags::Module)
	{
		return false;
	}
	let Some(right) = right else {
		return true;
	};
	if is_runtime_lib_call(&state.ast, right, "async") {
		return true;
	}
	let Some(declaration) = symbol
		.value_declaration()
		.filter(|&declaration| tsast::is_variable_declaration(declaration))
	else {
		return false;
	};
	let (Some(type_node), Some(initializer)) = (declaration.type_node(), declaration.initializer()) else {
		return false;
	};
	let written = state.ck().get_type_from_type_node(type_node);
	let at_initializer = state.ck().get_type_at_location(initializer);
	let base = state.ck().get_base_type_of_literal_type(at_initializer);
	let inferred = state.ck().get_widened_type(base);
	!state.ck().is_type_assignable_to(written, inferred) || !state.ck().is_type_assignable_to(inferred, written)
}

/// `isRuntimeLibCall`: a direct call of `TS.<name>(...)`.
fn is_runtime_lib_call(ast: &Ast, expression: NodeId, name: &str) -> bool {
	let Some(call) = ast.get::<CallExpression>(expression) else {
		return false;
	};
	let Some(access) = ast.get::<PropertyAccessExpression>(call.expression) else {
		return false;
	};
	if access.name != name {
		return false;
	}
	ast.get::<Identifier>(access.expression)
		.is_some_and(|root| root.name == "TS")
}

/// `luauTypedBinding`: annotates the identifier a local is bound to. An identifier shared
/// through `SymbolToID` is cloned first so the annotation stays on this binding alone.
pub fn luau_typed_binding(state: &mut State<'_>, symbol: P<Symbol>, id: NodeId, t: Option<NodeId>) -> NodeId {
	let Some(t) = t else {
		return id;
	};
	let mut id = id;
	if state.symbol_to_id.get(&symbol) == Some(&id) {
		id = clone_type(&mut state.ast, id);
	}
	state.ast.set_binding_type(id, Some(t));
	id
}
