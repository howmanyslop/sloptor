//! Stage 4b slice (iv): the macros.
//!
//! The registration tables and implementations of `callmacros.go`, `constructormacros.go`,
//! `propertycallmacros.go`, `stringmacros.go`, `arraymacros.go`, `arraymacros2.go`,
//! `collectionmacros.go`, and rotor's compile-time macros `envmacro.go`, `nameofmacro.go`,
//! `keysmacro.go` and `filemacro.go`.
//!
//! The tables carry every upstream registration name, so [`crate::macromanager`] resolves and
//! audits exactly what Go does; each entry points at a stub that reports `rotorNotYetPorted`.
//! The slice port replaces the stubs (and may add submodules below this file); the table layout
//! and the functions core code calls keep their signatures. See `SLICES.md`.

use rotor_tsrs::ast as tsast;

use crate::call::ChainItem;
use crate::macromanager::{CallMacro, ConstructorMacro, IdentifierMacro, PropertyCallMacro, PropertyCallMacroEntry};
use crate::prelude::*;

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

fn identifier_macro_stub(state: &mut State<'_>, node: P<Node>) -> NodeId {
	state.not_yet_ported(node, "identifier macros");
	state.ast.none()
}

fn constructor_macro_stub(state: &mut State<'_>, node: P<Node>) -> NodeId {
	state.not_yet_ported(node, "constructor macros");
	state.ast.none()
}

fn call_macro_stub(state: &mut State<'_>, node: P<Node>, _expression: NodeId, _args: Vec<NodeId>) -> NodeId {
	state.not_yet_ported(node, "call macros");
	state.ast.none()
}

fn property_call_macro_stub(state: &mut State<'_>, node: P<Node>, _expression: NodeId, _args: Vec<NodeId>) -> NodeId {
	state.not_yet_ported(node, "property call macros");
	state.ast.none()
}

/// `identifierMacroTable` (`IDENTIFIER_MACROS`).
pub(crate) const IDENTIFIER_MACROS: &[(&str, IdentifierMacro)] = &[("Promise", identifier_macro_stub)];

/// `callMacroTable` (`CALL_MACROS`).
pub(crate) const CALL_MACROS: &[(&str, CallMacro)] = &[
	("assert", call_macro_stub),
	("typeOf", call_macro_stub),
	("typeIs", call_macro_stub),
	("classIs", call_macro_stub),
	("identity", call_macro_stub),
	("$range", call_macro_stub),
	("$tuple", call_macro_stub),
	("$getModuleTree", call_macro_stub),
];

/// `constructorMacroTable` (`CONSTRUCTOR_MACROS`).
pub(crate) const CONSTRUCTOR_MACROS: &[(&str, ConstructorMacro)] = &[
	("ArrayConstructor", constructor_macro_stub),
	("SetConstructor", constructor_macro_stub),
	("MapConstructor", constructor_macro_stub),
	("WeakSetConstructor", constructor_macro_stub),
	("WeakMapConstructor", constructor_macro_stub),
	("ReadonlyMapConstructor", constructor_macro_stub),
	("ReadonlySetConstructor", constructor_macro_stub),
];

const S: PropertyCallMacro = property_call_macro_stub;

