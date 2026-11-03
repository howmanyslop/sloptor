//! `macromanager.go`: `classes/MacroManager.ts` and `macros/types.ts`. One manager owns all macro
//! identification and is shared by every file of a pass through [`crate::MultiState`].
//!
//! The registration tables themselves live in [`crate::macros`] (Stage 4b slice iv); this file
//! resolves them against the checker's ambient declarations exactly as Go does.
//!
//! Registration divergence (Go's): upstream throws `ProjectError` when a registration name does
//! not resolve; rotor skips the entry and records the exact upstream text, and the compile fails
//! hard on a non-empty [`MacroManager::missing`]. Gating on the package sentinels (compiler-types
//! present iff `LuaTuple` resolves, `@rbxts/types` iff `CFrame` does) keeps checker-light
//! projects working.
//!
//! Go iterates its registration maps in map order, which only reorders checker queries;
//! [`MacroManager::missing`] sorts. Rust iterates the tables in declaration order.

use std::collections::HashMap;

use rotor_tsrs::ast::{self as tsast, SymbolFlags};
use rotor_tsrs::checker::{Checker, TypeChecker};
use rotor_tsrs::types::NOMINAL_LUA_TUPLE_NAME;

use crate::macros;
use crate::prelude::*;

/// `IdentifierMacro` (`macros/types.ts`).
pub type IdentifierMacro = fn(&mut State<'_>, P<Node>) -> NodeId;
/// `ConstructorMacro`: `node` is the `new` expression.
pub type ConstructorMacro = fn(&mut State<'_>, P<Node>) -> NodeId;
/// `CallMacro`: the call node, the transformed callee and the transformed arguments.
pub type CallMacro = fn(&mut State<'_>, P<Node>, NodeId, Vec<NodeId>) -> NodeId;
/// `PropertyCallMacro`: the call node, the transformed receiver and the transformed arguments.
pub type PropertyCallMacro = CallMacro;

/// `IdentifierMacroEntry`
#[derive(Clone, Copy, Debug)]
pub struct IdentifierMacroEntry {
	/// The macro.
	pub macro_fn: IdentifierMacro,
}

/// `CallMacroEntry`
#[derive(Clone, Copy, Debug)]
pub struct CallMacroEntry {
	/// The registration name.
	pub name: &'static str,
	/// The macro.
	pub macro_fn: CallMacro,
}

/// `PropertyCallMacroEntry`: Go names it `"<class>.<method>"`.
#[derive(Clone, Copy, Debug)]
pub struct PropertyCallMacroEntry {
	/// The registering class.
	pub class_name: &'static str,
	/// The method.
	pub method_name: &'static str,
	/// The macro.
	pub macro_fn: PropertyCallMacro,
}

impl PropertyCallMacroEntry {
	/// The Go registration name, `"<class>.<method>"`.
	#[must_use]
	pub fn name(&self) -> String {
		format!("{}.{}", self.class_name, self.method_name)
	}
}

/// `symbolNames` (`MacroManager.ts` `SYMBOL_NAMES`), in upstream declaration order.
const SYMBOL_NAMES: &[&str] = &[
	"globalThis",
	"ArrayConstructor",
	"SetConstructor",
	"MapConstructor",
	"WeakSetConstructor",
	"WeakMapConstructor",
	"ReadonlyMapConstructor",
	"ReadonlySetConstructor",
	"Array",
	"Generator",
	"IterableFunction",
	"LuaTuple",
	"Map",
	"Object",
	"ReadonlyArray",
	"ReadonlyMap",
	"ReadonlySet",
	"ReadVoxelsArray",
	"Set",
	"SharedTable",
	"String",
	"TemplateStringsArray",
	"WeakMap",
	"WeakSet",
	"Iterable",
	"$range",
	"$tuple",
];

/// `typesNotice` (`TYPES_NOTICE`), appended to every registration failure.
const TYPES_NOTICE: &str = "\nYou may need to update your @rbxts/compiler-types!";

/// `rbxTypesClasses`: the property-call classes `@rbxts/types` declares; their failures gate on
/// the `CFrame` sentinel.
const RBX_TYPES_CLASSES: &[&str] = &[
	"CFrame",
	"UDim",
	"UDim2",
	"Vector2",
	"Vector2int16",
	"Vector3",
	"Vector3int16",
	"Number",
	"vector",
];

/// `optionalPropertyCallClasses`: rotor-extension rows whose types side may be absent from an
/// older `@rbxts/types`; they never fail the audit.
const OPTIONAL_PROPERTY_CALL_CLASSES: &[&str] = &["vector"];

/// `macroOnlyClasses` (`MACRO_ONLY_CLASSES`): classes that exist only as compiler macros.
const MACRO_ONLY_CLASSES: &[&str] = &[
	"ReadonlyArray",
	"Array",
	"ReadonlyMap",
	"WeakMap",
	"Map",
	"ReadonlySet",
	"WeakSet",
	"Set",
	"String",
];

/// `MacroManager`: symbol to macro maps built once per pass, plus the `SYMBOL_NAMES` ambient
/// symbol registry.
#[derive(Default)]
pub struct MacroManager {
	/// The `SYMBOL_NAMES` memo; other names memoize lazily, misses included.
	symbols: HashMap<String, Option<P<Symbol>>>,
	identifier_macros: HashMap<P<Symbol>, IdentifierMacroEntry>,
	call_macros: HashMap<P<Symbol>, CallMacroEntry>,
	constructor_macros: HashMap<P<Symbol>, ConstructorMacro>,
	property_call_macros: HashMap<P<Symbol>, PropertyCallMacroEntry>,
	lua_tuple_nominal: Option<P<Symbol>>,
	lua_tuple_nominal_resolved: bool,
	missing_compiler_types: Vec<String>,
	missing_rbx_types: Vec<String>,
	compiler_types_present: bool,
	rbx_types_present: bool,
}

impl MacroManager {
	/// `NewMacroManager`: resolves every registration through `resolveName` against the ambient
	/// declarations. Constructor macros are keyed by the construct-signature symbol of the
	/// global interface.
	#[expect(clippy::too_many_lines, reason = "one block per upstream registration table")]
	pub fn new(checker: &mut Checker) -> Self {
		let mut checker = TypeChecker::new(checker);
		let mut manager = Self {
			compiler_types_present: checker
				.resolve_name("LuaTuple", None, SymbolFlags::All, false)
				.is_some(),
			rbx_types_present: checker
				.resolve_name("CFrame", None, SymbolFlags::Interface, false)
				.is_some(),
			..Self::default()
		};

		for &(name, macro_fn) in macros::IDENTIFIER_MACROS {
			if let Some(symbol) = checker.resolve_name(name, None, SymbolFlags::Variable, false) {
				manager
					.identifier_macros
					.insert(symbol, IdentifierMacroEntry { macro_fn });
			} else {
				manager.record_missing(
					name,
					format!("MacroManager could not find symbol for {name}{TYPES_NOTICE}"),
				);
			}
		}

		for &(name, macro_fn) in macros::CALL_MACROS {
			if let Some(symbol) = checker.resolve_name(name, None, SymbolFlags::Function, false) {
				manager.call_macros.insert(symbol, CallMacroEntry { name, macro_fn });
			} else {
				manager.record_missing(
					name,
					format!("MacroManager could not find symbol for {name}{TYPES_NOTICE}"),
				);
			}
		}

		for &(name, macro_fn) in macros::CONSTRUCTOR_MACROS {
			let Some(symbol) = checker.resolve_name(name, None, SymbolFlags::Interface, false) else {
				manager.record_missing(
					name,
					format!("MacroManager could not find symbol for {name}{TYPES_NOTICE}"),
				);
				continue;
			};
			// getFirstDeclarationOrThrow + getConstructorSymbol: the first interface only.
			let interface_declaration = symbol
				.declarations()
				.iter()
				.copied()
				.find(|&declaration| tsast::is_interface_declaration(declaration));
			match interface_declaration {
				// getFirstDeclarationOrThrow throws ProjectError("") (MacroManager.ts L70).
				None => manager.record_missing(name, String::new()),
				Some(declaration) => match interface_construct_symbol(declaration) {
					Some(construct_symbol) => {
						manager.constructor_macros.insert(construct_symbol, macro_fn);
					}
					None => manager.record_missing(
						name,
						format!("MacroManager could not find constructor for {name}{TYPES_NOTICE}"),
					),
				},
			}
		}

		// PROPERTY_CALL_MACROS (MacroManager.ts L119-144): the method symbols of every
		// interface declaration of the class, keyed by the method's function type symbol.
		for &(class_name, methods) in macros::PROPERTY_CALL_MACROS {
			let optional = OPTIONAL_PROPERTY_CALL_CLASSES.contains(&class_name);
			let Some(symbol) = checker.resolve_name(class_name, None, SymbolFlags::Interface, false) else {
				if !optional {
					manager.record_missing(
						class_name,
						format!("MacroManager could not find symbol for {class_name}{TYPES_NOTICE}"),
					);
				}
				continue;
			};

			let mut method_map: HashMap<&'static str, P<Symbol>> = HashMap::new();
			for &declaration in symbol.declarations() {
				if !tsast::is_interface_declaration(declaration) {
					continue;
				}
				for &member in declaration.as_interface_declaration().members.nodes() {
					let Some(member_name) = member.name() else {
						continue;
					};
					if tsast::is_method_signature_declaration(member) && member_name.kind() == Kind::Identifier {
						// upstream getType: getTypeAtLocation(skipUpwards(member)).symbol
						let t = checker.get_type_at_location(tsast::skip_upwards(member));
						if let Some(method_symbol) = t.symbol() {
							method_map.insert(member_name.text(), method_symbol);
						}
					}
				}
			}

			for &(method_name, macro_fn) in methods {
				if let Some(&method_symbol) = method_map.get(method_name) {
					manager.property_call_macros.insert(
						method_symbol,
						PropertyCallMacroEntry {
							class_name,
							method_name,
							macro_fn,
						},
					);
				} else if !optional {
					manager.record_missing(
						class_name,
						format!("MacroManager could not find method for {class_name}.{method_name}{TYPES_NOTICE}"),
					);
				}
			}
		}

		// SYMBOL_NAMES (MacroManager.ts L146-153): resolved eagerly into the memo.
		for &name in SYMBOL_NAMES {
			let symbol = checker.resolve_name(name, None, SymbolFlags::All, false);
			manager.symbols.insert(name.to_owned(), symbol);
			if symbol.is_none() {
				manager.record_missing(
					name,
					format!("MacroManager could not find symbol for {name}{TYPES_NOTICE}"),
				);
			}
		}

		manager
	}

	/// `recordMissing`: files a failure under the package that declares `name`.
	fn record_missing(&mut self, name: &str, message: String) {
		if RBX_TYPES_CLASSES.contains(&name) {
			self.missing_rbx_types.push(message);
		} else {
			self.missing_compiler_types.push(message);
		}
	}

	/// `Missing`: the upstream `ProjectError` texts of every failed registration whose package is
	/// present, sorted. Empty when the audit passes.
	#[must_use]
	pub fn missing(&self) -> Vec<String> {
		let mut out = Vec::new();
		if self.compiler_types_present {
			out.extend(self.missing_compiler_types.iter().cloned());
		}
		if self.rbx_types_present {
			out.extend(self.missing_rbx_types.iter().cloned());
		}
		out.sort();
		out
	}

	/// `Symbol` (`getSymbolOrThrow`): `resolveName(name, undefined, All, false)`, memoized with
	/// misses. `None` instead of throwing for projects without `@rbxts/compiler-types`.
	pub fn symbol(&mut self, checker: &mut Checker, name: &str) -> Option<P<Symbol>> {
		if let Some(&symbol) = self.symbols.get(name) {
			return symbol;
		}
		let symbol = TypeChecker::new(checker).resolve_name(name, None, SymbolFlags::All, false);
		self.symbols.insert(name.to_owned(), symbol);
		symbol
	}

	/// The memoized registry entry for `name` without resolving it (Go's plain map read).
	fn peek_symbol(&self, name: &str) -> Option<P<Symbol>> {
		self.symbols.get(name).copied().flatten()
	}

	/// `LuaTupleNominalSymbol`: the `_nominal_LuaTuple` property of the ambient `LuaTuple<T>`
	/// alias, memoized.
	pub fn lua_tuple_nominal_symbol(&mut self, checker: &mut Checker) -> Option<P<Symbol>> {
		if self.lua_tuple_nominal_resolved {
			return self.lua_tuple_nominal;
		}
		self.lua_tuple_nominal_resolved = true;
		if let Some(lua_tuple) = self.symbol(checker, "LuaTuple")
			&& let Some(declaration) = lua_tuple
				.declarations()
				.iter()
				.copied()
				.find(|&declaration| tsast::is_type_alias_declaration(declaration))
		{
			let mut checker = TypeChecker::new(checker);
			let t = checker.get_type_at_location(declaration);
			self.lua_tuple_nominal = checker.get_property_of_type(t, NOMINAL_LUA_TUPLE_NAME);
		}
		self.lua_tuple_nominal
	}

	/// `IsMacroOnlyClass`: the symbol is the registered global of that name and the name is in
	/// `MACRO_ONLY_CLASSES`.
	#[must_use]
	pub fn is_macro_only_class(&self, symbol: P<Symbol>) -> bool {
		self.peek_symbol(symbol.name()) == Some(symbol) && MACRO_ONLY_CLASSES.contains(&symbol.name())
	}

	/// `GetIdentifierMacro`: table only.
	#[must_use]
	pub fn get_identifier_macro(&self, symbol: P<Symbol>) -> Option<IdentifierMacroEntry> {
		self.identifier_macros.get(&symbol).copied()
	}

	/// `GetCallMacro`: table only.
	#[must_use]
	pub fn get_call_macro(&self, symbol: P<Symbol>) -> Option<CallMacroEntry> {
		self.call_macros.get(&symbol).copied()
	}

	/// `IsTypeCheckCallMacro`: `macro === CALL_MACROS.typeIs || macro === CALL_MACROS.typeOf`.
	#[must_use]
	pub fn is_type_check_call_macro(&self, symbol: P<Symbol>) -> bool {
		self.get_call_macro(symbol)
			.is_some_and(|entry| entry.name == "typeIs" || entry.name == "typeOf")
	}

	/// `GetConstructorMacro`: table only.
	#[must_use]
	pub fn get_constructor_macro(&self, symbol: P<Symbol>) -> Option<ConstructorMacro> {
		self.constructor_macros.get(&symbol).copied()
	}

	/// `GetPropertyCallMacro` (`L190-201`): the table entry, or upstream's assert for a method of
	/// a macro-only class with no registered macro: `Err("Macro X.y() is not implemented!")`.
	///
	/// # Errors
	///
	/// The assert's message, which Go raises as a panic.
	pub fn get_property_call_macro(&self, symbol: P<Symbol>) -> Result<Option<PropertyCallMacroEntry>, String> {
		let entry = self.property_call_macros.get(&symbol).copied();
		if entry.is_none()
			&& let Some(parent) = symbol.parent()
			&& self.peek_symbol(parent.name()) == Some(parent)
			&& self.is_macro_only_class(parent)
		{
			return Err(format!(
				"Macro {}.{}() is not implemented!",
				parent.name(),
				symbol.name()
			));
		}
		Ok(entry)
	}
}

/// `interfaceConstructSymbol` (`getConstructorSymbol`): the first construct signature member.
fn interface_construct_symbol(interface_declaration: P<Node>) -> Option<P<Symbol>> {
	interface_declaration
		.as_interface_declaration()
		.members
		.nodes()
		.iter()
		.copied()
		.find(|&member| tsast::is_construct_signature_declaration(member))
		.and_then(|member| member.symbol())
}

impl State<'_> {
	/// [`MacroManager::get_property_call_macro`], recording Go's panic as an internal error.
	pub(crate) fn get_property_call_macro(&mut self, symbol: P<Symbol>) -> Option<PropertyCallMacroEntry> {
		match self.macros().get_property_call_macro(symbol) {
			Ok(entry) => entry,
			Err(message) => {
				self.ice(message);
				None
			}
		}
	}
}
