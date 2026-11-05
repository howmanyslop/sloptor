//! `luautypes_class.go`: classes and enums in `luauTypes` mode.
//!
//! A class becomes a structural instance alias plus a class table whose functions take an
//! explicit, annotated `self`; an enum becomes a union (or `number`) alias plus an annotated
//! object local. The shapes were settled by probing luau-analyze 0.730 and luau-lsp 1.70.1 (the
//! probes live in `testdata/luautypes/probes`).

use rotor_luau::{FunctionDeclaration, TypeAccess, TypeGeneric, is_reserved_identifier, is_valid_identifier};
use rotor_tsrs::ast::{self as tsast, SymbolFlags};
use rotor_tsrs::checker::{
	LiteralValue, ObjectFlags, Signature, TypeFlags, get_declaration_modifier_flags_from_symbol,
};

use super::LuauClass;
use super::luautypes::{
	Builder, BuiltType, EntryId, FidelityLossKind, SymKey, TypeContext, TypePosition, any_type, clone_type, types,
};
use super::luautypes_builder::{LUAU_TYPE_MAX_DEPTH, declared_arguments, is_readonly_property, recursive_args_match};
use super::luautypes_emit::{get_luau_binding_element_type, get_luau_parameter_type, get_luau_vararg_type};
use super::nativetypes::{NATIVE_TYPE_NUMBER, NATIVE_TYPE_STRING};
use crate::classes::transform_property_initializers;
use crate::dispatch::transform_expression;
use crate::identifier::transform_identifier_defined;
use crate::prelude::*;
use crate::shared::{
	create_class_name_function, find_constructor, get_extends_node, is_class_hoisted, is_omitted_binding_element,
	needs_inverse_entry, type_generic_names,
};
use crate::spreadhelpers::array_like_expression_contains_spread;
use crate::types::get_type_arguments;

/// `constructorMethodName` (`classes.go`).
const CONSTRUCTOR_METHOD_NAME: &str = "constructor";
/// `magicToStringMethod` (`classes.go`).
const MAGIC_TO_STRING_METHOD: &str = "toString";

// -----------------------------------------------------------------------------------------
// Instance alias body
// -----------------------------------------------------------------------------------------

impl Builder<'_, '_> {
	/// `buildClassInstance`: at the top of the class's own alias body the structural member
	/// table; everywhere else a reference to the alias.
	pub fn build_class_instance(&mut self, t: P<Type>, ctx: TypeContext) -> BuiltType {
		let symbol = t.symbol();
		let Some(entry) = self.alias_for(symbol.map(SymKey::Real)) else {
			let any = any_type(self.ast());
			let node = self.lose(
				FidelityLossKind::Unspellable,
				Some(t),
				any,
				ctx,
				"the class has no instance alias",
			);
			return self.finish(node, false);
		};
		if self.b().entry(entry).building && ctx.position == TypePosition::AliasBody {
			let table = self.build_class_instance_table(t, ctx);
			if let Some(symbol) = symbol {
				self.set_origin(table, SymKey::Real(symbol));
			}
			return self.finish(table, false);
		}
		let arguments = class_type_arguments(self.s, t);
		let reference = self.class_reference(entry, &arguments, ctx);
		self.finish(reference, false)
	}

	/// `ClassInstanceReference`: the reference to a class's instance alias with the class's own
	/// type parameters as arguments; `any` with a loss when the class has no alias.
	pub fn class_instance_reference(&mut self, t: P<Type>, ctx: TypeContext) -> NodeId {
		let Some(entry) = self.alias_for(t.symbol().map(SymKey::Real)) else {
			let any = any_type(self.ast());
			return self.lose(
				FidelityLossKind::Unspellable,
				Some(t),
				any,
				ctx,
				"the class has no instance alias",
			);
		};
		let arguments = class_type_arguments(self.s, t);
		self.class_reference(entry, &arguments, ctx)
	}

