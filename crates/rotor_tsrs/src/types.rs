//! rotor's type predicates: the port of `internal/transformer/types.go`, itself the port of
//! roblox-ts's `TSTransformer/util/types.ts`.
//!
//! [`is_definitely_type`] and [`is_possibly_type`] combine leaf [`TypeCheck`]s over unions,
//! intersections, base types and constraints. Some leaves compare a type's symbol with the global
//! ambient symbols the transformer's `MacroManager` resolves; a [`TypeContext`] supplies those, so
//! the transformer's state can share its own registry. [`TypeState`] is a ready-made context for
//! callers without one.

use std::collections::HashMap;

use tsrs_ast::{
	Node, Symbol, SymbolFlags, get_source_file_of_node, is_construct_signature_declaration, is_interface_declaration,
	is_type_alias_declaration,
};
use tsrs_core::P;
use tsrs_core::jsnum::Number;

use crate::ast::skip_upwards;
use crate::checker::{Checker, LiteralValue, ObjectFlags, Type, TypeChecker, TypeFlags};

/// The nominal property every `LuaTuple<T>` carries (`NominalLuaTupleName`).
pub const NOMINAL_LUA_TUPLE_NAME: &str = "_nominal_LuaTuple";

/// What the predicates need from the transformer: the checker, and the global ambient symbols the
/// `MacroManager` resolves.
pub trait TypeContext {
	/// The checker the types came from.
	fn checker(&mut self) -> &mut Checker;

	/// The global symbol `name` resolves to with `SymbolFlags::All`, memoized, misses included
	/// (`State.AmbientSymbol`). `None` in projects without `@rbxts/compiler-types`.
	fn ambient_symbol(&mut self, name: &str) -> Option<P<Symbol>>;

	/// The `_nominal_LuaTuple` property symbol of the ambient `LuaTuple<T>` alias
	/// (`State.LuaTupleNominalSymbol`).
	fn lua_tuple_nominal_symbol(&mut self) -> Option<P<Symbol>>;
}

/// A memo of ambient symbols, the registry half of the `MacroManager` (`MacroManager.Symbol` and
/// `MacroManager.LuaTupleNominalSymbol`).
#[derive(Debug, Default)]
pub struct AmbientSymbols {
	symbols: HashMap<String, Option<P<Symbol>>>,
	lua_tuple_nominal: Option<P<Symbol>>,
	lua_tuple_nominal_resolved: bool,
}

impl AmbientSymbols {
	/// The global symbol `name` resolves to, memoized (`MacroManager.Symbol`).
	pub fn symbol(&mut self, checker: &mut Checker, name: &str) -> Option<P<Symbol>> {
		if let Some(&symbol) = self.symbols.get(name) {
			return symbol;
		}
		let symbol = TypeChecker::new(checker).resolve_name(name, None, SymbolFlags::All, false);
		self.symbols.insert(name.to_owned(), symbol);
		symbol
	}

	/// The `_nominal_LuaTuple` property of the type of the first `LuaTuple` type alias
	/// declaration, memoized (`MacroManager.LuaTupleNominalSymbol`).
	pub fn lua_tuple_nominal(&mut self, checker: &mut Checker) -> Option<P<Symbol>> {
		if self.lua_tuple_nominal_resolved {
			return self.lua_tuple_nominal;
		}
		let nominal = self.symbol(checker, "LuaTuple").and_then(|lua_tuple| {
			let declaration = lua_tuple
				.declarations()
				.iter()
				.copied()
				.find(|&declaration| is_type_alias_declaration(declaration))?;
			let mut checker = TypeChecker::new(checker);
			let alias = checker.get_type_at_location(declaration);
			checker.get_property_of_type(alias, NOMINAL_LUA_TUPLE_NAME)
		});
		self.lua_tuple_nominal = nominal;
		self.lua_tuple_nominal_resolved = true;
		nominal
	}
}

/// A [`TypeContext`] over a borrowed checker and ambient-symbol memo.
pub struct TypeState<'a> {
	checker: &'a mut Checker,
	ambient: &'a mut AmbientSymbols,
}

impl<'a> TypeState<'a> {
	/// Pairs a checker with the memo of the symbols it resolved.
	pub const fn new(checker: &'a mut Checker, ambient: &'a mut AmbientSymbols) -> Self {
		Self { checker, ambient }
	}
}

impl TypeContext for TypeState<'_> {
	fn checker(&mut self) -> &mut Checker {
		self.checker
	}

	fn ambient_symbol(&mut self, name: &str) -> Option<P<Symbol>> {
		self.ambient.symbol(self.checker, name)
	}

	fn lua_tuple_nominal_symbol(&mut self) -> Option<P<Symbol>> {
		self.ambient.lua_tuple_nominal(self.checker)
	}
}

