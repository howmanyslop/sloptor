//! `luautypes.go`: the shared TS-to-Luau type builder's context, loss records, spelling hooks
//! and alias registry.
//!
//! One mapping from checker types to the typed Luau AST, used by the experimental `luauTypes`
//! mode at full fidelity and, through a [`Spelling`], by the `//!native` annotations. The builder
//! never inspects rendered text; the renderer owns parenthesization. Where Luau has no spelling
//! for a TS construct the builder substitutes the closest type and records a [`FidelityLoss`].
//! `luautypes_builder.rs` holds the mapping itself.

use std::collections::{HashMap, HashSet};

use rotor_luau::render::{RenderError, RenderState, render};
use rotor_luau::{TypeAccess, TypeGeneric, TypeReference};
use rotor_tsrs::ast::SymbolFlags;

use super::SliceState;
use super::nativetypes::{NATIVE_TYPE_ANY, NativeTypeKind};
use crate::prelude::*;

/// `typePosition`: where a type is being spelled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TypePosition {
	/// `positionNested`: any untagged position. Unspellable types widen to `any` here.
	#[default]
	Nested,
	/// `positionParameter`: a function's top-level parameter annotation.
	Parameter,
	/// `positionReturn`: a function's declared return, the only position where packs are legal.
	Return,
	/// `positionLocal`: a `local x: T` annotation.
	Local,
	/// `positionAliasBody`: the top of a `type Name = T` body.
	AliasBody,
}

/// `typeContext`: what the builder knows about the annotation site.
#[derive(Clone, Copy, Debug, Default)]
pub struct TypeContext {
	/// The position.
	pub position: TypePosition,
	/// What `self` means for method members of the table at this position.
	pub receiver: Option<NodeId>,
	/// What the polymorphic `this` type means at this site; survives into nested positions.
	pub this: Option<NodeId>,
	/// The declaration the annotation belongs to, for loss attribution.
	pub owner: Option<P<Node>>,
	/// Marks the table of a class instance alias.
	pub class_instance: bool,
}

impl TypeContext {
	/// A context at `position` owned by `owner`.
	#[must_use]
	pub fn at(position: TypePosition, owner: Option<P<Node>>) -> Self {
		Self {
			position,
			owner,
			..Self::default()
		}
	}

	/// `nested`: the context for a child slot, untagged and without a receiver of its own.
	#[must_use]
	pub const fn nested(self) -> Self {
		Self {
			position: TypePosition::Nested,
			receiver: None,
			this: self.this,
			owner: self.owner,
			class_instance: false,
		}
	}
}

/// `fidelityLossKind`: a TS construct Luau could not spell exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FidelityLossKind {
	/// `lossNumberLiteral`
	NumberLiteral,
	/// `lossTuple`
	Tuple,
	/// `lossTemplateLiteral`
	TemplateLiteral,
	/// `lossSymbolKey`
	SymbolKey,
	/// `lossBigInt`
	BigInt,
	/// `lossConstraintDropped`
	ConstraintDropped,
	/// `lossIndexerCollapsed`
	IndexerCollapsed,
	/// `lossRecursiveAliasArgs`
	RecursiveAliasArgs,
	/// `lossLuaTupleOutsideReturn`
	LuaTupleOutsideReturn,
	/// `lossOptionalPack`
	OptionalPack,
	/// `lossClassReadonlyDropped`
	ClassReadonlyDropped,
	/// `lossPrivateExposed`
	PrivateExposed,
	/// `lossObjectLiteralSelf`
	ObjectLiteralSelf,
	/// `lossMappedType`
	MappedType,
	/// `lossConditionalType`
	ConditionalType,
	/// `lossInferType`
	InferType,
	/// `lossKeyRemap`
	KeyRemap,
	/// `lossThisType`
	ThisType,
	/// `lossDepthExceeded`
	DepthExceeded,
	/// `lossUnspellable`
	Unspellable,
}