	/// `classReference`: `Name<A, B>` for a registered class alias.
	fn class_reference(&mut self, entry: EntryId, arguments: &[P<Type>], ctx: TypeContext) -> NodeId {
		let building = self.b().entry(entry).building;
		if building && !recursive_args_match(self.b().entry(entry), arguments) {
			let name = self.b().entry(entry).name.clone();
			let any = any_type(self.ast());
			return self.lose_text(
				FidelityLossKind::RecursiveAliasArgs,
				&name,
				any,
				ctx,
				"a recursive alias must be referenced with its own type arguments",
			);
		}
		let declared = declared_arguments(self.b().entry(entry), arguments);
		let type_arguments = self.build_type_args(&declared, ctx.nested());
		let (name, symbol) = {
			let data = self.b().entry(entry);
			(data.name.clone(), data.symbol)
		};
		let reference = self.ast().type_reference(name, type_arguments);
		self.set_origin(reference, symbol);
		reference
	}

	/// `buildClassInstanceTable`: the members of a class instance as the body of its alias.
	fn build_class_instance_table(&mut self, t: P<Type>, ctx: TypeContext) -> NodeId {
		let fields = self.ast().new_list::<element::TypeTableField>();
		let nested = ctx.nested();
		let mut symbol_key_lost = false;
		let mut readonly_lost = false;
		let mut private_lost = false;
		let properties = self.s.ck().get_properties_of_type(t);
		for &property in properties {
			let name = property.name();
			if name == CONSTRUCTOR_METHOD_NAME {
				continue;
			}
			if is_symbol_keyed_member(name) {
				if !symbol_key_lost {
					self.record(
						FidelityLossKind::SymbolKey,
						name.to_owned(),
						None,
						ctx,
						"symbol-keyed members are skipped",
					);
					symbol_key_lost = true;
				}
				continue;
			}
			let property_type = self.s.ck().get_type_of_symbol(property);
			let callable = self.s.ck().get_non_nullable_type(property_type);
			let mut value = if property.flags().intersects(SymbolFlags::Method)
				&& !self.s.ck().get_call_signatures(callable).is_empty()
			{
				let receiver = self.class_member_receiver(t, name, nested);
				self.build_callable(callable, Some(receiver), nested)
			} else {
				self.build(property_type, nested).node
			};
			if property.flags().intersects(SymbolFlags::Optional) {
				value = self.ast().optional_type(value);
			}
			if !readonly_lost && is_readonly_property(property) {
				self.lose_text(
					FidelityLossKind::ClassReadonlyDropped,
					name,
					value,
					ctx,
					"class constructors write through self, so readonly members stay writable",
				);
				readonly_lost = true;
			}
			if !private_lost
				&& get_declaration_modifier_flags_from_symbol(property)
					.intersects(tsast::ModifierFlags::Private | tsast::ModifierFlags::Protected)
			{
				self.lose_text(
					FidelityLossKind::PrivateExposed,
					name,
					value,
					ctx,
					"private and protected members are plain table fields in Luau",
				);
				private_lost = true;
			}
			let prop = self.ast().type_table_prop(TypeAccess::None, name, value);
			self.ast().list_push(fields, prop);
		}
		if let Some(indexer) = self.build_indexer(t, ctx) {
			self.ast().list_push(fields, indexer);
		}
		self.ast().type_table(fields)
	}

	/// `classMemberReceiver`: the `self` type of a method as listed in the alias of instance
	/// type `t`: the alias of the type that first declared the method, with the type arguments
	/// `t` passes down to it.
	fn class_member_receiver(&mut self, t: P<Type>, name: &str, ctx: TypeContext) -> NodeId {
		let mut symbol = t.symbol();
		let mut arguments = class_type_arguments(self.s, t);
		let mut depth = 0;
		while let Some(current) = symbol
			&& depth < LUAU_TYPE_MAX_DEPTH
		{
			let Some(next) = self.declaring_supertype(current, &arguments, name, depth > 0, ctx) else {
				break;
			};
			symbol = Some(next.symbol);
			arguments = next.arguments;
			depth += 1;
		}
		let mut entry = self.alias_for(symbol.map(SymKey::Real));
		if entry.is_none() {
			symbol = t.symbol();
			arguments = class_type_arguments(self.s, t);
			entry = self.alias_for(symbol.map(SymKey::Real));
		}
		let Some(entry) = entry else {
			let any = any_type(self.ast());
			return self.lose(
				FidelityLossKind::Unspellable,
				Some(t),
				any,
				ctx,
				"the class has no instance alias",
			);
		};
		self.class_reference(entry, &arguments, ctx)
	}