/// The context's checker, with rotor's API on it.
fn ck(context: &mut impl TypeContext) -> TypeChecker<'_> {
	TypeChecker::new(context.checker())
}

/// A leaf predicate (`TypeCheck`). Comparing two of them is the identity test roblox-ts makes on
/// its callbacks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeCheck {
	/// The checker's intrinsic `any` (`IsAnyType`).
	Any,
	/// Boolean-like (`IsBooleanType`).
	Boolean,
	/// The regular `true` or `false` literal, or any non-literal boolean (`IsBooleanLiteralType`).
	BooleanLiteral(bool),
	/// Number-like (`IsNumberType`).
	Number,
	/// A number literal with this value, or any non-literal number (`IsNumberLiteralType`).
	NumberLiteral(Number),
	/// Number-like but not a literal, so possibly `NaN` (`IsNaNType`).
	NaN,
	/// String-like (`IsStringType`).
	String,
	/// An object type (`IsObjectType`).
	Object,
	/// The `object` keyword type (`IsNonPrimitiveType`).
	NonPrimitive,
	/// `undefined` or `void` (`IsUndefinedType`).
	Undefined,
	/// `""`, an all-empty template literal, or any non-literal string (`IsEmptyStringType`).
	EmptyString,
	/// A tuple, an array-like type or an ambient array interface (`IsArrayType`).
	Array,
	/// A `LuaTuple` (`IsLuaTupleType`).
	LuaTuple,
	/// `Set`, `ReadonlySet` or `WeakSet` (`IsSetType`).
	Set,
	/// `Map`, `ReadonlyMap` or `WeakMap` (`IsMapType`).
	Map,
	/// `Generator` (`IsGeneratorType`).
	Generator,
	/// `IterableFunction` (`IsIterableFunctionType`).
	IterableFunction,
	/// `Iterable` (`IsIterableType`).
	Iterable,
	/// An `IterableFunction` of a `LuaTuple` (`IsIterableFunctionLuaTupleType`).
	IterableFunctionLuaTuple,
	/// Declared by `@rbxts/types` (`IsRobloxType`).
	Roblox,
}

impl TypeCheck {
	/// Runs the predicate on `t`.
	pub fn check(self, context: &mut impl TypeContext, t: P<Type>) -> bool {
		match self {
			Self::Any => t == ck(context).get_any_type(),
			Self::Boolean => is_boolean(t),
			Self::BooleanLiteral(value) => {
				if t.flags().intersects(TypeFlags::BooleanLiteral) {
					let literal = t.as_literal_type();
					return literal.value() == Some(LiteralValue::Boolean(value)) && literal.regular_type() == Some(t);
				}
				is_boolean(t)
			}
			Self::Number => is_number(t),
			Self::NumberLiteral(value) => {
				if t.is_number_literal() {
					return matches!(t.as_literal_type().value(), Some(LiteralValue::Number(number)) if number == value);
				}
				is_number(t)
			}
			Self::NaN => is_number(t) && !t.is_number_literal(),
			Self::String => is_string(t),
			Self::Object => t.flags().intersects(TypeFlags::Object),
			Self::NonPrimitive => t.flags().intersects(TypeFlags::NonPrimitive),
			Self::Undefined => is_undefined(t),
			Self::EmptyString => is_empty_string(t),
			Self::Array => is_array(context, t),
			Self::LuaTuple => is_lua_tuple(context, t),
			Self::Set => symbol_is_ambient(context, t, &["Set", "ReadonlySet", "WeakSet"]),
			Self::Map => symbol_is_ambient(context, t, &["Map", "ReadonlyMap", "WeakMap"]),
			Self::Generator => symbol_is_ambient(context, t, &["Generator"]),
			Self::IterableFunction => symbol_is_ambient(context, t, &["IterableFunction"]),
			Self::Iterable => symbol_is_ambient(context, t, &["Iterable"]),
			Self::IterableFunctionLuaTuple => {
				symbol_is_ambient(context, t, &["IterableFunction"])
					&& get_type_arguments(context, t)
						.first()
						.is_some_and(|&argument| is_lua_tuple(context, argument))
			}
			Self::Roblox => is_roblox(t),
		}
	}
}

fn is_boolean(t: P<Type>) -> bool {
	t.flags().intersects(TypeFlags::Boolean | TypeFlags::BooleanLiteral)
}

fn is_number(t: P<Type>) -> bool {
	t.flags()
		.intersects(TypeFlags::Number | TypeFlags::NumberLike | TypeFlags::NumberLiteral)
}

fn is_string(t: P<Type>) -> bool {
	t.flags()
		.intersects(TypeFlags::String | TypeFlags::StringLike | TypeFlags::StringLiteral)
}

