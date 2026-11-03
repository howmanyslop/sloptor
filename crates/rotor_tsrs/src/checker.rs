//! The checker: tsrs's types and symbols re-exported, plus [`TypeChecker`], the checker queries
//! rotor's Go code makes, under their Go names.
//!
//! tsrs exposes most of `typescript-go`'s exported checker API with an `_exported` suffix, and some
//! of it as plain public methods. [`TypeChecker`] gives the port one spelling for each: the Go
//! method's name in snake case. It derefs to [`Checker`], so the rest of tsrs's public checker API
//! stays reachable.

use std::ops::{Deref, DerefMut};

use tsrs_ast::{Node, Symbol, SymbolFlags};
use tsrs_core::P;

pub use tsrs_checker::*;

/// A checker borrowed for a run of queries, with rotor's checker API on it.
pub struct TypeChecker<'c> {
	checker: &'c mut Checker,
}

impl Deref for TypeChecker<'_> {
	type Target = Checker;

	fn deref(&self) -> &Checker {
		self.checker
	}
}

impl DerefMut for TypeChecker<'_> {
	fn deref_mut(&mut self) -> &mut Checker {
		self.checker
	}
}

impl<'c> TypeChecker<'c> {
	/// Wraps a checker borrowed from the pool.
	pub const fn new(checker: &'c mut Checker) -> Self {
		Self { checker }
	}

	/// The wrapped checker, for tsrs APIs that take it as an argument.
	pub const fn checker(&mut self) -> &mut Checker {
		self.checker
	}

	/// Go `Checker.GetAliasedSymbol`.
	pub fn get_aliased_symbol(&mut self, symbol: P<Symbol>) -> P<Symbol> {
		self.checker.get_aliased_symbol(symbol)
	}

	/// Go `Checker.GetAmbientModules`: the global `"name"` module symbols, pattern modules included.
	///
	/// The pin binds a pattern module (`declare module "*.json"`) in the globals under its quoted
	/// text, so Go returns one merged symbol per pattern. TypeScript 7.1 binds one symbol per
	/// pattern declaration, so this returns one symbol per declaration; the union of their
	/// declarations is Go's. Go ranges over a map, so neither list has a meaningful order.
	pub fn get_ambient_modules(&mut self) -> Vec<P<Symbol>> {
		self.checker.get_ambient_modules()
	}

	/// Go `Checker.GetAnyType`.
	#[must_use]
	pub fn get_any_type(&self) -> P<Type> {
		self.checker.get_any_type()
	}

	/// Go `Checker.GetApparentProperties`.
	pub fn get_apparent_properties(&mut self, t: P<Type>) -> Vec<P<Symbol>> {
		self.checker.get_apparent_properties(t)
	}

	/// Go `Checker.GetApparentType`.
	pub fn get_apparent_type(&mut self, t: P<Type>) -> P<Type> {
		self.checker.get_apparent_type_exported(t)
	}

	/// Go `Checker.GetBaseConstraintOfType` (TypeScript's public `Type.getConstraint`).
	pub fn get_base_constraint_of_type(&mut self, t: P<Type>) -> Option<P<Type>> {
		self.checker.get_base_constraint_of_type_exported(t)
	}

	/// Go `Checker.GetBaseTypeOfLiteralType`.
	pub fn get_base_type_of_literal_type(&mut self, t: P<Type>) -> P<Type> {
		self.checker.get_base_type_of_literal_type_exported(t)
	}