	/// `declaringSupertype`: the first supertype of `symbol` that declares a member called
	/// `name`: its base class before its implemented interfaces, in clause order.
	fn declaring_supertype(
		&mut self,
		symbol: P<Symbol>,
		arguments: &[P<Type>],
		name: &str,
		substitute: bool,
		ctx: TypeContext,
	) -> Option<Heritage> {
		for supertype in self.supertypes(symbol, arguments, substitute, ctx) {
			let declared = self.s.ck().get_declared_type_of_symbol(supertype.symbol);
			if self.s.ck().get_property_of_type(declared, name).is_some() {
				return Some(supertype);
			}
		}
		None
	}

	/// `supertypes`: the classes and interfaces `symbol` extends and implements, each with the
	/// arguments `symbol` passes to it.
	fn supertypes(
		&mut self,
		symbol: P<Symbol>,
		arguments: &[P<Type>],
		substitute: bool,
		ctx: TypeContext,
	) -> Vec<Heritage> {
		let declared = self.s.ck().get_declared_type_of_symbol(symbol);
		if !declared.flags().intersects(TypeFlags::Object)
			|| !declared.object_flags().intersects(ObjectFlags::ClassOrInterface)
		{
			return Vec::new();
		}
		let mut candidates: Vec<P<Type>> = self.s.ck().get_base_types(declared).to_vec();
		for &declaration in symbol.declarations() {
			if tsast::is_class_like(declaration) {
				for &node in tsast::get_implements_heritage_clause_elements(declaration) {
					candidates.push(self.s.ck().get_type_from_type_node(node));
				}
			}
		}
		let parameters = self
			.s
			.ck()
			.get_local_type_parameters_of_class_or_interface_or_type_alias(symbol);
		let mut result = Vec::new();
		for candidate in candidates {
			if !candidate.flags().intersects(TypeFlags::Object) {
				continue;
			}
			let Some(super_symbol) = candidate.symbol() else {
				continue;
			};
			if !super_symbol
				.flags()
				.intersects(SymbolFlags::Class | SymbolFlags::Interface)
			{
				continue;
			}
			let mut super_arguments = class_type_arguments(self.s, candidate);
			if substitute && !parameters.is_empty() {
				super_arguments = self.thread_type_arguments(&parameters, arguments, &super_arguments, ctx);
			}
			result.push(Heritage {
				symbol: super_symbol,
				arguments: super_arguments,
			});
		}
		result
	}

	/// `threadTypeArguments`: replaces, in a supertype's arguments, each of the subtype's own
	/// parameters by the argument the subtype received for it.
	fn thread_type_arguments(
		&mut self,
		parameters: &[P<Type>],
		arguments: &[P<Type>],
		super_arguments: &[P<Type>],
		ctx: TypeContext,
	) -> Vec<P<Type>> {
		let mut threaded = Vec::with_capacity(super_arguments.len());
		for &argument in super_arguments {
			let position = parameters.iter().position(|&parameter| parameter == argument);
			if let Some(position) = position
				&& position < arguments.len()
			{
				threaded.push(arguments[position]);
			} else if argument
				.flags()
				.intersects(TypeFlags::Object | TypeFlags::UnionOrIntersection | TypeFlags::Instantiable)
			{
				let any = any_type(self.ast());
				self.lose(
					FidelityLossKind::Unspellable,
					Some(argument),
					any,
					ctx,
					"a supertype argument over a generic type's parameter two levels up is not instantiated",
				);
				threaded.push(self.s.ck().get_any_type());
			} else {
				threaded.push(argument);
			}
		}
		threaded
	}

	/// `anonymousClassName`: numbers the alias of a class expression that neither has a name nor
	/// is assigned to a variable.
	fn anonymous_class_name(&mut self) -> String {
		self.b().anonymous_class_count += 1;
		format!("AnonymousClass_{}", self.b().anonymous_class_count)
	}
}

/// `heritage`: one supertype a class or interface declares, with the type arguments the
/// subtype passes to it.
struct Heritage {
	symbol: P<Symbol>,
	arguments: Vec<P<Type>>,
}

/// `classTypeArguments`: the type arguments of a class instance type that belong to the class's
/// own type parameters.
pub fn class_type_arguments(state: &mut State<'_>, t: P<Type>) -> Vec<P<Type>> {
	let mut arguments = get_type_arguments(state, t).to_vec();
	let Some(symbol) = t.symbol() else {
		return arguments;
	};
	let count = state
		.ck()
		.get_local_type_parameters_of_class_or_interface_or_type_alias(symbol)
		.len();
	arguments.truncate(count);
	arguments
}