impl FidelityLossKind {
	/// `fidelityLossKind.String` (Go keeps it for its tests only).
	#[cfg(test)]
	#[must_use]
	pub const fn name(self) -> &'static str {
		match self {
			Self::NumberLiteral => "NumberLiteral",
			Self::Tuple => "Tuple",
			Self::TemplateLiteral => "TemplateLiteral",
			Self::SymbolKey => "SymbolKey",
			Self::BigInt => "BigInt",
			Self::ConstraintDropped => "ConstraintDropped",
			Self::IndexerCollapsed => "IndexerCollapsed",
			Self::RecursiveAliasArgs => "RecursiveAliasArgs",
			Self::LuaTupleOutsideReturn => "LuaTupleOutsideReturn",
			Self::OptionalPack => "OptionalPack",
			Self::ClassReadonlyDropped => "ClassReadonlyDropped",
			Self::PrivateExposed => "PrivateExposed",
			Self::ObjectLiteralSelf => "ObjectLiteralSelf",
			Self::MappedType => "MappedType",
			Self::ConditionalType => "ConditionalType",
			Self::InferType => "InferType",
			Self::KeyRemap => "KeyRemap",
			Self::ThisType => "ThisType",
			Self::DepthExceeded => "DepthExceeded",
			Self::Unspellable => "Unspellable",
		}
	}
}

/// `fidelityLoss`: one substitution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FidelityLoss {
	/// Why.
	pub kind: FidelityLossKind,
	/// The TS type as the checker prints it.
	pub ts_text: String,
	/// The Luau text that replaced it.
	pub luau: String,
	/// The reason.
	pub reason: String,
	/// The declaration it belongs to.
	pub node: Option<P<Node>>,
}

/// `builtType`: the builder's answer for one type.
#[derive(Clone, Copy, Debug)]
pub struct BuiltType {
	/// Never `none`: `any` at worst.
	pub node: NodeId,
	/// The top-level classification the native policy reuses.
	pub kind: NativeTypeKind,
	/// Rule R1: the type names an enclosing function's generic in a parameter position.
	pub outer_generic: bool,
	/// Building this type recorded at least one loss.
	pub lossy: bool,
}

/// The `*ast.Symbol` keys of the registry and of `Origins`. Go synthesizes `&ast.Symbol{Name}`
/// for the `Anonymous_N` and prelude aliases; those are [`SymKey::Synthetic`] here, with no
/// flags, so the program's symbols (and their ids) are never touched.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SymKey {
	/// A checker symbol.
	Real(P<Symbol>),
	/// A synthesized symbol: a number unique within its builder, and its flags.
	Synthetic(u32, SymbolFlags),
}

impl SymKey {
	/// The symbol's flags.
	#[must_use]
	pub fn flags(self) -> SymbolFlags {
		match self {
			Self::Real(symbol) => symbol.flags(),
			Self::Synthetic(_, flags) => flags,
		}
	}
}

/// `typeSpelling`: the decisions a post-pass over the built tree cannot reverse.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Spelling {
	/// `defaultSpelling`: full fidelity.
	#[default]
	Default,
	/// `nativeSpelling` (`nativetypes_spelling.go`).
	Native,
}

/// `aliasPlacement`: where an alias statement is emitted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AliasPlacement {
	/// `atDeclaration`
	#[default]
	AtDeclaration,
	/// `hoisted`
	Hoisted,
}

/// An index into a builder's entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EntryId(pub(crate) usize);

/// `typeAliasEntry`: one registered `type` statement (or `type function`).
#[derive(Clone, Debug)]
#[expect(clippy::struct_excessive_bools, reason = "Go's typeAliasEntry fields")]
pub struct TypeAliasEntry {
	/// The symbol.
	pub symbol: SymKey,
	/// The alias statement; `None` once `type_function` is set.
	pub statement: Option<NodeId>,
	/// The `type function` that replaces the statement.
	pub type_function: Option<NodeId>,
	/// The alias name.
	pub name: String,
	/// The declared generics with their defaults.
	pub generics: Vec<TypeGeneric>,
	/// Each generic's index into `params`.
	pub generic_indexes: Vec<usize>,
	/// What the alias names at runtime.
	pub kind: NativeTypeKind,
	/// Whether the alias names a function.
	pub callable: bool,
	/// Part of this module's type surface (`export type`).
	pub exported: bool,
	/// Where the statement is emitted.
	pub placement: AliasPlacement,
	/// The alias's own type parameter types, in TS order.
	pub params: Vec<Option<P<Type>>>,
	/// The losses recorded while the body and defaults were built.
	pub losses: Vec<FidelityLoss>,
	/// The body is under construction.
	pub(crate) building: bool,
	/// The statement has been emitted.
	pub(crate) placed: bool,
}

