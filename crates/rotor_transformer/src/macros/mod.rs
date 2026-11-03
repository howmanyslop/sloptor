//! Stage 4b slice (iv): the macros.
//!
//! The registration tables and implementations of `callmacros.go`, `constructormacros.go`,
//! `propertycallmacros.go`, `stringmacros.go`, `arraymacros.go`, `arraymacros2.go`,
//! `collectionmacros.go`, and rotor's compile-time macros `envmacro.go`, `nameofmacro.go`,
//! `keysmacro.go` and `filemacro.go`.
//!
//! The tables carry every upstream registration name, so [`crate::macromanager`] resolves and
//! audits exactly what Go does. Each Go file is one submodule below this file; the tables keep
//! Go's registration order. See `SLICES.md`.

mod arraymacros;
mod arraymacros2;
mod callmacros;
mod collectionmacros;
mod constructormacros;
mod envmacro;
mod filemacro;
mod keysmacro;
mod nameofmacro;
mod propertycallmacros;
mod stringmacros;

#[cfg(test)]
mod tests;

use rotor_tsrs::ast as tsast;

use crate::macromanager::{CallMacro, ConstructorMacro, IdentifierMacro, PropertyCallMacro};
use crate::prelude::*;

pub(crate) use self::envmacro::{env_macro_symbol, intercept_env_chain};
pub(crate) use self::filemacro::{file_macro_symbol, intercept_file_chain};
pub(crate) use self::keysmacro::{intercept_keys_chain, keys_macro_symbol};
pub(crate) use self::nameofmacro::{intercept_nameof_chain, nameof_macro_symbol};
pub(crate) use self::propertycallmacros::run_property_call_macro;

/// Per-file state owned by this slice.
#[derive(Debug, Default)]
pub(crate) struct SliceState {}

/// Pass-wide state owned by this slice.
#[derive(Debug, Default)]
pub(crate) struct MultiSliceState {}

/// `FileResolver` (`filemacro.go`): reads the data files the `$file` macro inlines.
pub trait FileResolver {
	/// Reads `path` as imported from `importer_path`.
	///
	/// # Errors
	///
	/// The read failure; `ErrorKind::NotFound` is Go's `ErrFileNotFound`.
	fn read(&self, importer_path: &str, path: &str) -> std::io::Result<Vec<u8>>;
}

/// `args[index]`: Go indexes the macro arguments directly and panics on a missing one.
fn arg(state: &mut State<'_>, args: &[NodeId], index: usize) -> NodeId {
	if let Some(&argument) = args.get(index) {
		return argument;
	}
	state.ice(format!("transformer: macro argument {index} is missing"));
	state.ast.none()
}

/// `is<X>MacroNode`: `node`, skipped downwards, is an identifier spelled `name` bound to the
/// macro's global symbol (the text test keeps symbol lookups off the hot path).
fn is_macro_node(state: &mut State<'_>, node: P<Node>, name: &str) -> bool {
	let node = tsast::skip_downwards(node);
	if node.kind() != Kind::Identifier || node.text() != name {
		return false;
	}
	let Some(macro_symbol) = state.ambient_symbol(name) else {
		return false;
	};
	state.ck().get_symbol_at_location(node) == Some(macro_symbol)
}

/// The cooked text of a string-literal-like argument (`envStringLiteralText`,
/// `fileStringLiteralText`): `None` for any other expression, whose value is unknown at compile
/// time.
fn string_literal_text(node: P<Node>) -> Option<&'static str> {
	let node = tsast::skip_downwards(node);
	tsast::is_string_literal_like(node).then(|| node.text())
}

/// `identifierMacroTable` (`IDENTIFIER_MACROS`).
pub(crate) const IDENTIFIER_MACROS: &[(&str, IdentifierMacro)] = &[("Promise", callmacros::promise)];

/// `callMacroTable` (`CALL_MACROS`).
pub(crate) const CALL_MACROS: &[(&str, CallMacro)] = &[
	("assert", callmacros::assert),
	("typeOf", callmacros::type_of),
	("typeIs", callmacros::type_is),
	("classIs", callmacros::class_is),
	("identity", callmacros::identity),
	("$range", callmacros::range),
	("$tuple", callmacros::tuple),
	("$getModuleTree", callmacros::get_module_tree),
];

/// `constructorMacroTable` (`CONSTRUCTOR_MACROS`).
pub(crate) const CONSTRUCTOR_MACROS: &[(&str, ConstructorMacro)] = &[
	("ArrayConstructor", constructormacros::array_constructor),
	("SetConstructor", constructormacros::set_constructor),
	("MapConstructor", constructormacros::map_constructor),
	("WeakSetConstructor", constructormacros::weak_set_constructor),
	("WeakMapConstructor", constructormacros::weak_map_constructor),
	("ReadonlyMapConstructor", constructormacros::map_constructor),
	("ReadonlySetConstructor", constructormacros::set_constructor),
];

/// `makeMathSet("+", "-", "*", "/", "//")`
const MATH_ALL: &[(&str, PropertyCallMacro)] = &[
	("add", propertycallmacros::math_add),
	("sub", propertycallmacros::math_sub),
	("mul", propertycallmacros::math_mul),
	("div", propertycallmacros::math_div),
	("idiv", propertycallmacros::math_idiv),
];

/// `makeMathSet("+", "-", "*", "/")`
const MATH_NO_IDIV: &[(&str, PropertyCallMacro)] = &[
	("add", propertycallmacros::math_add),
	("sub", propertycallmacros::math_sub),
	("mul", propertycallmacros::math_mul),
	("div", propertycallmacros::math_div),
];

/// `propertyCallMacroTable` (`PROPERTY_CALL_MACROS`): class name to method rows, each class's
/// methods registered from its own interface declarations only. Every entry runs through
/// [`run_property_call_macro`], which applies upstream's `wrapComments`.
pub(crate) const PROPERTY_CALL_MACROS: &[(&str, &[(&str, PropertyCallMacro)])] = &[
	// math classes (makeMathSet)
	(
		"CFrame",
		&[
			("add", propertycallmacros::math_add),
			("sub", propertycallmacros::math_sub),
			("mul", propertycallmacros::math_mul),
		],
	),
	(
		"UDim",
		&[
			("add", propertycallmacros::math_add),
			("sub", propertycallmacros::math_sub),
		],
	),
	(
		"UDim2",
		&[
			("add", propertycallmacros::math_add),
			("sub", propertycallmacros::math_sub),
		],
	),
	("Vector2", MATH_ALL),
	("Vector2int16", MATH_NO_IDIV),
	("Vector3", MATH_ALL),
	("Vector3int16", MATH_NO_IDIV),
	("Number", &[("idiv", propertycallmacros::math_idiv)]),
	// rotor extension: Luau's native `vector`
	("vector", MATH_ALL),
	("String", stringmacros::STRING_CALLBACKS),
	("ArrayLike", stringmacros::ARRAY_LIKE_METHODS),
	("ReadonlyArray", arraymacros::READONLY_ARRAY_METHODS),
	("Array", arraymacros2::ARRAY_METHODS),
	("ReadonlySet", collectionmacros::READONLY_SET_METHODS),
	("Set", collectionmacros::SET_METHODS),
	("ReadonlyMap", collectionmacros::READONLY_MAP_METHODS),
	("Map", collectionmacros::MAP_METHODS),
	("Promise", collectionmacros::PROMISE_METHODS),
];