/// `isSymbolKeyedMember`: a computed or ES-symbol member name.
pub fn is_symbol_keyed_member(name: &str) -> bool {
	name.starts_with("__@") || name.starts_with(tsast::INTERNAL_SYMBOL_NAME_PREFIX)
}

/// `classExpressionAliasName`: the name of the variable a class expression initializes, else
/// `AnonymousClass_N`.
pub fn class_expression_alias_name(state: &mut State<'_>, symbol: P<Symbol>) -> String {
	if let Some(declaration) = symbol.value_declaration()
		&& tsast::is_class_expression(declaration)
		&& let Some(parent) = declaration.parent()
		&& tsast::is_variable_declaration(parent)
		&& let Some(name) = parent.name()
		&& tsast::is_identifier(name)
	{
		return name.text().to_owned();
	}
	types(state).anonymous_class_name()
}

/// `luauHoistedDeclarationType`: the annotation of the hoisted `local` of a class or enum
/// referenced before its declaration.
pub fn luau_hoisted_declaration_type(
	state: &mut State<'_>,
	symbol: P<Symbol>,
	owner: Option<P<Node>>,
) -> Option<NodeId> {
	let flags = symbol.flags();
	if flags.intersects(SymbolFlags::Enum) && state.is_luau_typed {
		let declaration = symbol
			.value_declaration()
			.filter(|&declaration| tsast::is_enum_declaration(declaration))?;
		return Some(luau_enum_object_type(state, declaration));
	}
	if flags.intersects(SymbolFlags::Class) && state.luau_typed_signatures() {
		let ctx = TypeContext::at(TypePosition::Local, owner);
		let mut builder = types(state);
		let any = any_type(builder.ast());
		return Some(builder.lose_text(
			FidelityLossKind::Unspellable,
			symbol.name(),
			any,
			ctx,
			"a class referenced before its declaration has no type Luau can name there",
		));
	}
	None
}

// -----------------------------------------------------------------------------------------
// Class emission
// -----------------------------------------------------------------------------------------

/// `newLuauClass`: registers the class's instance alias and collects its generics.
pub fn new_luau_class(state: &mut State<'_>, node: P<Node>, name: NodeId) -> LuauClass {
	let instance_type = node
		.symbol()
		.map(|symbol| state.ck().get_declared_type_of_symbol(symbol));
	let mut builder = types(state);
	builder.alias_for(instance_type.and_then(|t| t.symbol()).map(SymKey::Real));
	let generics = builder.generics(node);
	LuauClass {
		node,
		name,
		instance_type,
		generics,
	}
}

impl LuauClass {
	/// `genericNames`: the class generics for the scope pushed around the class body.
	#[must_use]
	pub fn generic_names(&self) -> Vec<String> {
		type_generic_names(&self.generics)
	}

	/// `functionGenerics`: the generic list every instance function re-declares.
	fn function_generics(&self) -> Vec<TypeGeneric> {
		self.generics.clone()
	}

	/// `instanceReference`: a fresh `Foo<T...>` for one annotation site.
	fn instance_reference(&self, state: &mut State<'_>) -> NodeId {
		let ctx = TypeContext::at(TypePosition::Parameter, Some(self.node));
		let mut builder = types(state);
		if let Some(t) = self.instance_type {
			return builder.class_instance_reference(t, ctx);
		}
		let any = any_type(builder.ast());
		builder.lose(
			FidelityLossKind::Unspellable,
			None,
			any,
			ctx,
			"the class has no instance alias",
		)
	}

	/// `classReference`: a fresh `typeof(Foo)`, the type statics take as `self`.
	fn class_reference(&self, state: &mut State<'_>) -> NodeId {
		let name = clone_type(&mut state.ast, self.name);
		state.ast.type_typeof(name)
	}

	/// `receiverFor`: `typeof(Foo)` for a static member, the instance alias otherwise.
	#[expect(clippy::unnecessary_wraps, reason = "the signature slice iii calls")]
	pub fn receiver_for(&self, state: &mut State<'_>, member: P<Node>) -> Option<NodeId> {
		if tsast::has_static_modifier(member) {
			return Some(self.class_reference(state));
		}
		Some(self.instance_reference(state))
	}