impl TypeAliasEntry {
	pub const fn new(symbol: SymKey, name: String) -> Self {
		Self {
			symbol,
			statement: None,
			type_function: None,
			name,
			generics: Vec::new(),
			generic_indexes: Vec::new(),
			kind: NativeTypeKind::Unspellable,
			callable: false,
			exported: false,
			placement: AliasPlacement::AtDeclaration,
			params: Vec::new(),
			losses: Vec::new(),
			building: false,
			placed: false,
		}
	}
}

/// `typeBuilder`: maps checker types to Luau type nodes for one file.
#[derive(Debug, Default)]
pub struct TypeBuilder {
	pub(crate) spelling: Spelling,
	/// The anonymous object types being spelled, for cycle detection.
	pub(crate) stack: Vec<P<Type>>,
	pub(crate) losses: Vec<FidelityLoss>,
	/// `Origins`: every node spelled from a TS symbol, to that symbol.
	pub(crate) origins: HashMap<NodeId, SymKey>,
	/// Nested build calls, for the hard recursion guard.
	pub(crate) depth: usize,
	/// Every entry this builder made; `aliases` and `alias_order` index into it.
	pub(crate) entries: Vec<TypeAliasEntry>,
	/// The registry: `None` marks a symbol the spelling declined.
	pub(crate) aliases: HashMap<SymKey, Option<EntryId>>,
	pub(crate) alias_order: Vec<EntryId>,
	pub(crate) alias_names: HashSet<String>,
	/// A recursive anonymous object type to its `Anonymous_N` alias.
	pub(crate) anonymous: HashMap<P<Type>, EntryId>,
	pub(crate) anonymous_count: u32,
	pub(crate) anonymous_class_count: u32,
	pub(crate) synthetic_count: u32,
	pub(crate) reserved_type_parameters: Option<HashSet<String>>,
	pub(crate) imported_names: Option<HashMap<P<Symbol>, String>>,
	pub(crate) imported_namespaces: HashMap<P<Symbol>, String>,
	pub(crate) constraint_losses: HashSet<P<Symbol>>,
	/// A TS helper name (`Partial`) to its prelude type function.
	pub(crate) preludes: HashMap<String, EntryId>,
}

impl TypeBuilder {
	/// `newTypeBuilder`
	pub fn new(spelling: Spelling) -> Self {
		Self {
			spelling,
			..Self::default()
		}
	}

	/// The entry behind `id`.
	pub fn entry(&self, id: EntryId) -> &TypeAliasEntry {
		&self.entries[id.0]
	}

	/// The entry behind `id`, mutably.
	pub fn entry_mut(&mut self, id: EntryId) -> &mut TypeAliasEntry {
		&mut self.entries[id.0]
	}

	/// Stores a new entry.
	pub fn push_entry(&mut self, entry: TypeAliasEntry) -> EntryId {
		self.entries.push(entry);
		EntryId(self.entries.len() - 1)
	}

	/// A fresh synthesized symbol (Go `&ast.Symbol{Name: name}`).
	pub const fn synthetic_symbol(&mut self) -> SymKey {
		self.synthetic_count += 1;
		SymKey::Synthetic(self.synthetic_count, SymbolFlags::None)
	}

	/// The registered entry of `symbol`, if any (Go `b.aliases[symbol]`, nil included).
	pub fn alias_entry(&self, symbol: SymKey) -> Option<EntryId> {
		self.aliases.get(&symbol).copied().flatten()
	}