fn is_undefined(t: P<Type>) -> bool {
	t.flags().intersects(TypeFlags::Undefined | TypeFlags::Void)
}

fn is_empty_string(t: P<Type>) -> bool {
	if t.is_string_literal() {
		return t.as_literal_type().value() == Some(LiteralValue::String(""));
	}
	if t.flags().intersects(TypeFlags::TemplateLiteral) {
		return t.as_template_literal_type().texts().iter().all(|text| text.is_empty());
	}
	is_string(t)
}

fn is_array(context: &mut impl TypeContext, t: P<Type>) -> bool {
	// The checker's isArrayLikeType is true for `any`, so rule it out first.
	if t.flags().intersects(TypeFlags::Any) {
		return false;
	}
	if t.is_tuple_type() || ck(context).is_array_like_type(t) {
		return true;
	}
	symbol_is_ambient(
		context,
		t,
		&["ReadonlyArray", "Array", "ReadVoxelsArray", "TemplateStringsArray"],
	)
}

fn is_lua_tuple(context: &mut impl TypeContext, t: P<Type>) -> bool {
	let Some(nominal) = context.lua_tuple_nominal_symbol() else {
		return false;
	};
	ck(context).get_property_of_type(t, NOMINAL_LUA_TUPLE_NAME) == Some(nominal)
}

fn is_roblox(t: P<Type>) -> bool {
	t.symbol().is_some_and(|symbol| {
		symbol.declarations().iter().any(|&declaration| {
			get_source_file_of_node(declaration)
				.is_some_and(|file| file.file_name().contains("node_modules/@rbxts/types/"))
		})
	})
}

/// Whether `t`'s symbol is one of the named global ambient symbols. A name that resolves to
/// nothing never matches.
fn symbol_is_ambient(context: &mut impl TypeContext, t: P<Type>, names: &[&str]) -> bool {
	let Some(symbol) = t.symbol() else {
		return false;
	};
	names.iter().any(|name| context.ambient_symbol(name) == Some(symbol))
}

/// Whether `t` is the ambient `SharedTable` type (`IsSharedTableType`).
pub fn is_shared_table_type(context: &mut impl TypeContext, t: P<Type>) -> bool {
	symbol_is_ambient(context, t, &["SharedTable"])
}

/// TypeScript's `Type.isClassOrInterface()`: object flags are only read from types that have them.
#[must_use]
pub fn is_class_or_interface(t: P<Type>) -> bool {
	t.flags().intersects(TypeFlags::ObjectFlagsType) && t.object_flags().intersects(ObjectFlags::ClassOrInterface)
}

/// Every base type of a class or interface, transitively (`getRecursiveBaseTypes`).
pub fn get_recursive_base_types(context: &mut impl TypeContext, t: P<Type>) -> Vec<P<Type>> {
	fn inner(context: &mut impl TypeContext, t: P<Type>, result: &mut Vec<P<Type>>) {
		let bases = ck(context).get_base_types(t);
		for &base in bases {
			result.push(base);
			if is_class_or_interface(base) {
				inner(context, base, result);
			}
		}
	}
	let mut result = Vec::new();
	inner(context, t, &mut result);
	result
}

/// `type.getConstraint() ?? type`.
fn constraint_or_self(context: &mut impl TypeContext, t: P<Type>) -> P<Type> {
	ck(context).get_base_constraint_of_type(t).unwrap_or(t)
}

fn is_definitely_type_inner(context: &mut impl TypeContext, t: P<Type>, callbacks: &[TypeCheck]) -> bool {
	if t.is_union() {
		return t
			.types()
			.iter()
			.all(|&member| is_definitely_type_inner(context, member, callbacks));
	}
	if t.is_intersection() {
		return t
			.types()
			.iter()
			.any(|&member| is_definitely_type_inner(context, member, callbacks));
	}
	if is_class_or_interface(t)
		&& get_recursive_base_types(context, t)
			.into_iter()
			.any(|base| is_definitely_type_inner(context, base, callbacks))
	{
		return true;
	}
	callbacks.iter().any(|callback| callback.check(context, t))
}

/// Whether `t`, or its constraint, certainly satisfies one of `callbacks` (`IsDefinitelyType`): every
/// member of a union, some member of an intersection, or a base type of a class or interface.
pub fn is_definitely_type(context: &mut impl TypeContext, t: P<Type>, callbacks: &[TypeCheck]) -> bool {
	let t = constraint_or_self(context, t);
	is_definitely_type_inner(context, t, callbacks)
}