	/// `selfParameter`: the leading `self: Foo<T...>` of a constructor or metamethod.
	fn self_parameter(&self, state: &mut State<'_>) -> NodeId {
		let this = state.ast.global_id("self");
		let reference = self.instance_reference(state);
		state.ast.set_binding_type(this, Some(reference));
		this
	}

	/// `boilerplate`: the statements that declare the class table and the statements that open
	/// the block (`Foo.__index = Foo`, `Foo.new`).
	pub fn boilerplate(&self, state: &mut State<'_>, is_class_expression: bool) -> (Statements, Statements) {
		let node = self.node;
		let class_name = self.name;
		let outer = state.ast.new_list();
		let inner = state.ast.new_list();
		let is_abstract = tsast::has_abstract_modifier(node);
		let extends_node = get_extends_node(node);
		let plain_table = is_abstract && extends_node.is_none();

		let value = if plain_table {
			let fields = state.ast.new_list::<element::MapField>();
			state.ast.map(fields)
		} else {
			let metatable_fields = state.ast.new_list::<element::MapField>();
			let tostring_name = state
				.ast
				.get::<rotor_luau::Identifier>(class_name)
				.map_or_else(|| "Anonymous".to_owned(), |id| id.name.clone());
			let key = state.ast.str("__tostring");
			let function = create_class_name_function(&mut state.ast, &tostring_name);
			let field = state.ast.map_field(key, function);
			state.ast.list_push(metatable_fields, field);
			if let Some(extends_node) = extends_node {
				let (extends_exp, extends_exp_prereqs) = state.capture(|state| match extends_node.expression() {
					Some(expression) => transform_expression(state, expression),
					None => state.ast.none(),
				});
				let super_id = state.ast.global_id("super");
				state.ast.list_push_list(outer, extends_exp_prereqs);
				let declaration = state.ast.variable_declaration(super_id, Some(extends_exp.into()));
				state.ast.list_push(outer, declaration);
				let key = state.ast.str("__index");
				let field = state.ast.map_field(key, super_id);
				state.ast.list_push(metatable_fields, field);
			}
			let setmetatable = state.ast.global_id("setmetatable");
			let empty_fields = state.ast.new_list::<element::MapField>();
			let empty = state.ast.map(empty_fields);
			let metatable = state.ast.map(metatable_fields);
			let args = state.ast.list_from::<element::Expression>([empty, metatable]);
			state.ast.call(setmetatable, args)
		};

		if is_class_expression && let Some(name) = node.name() {
			// The named class expression's inner name is a local of the block.
			let id = transform_identifier_defined(state, name);
			let declaration = state.ast.variable_declaration(id, Some(value.into()));
			state.ast.list_push(inner, declaration);
		} else if is_class_hoisted(state, node) {
			let assignment = state.ast.assignment(class_name, AssignmentOperator::Assign, value);
			state.ast.list_push(outer, assignment);
		} else {
			let declaration = state.ast.variable_declaration(class_name, Some(value.into()));
			state.ast.list_push(outer, declaration);
		}

		if !plain_table {
			let access = state.ast.property_access(class_name, "__index");
			let assignment = state.ast.assignment(access, AssignmentOperator::Assign, class_name);
			state.ast.list_push(inner, assignment);
		}
		if !is_abstract {
			let function = self.new_function(state);
			state.ast.list_push(inner, function);
		}
		(outer, inner)
	}

