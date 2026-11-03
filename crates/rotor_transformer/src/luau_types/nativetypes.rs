//! `nativetypes.go`: native-codegen type annotations.
//!
//! Under `--!native` Luau's compiler turns parameter annotations into bytecode type tags, and
//! the native code generator uses them to drop tag checks and fallback paths. Only `number`,
//! `vector`/`Vector3` and table types buy a real speedup; `string` is a marginal win. A bare
//! generic name is only understood when it belongs to the annotated function itself (rule R1).
//!
//! The annotations are built in two steps: the shared builder spells the type at full fidelity,
//! consulting the native spelling, and `apply_native_policy` then widens or drops what would hurt
//! native codegen.

use rotor_luau::{TypeGeneric, is_valid_identifier};
use rotor_tsrs::ast::{self as tsast, Kind as TsKind, ModifierFlags, SymbolFlags};
use rotor_tsrs::scanner;

use super::luautypes::{TypeContext, TypePosition, native_types};
use crate::prelude::*;

/// `nativeTypeNumber`
pub const NATIVE_TYPE_NUMBER: &str = "number";
/// `nativeTypeString`
pub const NATIVE_TYPE_STRING: &str = "string";
/// `nativeTypeBoolean`
pub const NATIVE_TYPE_BOOLEAN: &str = "boolean";
/// `nativeTypeAny`
pub const NATIVE_TYPE_ANY: &str = "any";
/// `nativeTypeNil`
pub const NATIVE_TYPE_NIL: &str = "nil";
/// `nativeTypeMaxDepth`: how far nested table shapes are followed.
pub const NATIVE_TYPE_MAX_DEPTH: usize = 4;

/// `nativePosition`: where a type is being spelled, as the native policy sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativePosition {
	/// `nativePositionNested`: any untagged position; an unspellable type widens to `any`.
	Nested,
	/// `nativePositionReturn`: a function's declared return type.
	Return,
	/// `nativePositionParameter`: a function's top-level parameter type.
	Parameter,
}

/// `nativePositionOf`: the builder's position mapped onto the three the policy distinguishes.
#[must_use]
pub const fn native_position_of(position: TypePosition) -> NativePosition {
	match position {
		TypePosition::Parameter => NativePosition::Parameter,
		TypePosition::Return => NativePosition::Return,
		_ => NativePosition::Nested,
	}
}

/// `nativeTypeKind`: what a spelled type would tag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NativeTypeKind {
	/// `nativeKindUnspellable`
	#[default]
	Unspellable,
	/// `nativeKindNil`
	Nil,
	/// `nativeKindNumber`
	Number,
	/// `nativeKindString`
	String,
	/// `nativeKindBoolean`
	Boolean,
	/// `nativeKindVector`
	Vector,
	/// `nativeKindTable`
	Table,
	/// `nativeKindFunction`
	Function,
	/// `nativeKindGeneric`
	Generic,
}

/// `hasNativeDirective`: the file's leading comments contain `//!native`.
pub fn has_native_directive(state: &mut State<'_>) -> bool {
	if state.remove_comments() {
		return false;
	}
	let text = state.source_file_text;
	for range in scanner::get_leading_comment_ranges(text, 0) {
		let mut end = range.end() as usize;
		if range.kind != TsKind::SingleLineCommentTrivia {
			end = end.saturating_sub(2);
		}
		let start = range.pos() as usize + 2;
		// Go slices `text[pos+2:end]`, which panics for an unterminated `/*` (end - 2 < start).
		let Some(body) = text.get(start..end) else {
			state.ice("transformer: hasNativeDirective: comment range out of bounds");
			return false;
		};
		if body.trim() == "!native" {
			return true;
		}
	}
	false
}

/// `getNativeParameterType`: the annotation for one plain-identifier parameter, or `None` when
/// it must stay unannotated.
pub fn get_native_parameter_type(state: &mut State<'_>, parameter_node: P<Node>) -> Option<NodeId> {
	let parameter = parameter_node.as_parameter_declaration();
	let t = state.ck().get_type_at_location(parameter.name);
	let mut builder = native_types(state);
	let built = builder.build(t, TypeContext::at(TypePosition::Parameter, Some(parameter_node)));
	let (node, _, _) = builder.native_policy(built.node, NativePosition::Parameter, 0);
	let mut node = node?;
	if parameter.question_token().is_some() || parameter_node.initializer().is_some() {
		node = state.ast.optional_type(node);
	}
	Some(node)
}

/// `getNativeReturnType`: the annotation for a function's return, `None` outside `--!native`
/// files and for async and generator functions.
pub fn get_native_return_type(state: &mut State<'_>, node: P<Node>) -> Option<NodeId> {
	if !state.is_native || tsast::has_syntactic_modifier(node, ModifierFlags::Async) || is_generator_function(node) {
		return None;
	}
	let signature = state.ck().get_signature_from_declaration(node);
	let return_type = state.ck().get_return_type_of_signature(signature);
	let mut builder = native_types(state);
	let pack = builder.build_return_pack(Some(return_type), TypeContext::at(TypePosition::Return, Some(node)));
	let (annotation, _, _) = builder.native_policy(pack, NativePosition::Return, 0);
	annotation
}

fn is_generator_function(node: P<Node>) -> bool {
	match node.kind() {
		Kind::FunctionDeclaration => node.as_function_declaration().asterisk_token().is_some(),
		Kind::FunctionExpression => node.as_function_expression().asterisk_token().is_some(),
		Kind::MethodDeclaration => node.as_method_declaration().asterisk_token().is_some(),
		_ => false,
	}
}

/// `getNativeFunctionGenerics`: the generic parameters a function declares, ready for
/// `function f<K, V>`.
pub fn get_native_function_generics(state: &mut State<'_>, node: P<Node>) -> Vec<TypeGeneric> {
	if !state.is_native {
		return Vec::new();
	}
	native_types(state).generics(node)
}

// -----------------------------------------------------------------------------------------
// Generic scope
// -----------------------------------------------------------------------------------------

/// `typeGenericScope`: one level of generic names visible to the annotation being rendered.
/// `barrier` marks a type alias body.
#[derive(Debug, Default)]
pub struct TypeGenericScope {
	pub(crate) names: Vec<String>,
	pub(crate) barrier: bool,
}

/// `typeGenericInScope`: whether `name` is a generic visible from the current annotation, and
/// whether it belongs to the innermost function (rule R1).
pub fn type_generic_in_scope(scopes: &[TypeGenericScope], name: &str) -> (bool, bool) {
	for (index, scope) in scopes.iter().enumerate().rev() {
		if scope.names.iter().any(|existing| existing == name) {
			return (true, index == scopes.len() - 1 && !scope.barrier);
		}
		if scope.barrier {
			return (false, false);
		}
	}
	(false, false)
}

/// `getNativeTypeParameterName`: a type variable's Luau generic name when that generic is in
/// scope, with whether it is the innermost function's own.
pub fn get_native_type_parameter_name(state: &State<'_>, t: P<Type>) -> Option<(String, bool)> {
	let symbol = t
		.symbol()
		.filter(|symbol| symbol.flags().intersects(SymbolFlags::TypeParameter))?;
	let name = symbol.name();
	if !is_valid_identifier(name) {
		return None;
	}
	let (in_scope, own) = type_generic_in_scope(&state.slices.luau_types.type_scopes, name);
	if !in_scope {
		return None;
	}
	Some((name.to_owned(), own))
}