	/// `Aliases`: the registered entries in registration order.
	pub fn aliases_in_order(&self) -> Vec<EntryId> {
		self.alias_order.clone()
	}
}

/// Which of the file's builders: the shared one (`luauTypeBuilder`) or the native one
/// (`nativeTypeBuilder`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuilderId {
	/// `State.luauTypeBuilder`
	Shared,
	/// `State.nativeTypeBuilder`
	Native,
}

impl SliceState {
	/// The builder behind `id`, created with the default spelling if it does not exist yet.
	pub fn builder_mut(&mut self, id: BuilderId) -> &mut TypeBuilder {
		match id {
			BuilderId::Shared => self.shared.get_or_insert_with(TypeBuilder::default),
			BuilderId::Native => self.native.get_or_insert_with(|| TypeBuilder::new(Spelling::Native)),
		}
	}

	/// The builder behind `id`, if it exists.
	pub const fn builder(&self, id: BuilderId) -> Option<&TypeBuilder> {
		match id {
			BuilderId::Shared => self.shared.as_ref(),
			BuilderId::Native => self.native.as_ref(),
		}
	}
}

/// A builder in use: Go's `*typeBuilder`, whose `state` field is the borrowed state.
pub struct Builder<'s, 'a> {
	/// The state the builder belongs to.
	pub s: &'s mut State<'a>,
	/// Which of the state's builders.
	pub id: BuilderId,
}

impl Builder<'_, '_> {
	/// The builder's data.
	pub fn b(&mut self) -> &mut TypeBuilder {
		self.s.slices.luau_types.builder_mut(self.id)
	}

	/// The state's Luau arena.
	pub const fn ast(&mut self) -> &mut Ast {
		&mut self.s.ast
	}

	/// [`render_type_text`], with Go's render panic as an internal error.
	pub fn luau_type_text(&mut self, node: NodeId) -> String {
		render_type_text(&self.s.ast, node).unwrap_or_else(|error| {
			self.s.ice(format!("transformer: renderTypeText: {error}"));
			String::new()
		})
	}

	/// The builder's spelling.
	pub fn spelling(&mut self) -> Spelling {
		self.b().spelling
	}

	/// `lose`: records that `t` was spelled as `replacement` and returns the replacement.
	pub fn lose(
		&mut self,
		kind: FidelityLossKind,
		t: Option<P<Type>>,
		replacement: NodeId,
		ctx: TypeContext,
		reason: &str,
	) -> NodeId {
		let text = self.type_text(t);
		self.record(kind, text, Some(replacement), ctx, reason);
		replacement
	}

	/// `loseText`: [`Builder::lose`] for a construct without a checker type of its own.
	pub fn lose_text(
		&mut self,
		kind: FidelityLossKind,
		ts_text: &str,
		replacement: NodeId,
		ctx: TypeContext,
		reason: &str,
	) -> NodeId {
		self.record(kind, ts_text.to_owned(), Some(replacement), ctx, reason);
		replacement
	}

	/// `record`: appends one loss. `replacement` is `None` when the construct was skipped.
	pub fn record(
		&mut self,
		kind: FidelityLossKind,
		ts_text: String,
		replacement: Option<NodeId>,
		ctx: TypeContext,
		reason: &str,
	) {
		let luau = replacement.map_or_else(String::new, |node| self.luau_type_text(node));
		self.b().losses.push(FidelityLoss {
			kind,
			ts_text,
			luau,
			reason: reason.to_owned(),
			node: ctx.owner,
		});
	}

	/// `typeText`: the checker's spelling of `t`, `""` for no type.
	pub fn type_text(&mut self, t: Option<P<Type>>) -> String {
		t.map_or_else(String::new, |t| self.s.ck().type_to_string(t))
	}

	/// `clone`: copies a node reused at several sites and carries its origin along.
	pub fn clone_node(&mut self, node: NodeId) -> NodeId {
		let clone = clone_type(&mut self.s.ast, node);
		if let Some(&symbol) = self.b().origins.get(&node) {
			self.b().origins.insert(clone, symbol);
		}
		clone
	}