fn is_possibly_type_inner(context: &mut impl TypeContext, t: P<Type>, callbacks: &[TypeCheck]) -> bool {
	if t.flags().intersects(TypeFlags::UnionOrIntersection) {
		return t
			.types()
			.iter()
			.any(|&member| is_possibly_type_inner(context, member, callbacks));
	}
	if is_class_or_interface(t)
		&& get_recursive_base_types(context, t)
			.into_iter()
			.any(|base| is_possibly_type_inner(context, base, callbacks))
	{
		return true;
	}
	// An unconstrained type variable, `any` or `unknown` could be anything.
	if t.flags().intersects(TypeFlags::TypeVariable | TypeFlags::AnyOrUnknown) {
		return true;
	}
	if is_defined_type(context, t) {
		// `defined` is anything but undefined, so it rules out only a lone undefined check.
		return callbacks != [TypeCheck::Undefined];
	}
	callbacks.iter().any(|callback| callback.check(context, t))
}

/// Whether `t`, or its constraint, could satisfy one of `callbacks` (`IsPossiblyType`).
///
/// That holds for some member of a union or intersection, a base type, an unconstrained type
/// variable, `any` or `unknown`, and rbxts's `defined` unless only `undefined` is asked about.
pub fn is_possibly_type(context: &mut impl TypeContext, t: P<Type>, callbacks: &[TypeCheck]) -> bool {
	let t = constraint_or_self(context, t);
	is_possibly_type_inner(context, t, callbacks)
}

/// Whether `t` is rbxts's `defined`: exactly `Object`, with no properties, signatures or index
/// types (`isDefinedType`).
pub fn is_defined_type(context: &mut impl TypeContext, t: P<Type>) -> bool {
	let mut checker = ck(context);
	t.flags() == TypeFlags::Object
		&& checker.get_properties_of_type(t).is_empty()
		&& checker.get_call_signatures(t).is_empty()
		&& checker.get_construct_signatures(t).is_empty()
		&& checker.get_number_index_type(t).is_none()
		&& checker.get_string_index_type(t).is_none()
}

/// The type arguments of a type reference, or none for any other type (`getTypeArguments`).
pub fn get_type_arguments(context: &mut impl TypeContext, t: P<Type>) -> &'static [P<Type>] {
	if !t.flags().intersects(TypeFlags::ObjectFlagsType) || !t.object_flags().intersects(ObjectFlags::Reference) {
		return &[];
	}
	ck(context).get_type_arguments(t)
}

/// Calls `callback` on each leaf of `t`: union and intersection members recursively, through each
/// constraint that differs from the type itself (`WalkTypes`).
pub fn walk_types(context: &mut impl TypeContext, t: P<Type>, callback: &mut dyn FnMut(P<Type>)) {
	if t.flags().intersects(TypeFlags::UnionOrIntersection) {
		for &member in t.types() {
			walk_types(context, member, callback);
		}
		return;
	}
	match ck(context).get_base_constraint_of_type(t) {
		// A template literal type is its own constraint; following it would never end.
		Some(constraint) if constraint != t => walk_types(context, constraint, callback),
		_ => callback(t),
	}
}

/// The symbol of the first construct signature declared by an interface declaration of
/// `expression`'s type's symbol (`getFirstConstructSymbol`).
///
/// The type is the one `State.GetType` gives: the checker's type at [`skip_upwards`] of
/// `expression`.
pub fn get_first_construct_symbol_of_expression(
	context: &mut impl TypeContext,
	expression: P<Node>,
) -> Option<P<Symbol>> {
	let t = ck(context).get_type_at_location(skip_upwards(expression));
	get_first_construct_symbol(t)
}

/// `getFirstConstructSymbol` for a caller that already holds the expression's type.
///
/// The type must be the one at [`skip_upwards`] of the expression, as `State.GetType` gives it;
/// [`get_first_construct_symbol_of_expression`] takes the expression instead.
#[must_use]
pub fn get_first_construct_symbol(t: P<Type>) -> Option<P<Symbol>> {
	let symbol = t.symbol()?;
	symbol
		.declarations()
		.iter()
		.copied()
		.filter(|&declaration| is_interface_declaration(declaration))
		.find_map(|declaration| {
			declaration
				.as_interface_declaration()
				.members
				.nodes()
				.iter()
				.copied()
				.find(|&member| is_construct_signature_declaration(member))
		})?
		.symbol()
}

/// For a union or intersection, the first member symbol that is not `undefined`'s; otherwise the
/// type's own symbol (`GetFirstDefinedSymbol`).
pub fn get_first_defined_symbol(context: &mut impl TypeContext, t: P<Type>) -> Option<P<Symbol>> {
	if t.flags().intersects(TypeFlags::UnionOrIntersection) {
		return t.types().iter().find_map(|member| {
			member
				.symbol()
				.filter(|&symbol| !ck(context).is_undefined_symbol(symbol))
		});
	}
	t.symbol()
}