	/// `newFunction`: `function Foo.new<T...>(params): Foo<T...>` constructing the instance
	/// through the class table's constructor.
	fn new_function(&self, state: &mut State<'_>) -> NodeId {
		let class_name = self.name;
		let arity = self.constructor_arity(state);
		let (parameters, has_dot_dot_dot, var_args_type, arguments) = arity.parameters(self, state);

		let statements = state.ast.new_list::<element::Statement>();
		// local self = (setmetatable({}, className) :: any) :: Foo<T...>
		let this = state.ast.global_id("self");
		let setmetatable = state.ast.global_id("setmetatable");
		let empty_fields = state.ast.new_list::<element::MapField>();
		let empty = state.ast.map(empty_fields);
		let args = state.ast.list_from::<element::Expression>([empty, class_name]);
		let call = state.ast.call(setmetatable, args);
		let any = any_type(&mut state.ast);
		let inner_cast = state.ast.cast(call, any);
		let reference = self.instance_reference(state);
		let outer_cast = state.ast.cast(inner_cast, reference);
		let declaration = state.ast.variable_declaration(this, Some(outer_cast.into()));
		state.ast.list_push(statements, declaration);
		// return className.constructor(self, ...) or self
		let this_argument = state.ast.global_id("self");
		state.ast.list_unshift(arguments, this_argument);
		let constructor = state.ast.property_access(class_name, CONSTRUCTOR_METHOD_NAME);
		let call = state.ast.call(constructor, arguments);
		let this_fallback = state.ast.global_id("self");
		let binary = state.ast.binary(call, BinaryOperator::Or, this_fallback);
		let ret = state.ast.return_statement(binary);
		state.ast.list_push(statements, ret);

		let name = state.ast.property_access(class_name, "new");
		let function = state
			.ast
			.function_declaration(false, name, parameters, has_dot_dot_dot, statements);
		let generics = self.function_generics();
		let return_type = self.instance_reference(state);
		if let Some(payload) = state.ast.get_mut::<FunctionDeclaration>(function) {
			payload.generics = generics;
			payload.return_type = Some(return_type);
			payload.var_args_type = var_args_type;
		}
		function
	}

	/// `constructorFunction`: a transformed constructor body as
	/// `function Foo.constructor<T...>(self: Foo<T...>, params)`.
	pub fn constructor_function(
		&self,
		state: &mut State<'_>,
		parameters: List<element::AnyIdentifier>,
		has_dot_dot_dot: bool,
		statements: Statements,
		var_args_type: Option<NodeId>,
	) -> NodeId {
		let this = self.self_parameter(state);
		state.ast.list_unshift(parameters, this);
		let name = state.ast.property_access(self.name, CONSTRUCTOR_METHOD_NAME);
		let function = state
			.ast
			.function_declaration(false, name, parameters, has_dot_dot_dot, statements);
		let generics = self.function_generics();
		if let Some(payload) = state.ast.get_mut::<FunctionDeclaration>(function) {
			payload.generics = generics;
			payload.var_args_type = var_args_type;
		}
		function
	}

	/// `implicitConstructor`: a derived class forwards the base constructor's parameters, then
	/// the property initializers run.
	pub fn implicit_constructor(&self, state: &mut State<'_>) -> Statements {
		let node = self.node;
		let statements = state.ast.new_list::<element::Statement>();
		let mut parameters = state.ast.new_list::<element::AnyIdentifier>();
		let mut has_dot_dot_dot = false;
		let mut var_args_type = None;
		if get_extends_node(node).is_some() {
			let arity = self.constructor_arity(state);
			let arguments;
			(parameters, has_dot_dot_dot, var_args_type, arguments) = arity.parameters(self, state);
			let this = state.ast.global_id("self");
			state.ast.list_unshift(arguments, this);
			let super_id = state.ast.global_id("super");
			let access = state.ast.property_access(super_id, CONSTRUCTOR_METHOD_NAME);
			let call = state.ast.call(access, arguments);
			let statement = state.ast.call_statement(call);
			state.ast.list_push(statements, statement);
		}
		let initializers = transform_property_initializers(state, node);
		state.ast.list_push_list(statements, initializers);
		let constructor = self.constructor_function(state, parameters, has_dot_dot_dot, statements, var_args_type);
		state.ast.list_from([constructor])
	}

	/// `toStringFunction`: `function Foo.__tostring<T...>(self: Foo<T...>): string` forwarding
	/// to the TS `toString` method.
	pub fn to_string_function(&self, state: &mut State<'_>) -> NodeId {
		let name = state.ast.property_access(self.name, "__tostring");
		let this = self.self_parameter(state);
		let parameters = state.ast.list_from::<element::AnyIdentifier>([this]);
		let receiver = state.ast.global_id("self");
		let args = state.ast.new_list::<element::Expression>();
		let call = state.ast.method_call(MAGIC_TO_STRING_METHOD, receiver, args);
		let ret = state.ast.return_statement(call);
		let statements = state.ast.list_from::<element::Statement>([ret]);
		let function = state
			.ast
			.function_declaration(false, name, parameters, false, statements);
		let generics = self.function_generics();
		let return_type = state.ast.type_name(NATIVE_TYPE_STRING);
		if let Some(payload) = state.ast.get_mut::<FunctionDeclaration>(function) {
			payload.generics = generics;
			payload.return_type = Some(return_type);
		}
		function
	}