	/// Go `Checker.GetBaseTypes`.
	pub fn get_base_types(&mut self, t: P<Type>) -> &'static [P<Type>] {
		self.checker.get_base_types_exported(t)
	}

	/// Go `Checker.GetBooleanType`.
	#[must_use]
	pub fn get_boolean_type(&self) -> P<Type> {
		self.checker.get_boolean_type()
	}

	/// Go `Checker.GetCallSignatures`.
	pub fn get_call_signatures(&mut self, t: P<Type>) -> &'static [P<Signature>] {
		self.checker.get_call_signatures(t)
	}

	/// Go `Checker.GetConstantValue`: an enum member's or constant's value, if it has one.
	pub fn get_constant_value(&mut self, node: P<Node>) -> Option<LiteralValue> {
		self.checker.get_constant_value(node)
	}

	/// Go `Checker.GetConstraintOfTypeParameter`.
	pub fn get_constraint_of_type_parameter(&mut self, type_parameter: P<Type>) -> Option<P<Type>> {
		self.checker.get_constraint_of_type_parameter_exported(type_parameter)
	}

	/// Go `Checker.GetConstructSignatures`.
	pub fn get_construct_signatures(&mut self, t: P<Type>) -> &'static [P<Signature>] {
		self.checker
			.get_signatures_of_type_exported(t, SignatureKind::Construct)
	}

	/// Go `Checker.GetContextualType`.
	pub fn get_contextual_type(&mut self, node: P<Node>, context_flags: ContextFlags) -> Option<P<Type>> {
		self.checker.get_contextual_type_exported(node, context_flags)
	}

	/// Go `Checker.GetContextualTypeForObjectLiteralElement`.
	pub fn get_contextual_type_for_object_literal_element(
		&mut self,
		element: P<Node>,
		context_flags: ContextFlags,
	) -> Option<P<Type>> {
		self.checker
			.get_contextual_type_for_object_literal_element_exported(element, context_flags)
	}

	/// Go `Checker.GetDeclaredTypeOfSymbol`.
	pub fn get_declared_type_of_symbol(&mut self, symbol: P<Symbol>) -> P<Type> {
		self.checker.get_declared_type_of_symbol_exported(symbol)
	}

	/// Go `Checker.GetDefaultFromTypeParameter`.
	pub fn get_default_from_type_parameter(&mut self, type_parameter: P<Type>) -> Option<P<Type>> {
		self.checker.get_default_from_type_parameter_exported(type_parameter)
	}

	/// Go `Checker.GetElementTypeOfArrayType`.
	pub fn get_element_type_of_array_type(&mut self, t: P<Type>) -> Option<P<Type>> {
		self.checker.get_element_type_of_array_type_exported(t)
	}

	/// Go `Checker.GetEmitResolver`.
	pub fn get_emit_resolver(&mut self) -> P<EmitResolver> {
		self.checker.get_emit_resolver()
	}

	/// Go `EmitResolver.IsReferencedAliasDeclarationUnsafe`: whether an import or export alias is
	/// referenced as a value. Ask the checker that checked the file: the marks live on it.
	pub fn is_referenced_alias_declaration(&mut self, node: P<Node>) -> bool {
		let resolver = self.checker.get_emit_resolver();
		resolver.is_referenced_alias_declaration(self.checker, node)
	}

	/// Go `EmitResolver.GetJsxFactoryEntityUnsafe`.
	pub fn get_jsx_factory_entity(&mut self, location: P<Node>) -> Option<P<Node>> {
		let resolver = self.checker.get_emit_resolver();
		resolver.get_jsx_factory_entity(self.checker, location)
	}

	/// Go `EmitResolver.GetJsxFragmentFactoryEntityUnsafe`.
	pub fn get_jsx_fragment_factory_entity(&mut self, location: P<Node>) -> Option<P<Node>> {
		let resolver = self.checker.get_emit_resolver();
		resolver.get_jsx_fragment_factory_entity(self.checker, location)
	}

	/// Go `Checker.GetExportSpecifierLocalTargetSymbol`.
	pub fn get_export_specifier_local_target_symbol(&mut self, node: P<Node>) -> Option<P<Symbol>> {
		self.checker.get_export_specifier_local_target_symbol(node)
	}

	/// Go `Checker.GetExportsOfModule`.
	pub fn get_exports_of_module(&mut self, symbol: P<Symbol>) -> Vec<P<Symbol>> {
		self.checker.get_exports_of_module_exported(symbol)
	}

	/// Go `Checker.GetGlobalSymbol`.
	pub fn get_global_symbol(
		&mut self,
		name: &str,
		meaning: SymbolFlags,
		diagnostic: Option<&'static tsrs_diagnostics::Message>,
	) -> Option<P<Symbol>> {
		self.checker.get_global_symbol_exported(name, meaning, diagnostic)
	}

	/// Go `Checker.GetIndexInfosOfType`.
	pub fn get_index_infos_of_type(&mut self, t: P<Type>) -> &'static [P<IndexInfo>] {
		self.checker.get_index_infos_of_type_exported(t)
	}

	/// Go `Checker.GetIndexTypeOfType` (rotor's shim): pass [`Self::get_number_type`] for
	/// TypeScript's `IndexKind.Number`, which gives an array's element type.
	pub fn get_index_type_of_type(&mut self, t: P<Type>, key_type: P<Type>) -> Option<P<Type>> {
		self.checker.get_index_type_of_type(t, key_type)
	}

	/// Go `Checker.GetLocalTypeParametersOfClassOrInterfaceOrTypeAlias`.
	pub fn get_local_type_parameters_of_class_or_interface_or_type_alias(&mut self, symbol: P<Symbol>) -> Vec<P<Type>> {
		self.checker
			.get_local_type_parameters_of_class_or_interface_or_type_alias_exported(symbol)
	}

	/// Go `Checker.GetMergedSymbol`.
	pub fn get_merged_symbol(&mut self, symbol: P<Symbol>) -> P<Symbol> {
		self.checker.get_merged_symbol_exported(symbol)
	}

	/// Go `Checker.GetNonNullableType`.
	pub fn get_non_nullable_type(&mut self, t: P<Type>) -> P<Type> {
		self.checker.get_non_nullable_type(t)
	}

	/// Go `Checker.GetNonOptionalType`.
	pub fn get_non_optional_type(&mut self, t: P<Type>) -> P<Type> {
		self.checker.get_non_optional_type(t)
	}

	/// Go `Checker.GetNumberIndexType`.
	pub fn get_number_index_type(&mut self, t: P<Type>) -> Option<P<Type>> {
		self.checker.get_number_index_type(t)
	}

	/// Go `Checker.GetNumberType`.
	#[must_use]
	pub fn get_number_type(&self) -> P<Type> {
		self.checker.get_number_type()
	}

	/// Go `Checker.GetPromisedTypeOfPromise`.
	pub fn get_promised_type_of_promise(&mut self, t: P<Type>) -> Option<P<Type>> {
		self.checker.get_promised_type_of_promise(t)
	}

	/// Go `Checker.GetPropertiesOfType`.
	pub fn get_properties_of_type(&mut self, t: P<Type>) -> &'static [P<Symbol>] {
		self.checker.get_properties_of_type_exported(t)
	}

	/// Go `Checker.GetPropertyOfType`.
	pub fn get_property_of_type(&mut self, t: P<Type>, name: &str) -> Option<P<Symbol>> {
		self.checker.get_property_of_type_exported(t, name)
	}

	/// Go `Checker.GetResolvedSignature`.
	pub fn get_resolved_signature(&mut self, node: P<Node>) -> P<Signature> {
		self.checker.get_resolved_signature_exported(node)
	}

	/// Go `Checker.GetReturnTypeOfSignature`.
	pub fn get_return_type_of_signature(&mut self, signature: P<Signature>) -> P<Type> {
		self.checker.get_return_type_of_signature_exported(signature)
	}

	/// Go `Checker.GetShorthandAssignmentValueSymbol`.
	pub fn get_shorthand_assignment_value_symbol(&mut self, location: Option<P<Node>>) -> Option<P<Symbol>> {
		self.checker.get_shorthand_assignment_value_symbol(location)
	}

	/// Go `Checker.GetSignatureFromDeclaration`.
	pub fn get_signature_from_declaration(&mut self, declaration: P<Node>) -> P<Signature> {
		self.checker.get_signature_from_declaration_exported(declaration)
	}

	/// Go `Checker.GetSignaturesOfType`.
	pub fn get_signatures_of_type(&mut self, t: P<Type>, kind: SignatureKind) -> &'static [P<Signature>] {
		self.checker.get_signatures_of_type_exported(t, kind)
	}

	/// Go `Checker.GetStringIndexType`.
	pub fn get_string_index_type(&mut self, t: P<Type>) -> Option<P<Type>> {
		self.checker.get_string_index_type(t)
	}

	/// Go `Checker.GetStringType`.
	#[must_use]
	pub fn get_string_type(&self) -> P<Type> {
		self.checker.get_string_type()
	}

	/// Go `Checker.GetSymbolAtLocation`.
	pub fn get_symbol_at_location(&mut self, node: P<Node>) -> Option<P<Symbol>> {
		self.checker.get_symbol_at_location_exported(node)
	}

	/// Go `Checker.GetSymbolsInScope`.
	pub fn get_symbols_in_scope(&mut self, location: P<Node>, meaning: SymbolFlags) -> Vec<P<Symbol>> {
		self.checker.get_symbols_in_scope_exported(location, meaning)
	}

	/// Go `Checker.GetTypeArguments`: the type arguments of a type reference.
	pub fn get_type_arguments(&mut self, t: P<Type>) -> &'static [P<Type>] {
		self.checker.get_type_arguments_exported(t)
	}

	/// Go `Checker.GetTypeAtLocation`.
	pub fn get_type_at_location(&mut self, node: P<Node>) -> P<Type> {
		self.checker.get_type_at_location(node)
	}

	/// Go `Checker.GetTypeAtPosition`.
	pub fn get_type_at_position(&mut self, signature: P<Signature>, position: usize) -> P<Type> {
		self.checker.get_type_at_position_exported(signature, position)
	}

	/// Go `Checker.GetTypeFromTypeNode`.
	pub fn get_type_from_type_node(&mut self, node: P<Node>) -> P<Type> {
		self.checker.get_type_from_type_node_exported(node)
	}

	/// Go `Checker.GetTypeOfAssignmentPattern` (rotor's shim): the type of an array or object literal
	/// used as a destructuring assignment target, or the error type, as TypeScript's public API
	/// falls back to.
	pub fn get_type_of_assignment_pattern(&mut self, expression: P<Node>) -> P<Type> {
		self.checker
			.get_type_of_assignment_pattern(expression)
			.unwrap_or(self.checker.error_type)
	}

	/// Go `Checker.GetTypeOfPropertyOfType`.
	pub fn get_type_of_property_of_type(&mut self, t: P<Type>, name: &str) -> Option<P<Type>> {
		self.checker.get_type_of_property_of_type_exported(t, name)
	}

	/// Go `Checker.GetTypeOfSymbol`.
	pub fn get_type_of_symbol(&mut self, symbol: P<Symbol>) -> P<Type> {
		self.checker.get_type_of_symbol_exported(symbol)
	}

	/// Go `Checker.GetTypeOfSymbolAtLocation`.
	pub fn get_type_of_symbol_at_location(&mut self, symbol: P<Symbol>, location: Option<P<Node>>) -> Option<P<Type>> {
		self.checker.get_type_of_symbol_at_location(symbol, location)
	}

	/// Go `Checker.GetUndefinedType`.
	#[must_use]
	pub fn get_undefined_type(&self) -> P<Type> {
		self.checker.get_undefined_type()
	}

	/// Go `Checker.GetVoidType`.
	#[must_use]
	pub fn get_void_type(&self) -> P<Type> {
		self.checker.get_void_type()
	}

	/// Go `Checker.GetWidenedType`.
	pub fn get_widened_type(&mut self, t: P<Type>) -> P<Type> {
		self.checker.get_widened_type_exported(t)
	}

	/// Go `Checker.IsArgumentsSymbol`.
	#[must_use]
	pub fn is_arguments_symbol(&self, symbol: P<Symbol>) -> bool {
		self.checker.is_arguments_symbol(symbol)
	}

	/// Go `Checker.IsArrayLikeType`.
	pub fn is_array_like_type(&mut self, t: P<Type>) -> bool {
		self.checker.is_array_like_type_exported(t)
	}

	/// Go `Checker.IsTypeAssignableTo`.
	pub fn is_type_assignable_to(&mut self, source: P<Type>, target: P<Type>) -> bool {
		self.checker.is_type_assignable_to_exported(source, target)
	}

	/// Go `Checker.IsUndefinedSymbol`.
	#[must_use]
	pub fn is_undefined_symbol(&self, symbol: P<Symbol>) -> bool {
		self.checker.is_undefined_symbol(symbol)
	}

	/// Go `Checker.ResolveExternalModuleName` (no import attributes).
	pub fn resolve_external_module_name(&mut self, module_specifier: P<Node>) -> Option<P<Symbol>> {
		self.checker
			.resolve_external_module_name_exported(module_specifier, None)
	}

	/// Go `Checker.ResolveName`.
	pub fn resolve_name(
		&mut self,
		name: &str,
		location: Option<P<Node>>,
		meaning: SymbolFlags,
		exclude_globals: bool,
	) -> Option<P<Symbol>> {
		self.checker
			.resolve_name_exported(name, location, meaning, exclude_globals)
	}

	/// Go `Checker.TypeToString`.
	pub fn type_to_string(&mut self, t: P<Type>) -> String {
		self.checker.type_to_string_exported(t)
	}

	/// Go `Checker.TypeToTypeNode`.
	pub fn type_to_type_node(
		&mut self,
		t: P<Type>,
		enclosing_declaration: Option<P<Node>>,
		flags: NodeBuilderFlags,
	) -> Option<P<Node>> {
		self.checker.type_to_type_node(t, enclosing_declaration, flags, None)
	}
}