/// `propertyCallMacroTable` (`PROPERTY_CALL_MACROS`): class name to method rows, each class's
/// methods registered from its own interface declarations only.
pub(crate) const PROPERTY_CALL_MACROS: &[(&str, &[(&str, PropertyCallMacro)])] = &[
	// math classes (makeMathSet)
	("CFrame", &[("add", S), ("sub", S), ("mul", S)]),
	("UDim", &[("add", S), ("sub", S)]),
	("UDim2", &[("add", S), ("sub", S)]),
	(
		"Vector2",
		&[("add", S), ("sub", S), ("mul", S), ("div", S), ("idiv", S)],
	),
	("Vector2int16", &[("add", S), ("sub", S), ("mul", S), ("div", S)]),
	(
		"Vector3",
		&[("add", S), ("sub", S), ("mul", S), ("div", S), ("idiv", S)],
	),
	("Vector3int16", &[("add", S), ("sub", S), ("mul", S), ("div", S)]),
	("Number", &[("idiv", S)]),
	// rotor extension: Luau's native `vector`
	("vector", &[("add", S), ("sub", S), ("mul", S), ("div", S), ("idiv", S)]),
	(
		"String",
		&[
			("size", S),
			("byte", S),
			("find", S),
			("format", S),
			("gmatch", S),
			("gsub", S),
			("lower", S),
			("match", S),
			("rep", S),
			("reverse", S),
			("split", S),
			("sub", S),
			("upper", S),
		],
	),
	("ArrayLike", &[("size", S)]),
	(
		"ReadonlyArray",
		&[
			("isEmpty", S),
			("join", S),
			("move", S),
			("includes", S),
			("indexOf", S),
			("every", S),
			("some", S),
			("forEach", S),
			("map", S),
			("mapFiltered", S),
			("filterUndefined", S),
			("filter", S),
			("reduce", S),
			("find", S),
			("findIndex", S),
		],
	),
	(
		"Array",
		&[
			("push", S),
			("pop", S),
			("shift", S),
			("unshift", S),
			("insert", S),
			("remove", S),
			("unorderedRemove", S),
			("sort", S),
			("clear", S),
		],
	),
	(
		"ReadonlySet",
		&[("isEmpty", S), ("size", S), ("has", S), ("forEach", S)],
	),
	("Set", &[("delete", S), ("clear", S), ("add", S)]),
	(
		"ReadonlyMap",
		&[("isEmpty", S), ("size", S), ("has", S), ("forEach", S), ("get", S)],
	),
	("Map", &[("delete", S), ("clear", S), ("set", S)]),
	("Promise", &[("then", S)]),
];

/// Runs a property-call macro through `runCallMacro` with upstream's comment wrapping
/// (`propertycallmacros.go` `wrapComments`).
pub(crate) fn run_property_call_macro(
	entry: PropertyCallMacroEntry,
	state: &mut State<'_>,
	node: P<Node>,
	_base_expression: NodeId,
	_node_arguments: &[P<Node>],
) -> NodeId {
	state.not_yet_ported(node, &format!("macro `{}`", entry.name()));
	state.ast.none()
}

/// `envMacroSymbol` (`envmacro.go`): the global `$env` symbol.
pub(crate) fn env_macro_symbol(state: &mut State<'_>) -> Option<P<Symbol>> {
	state.ambient_symbol("$env")
}

/// `nameofMacroSymbol` (`nameofmacro.go`): the global `$nameof` symbol.
pub(crate) fn nameof_macro_symbol(state: &mut State<'_>) -> Option<P<Symbol>> {
	state.ambient_symbol("$nameof")
}

/// `keysMacroSymbol` (`keysmacro.go`): the global `$keys` symbol.
pub(crate) fn keys_macro_symbol(state: &mut State<'_>) -> Option<P<Symbol>> {
	state.ambient_symbol("$keys")
}

/// `fileMacroSymbol` (`filemacro.go`): the global `$file` symbol.
pub(crate) fn file_macro_symbol(state: &mut State<'_>) -> Option<P<Symbol>> {
	state.ambient_symbol("$file")
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

fn intercept_stub(state: &mut State<'_>, chain: &[ChainItem], expression: P<Node>, name: &str) -> Option<NodeId> {
	if chain.is_empty() || !is_macro_node(state, expression, name) {
		return None;
	}
	state.not_yet_ported(chain[0].node, &format!("the {name} macro"));
	Some(state.ast.none())
}

/// `interceptEnvChain` (`envmacro.go`): `None` when the chain's base is not `$env`.
pub(crate) fn intercept_env_chain(state: &mut State<'_>, chain: &[ChainItem], expression: P<Node>) -> Option<NodeId> {
	intercept_stub(state, chain, expression, "$env")
}

/// `interceptNameofChain` (`nameofmacro.go`).
pub(crate) fn intercept_nameof_chain(
	state: &mut State<'_>,
	chain: &[ChainItem],
	expression: P<Node>,
) -> Option<NodeId> {
	intercept_stub(state, chain, expression, "$nameof")
}

/// `interceptKeysChain` (`keysmacro.go`).
pub(crate) fn intercept_keys_chain(state: &mut State<'_>, chain: &[ChainItem], expression: P<Node>) -> Option<NodeId> {
	intercept_stub(state, chain, expression, "$keys")
}

/// `interceptFileChain` (`filemacro.go`).
pub(crate) fn intercept_file_chain(state: &mut State<'_>, chain: &[ChainItem], expression: P<Node>) -> Option<NodeId> {
	intercept_stub(state, chain, expression, "$file")
}