	/// `constructorArity`: how `new` and an implicit constructor take their arguments.
	fn constructor_arity(&self, state: &mut State<'_>) -> LuauConstructorArity {
		if let Some(constructor) = find_constructor(self.node) {
			return LuauConstructorArity::Declaration(constructor);
		}
		let class_type = self
			.node
			.symbol()
			.and_then(|symbol| state.ck().get_type_of_symbol_at_location(symbol, Some(self.node)));
		if let Some(class_type) = class_type {
			let signatures = state.ck().get_construct_signatures(class_type);
			if signatures.len() == 1 {
				return LuauConstructorArity::Signature(signatures[0]);
			}
		}
		if get_extends_node(self.node).is_some() {
			LuauConstructorArity::Variadic
		} else {
			LuauConstructorArity::None
		}
	}
}

/// `luauConstructorArity`: builds fresh parameter lists plus the argument list that forwards
/// them.
enum LuauConstructorArity {
	Declaration(P<Node>),
	Signature(P<Signature>),
	Variadic,
	None,
}

/// What `parameters` returns: the annotated parameters, whether they end in `...`, the `...`
/// annotation, and the forwarding arguments.
type ArityParameters = (
	List<element::AnyIdentifier>,
	bool,
	Option<NodeId>,
	List<element::Expression>,
);

impl LuauConstructorArity {
	/// `parameters`
	fn parameters(&self, class: &LuauClass, state: &mut State<'_>) -> ArityParameters {
		let parameters = state.ast.new_list::<element::AnyIdentifier>();
		let arguments = state.ast.new_list::<element::Expression>();
		let mut add = |state: &mut State<'_>, id: NodeId, t: Option<NodeId>| {
			state.ast.set_binding_type(id, t);
			state.ast.list_push(parameters, id);
			let argument = clone_type(&mut state.ast, id);
			state.ast.set_binding_type(argument, None);
			state.ast.list_push(arguments, argument);
		};
		let (has_dot_dot_dot, var_args_type) = match *self {
			Self::Declaration(declaration) => declared_parameters(state, declaration, &mut add),
			Self::Signature(signature) => signature_parameters(state, class, signature, &mut add),
			Self::Variadic => (true, None),
			Self::None => (false, None),
		};
		if has_dot_dot_dot {
			let var_args = state.ast.var_args();
			state.ast.list_push(arguments, var_args);
		}
		(parameters, has_dot_dot_dot, var_args_type, arguments)
	}
}

type AddParameter<'f> = dyn FnMut(&mut State<'_>, NodeId, Option<NodeId>) + 'f;

/// `declaredParameters`: mirrors `transformParameters` over the constructor declaration.
fn declared_parameters(
	state: &mut State<'_>,
	declaration: P<Node>,
	add: &mut AddParameter<'_>,
) -> (bool, Option<NodeId>) {
	for &parameter_node in declaration.parameters() {
		let parameter = parameter_node.as_parameter_declaration();
		let name = parameter.name;
		if tsast::is_this_identifier(name) {
			continue;
		}
		let rest = parameter.dot_dot_dot_token().is_some();
		if rest && tsast::is_array_binding_pattern(name) && !array_like_expression_contains_spread(name) {
			for &element in name.elements() {
				if is_omitted_binding_element(element) {
					let temp = state.ast.temp_id("");
					add(state, temp, None);
					continue;
				}
				let binding_element = element.as_binding_element();
				if binding_element.dot_dot_dot_token().is_some() {
					break;
				}
				let id = match binding_element.name {
					Some(name) => parameter_identifier(state, name),
					None => state.ast.temp_id("param"),
				};
				let t = Some(get_luau_binding_element_type(state, element));
				add(state, id, t);
			}
			continue;
		}
		if rest {
			return (true, get_luau_vararg_type(state, declaration));
		}
		let id = parameter_identifier(state, name);
		let t = Some(get_luau_parameter_type(state, parameter_node));
		add(state, id, t);
	}
	(false, None)
}