	/// Records `node`'s origin.
	pub fn set_origin(&mut self, node: NodeId, symbol: SymKey) {
		self.b().origins.insert(node, symbol);
	}

	/// `Aliases`
	pub fn aliases(&mut self) -> Vec<EntryId> {
		self.b().aliases_in_order()
	}
}

/// `spellingFor`: a `//!native` file annotated by nativeTypes alone spells for Luau's native
/// code generator; with luauTypes on as well the shared builder belongs to luauTypes.
pub const fn spelling_for(state: &State<'_>) -> Spelling {
	if state.is_native && !state.is_luau_typed {
		Spelling::Native
	} else {
		Spelling::Default
	}
}

/// `State.types`: the file's builder, created on first use.
pub fn types<'s, 'a>(state: &'s mut State<'a>) -> Builder<'s, 'a> {
	if state.slices.luau_types.shared.is_none() {
		let spelling = spelling_for(state);
		state.slices.luau_types.shared = Some(TypeBuilder::new(spelling));
	}
	Builder {
		s: state,
		id: BuilderId::Shared,
	}
}

/// `State.nativeTypes`: the builder the `//!native` annotations use.
pub fn native_types<'s, 'a>(state: &'s mut State<'a>) -> Builder<'s, 'a> {
	if !state.is_luau_typed {
		return types(state);
	}
	if state.slices.luau_types.native.is_none() {
		state.slices.luau_types.native = Some(TypeBuilder::new(Spelling::Native));
	}
	Builder {
		s: state,
		id: BuilderId::Native,
	}
}

/// `LuauTypeLosses`: every fidelity loss recorded while this file's types were built (Go keeps it for its tests only).
#[cfg(test)]
pub fn luau_type_losses(state: &mut State<'_>) -> Vec<FidelityLoss> {
	types(state).b().losses.clone()
}

/// `typeAliasFor`: registers (on first use) and returns the alias for a symbol the spelling
/// agreed to alias. Production code calls the builder's `alias_for`; Go keeps it for its tests only.
#[cfg(test)]
pub fn type_alias_for(state: &mut State<'_>, symbol: Option<P<Symbol>>) -> Option<EntryId> {
	types(state).alias_for(symbol.map(SymKey::Real))
}

/// `renderTypeText`: a type node rendered on one line. Go's `render.Render` panics on a node it
/// cannot render; callers turn the error into an internal error.
///
/// # Errors
///
/// When the node does not render.
pub fn render_type_text(ast: &Ast, node: NodeId) -> Result<String, RenderError> {
	render(&mut RenderState::new(), ast, node)
}

/// `cloneType`: an occurrence-specific copy of a node reused at several sites.
pub fn clone_type(ast: &mut Ast, node: NodeId) -> NodeId {
	let origin = ast.origin(node);
	ast.with_origin(node, origin)
}

/// `anyType`
pub fn any_type(ast: &mut Ast) -> NodeId {
	ast.type_name(NATIVE_TYPE_ANY)
}

/// `anyTable`: `{ [any]: any }`.
pub fn any_table(ast: &mut Ast) -> NodeId {
	let key = any_type(ast);
	let value = any_type(ast);
	let indexer = ast.type_table_indexer(TypeAccess::None, key, value);
	let fields = ast.list_from::<element::TypeTableField>([indexer]);
	ast.type_table(fields)
}

/// `anyFunction`: `(...any) -> ...any`.
pub fn any_function(ast: &mut Ast) -> NodeId {
	let parameter = any_type(ast);
	let var_args = ast.type_variadic(parameter);
	let ret = any_type(ast);
	let ret = ast.type_variadic(ret);
	ast.type_function(Vec::new(), Vec::new(), Some(var_args), ret)
}

/// `isAnyType`: a bare `any` reference.
pub fn is_any_type(ast: &Ast, node: NodeId) -> bool {
	ast.get::<TypeReference>(node).is_some_and(|reference| {
		reference.module.is_empty()
			&& reference.name == NATIVE_TYPE_ANY
			&& reference.arguments.is_none_or(|arguments| ast.list_is_empty(arguments))
	})
}
