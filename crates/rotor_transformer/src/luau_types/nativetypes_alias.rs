//! `nativetypes_alias.go`: what the `//!native` spelling decides about the shared alias
//! registry: which symbols are aliased at all, and where a statement is emitted.
//!
//! Only interfaces and non-generic type aliases of object types are aliased; library types are
//! never aliased: they are spelled structurally, or by name for the Roblox allowlist.

use rotor_luau::is_valid_identifier;
use rotor_tsrs::ast::{self as tsast, SymbolFlags};
use rotor_tsrs::checker::TypeFlags;
use rotor_tsrs::types::TypeCheck;

use super::luautypes::{AliasPlacement, SymKey, types};
use super::luautypes_alias::luau_first_declaration;
use crate::prelude::*;
use crate::types::get_recursive_base_types;

/// `nativeReservedTypeAliasNames`: names Luau already reads as something else.
pub const NATIVE_RESERVED_TYPE_ALIAS_NAMES: &[&str] = &[
	"number", "string", "boolean", "any", "nil", "never", "unknown", "vector", "Vector3", "buffer", "thread", "table",
	"typeof",
];

/// `isNativeVectorTypeName`
#[must_use]
pub fn is_native_vector_type_name(name: &str) -> bool {
	name == "vector" || name == "Vector3"
}

/// `isNativeAliasableSymbol`: the `//!native` alias policy.
pub fn is_native_aliasable_symbol(state: &mut State<'_>, symbol: P<Symbol>, declaration: Option<P<Node>>) -> bool {
	let Some(declaration) = declaration else {
		return false;
	};
	if is_native_library_symbol(state, symbol) {
		return false;
	}
	let declared = state.ck().get_declared_type_of_symbol(symbol);
	if TypeCheck::LuaTuple.check(state, declared) {
		return false;
	}
	let flags = symbol.flags();
	if flags.intersects(SymbolFlags::Interface) && !flags.intersects(SymbolFlags::Class) {
		return get_native_aliasable_declaration(symbol, tsast::is_interface_declaration)
			&& state.ck().get_call_signatures(declared).is_empty();
	}
	if flags.intersects(SymbolFlags::TypeAlias) {
		if !get_native_aliasable_declaration(symbol, tsast::is_type_alias_declaration)
			|| !tsast::is_type_alias_declaration(declaration)
		{
			return false;
		}
		// A generic type alias is expanded inline.
		return declaration.type_parameters().is_empty() && declared.flags().intersects(TypeFlags::Object);
	}
	false
}

/// `getNativeAliasableDeclaration`: the symbol has a declaration of the expected kind whose type
/// parameters Luau can all spell.
fn get_native_aliasable_declaration(symbol: P<Symbol>, is_kind: fn(P<Node>) -> bool) -> bool {
	let Some(&declaration) = symbol.declarations().iter().find(|&&declaration| is_kind(declaration)) else {
		return false;
	};
	declaration
		.type_parameters()
		.iter()
		.all(|parameter| parameter.name().is_some_and(|name| is_valid_identifier(name.text())))
}

/// `nativeDeclarationAlias`: the `type` statement to emit at a top-level interface or type
/// alias declaration of this file, or an empty list.
pub fn native_declaration_alias(state: &mut State<'_>, node: P<Node>) -> Statements {
	let statements = state.ast.new_list();
	if !state.is_native {
		return statements;
	}
	let Some(name) = node.name() else {
		return statements;
	};
	let Some(symbol) = state.ck().get_symbol_at_location(name) else {
		return statements;
	};
	if luau_first_declaration(state, symbol) != Some(node) {
		return statements;
	}
	let mut builder = types(state);
	let Some(entry) = builder.alias_for(Some(SymKey::Real(symbol))) else {
		return statements;
	};
	let data = builder.b().entry(entry);
	let Some(statement) = data.statement else {
		return statements;
	};
	if data.placement != AliasPlacement::AtDeclaration || data.placed {
		return statements;
	}
	builder.b().entry_mut(entry).placed = true;
	state.ast.list_push(statements, statement);
	statements
}

/// `nativeHoistedTypeStatements`: the `type` statements registered while annotations were built
/// that no declaration placed, in registration order, without export markers or comments.
pub fn native_hoisted_type_statements(state: &mut State<'_>) -> Statements {
	let statements = state.ast.new_list();
	if state.is_luau_typed || state.slices.luau_types.shared.is_none() {
		return statements;
	}
	let mut builder = types(state);
	for entry in builder.aliases() {
		let data = builder.b().entry(entry);
		let Some(statement) = data.statement else {
			continue;
		};
		if data.placed {
			continue;
		}
		builder.b().entry_mut(entry).placed = true;
		builder.ast().list_push(statements, statement);
	}
	statements
}

/// `isNativeLibrarySymbol`: a declaration of `symbol` lives in a library file.
fn is_native_library_symbol(state: &State<'_>, symbol: P<Symbol>) -> bool {
	symbol.declarations().iter().any(|&declaration| {
		tsast::get_source_file_of_node(declaration).is_some_and(|file| is_native_library_file(state, file))
	})
}

fn is_native_library_file(state: &State<'_>, file: P<SourceFile>) -> bool {
	let name = file.file_name().replace('\\', "/");
	if name.contains("node_modules/@rbxts/") || name.contains("node_modules/typescript/") {
		return true;
	}
	state.program.is_lib_file(file)
}

/// `nativeRobloxTypeAllowlist`: the Roblox datatypes that name a real type in Luau, plus
/// `Instance`.
const NATIVE_ROBLOX_TYPE_ALLOWLIST: &[&str] = &[
	"Axes",
	"BrickColor",
	"CFrame",
	"Color3",
	"ColorSequence",
	"ColorSequenceKeypoint",
	"DateTime",
	"Faces",
	"FloatCurveKey",
	"Font",
	"Instance",
	"NumberRange",
	"NumberSequence",
	"NumberSequenceKeypoint",
	"OverlapParams",
	"PathWaypoint",
	"PhysicalProperties",
	"Random",
	"Ray",
	"RaycastParams",
	"RaycastResult",
	"RBXScriptConnection",
	"RBXScriptSignal",
	"Rect",
	"Region3",
	"Region3int16",
	"TweenInfo",
	"UDim",
	"UDim2",
	"Vector2",
	"Vector2int16",
	"Vector3",
	"Vector3int16",
	"vector",
];

/// `getNativeRobloxTypeName`: the Luau name of an allowlisted Roblox type, `""` for anything
/// else.
pub fn get_native_roblox_type_name(state: &mut State<'_>, t: P<Type>) -> String {
	if !TypeCheck::Roblox.check(state, t) {
		return String::new();
	}
	let Some(symbol) = t.symbol() else {
		return String::new();
	};
	if !is_valid_identifier(symbol.name()) {
		return String::new();
	}
	if NATIVE_ROBLOX_TYPE_ALLOWLIST.contains(&symbol.name()) {
		return symbol.name().to_owned();
	}
	for base in get_recursive_base_types(state, t) {
		if let Some(base_symbol) = base.symbol()
			&& base_symbol.name() == "Instance"
			&& TypeCheck::Roblox.check(state, base)
		{
			return symbol.name().to_owned();
		}
	}
	String::new()
}