/// `parameterIdentifier`: a fresh identifier for a declared parameter name.
fn parameter_identifier(state: &mut State<'_>, name: P<Node>) -> NodeId {
	if tsast::is_identifier(name) {
		let id = transform_identifier_defined(state, name);
		return clone_type(&mut state.ast, id);
	}
	state.ast.temp_id("param")
}

/// `signatureParameters`: an inherited construct signature, spelled the way `BuildSignature`
/// spells one.
fn signature_parameters(
	state: &mut State<'_>,
	class: &LuauClass,
	signature: P<Signature>,
	add: &mut AddParameter<'_>,
) -> (bool, Option<NodeId>) {
	let ctx = TypeContext::at(TypePosition::Parameter, Some(class.node));
	let signature_parameters = signature.parameters();
	let min_argument_count = signature.min_argument_count();
	let has_rest = signature.has_rest_parameter();
	for (index, &parameter) in signature_parameters.iter().enumerate() {
		let parameter_type = state.ck().get_type_of_symbol(parameter);
		if has_rest && index == signature_parameters.len() - 1 {
			let element = state.ck().get_element_type_of_array_type(parameter_type);
			let mut builder = types(state);
			if let Some(element) = element {
				return (true, Some(builder.build(element, ctx).node));
			}
			let any = any_type(builder.ast());
			return (
				true,
				Some(builder.lose(
					FidelityLossKind::Unspellable,
					Some(parameter_type),
					any,
					ctx,
					"a rest parameter that is not an array has no Luau spelling",
				)),
			);
		}
		let mut builder = types(state);
		let mut node = builder.build(parameter_type, ctx).node;
		if index >= min_argument_count {
			node = builder.ast().optional_type(node);
		}
		let name = parameter.name();
		let id = if is_valid_identifier(name) && !is_reserved_identifier(name) && name != "self" {
			state.ast.id(name)
		} else {
			state.ast.temp_id("param")
		};
		add(state, id, Some(node));
	}
	(false, None)
}

// -----------------------------------------------------------------------------------------
// Enums
// -----------------------------------------------------------------------------------------

/// `luauEnumObjectType`: the annotation of an enum's object local.
pub fn luau_enum_object_type(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let members = node.as_enum_declaration().members.nodes();
	let all_strings = !members.iter().any(|&member| needs_inverse_entry(state, member));
	let access = if all_strings {
		TypeAccess::Read
	} else {
		TypeAccess::None
	};
	let fields = state.ast.new_list::<element::TypeTableField>();
	for &member in members {
		let Some(member_name) = member.name() else {
			continue;
		};
		let name = tsast::get_property_name_for_property_name_node(member_name);
		if name == tsast::INTERNAL_SYMBOL_NAME_MISSING {
			continue;
		}
		let value = match state.ck().get_constant_value(member) {
			Some(LiteralValue::String(text)) => state.ast.type_string_singleton(text),
			_ => state.ast.type_name(NATIVE_TYPE_NUMBER),
		};
		let prop = state.ast.type_table_prop(access, name.as_ref(), value);
		state.ast.list_push(fields, prop);
	}
	if !all_strings {
		let key = state.ast.type_name(NATIVE_TYPE_NUMBER);
		let value = state.ast.type_name(NATIVE_TYPE_STRING);
		let indexer = state.ast.type_table_indexer(TypeAccess::None, key, value);
		state.ast.list_push(fields, indexer);
	}
	state.ast.type_table(fields)
}

/// `luauEnumBinding`: the enum's identifier annotated with its object type, cloned so the
/// annotation stays on the declaration alone.
pub fn luau_enum_binding(state: &mut State<'_>, node: P<Node>, id: NodeId) -> NodeId {
	if !state.is_luau_typed {
		return id;
	}
	let declared = clone_type(&mut state.ast, id);
	let object_type = luau_enum_object_type(state, node);
	state.ast.set_binding_type(declared, Some(object_type));
	declared
}

/// `luauEnumTableCast`: the `setmetatable({}, { __index = _inverse })` a numeric enum starts
/// from, widened to `any` in `luauTypes` mode.
pub fn luau_enum_table_cast(state: &mut State<'_>, _node: P<Node>, table: NodeId) -> NodeId {
	if !state.is_luau_typed {
		return table;
	}
	let any = any_type(&mut state.ast);
	state.ast.cast(table, any)
}