/// The node builder's flags (Go `nodebuilder.Flags`).
pub use tsrs_checker::Flags as NodeBuilderFlags;

/// Go `ConditionalType.RootCheckType` (rotor's shim): the check type of a conditional type's root.
#[must_use]
pub fn conditional_root_check_type(t: P<Type>) -> Option<P<Type>> {
	t.as_conditional_type().root.get()?.check_type.get()
}

/// Go `ConditionalType.RootNode` (rotor's shim): the conditional type node a conditional type
/// came from.
#[must_use]
pub fn conditional_root_node(t: P<Type>) -> Option<P<Node>> {
	t.as_conditional_type().root.get()?.node.get()
}

/// Go `checker.IsTupleType`.
#[must_use]
pub fn is_tuple_type(t: P<Type>) -> bool {
	is_tuple_type_exported(t)
}

/// Go `checker.GetDeclarationModifierFlagsFromSymbol`.
#[must_use]
pub fn get_declaration_modifier_flags_from_symbol(symbol: P<Symbol>) -> tsrs_ast::ModifierFlags {
	get_declaration_modifier_flags_from_symbol_exported(symbol)
}

/// Go `checker.SkipAlias`.
pub fn skip_alias(symbol: P<Symbol>, checker: &mut Checker) -> P<Symbol> {
	tsrs_checker::skip_alias(symbol, checker)
}

/// Go `Type.Distributed`: a union's members, nothing for `never`, otherwise the type itself.
#[must_use]
pub fn distributed(t: P<Type>) -> Vec<P<Type>> {
	if t.flags().intersects(TypeFlags::Union) {
		return t.types().to_vec();
	}
	if t.flags().intersects(TypeFlags::Never) {
		return Vec::new();
	}
	vec![t]
}
