//! `luautypes_typefunction.go`: type-function translation.
//!
//! A generic type alias whose body contains a mapped, conditional or template literal type has
//! no Luau spelling as an alias, but Luau's new solver can compute it from a user-defined
//! `type function`. The translator works from the TS AST of the alias body and emits the Lua of
//! that body over the `types` library. Anything outside the translated subset makes the whole
//! alias fall back to `type Name<...> = any` with a loss comment.

use std::collections::{HashMap, HashSet};

use rotor_luau::{Identifier, TypeFunctionDeclaration, is_valid_identifier};
use rotor_tsrs::ast::{self as tsast, ModifierFlags, SymbolFlags};
use rotor_tsrs::checker::{ObjectFlags, TypeFlags};

use super::luautypes::{
	Builder, BuiltType, EntryId, FidelityLoss, FidelityLossKind, SymKey, TypeAliasEntry, TypeContext, any_type, types,
};
use super::luautypes_alias::{alias_declaration_of_symbol, alias_placement_for, is_luau_exported_type};
use super::luautypes_builder::declared_type_parameters;
use super::luautypes_typefunction_prelude::{
	HELPER_DISTRIBUTE, HELPER_INDEX_OF, HELPER_JOIN, HELPER_KEYS, HELPER_RETURN_OF, HELPER_TEMPLATE,
	HELPER_WITHOUT_NIL, LUAU_TYPES_PRELUDE_NAMES, TYPE_FUNCTION_HELPER_ORDER, require_prelude, tf_and, tf_array,
	tf_call_id, tf_call_statement, tf_eq, tf_error, tf_field, tf_for_in, tf_if, tf_if_expr, tf_insert, tf_is, tf_local,
	tf_map, tf_method, tf_not, tf_or, tf_prop, tf_return, tf_set, tf_stmts, tf_type_function, tf_typed_local, tf_types,
	tf_types_call, type_function_helper_statements,
};
use super::nativetypes::{NATIVE_TYPE_ANY, NATIVE_TYPE_NUMBER, NativeTypeKind};
use crate::prelude::*;
use crate::types::get_type_arguments;

/// `inferReason`: the loss reason for an `infer` outside the two shapes.
const INFER_REASON: &str =
	"`infer` is only translated in `T extends (infer U)[]` and `T extends (...args: any) => infer R`";

/// `valueSink`: where a computed type value goes: `return v`, `name = v` or
/// `table.insert(parts, v)`.
#[derive(Clone, Debug)]
enum Sink {
	Return,
	Set(String),
	Insert(String),
}

impl Sink {
	fn apply(&self, ast: &mut Ast, value: NodeId) -> NodeId {
		match self {
			Self::Return => tf_return(ast, value),
			Self::Set(name) => {
				let target = ast.id(name.as_str());
				tf_set(ast, target, value)
			}
			Self::Insert(name) => {
				let list = ast.id(name.as_str());
				tf_insert(ast, list, value)
			}
		}
	}
}

/// An expression factory: every use site adopts its own node.
#[derive(Clone, Debug)]
enum Factory {
	/// `luau.ID(name)`
	Id(String),
	/// `tfProp(luau.ID(name), property)`
	Prop(String, &'static str),
}

impl Factory {
	fn make(&self, ast: &mut Ast) -> NodeId {
		match self {
			Self::Id(name) => ast.id(name.as_str()),
			Self::Prop(name, property) => {
				let receiver = ast.id(name.as_str());
				tf_prop(ast, receiver, property)
			}
		}
	}
}

/// `mappedModifier`: a mapped type's `?` or `readonly` modifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MappedModifier {
	None,
	Add,
	Remove,
}

/// `indexedBinding`: what `Object[Key]` means while a mapped type iterates the properties of
/// `Object`. A `None` value means the access goes through the `indexof` helper.
#[derive(Clone, Debug, Default)]
struct IndexedBinding {
	object: Option<P<Symbol>>,
	value: Option<Factory>,
}

/// `typeFunctionTranslator`: turns one alias body into type-function statements.
struct Translator<'s, 'a> {
	s: &'s mut State<'a>,
	name: String,
	declaration: P<Node>,
	distributable: HashSet<P<Symbol>>,
	env: HashMap<P<Symbol>, Factory>,
	indexed: HashMap<P<Symbol>, IndexedBinding>,
	losses: Vec<FidelityLoss>,
	failure: Option<FidelityLoss>,
	top_kind: FidelityLossKind,
	names: HashSet<String>,
	helpers: HashSet<&'static str>,
	block: Statements,
}

/// `luauTypeFunctionFor`: translates the body of a generic type alias into a `type function`.
/// `None`, with the failure recorded as a loss against the declaration, when the body is outside
/// the supported subset.
pub fn luau_type_function_for(state: &mut State<'_>, alias: P<Symbol>, declaration: P<Node>) -> Option<NodeId> {
	if !tsast::is_type_alias_declaration(declaration) {
		return None;
	}
	types(state);
	let block = state.ast.new_list();
	let mut tr = Translator {
		s: state,
		name: alias.name().to_owned(),
		declaration,
		distributable: HashSet::new(),
		env: HashMap::new(),
		indexed: HashMap::new(),
		losses: Vec::new(),
		failure: None,
		top_kind: FidelityLossKind::Unspellable,
		names: HashSet::new(),
		helpers: HashSet::new(),
		block,
	};
	for reserved in ["types", "type", "error", "table", "tostring", "nil"] {
		tr.names.insert(reserved.to_owned());
	}
	for helper in TYPE_FUNCTION_HELPER_ORDER {
		tr.names.insert((*helper).to_owned());
	}
	let mut parameters = Vec::new();
	for &parameter in declared_type_parameters(declaration) {
		let (Some(name), Some(symbol)) = (parameter.name(), parameter.symbol()) else {
			return None;
		};
		if !is_valid_identifier(name.text()) {
			return None;
		}
		let parameter_name = name.text().to_owned();
		parameters.push(parameter_name.clone());
		tr.names.insert(parameter_name.clone());
		tr.distributable.insert(symbol);
		tr.env.insert(symbol, Factory::Id(parameter_name));
	}
	let body = declaration.type_node();
	let statements = tr.translate(body);
	let losses = core::mem::take(&mut tr.losses);
	let failure = tr.failure.take();
	let top_kind = tr.top_kind;
	let failure = match statements {
		Some(_) => None,
		None => Some(failure.unwrap_or_else(|| tr.loss(top_kind, "the alias body is outside the translated subset"))),
	};
	drop(tr);
	let mut builder = types(state);
	builder.b().losses.extend(losses);
	if let Some(failure) = failure {
		builder.b().losses.push(failure);
		return None;
	}
	let statements = statements?;
	Some(tf_type_function(&mut state.ast, alias.name(), &parameters, statements))
}

impl Translator<'_, '_> {
	/// `translate`: the helpers the body needed first, then the statements, ending in
	/// `return <type>`.
	fn translate(&mut self, body: Option<P<Node>>) -> Option<Statements> {
		let Some(body) = body else {
			self.fail(FidelityLossKind::Unspellable, "the alias has no body");
			return None;
		};
		self.top_kind = top_level_loss_kind(Some(body));
		self.block = self.s.ast.new_list();
		if !self.value(body, &Sink::Return, true) {
			return None;
		}
		let helpers = type_function_helper_statements(&mut self.s.ast, &self.helpers);
		let statements = tf_stmts(&mut self.s.ast, helpers);
		self.s.ast.list_push_list(statements, self.block);
		Some(statements)
	}

	// -------------------------------------------------------------------------------------
	// Bookkeeping
	// -------------------------------------------------------------------------------------

	/// `loss`: a loss record against the declaration, with `any` as the replacement.
	fn loss(&mut self, kind: FidelityLossKind, reason: &str) -> FidelityLoss {
		let ts_text = if let Some(symbol) = self.declaration.symbol() {
			let declared = self.s.ck().get_declared_type_of_symbol(symbol);
			self.s.ck().type_to_string(declared)
		} else {
			self.name.clone()
		};
		FidelityLoss {
			kind,
			ts_text,
			luau: NATIVE_TYPE_ANY.to_owned(),
			reason: reason.to_owned(),
			node: Some(self.declaration),
		}
	}

	/// `fail`: abandons the translation with the first failure's kind and reason. Always
	/// returns `false`.
	fn fail(&mut self, kind: FidelityLossKind, reason: &str) -> bool {
		if self.failure.is_none() {
			self.failure = Some(self.loss(kind, reason));
		}
		false
	}

	/// `fail` for an expression.
	fn fail_expr(&mut self, kind: FidelityLossKind, reason: &str) -> Option<NodeId> {
		self.fail(kind, reason);
		None
	}

	/// `lose`: a non-fatal substitution.
	fn lose(&mut self, kind: FidelityLossKind, ts_text: String, luau_text: &str, reason: &str) {
		self.losses.push(FidelityLoss {
			kind,
			ts_text,
			luau: luau_text.to_owned(),
			reason: reason.to_owned(),
			node: Some(self.declaration),
		});
	}

	/// `fresh`: a local name that collides with no parameter, helper or earlier local.
	fn fresh(&mut self, base: &str) -> String {
		let mut name = base.to_owned();
		let mut suffix = 2_u32;
		loop {
			if !self.names.contains(&name) && is_valid_identifier(&name) {
				self.names.insert(name.clone());
				return name;
			}
			name = format!("{base}_{suffix}");
			suffix += 1;
		}
	}

	fn emit(&mut self, statement: NodeId) {
		self.s.ast.list_push(self.block, statement);
	}

	/// `nested`: runs `run` into a fresh block and returns it.
	fn nested(&mut self, run: impl FnOnce(&mut Self) -> bool) -> (Statements, bool) {
		let saved = self.block;
		self.block = self.s.ast.new_list();
		let ok = run(self);
		let block = self.block;
		self.block = saved;
		(block, ok)
	}

	/// `bind`: maps `symbol` to a factory for the duration of `run`.
	fn bind(&mut self, symbol: P<Symbol>, factory: Factory, run: impl FnOnce(&mut Self) -> bool) -> bool {
		let saved = self.env.insert(symbol, factory);
		let ok = run(self);
		match saved {
			Some(saved) => {
				self.env.insert(symbol, saved);
			}
			None => {
				self.env.remove(&symbol);
			}
		}
		ok
	}

	/// `pin`: makes an expression reusable: an identifier is re-spelled at each use, anything
	/// else is bound to a local first.
	fn pin(&mut self, expression: NodeId, base: &str) -> Factory {
		if let Some(id) = self.s.ast.get::<Identifier>(expression) {
			return Factory::Id(id.name.clone());
		}
		let name = self.fresh(base);
		let local = tf_local(&mut self.s.ast, &name, expression);
		self.emit(local);
		Factory::Id(name)
	}

	fn helper(&mut self, name: &'static str) -> &'static str {
		self.helpers.insert(name);
		name
	}

	fn make(&mut self, factory: &Factory) -> NodeId {
		factory.make(&mut self.s.ast)
	}

	// -------------------------------------------------------------------------------------
	// Values and expressions
	// -------------------------------------------------------------------------------------

	/// `value`: computes `node` and hands the result to `sink`. `tail` marks the body's own
	/// result position, where a guard may `return` early.
	fn value(&mut self, node: P<Node>, sink: &Sink, tail: bool) -> bool {
		let node = skip_parenthesized_types(node);
		match node.kind() {
			Kind::ConditionalType => return self.conditional(node, sink, tail),
			Kind::MappedType => return self.mapped(node, sink, tail),
			_ => {}
		}
		let Some(expression) = self.type_expr(node) else {
			return false;
		};
		let statement = sink.apply(&mut self.s.ast, expression);
		self.emit(statement);
		true
	}

	/// `typeExpr`: `node` as a Lua expression of type `type`, emitting the statements it needs
	/// first.
	fn type_expr(&mut self, node: P<Node>) -> Option<NodeId> {
		let node = skip_parenthesized_types(node);
		match node.kind() {
			Kind::ConditionalType | Kind::MappedType => {
				let name = self.fresh("value");
				let local = tf_typed_local(&mut self.s.ast, &name);
				self.emit(local);
				let ok = self.value(node, &Sink::Set(name.clone()), false);
				ok.then(|| self.s.ast.id(name))
			}
			Kind::TypeReference => self.reference(node),
			Kind::TypeOperator => self.operator(node),
			Kind::IndexedAccessType => self.indexed_access(node),
			Kind::TemplateLiteralType => self.template(node),
			Kind::UnionType => self.union(node),
			Kind::IntersectionType => self.intersection(node),
			Kind::InferType => self.fail_expr(FidelityLossKind::InferType, INFER_REASON),
			_ => self.static_type(node),
		}
	}

	/// `reference`: a type parameter, infer variable or mapped key from the environment; any
	/// other name is outside the type function.
	fn reference(&mut self, node: P<Node>) -> Option<NodeId> {
		let type_name = node.as_type_reference_node().type_name;
		if let Some(symbol) = self.s.ck().get_symbol_at_location(type_name)
			&& let Some(factory) = self.env.get(&symbol).cloned()
			&& node.type_arguments().is_empty()
		{
			return Some(self.make(&factory));
		}
		let reason = format!(
			"`{}` names a type outside the type function; type-function bodies see no outer names",
			entity_name_text(type_name)
		);
		self.fail_expr(self.top_kind, &reason)
	}

	/// `envSymbol`: the environment symbol `node` refers to.
	fn env_symbol(&mut self, node: P<Node>) -> Option<P<Symbol>> {
		let node = skip_parenthesized_types(node);
		if node.kind() != Kind::TypeReference || !node.type_arguments().is_empty() {
			return None;
		}
		let symbol = self
			.s
			.ck()
			.get_symbol_at_location(node.as_type_reference_node().type_name)?;
		self.env.contains_key(&symbol).then_some(symbol)
	}

	/// `operator`: `keyof T` through the keys helper and `readonly T[]` as a read-only array.
	fn operator(&mut self, node: P<Node>) -> Option<NodeId> {
		let operator = node.as_type_operator_node();
		match operator.operator {
			Kind::KeyOfKeyword => {
				let inner = self.type_expr(operator.type_)?;
				let helper = self.helper(HELPER_KEYS);
				Some(tf_call_id(&mut self.s.ast, helper, [inner]))
			}
			Kind::ReadonlyKeyword => {
				let inner = skip_parenthesized_types(operator.type_);
				match inner.kind() {
					Kind::ArrayType => self.array(inner, true),
					Kind::TupleType => self.tuple(inner, true),
					_ => self.type_expr(operator.type_),
				}
			}
			_ => self.fail_expr(self.top_kind, "`unique symbol` has no Luau spelling"),
		}
	}

	/// `indexedAccess`: `T[K]`: the property record of the mapped iteration over `T` when `K`
	/// is its key, else the indexof helper.
	fn indexed_access(&mut self, node: P<Node>) -> Option<NodeId> {
		let access = node.as_indexed_access_type_node();
		if let Some(key) = self.env_symbol(access.index_type)
			&& let Some(binding) = self.indexed.get(&key).cloned()
			&& let (Some(value), Some(object)) = (binding.value, binding.object)
			&& self.env_symbol(access.object_type) == Some(object)
		{
			return Some(self.make(&value));
		}
		let object = self.type_expr(access.object_type)?;
		let index = self.type_expr(access.index_type)?;
		let helper = self.helper(HELPER_INDEX_OF);
		Some(tf_call_id(&mut self.s.ast, helper, [object, index]))
	}

	/// `template`: a template literal type through the template helper.
	fn template(&mut self, node: P<Node>) -> Option<NodeId> {
		let literal = node.as_template_literal_type_node();
		let mut parts = Vec::new();
		let head = literal.head.text();
		if !head.is_empty() {
			parts.push(self.s.ast.str(head));
		}
		for &span in literal.template_spans.nodes() {
			let type_span = span.as_template_literal_type_span();
			let hole = self.type_expr(type_span.type_())?;
			parts.push(hole);
			let text = type_span.literal().text();
			if !text.is_empty() {
				parts.push(self.s.ast.str(text));
			}
		}
		let array = tf_array(&mut self.s.ast, parts);
		let helper = self.helper(HELPER_TEMPLATE);
		Some(tf_call_id(&mut self.s.ast, helper, [array]))
	}

	/// `union`: `A | B` as `types.unionof`, with nil-like members folded into
	/// `types.optional`.
	fn union(&mut self, node: P<Node>) -> Option<NodeId> {
		let mut members = Vec::new();
		let mut optional = false;
		for &member in node.as_union_type_node().types().nodes() {
			if is_nil_type_node(member) {
				optional = true;
				continue;
			}
			members.push(self.type_expr(member)?);
		}
		let mut result = match members.len() {
			0 => {
				let nil = self.s.ast.nil();
				return Some(tf_types_call(&mut self.s.ast, "singleton", [nil]));
			}
			1 => members[0],
			_ => tf_types_call(&mut self.s.ast, "unionof", members),
		};
		if optional {
			result = tf_types_call(&mut self.s.ast, "optional", [result]);
		}
		Some(result)
	}

	/// `intersection`: `A & B` as `types.intersectionof`.
	fn intersection(&mut self, node: P<Node>) -> Option<NodeId> {
		let mut members = Vec::new();
		for &member in node.as_intersection_type_node().types().nodes() {
			members.push(self.type_expr(member)?);
		}
		if members.len() == 1 {
			return Some(members[0]);
		}
		Some(tf_types_call(&mut self.s.ast, "intersectionof", members))
	}

	/// `staticType`: the type nodes that need no environment.
	fn static_type(&mut self, node: P<Node>) -> Option<NodeId> {
		let ast = &mut self.s.ast;
		match node.kind() {
			Kind::LiteralType => self.literal(node),
			Kind::StringKeyword => Some(tf_types(ast, "string")),
			Kind::NumberKeyword => Some(tf_types(ast, "number")),
			Kind::BooleanKeyword => Some(tf_types(ast, "boolean")),
			Kind::AnyKeyword => Some(tf_types(ast, "any")),
			Kind::UnknownKeyword => Some(tf_types(ast, "unknown")),
			Kind::NeverKeyword => Some(tf_types(ast, "never")),
			Kind::UndefinedKeyword | Kind::VoidKeyword | Kind::NullKeyword => {
				let nil = ast.nil();
				Some(tf_types_call(ast, "singleton", [nil]))
			}
			Kind::ObjectKeyword => {
				let nil = ast.nil();
				let any = tf_types(ast, "any");
				let index = tf_field(ast, "index", any);
				let any = tf_types(ast, "any");
				let read = tf_field(ast, "readresult", any);
				let any = tf_types(ast, "any");
				let write = tf_field(ast, "writeresult", any);
				let fields = tf_map(ast, [index, read, write]);
				Some(tf_types_call(ast, "newtable", [nil, fields]))
			}
			Kind::SymbolKeyword => {
				self.lose(
					FidelityLossKind::SymbolKey,
					"symbol".to_owned(),
					NATIVE_TYPE_ANY,
					"Luau has no symbol type",
				);
				Some(tf_types(&mut self.s.ast, "any"))
			}
			Kind::BigIntKeyword => {
				self.lose(
					FidelityLossKind::BigInt,
					"bigint".to_owned(),
					NATIVE_TYPE_NUMBER,
					"Luau has no bigint",
				);
				Some(tf_types(&mut self.s.ast, "number"))
			}
			Kind::ArrayType => self.array(node, false),
			Kind::TupleType => self.tuple(node, false),
			Kind::TypeLiteral => self.type_literal(node),
			Kind::FunctionType => self.function_type(node),
			Kind::ThisType => self.fail_expr(self.top_kind, "`this` has no meaning inside a type function"),
			Kind::TypeQuery => self.fail_expr(self.top_kind, "`typeof` names a value outside the type function"),
			kind => {
				let reason = format!("a {} has no translation", tsast::kind_string(kind));
				self.fail_expr(self.top_kind, &reason)
			}
		}
	}

	/// `literal`: string and boolean singletons, nil, and number widened to `number`.
	fn literal(&mut self, node: P<Node>) -> Option<NodeId> {
		let literal = node.as_literal_type_node().literal;
		match literal.kind() {
			Kind::StringLiteral | Kind::NoSubstitutionTemplateLiteral => {
				let text = self.s.ast.str(literal.text());
				Some(tf_types_call(&mut self.s.ast, "singleton", [text]))
			}
			Kind::TrueKeyword => {
				let value = self.s.ast.bool(true);
				Some(tf_types_call(&mut self.s.ast, "singleton", [value]))
			}
			Kind::FalseKeyword => {
				let value = self.s.ast.bool(false);
				Some(tf_types_call(&mut self.s.ast, "singleton", [value]))
			}
			Kind::NullKeyword => {
				let nil = self.s.ast.nil();
				Some(tf_types_call(&mut self.s.ast, "singleton", [nil]))
			}
			Kind::NumericLiteral | Kind::PrefixUnaryExpression => {
				let t = self.s.ck().get_type_from_type_node(node);
				let text = self.s.ck().type_to_string(t);
				self.lose(
					FidelityLossKind::NumberLiteral,
					text,
					NATIVE_TYPE_NUMBER,
					"Luau has no number singletons",
				);
				Some(tf_types(&mut self.s.ast, "number"))
			}
			Kind::BigIntLiteral => {
				self.lose(
					FidelityLossKind::BigInt,
					literal.text().to_owned(),
					NATIVE_TYPE_NUMBER,
					"Luau has no bigint",
				);
				Some(tf_types(&mut self.s.ast, "number"))
			}
			_ => self.fail_expr(self.top_kind, "unknown literal type"),
		}
	}

	/// `array`: `E[]` as a number-indexed table; readonly drops the write result.
	fn array(&mut self, node: P<Node>, readonly: bool) -> Option<NodeId> {
		let element = self.type_expr(node.as_array_type_node().element_type)?;
		Some(self.array_of(element, readonly))
	}

	fn array_of(&mut self, element: NodeId, readonly: bool) -> NodeId {
		if readonly {
			let ast = &mut self.s.ast;
			let nil = ast.nil();
			let number = tf_types(ast, "number");
			let index = tf_field(ast, "index", number);
			let read = tf_field(ast, "readresult", element);
			let fields = tf_map(ast, [index, read]);
			return tf_types_call(ast, "newtable", [nil, fields]);
		}
		let pinned = self.pin(element, "element");
		let ast = &mut self.s.ast;
		let nil = ast.nil();
		let number = tf_types(ast, "number");
		let index = tf_field(ast, "index", number);
		let value = pinned.make(ast);
		let read = tf_field(ast, "readresult", value);
		let value = pinned.make(ast);
		let write = tf_field(ast, "writeresult", value);
		let fields = tf_map(ast, [index, read, write]);
		tf_types_call(ast, "newtable", [nil, fields])
	}

	/// `tuple`: `[A, B]` as an array of the element union (positions are lost).
	fn tuple(&mut self, node: P<Node>, readonly: bool) -> Option<NodeId> {
		let mut elements = Vec::new();
		for &element in node.as_tuple_type_node().elements().nodes() {
			let element = if element.kind() == Kind::NamedTupleMember {
				element.as_named_tuple_member().type_
			} else {
				element
			};
			if matches!(
				skip_parenthesized_types(element).kind(),
				Kind::OptionalType | Kind::RestType
			) {
				return self.fail_expr(self.top_kind, "optional and rest tuple elements are not translated");
			}
			elements.push(self.type_expr(element)?);
		}
		let member = match elements.len() {
			0 => tf_types(&mut self.s.ast, "never"),
			1 => elements[0],
			_ => tf_types_call(&mut self.s.ast, "unionof", elements),
		};
		let t = self.s.ck().get_type_from_type_node(node);
		let text = self.s.ck().type_to_string(t);
		self.lose(
			FidelityLossKind::Tuple,
			text,
			"{ any }",
			"Luau has no tuple types; positions are lost",
		);
		Some(self.array_of(member, readonly))
	}

	/// `typeLiteral`: `{ a: A, readonly b?: B, [k: string]: V }` through `types.newtable`.
	fn type_literal(&mut self, node: P<Node>) -> Option<NodeId> {
		struct Property {
			name: String,
			value: Factory,
			readonly: bool,
		}
		let mut properties: Vec<Property> = Vec::new();
		let mut indexer: Option<NodeId> = None;
		let mut any_readonly = false;
		for &member in node.members() {
			match member.kind() {
				Kind::PropertySignature => {
					let Some(name) = member.name().and_then(tsast::try_get_text_of_property_name) else {
						return self.fail_expr(self.top_kind, "a computed property name has no translation");
					};
					if name.starts_with("__@") {
						self.lose(
							FidelityLossKind::SymbolKey,
							name,
							"",
							"symbol-keyed members are skipped",
						);
						continue;
					}
					let mut value = self.member_type(member.type_node())?;
					if member
						.postfix_token()
						.is_some_and(|token| token.kind() == Kind::QuestionToken)
					{
						value = tf_types_call(&mut self.s.ast, "optional", [value]);
					}
					let readonly = tsast::has_syntactic_modifier(member, ModifierFlags::Readonly);
					any_readonly = any_readonly || readonly;
					let value = self.pin(value, "member");
					properties.push(Property { name, value, readonly });
				}
				Kind::IndexSignature => {
					if indexer.is_some() {
						return self.fail_expr(self.top_kind, "Luau allows one indexer");
					}
					let parameters = member.parameters();
					if parameters.len() != 1 {
						return self.fail_expr(self.top_kind, "an index signature needs one parameter");
					}
					let key = self.member_type(parameters[0].type_node())?;
					let value = self.member_type(member.type_node())?;
					let index = tf_field(&mut self.s.ast, "index", key);
					let fields = if tsast::has_syntactic_modifier(member, ModifierFlags::Readonly) {
						let read = tf_field(&mut self.s.ast, "readresult", value);
						vec![index, read]
					} else {
						let pinned = self.pin(value, "indexed");
						let ast = &mut self.s.ast;
						let read_value = pinned.make(ast);
						let read = tf_field(ast, "readresult", read_value);
						let write_value = pinned.make(ast);
						let write = tf_field(ast, "writeresult", write_value);
						vec![index, read, write]
					};
					indexer = Some(tf_map(&mut self.s.ast, fields));
				}
				kind => {
					let reason = format!("a {} member has no translation", tsast::kind_string(kind));
					return self.fail_expr(self.top_kind, &reason);
				}
			}
		}
		let ast = &mut self.s.ast;
		let mut arguments = Vec::new();
		if !properties.is_empty() {
			let mut fields = Vec::with_capacity(properties.len());
			for property in &properties {
				let name = ast.str(property.name.as_str());
				let key = tf_types_call(ast, "singleton", [name]);
				let value = if !any_readonly {
					property.value.make(ast)
				} else if property.readonly {
					let value = property.value.make(ast);
					let read = tf_field(ast, "read", value);
					tf_map(ast, [read])
				} else {
					let value = property.value.make(ast);
					let read = tf_field(ast, "read", value);
					let value = property.value.make(ast);
					let write = tf_field(ast, "write", value);
					tf_map(ast, [read, write])
				};
				fields.push(ast.map_field(key, value));
			}
			arguments.push(tf_map(ast, fields));
		}
		if let Some(indexer) = indexer {
			if arguments.is_empty() {
				arguments.push(ast.nil());
			}
			arguments.push(indexer);
		}
		Some(tf_types_call(ast, "newtable", arguments))
	}

	/// `memberType`: a member's written type, `any` when it has none.
	fn member_type(&mut self, node: Option<P<Node>>) -> Option<NodeId> {
		match node {
			None => Some(tf_types(&mut self.s.ast, "any")),
			Some(node) => self.type_expr(node),
		}
	}

	/// `functionType`: `(a: A, b?: B, ...rest: R[]) => T` through `types.newfunction`.
	fn function_type(&mut self, node: P<Node>) -> Option<NodeId> {
		if !node.type_parameters().is_empty() {
			return self.fail_expr(self.top_kind, "generic function types are not translated");
		}
		let mut head = Vec::new();
		let mut tail = None;
		for &parameter_node in node.parameters() {
			let parameter = parameter_node.as_parameter_declaration();
			if parameter.dot_dot_dot_token().is_some() {
				let rest = parameter.type_().map(skip_parenthesized_types);
				match rest {
					None => tail = Some(tf_types(&mut self.s.ast, "any")),
					Some(rest) if rest.kind() == Kind::AnyKeyword => tail = Some(tf_types(&mut self.s.ast, "any")),
					Some(rest) if rest.kind() == Kind::ArrayType => {
						tail = Some(self.type_expr(rest.as_array_type_node().element_type)?);
					}
					Some(_) => {
						return self.fail_expr(
							self.top_kind,
							"a rest parameter that is not an array has no Luau spelling",
						);
					}
				}
				continue;
			}
			let mut value = self.member_type(parameter.type_())?;
			if parameter.question_token().is_some() {
				value = tf_types_call(&mut self.s.ast, "optional", [value]);
			}
			head.push(value);
		}
		let mut parameter_fields = Vec::new();
		if !head.is_empty() {
			let array = tf_array(&mut self.s.ast, head);
			parameter_fields.push(tf_field(&mut self.s.ast, "head", array));
		}
		if let Some(tail) = tail {
			parameter_fields.push(tf_field(&mut self.s.ast, "tail", tail));
		}
		let returns = match node.type_node().filter(|&return_node| !is_nil_type_node(return_node)) {
			Some(return_node) => {
				let value = self.type_expr(return_node)?;
				let array = tf_array(&mut self.s.ast, [value]);
				let head = tf_field(&mut self.s.ast, "head", array);
				tf_map(&mut self.s.ast, [head])
			}
			None => tf_map(&mut self.s.ast, []),
		};
		let parameters = tf_map(&mut self.s.ast, parameter_fields);
		Some(tf_types_call(&mut self.s.ast, "newfunction", [parameters, returns]))
	}

	// -------------------------------------------------------------------------------------
	// Conditional types
	// -------------------------------------------------------------------------------------

	/// `conditional`: `T extends U ? X : Y`, distributed over the components of a naked type
	/// parameter.
	fn conditional(&mut self, node: P<Node>, sink: &Sink, tail: bool) -> bool {
		let conditional = node.as_conditional_type_node();
		let Some(check) = self.env_symbol(conditional.check_type) else {
			return self.branches(node, sink, tail);
		};
		if !self.distributable.contains(&check) {
			return self.branches(node, sink, tail);
		}
		let parts = self.fresh("parts");
		let item = self.fresh("item");
		let empty = tf_map(&mut self.s.ast, []);
		let local = tf_local(&mut self.s.ast, &parts, empty);
		self.emit(local);
		let source_factory = self.env.get(&check).cloned();
		let source = match source_factory {
			Some(factory) => self.make(&factory),
			None => self.s.ast.none(),
		};
		let insert = Sink::Insert(parts.clone());
		let (body, ok) =
			self.nested(|tr| tr.bind(check, Factory::Id(item.clone()), |tr| tr.branches(node, &insert, false)));
		if !ok {
			return false;
		}
		let helper = self.helper(HELPER_DISTRIBUTE);
		let distribute = tf_call_id(&mut self.s.ast, helper, [source]);
		let for_in = tf_for_in(&mut self.s.ast, &["_", &item], distribute, body);
		self.emit(for_in);
		let helper = self.helper(HELPER_JOIN);
		let parts_id = self.s.ast.id(parts);
		let join = tf_call_id(&mut self.s.ast, helper, [parts_id]);
		let statement = sink.apply(&mut self.s.ast, join);
		self.emit(statement);
		true
	}

	/// `branches`: the `if check:issubtypeof(extends) then X else Y end` of a conditional,
	/// binding an `infer` variable for the true branch when the extends type is `(infer U)[]` or
	/// `(...args: any) => infer R`.
	fn branches(&mut self, node: P<Node>, sink: &Sink, tail: bool) -> bool {
		let conditional = node.as_conditional_type_node();
		let Some(check_expression) = self.type_expr(conditional.check_type) else {
			return false;
		};
		let check = self.pin(check_expression, "check");
		let extends = skip_parenthesized_types(conditional.extends_type);
		let mut inferred: Option<P<Symbol>> = None;
		let mut inferred_value: Option<Factory> = None;
		let mut inferred_prelude: Option<NodeId> = None;
		let array_infer = array_infer_symbol(extends);
		let function_infer = function_infer_symbol(extends);
		let condition = if let Some(array_infer) = array_infer {
			inferred = Some(array_infer);
			let indexer = self.fresh("indexer");
			let ast = &mut self.s.ast;
			let receiver = check.make(ast);
			let is_table = tf_is(ast, receiver, "table");
			let receiver = check.make(ast);
			let method = tf_method(ast, receiver, "indexer", []);
			let nil = ast.nil();
			let value = tf_if_expr(ast, is_table, method, nil);
			let local = tf_local(ast, &indexer, value);
			self.emit(local);
			let ast = &mut self.s.ast;
			let indexer_id = ast.id(indexer.as_str());
			let index_receiver = ast.id(indexer.as_str());
			let index = tf_prop(ast, index_receiver, "index");
			let number = tf_types(ast, "number");
			let equal = tf_eq(ast, index, number);
			inferred_value = Some(Factory::Prop(indexer, "readresult"));
			tf_and(ast, indexer_id, equal)
		} else if let Some(function_infer) = function_infer {
			inferred = Some(function_infer);
			let receiver = check.make(&mut self.s.ast);
			let condition = tf_is(&mut self.s.ast, receiver, "function");
			let name = self.fresh(function_infer.name());
			let helper = self.helper(HELPER_RETURN_OF);
			let receiver = check.make(&mut self.s.ast);
			let call = tf_call_id(&mut self.s.ast, helper, [receiver]);
			inferred_prelude = Some(tf_local(&mut self.s.ast, &name, call));
			inferred_value = Some(Factory::Id(name));
			condition
		} else if contains_infer_type(extends) {
			return self.fail(FidelityLossKind::InferType, INFER_REASON);
		} else {
			let Some(extends_expression) = self.type_expr(conditional.extends_type) else {
				return false;
			};
			let receiver = check.make(&mut self.s.ast);
			tf_method(&mut self.s.ast, receiver, "issubtypeof", [extends_expression])
		};
		let true_type = conditional.true_type;
		let false_type = conditional.false_type;
		let (true_block, ok) = self.nested(|tr| {
			let Some(inferred) = inferred else {
				return tr.value(true_type, sink, tail);
			};
			if let Some(prelude) = inferred_prelude {
				tr.emit(prelude);
			}
			tr.distributable.insert(inferred);
			let factory = inferred_value.unwrap_or_else(|| Factory::Id(String::new()));
			let ok = tr.bind(inferred, factory, |tr| tr.value(true_type, sink, tail));
			tr.distributable.remove(&inferred);
			ok
		});
		if !ok {
			return false;
		}
		let (false_block, ok) = self.nested(|tr| tr.value(false_type, sink, tail));
		if !ok {
			return false;
		}
		let statement = tf_if(&mut self.s.ast, condition, true_block, Some(false_block.into()));
		self.emit(statement);
		true
	}

	// -------------------------------------------------------------------------------------
	// Mapped types
	// -------------------------------------------------------------------------------------

	/// `mapped`: `{ [K in Keys as Name]?: Value }`: over the properties of a table when the keys
	/// are `keyof T` (or a parameter bound by `keyof T`), else over the components of the key
	/// union.
	fn mapped(&mut self, node: P<Node>, sink: &Sink, tail: bool) -> bool {
		let mapped = node.as_mapped_type_node();
		if !node.members().is_empty() {
			return self.fail(
				FidelityLossKind::MappedType,
				"a mapped type with extra members is not translated",
			);
		}
		let Some(value_type) = mapped.type_ else {
			return self.fail(
				FidelityLossKind::MappedType,
				"a mapped type without a key constraint or a value type is not translated",
			);
		};
		let type_parameter = mapped.type_parameter;
		let key_symbol = type_parameter.symbol();
		let constraint = type_parameter.as_type_parameter_declaration().constraint;
		let (Some(key_symbol), Some(constraint)) = (key_symbol, constraint) else {
			return self.fail(
				FidelityLossKind::MappedType,
				"a mapped type without a key constraint is not translated",
			);
		};
		let mapping = Mapping {
			name_type: mapped.name_type,
			value_type,
			key_symbol,
			readonly: mapped_modifier_of(mapped.readonly_token),
			optional: mapped_modifier_of(mapped.question_token),
		};
		if let Some((source, filter)) = self.homomorphic_source(constraint) {
			return self.mapped_over_properties(&mapping, source, filter, sink, tail);
		}
		self.mapped_over_keys(&mapping, constraint, sink)
	}

	/// `homomorphicSource`: the object type a mapped type iterates when its keys are
	/// `keyof Source`, with the type parameter that filters the keys when the constraint is a
	/// parameter itself bound by `keyof Source`.
	fn homomorphic_source(&mut self, constraint: P<Node>) -> Option<(P<Node>, Option<P<Symbol>>)> {
		let constraint = skip_parenthesized_types(constraint);
		if constraint.kind() == Kind::TypeOperator && constraint.as_type_operator_node().operator == Kind::KeyOfKeyword
		{
			return Some((constraint.as_type_operator_node().type_, None));
		}
		let symbol = self.env_symbol(constraint)?;
		if !self.distributable.contains(&symbol) {
			return None;
		}
		for &declaration in symbol.declarations() {
			if !tsast::is_type_parameter_declaration(declaration) {
				continue;
			}
			let bound = declaration.as_type_parameter_declaration().constraint?;
			let bound = skip_parenthesized_types(bound);
			if bound.kind() == Kind::TypeOperator && bound.as_type_operator_node().operator == Kind::KeyOfKeyword {
				return Some((bound.as_type_operator_node().type_, Some(symbol)));
			}
		}
		None
	}

	/// `mappedOverProperties`: the homomorphic shape: a non-table source is itself, a table is
	/// copied and every property rewritten; with an `as` clause or a key filter the table is
	/// rebuilt from the kept properties instead.
	fn mapped_over_properties(
		&mut self,
		mapping: &Mapping,
		source_node: P<Node>,
		filter: Option<P<Symbol>>,
		sink: &Sink,
		tail: bool,
	) -> bool {
		let Some(source_expression) = self.type_expr(source_node) else {
			return false;
		};
		let source_symbol = self.env_symbol(source_node);
		let source = self.pin(source_expression, "source");
		let rebuild = mapping.name_type.is_some() || filter.is_some();
		let result = self.fresh("result");
		let key = self.fresh("key");
		let prop = self.fresh("prop");
		let key_symbol = mapping.key_symbol;

		let (loop_body, ok) = self.nested(|tr| {
			tr.bind(key_symbol, Factory::Id(key.clone()), |tr| {
				tr.indexed.insert(
					key_symbol,
					IndexedBinding {
						object: source_symbol,
						value: None,
					},
				);
				let ok = tr.mapped_property_loop(mapping, filter, &key, &result, &prop, rebuild);
				tr.indexed.remove(&key_symbol);
				ok
			})
		});
		if !ok {
			return false;
		}

		let ast = &mut self.s.ast;
		let mut body = Vec::new();
		let receiver = source.make(ast);
		if rebuild {
			let is_table = tf_is(ast, receiver, "table");
			let condition = tf_not(ast, is_table);
			let error = tf_error(ast, &format!("{}: expected a table type", self.name));
			let then = tf_stmts(ast, [error]);
			let guard = tf_if(ast, condition, then, None);
			ast.list_push(self.block, guard);
			let newtable = tf_types_call(ast, "newtable", []);
			body.push(tf_local(ast, &result, newtable));
		} else {
			let copy = tf_types_call(ast, "copy", [receiver]);
			body.push(tf_local(ast, &result, copy));
		}
		let receiver = source.make(ast);
		let properties = tf_method(ast, receiver, "properties", []);
		body.push(tf_for_in(ast, &[&key, &prop], properties, loop_body));
		let result_id = ast.id(result.as_str());
		body.push(sink.apply(ast, result_id));
		if rebuild {
			for statement in body {
				ast.list_push(self.block, statement);
			}
		} else if tail {
			let receiver = source.make(ast);
			let is_table = tf_is(ast, receiver, "table");
			let condition = tf_not(ast, is_table);
			let returned = source.make(ast);
			let ret = tf_return(ast, returned);
			let then = tf_stmts(ast, [ret]);
			let guard = tf_if(ast, condition, then, None);
			ast.list_push(self.block, guard);
			for statement in body {
				ast.list_push(self.block, statement);
			}
		} else {
			let receiver = source.make(ast);
			let is_table = tf_is(ast, receiver, "table");
			let condition = tf_not(ast, is_table);
			let returned = source.make(ast);
			let sunk_source = sink.apply(ast, returned);
			let then = tf_stmts(ast, [sunk_source]);
			let otherwise = tf_stmts(ast, body);
			let statement = tf_if(ast, condition, then, Some(otherwise.into()));
			ast.list_push(self.block, statement);
		}
		true
	}

	/// The body of `mappedOverProperties`'s loop, with the key bound.
	fn mapped_property_loop(
		&mut self,
		mapping: &Mapping,
		filter: Option<P<Symbol>>,
		key: &str,
		result: &str,
		prop: &str,
		rebuild: bool,
	) -> bool {
		let target = if let Some(name_type) = mapping.name_type {
			let Some(name_expression) = self.type_expr(name_type) else {
				if let Some(failure) = &mut self.failure {
					failure.kind = FidelityLossKind::KeyRemap;
				}
				return false;
			};
			let name = self.fresh("name");
			let local = tf_local(&mut self.s.ast, &name, name_expression);
			self.emit(local);
			Factory::Id(name)
		} else {
			Factory::Id(key.to_owned())
		};
		let (mut rewrite, ok) = self.nested(|tr| tr.property_rewrite(mapping, result, prop, &target, !rebuild));
		if !ok {
			return false;
		}
		if mapping.name_type.is_some() {
			let Factory::Id(named) = &target else {
				return false;
			};
			let ast = &mut self.s.ast;
			let named_id = ast.id(named.as_str());
			let is_singleton = tf_is(ast, named_id, "singleton");
			let named_id = ast.id(named.as_str());
			let is_never = tf_is(ast, named_id, "never");
			let not_never = tf_not(ast, is_never);
			let error = tf_error(ast, &format!("{}: a mapped key must be a string literal", self.name));
			let error_then = tf_stmts(ast, [error]);
			let never_check = tf_if(ast, not_never, error_then, None);
			let statement = tf_if(ast, is_singleton, rewrite, Some(never_check.into()));
			rewrite = tf_stmts(ast, [statement]);
		}
		if let Some(filter) = filter {
			let filter_factory = self.env.get(&filter).cloned();
			let ast = &mut self.s.ast;
			let key_id = ast.id(key);
			let filter_value = match filter_factory {
				Some(factory) => factory.make(ast),
				None => ast.none(),
			};
			let condition = tf_method(ast, key_id, "issubtypeof", [filter_value]);
			let statement = tf_if(ast, condition, rewrite, None);
			rewrite = tf_stmts(ast, [statement]);
		}
		self.s.ast.list_push_list(self.block, rewrite);
		true
	}

	/// `propertyRewrite`: the per-property statements of a homomorphic mapped type for the
	/// property record `prop` under key `target`.
	fn property_rewrite(
		&mut self,
		mapping: &Mapping,
		result: &str,
		prop: &str,
		target: &Factory,
		copied: bool,
	) -> bool {
		if mapping.readonly == MappedModifier::None {
			let Some(read) = self.slot(mapping, Factory::Prop(prop.to_owned(), "read"), result, target, |()| {
				&[("setreadproperty", false)]
			}) else {
				return false;
			};
			let Some(write) = self.slot(mapping, Factory::Prop(prop.to_owned(), "write"), result, target, |()| {
				&[("setwriteproperty", false)]
			}) else {
				return false;
			};
			let ast = &mut self.s.ast;
			let receiver = ast.id(prop);
			let read_condition = tf_prop(ast, receiver, "read");
			let read_check = tf_if(ast, read_condition, read, None);
			ast.list_push(self.block, read_check);
			let receiver = ast.id(prop);
			let write_condition = tf_prop(ast, receiver, "write");
			let write_check = tf_if(ast, write_condition, write, None);
			ast.list_push(self.block, write_check);
			return true;
		}
		let current = self.fresh("current");
		let ast = &mut self.s.ast;
		let receiver = ast.id(prop);
		let read = tf_prop(ast, receiver, "read");
		let receiver = ast.id(prop);
		let write = tf_prop(ast, receiver, "write");
		let either = tf_or(ast, read, write);
		let local = tf_local(ast, &current, either);
		self.emit(local);
		let readonly = mapping.readonly;
		let Some(block) = self.slot(mapping, Factory::Id(current.clone()), result, target, move |()| {
			if readonly == MappedModifier::Remove {
				&[("setproperty", false)]
			} else if copied {
				&[("setreadproperty", false), ("setwriteproperty", true)]
			} else {
				&[("setreadproperty", false)]
			}
		}) else {
			return false;
		};
		let current_id = self.s.ast.id(current);
		let statement = tf_if(&mut self.s.ast, current_id, block, None);
		self.emit(statement);
		true
	}

	/// The `slot` closure of `propertyRewrite`: the value type spelled once for one slot, with
	/// `Source[K]` bound to it, then handed to the setters `consume` names (`true` passes `nil`
	/// instead of the value).
	fn slot(
		&mut self,
		mapping: &Mapping,
		current: Factory,
		result: &str,
		target: &Factory,
		consume: impl FnOnce(()) -> &'static [(&'static str, bool)],
	) -> Option<Statements> {
		let key_symbol = mapping.key_symbol;
		let (block, ok) = self.nested(|tr| {
			let mut binding = tr.indexed.get(&key_symbol).cloned().unwrap_or_default();
			binding.value = Some(current);
			tr.indexed.insert(key_symbol, binding.clone());
			let value = tr.type_expr(mapping.value_type);
			binding.value = None;
			tr.indexed.insert(key_symbol, binding);
			let Some(value) = value else {
				return false;
			};
			let value = tr.wrap_optional(value, mapping.optional);
			for &(setter, nil) in consume(()) {
				let ast = &mut tr.s.ast;
				let argument = if nil { ast.nil() } else { value };
				let receiver = ast.id(result);
				let target_value = target.make(ast);
				let call = tf_method(ast, receiver, setter, [target_value, argument]);
				let statement = tf_call_statement(ast, call);
				tr.emit(statement);
			}
			true
		});
		ok.then_some(block)
	}

	/// `wrapOptional`: a `?` modifier applied to a property type.
	fn wrap_optional(&mut self, value: NodeId, optional: MappedModifier) -> NodeId {
		match optional {
			MappedModifier::Add => tf_types_call(&mut self.s.ast, "optional", [value]),
			MappedModifier::Remove => {
				let helper = self.helper(HELPER_WITHOUT_NIL);
				tf_call_id(&mut self.s.ast, helper, [value])
			}
			MappedModifier::None => value,
		}
	}

	/// `mappedOverKeys`: the non-homomorphic shape: a new table with one property per singleton
	/// in the key union.
	fn mapped_over_keys(&mut self, mapping: &Mapping, constraint: P<Node>, sink: &Sink) -> bool {
		let Some(keys) = self.type_expr(constraint) else {
			return false;
		};
		let result = self.fresh("result");
		let key = self.fresh("key");
		let key_symbol = mapping.key_symbol;
		let (loop_body, ok) = self.nested(|tr| {
			tr.bind(key_symbol, Factory::Id(key.clone()), |tr| {
				tr.indexed.insert(key_symbol, IndexedBinding::default());
				let ok = tr.mapped_key_loop(mapping, &key, &result);
				tr.indexed.remove(&key_symbol);
				ok
			})
		});
		if !ok {
			return false;
		}
		let newtable = tf_types_call(&mut self.s.ast, "newtable", []);
		let local = tf_local(&mut self.s.ast, &result, newtable);
		self.emit(local);
		let helper = self.helper(HELPER_DISTRIBUTE);
		let distribute = tf_call_id(&mut self.s.ast, helper, [keys]);
		let for_in = tf_for_in(&mut self.s.ast, &["_", &key], distribute, loop_body);
		self.emit(for_in);
		let result_id = self.s.ast.id(result);
		let statement = sink.apply(&mut self.s.ast, result_id);
		self.emit(statement);
		true
	}

	/// The body of `mappedOverKeys`'s loop, with the key bound.
	fn mapped_key_loop(&mut self, mapping: &Mapping, key: &str, result: &str) -> bool {
		let mut target = key.to_owned();
		if let Some(name_type) = mapping.name_type {
			let Some(name_expression) = self.type_expr(name_type) else {
				if let Some(failure) = &mut self.failure {
					failure.kind = FidelityLossKind::KeyRemap;
				}
				return false;
			};
			target = self.fresh("name");
			let local = tf_local(&mut self.s.ast, &target, name_expression);
			self.emit(local);
		}
		let Some(value_expression) = self.type_expr(mapping.value_type) else {
			return false;
		};
		let value = self.fresh("value");
		let wrapped = self.wrap_optional(value_expression, mapping.optional);
		let local = tf_local(&mut self.s.ast, &value, wrapped);
		self.emit(local);
		let (property, indexer) = if mapping.readonly == MappedModifier::Add {
			("setreadproperty", "setreadindexer")
		} else {
			("setproperty", "setindexer")
		};
		let ast = &mut self.s.ast;
		let target_id = ast.id(target.as_str());
		let is_singleton = tf_is(ast, target_id, "singleton");
		let receiver = ast.id(result);
		let target_id = ast.id(target.as_str());
		let value_id = ast.id(value.as_str());
		let set_property = tf_method(ast, receiver, property, [target_id, value_id]);
		let set_property = tf_call_statement(ast, set_property);
		let singleton_then = tf_stmts(ast, [set_property]);
		let target_id = ast.id(target.as_str());
		let is_string = tf_is(ast, target_id, "string");
		let target_id = ast.id(target.as_str());
		let is_number = tf_is(ast, target_id, "number");
		let either = tf_or(ast, is_string, is_number);
		let receiver = ast.id(result);
		let target_id = ast.id(target.as_str());
		let value_id = ast.id(value.as_str());
		let set_indexer = tf_method(ast, receiver, indexer, [target_id, value_id]);
		let set_indexer = tf_call_statement(ast, set_indexer);
		let indexer_then = tf_stmts(ast, [set_indexer]);
		let target_id = ast.id(target.as_str());
		let is_never = tf_is(ast, target_id, "never");
		let not_never = tf_not(ast, is_never);
		let error = tf_error(
			ast,
			&format!("{}: a mapped key must be a string literal, string or number", self.name),
		);
		let error_then = tf_stmts(ast, [error]);
		let never_check = tf_if(ast, not_never, error_then, None);
		let indexer_check = tf_if(ast, either, indexer_then, Some(never_check.into()));
		let statement = tf_if(ast, is_singleton, singleton_then, Some(indexer_check.into()));
		ast.list_push(self.block, statement);
		true
	}
}

/// The parts of a mapped type node the translation reads.
struct Mapping {
	name_type: Option<P<Node>>,
	value_type: P<Node>,
	key_symbol: P<Symbol>,
	readonly: MappedModifier,
	optional: MappedModifier,
}

/// `mappedModifierOf`: a `+`, `-`, `readonly` or `?` token.
fn mapped_modifier_of(token: Option<P<Node>>) -> MappedModifier {
	match token {
		None => MappedModifier::None,
		Some(token) if token.kind() == Kind::MinusToken => MappedModifier::Remove,
		Some(_) => MappedModifier::Add,
	}
}

/// `arrayInferSymbol`: the infer variable of `(infer U)[]` or `readonly (infer U)[]`.
fn array_infer_symbol(extends: P<Node>) -> Option<P<Symbol>> {
	let mut extends = extends;
	if extends.kind() == Kind::TypeOperator && extends.as_type_operator_node().operator == Kind::ReadonlyKeyword {
		extends = skip_parenthesized_types(extends.as_type_operator_node().type_);
	}
	if extends.kind() != Kind::ArrayType {
		return None;
	}
	let element = skip_parenthesized_types(extends.as_array_type_node().element_type);
	if element.kind() != Kind::InferType {
		return None;
	}
	element.as_infer_type_node().type_parameter.symbol()
}

/// `functionInferSymbol`: the infer variable of `(...args: any) => infer R`.
fn function_infer_symbol(extends: P<Node>) -> Option<P<Symbol>> {
	if extends.kind() != Kind::FunctionType {
		return None;
	}
	let return_type = extends.type_node()?;
	let return_type = skip_parenthesized_types(return_type);
	if return_type.kind() != Kind::InferType {
		return None;
	}
	let parameters = extends.parameters();
	if parameters.len() > 1 || !extends.type_parameters().is_empty() {
		return None;
	}
	if let Some(&parameter) = parameters.first() {
		let declaration = parameter.as_parameter_declaration();
		if declaration.dot_dot_dot_token().is_none() || declaration.type_().is_some_and(contains_infer_type) {
			return None;
		}
	}
	return_type.as_infer_type_node().type_parameter.symbol()
}

/// `containsInferType`: an `infer` anywhere under `node`.
fn contains_infer_type(node: P<Node>) -> bool {
	fn visit(child: P<Node>) -> bool {
		if child.kind() == Kind::InferType {
			return true;
		}
		child.for_each_child(&mut visit)
	}
	if node.kind() == Kind::InferType {
		return true;
	}
	node.for_each_child(&mut visit)
}

// -----------------------------------------------------------------------------------------
// AST helpers
// -----------------------------------------------------------------------------------------

/// `skipParenthesizedTypes`
pub fn skip_parenthesized_types(mut node: P<Node>) -> P<Node> {
	while node.kind() == Kind::ParenthesizedType {
		node = node.as_parenthesized_type_node().type_;
	}
	node
}

/// `isNilTypeNode`: `undefined`, `void` and `null`.
fn is_nil_type_node(node: P<Node>) -> bool {
	let node = skip_parenthesized_types(node);
	match node.kind() {
		Kind::UndefinedKeyword | Kind::VoidKeyword | Kind::NullKeyword => true,
		Kind::LiteralType => node.as_literal_type_node().literal.kind() == Kind::NullKeyword,
		_ => false,
	}
}

/// `entityNameText`: `A.B.C`.
fn entity_name_text(name: P<Node>) -> String {
	if name.kind() == Kind::QualifiedName {
		let qualified = name.as_qualified_name();
		return format!("{}.{}", entity_name_text(qualified.left), qualified.right.text());
	}
	name.text().to_owned()
}

/// `isTypeLevelNodeKind`: the type nodes that make a generic alias body a type-level
/// computation.
const fn is_type_level_node_kind(kind: Kind) -> bool {
	matches!(
		kind,
		Kind::MappedType | Kind::ConditionalType | Kind::TemplateLiteralType
	)
}

/// `typeLevelLossKind`
const fn type_level_loss_kind(kind: Kind) -> FidelityLossKind {
	match kind {
		Kind::MappedType => FidelityLossKind::MappedType,
		Kind::ConditionalType => FidelityLossKind::ConditionalType,
		Kind::TemplateLiteralType => FidelityLossKind::TemplateLiteral,
		_ => FidelityLossKind::Unspellable,
	}
}

/// `firstTypeLevelNode`: the outermost mapped, conditional or template literal node under
/// `body`, `body` itself included.
fn first_type_level_node(body: Option<P<Node>>) -> Option<P<Node>> {
	fn visit(child: P<Node>, found: &mut Option<P<Node>>) -> bool {
		if is_type_level_node_kind(child.kind()) {
			*found = Some(child);
			return true;
		}
		child.for_each_child(&mut |grandchild| visit(grandchild, found))
	}
	let body = skip_parenthesized_types(body?);
	if is_type_level_node_kind(body.kind()) {
		return Some(body);
	}
	let mut found = None;
	body.for_each_child(&mut |child| visit(child, &mut found));
	found
}

/// `topLevelLossKind`: the loss kind of the body's outermost type-level node.
fn top_level_loss_kind(body: Option<P<Node>>) -> FidelityLossKind {
	first_type_level_node(body).map_or(FidelityLossKind::Unspellable, |node| type_level_loss_kind(node.kind()))
}

// -----------------------------------------------------------------------------------------
// Registration: type function instead of alias
// -----------------------------------------------------------------------------------------

/// `isTypeLevelAlias`: a generic type alias whose body contains a mapped, conditional or
/// template literal type.
pub fn is_type_level_alias(symbol: P<Symbol>, declaration: Option<P<Node>>) -> bool {
	let Some(declaration) = declaration else {
		return false;
	};
	if !symbol.flags().intersects(SymbolFlags::TypeAlias) || !tsast::is_type_alias_declaration(declaration) {
		return false;
	}
	if declaration.type_parameters().is_empty() {
		return false;
	}
	first_type_level_node(declaration.type_node()).is_some()
}

/// `preludeNameFor`: the TS helper name when `symbol` is one of the lib's built-in helper types
/// with a prelude twin, else `""`.
pub fn prelude_name_for(state: &mut State<'_>, symbol: P<Symbol>) -> String {
	if !symbol.flags().intersects(SymbolFlags::TypeAlias) || !LUAU_TYPES_PRELUDE_NAMES.contains(&symbol.name()) {
		return String::new();
	}
	let Some(global) = state.ck().get_global_symbol(symbol.name(), SymbolFlags::Type, None) else {
		return String::new();
	};
	let merged_global = state.ck().get_merged_symbol(global);
	if merged_global != state.ck().get_merged_symbol(symbol) {
		return String::new();
	}
	symbol.name().to_owned()
}

impl Builder<'_, '_> {
	/// `isTypeLevelSymbol`: a symbol whose alias is a type function (or its `any` fallback).
	pub fn is_type_level_symbol(&mut self, symbol: P<Symbol>, declaration: Option<P<Node>>) -> bool {
		!prelude_name_for(self.s, symbol).is_empty() || is_type_level_alias(symbol, declaration)
	}

	/// `typeLevelAlias`: registers the entry of a type-level symbol: the prelude twin of a
	/// built-in helper, the translated type function, or the `type Name<...> = any` fallback.
	/// `None` for every other symbol; `Some(None)` declines the alias.
	#[expect(
		clippy::option_option,
		reason = "Go's (entry, handled) pair, where the entry may be nil"
	)]
	pub fn type_level_alias(&mut self, symbol: P<Symbol>, declaration: P<Node>) -> Option<Option<EntryId>> {
		let prelude = prelude_name_for(self.s, symbol);
		if !prelude.is_empty() {
			require_prelude(self.s, &prelude);
			let entry = self.b().preludes.get(&prelude).copied();
			self.b().aliases.insert(SymKey::Real(symbol), entry);
			return Some(entry);
		}
		if !is_type_level_alias(symbol, Some(declaration)) {
			return None;
		}
		let base = self.alias_name(symbol);
		let name = self.unique_name(&base);
		let mut entry = TypeAliasEntry::new(SymKey::Real(symbol), name.clone());
		entry.generics = self.generics(declaration);
		entry.kind = NativeTypeKind::Unspellable;
		entry.exported = is_luau_exported_type(self.s, symbol);
		entry.placement = alias_placement_for(self.s, declaration);
		self.declared_params(&mut entry, declaration);
		let declared = self.s.ck().get_declared_type_of_symbol(symbol);
		if declared.flags().intersects(TypeFlags::Object) && declared.object_flags().intersects(ObjectFlags::Mapped) {
			entry.kind = NativeTypeKind::Table;
		}
		let complete = entry.generic_indexes.len() == entry.params.len();
		let generics = entry.generics.clone();
		let entry = self.b().push_entry(entry);
		self.b().aliases.insert(SymKey::Real(symbol), Some(entry));
		self.b().alias_order.push(entry);

		let losses_before = self.b().losses.len();
		if complete {
			if let Some(type_function) = luau_type_function_for(self.s, symbol, declaration) {
				if let Some(payload) = self.s.ast.get_mut::<TypeFunctionDeclaration>(type_function) {
					payload.name.clone_from(&name);
				}
				self.b().entry_mut(entry).type_function = Some(type_function);
			}
		} else {
			let kind = top_level_loss_kind(declaration.type_node());
			let declared = self.s.ck().get_declared_type_of_symbol(symbol);
			let text = self.type_text(Some(declared));
			let any = any_type(self.ast());
			self.record(
				kind,
				text,
				Some(any),
				TypeContext::at(super::luautypes::TypePosition::default(), Some(declaration)),
				"a type parameter Luau cannot name has no type-function parameter",
			);
		}
		if self.b().entry(entry).type_function.is_none() {
			let any = any_type(self.ast());
			let statement = self.ast().type_alias(name, generics.clone(), Some(any));
			let adopted = self
				.s
				.ast
				.get::<rotor_luau::TypeAlias>(statement)
				.map_or(generics, |alias| alias.generics.clone());
			let data = self.b().entry_mut(entry);
			data.statement = Some(statement);
			data.generics = adopted;
		}
		let attributed: Vec<_> = self.b().losses[losses_before..]
			.iter()
			.filter(|loss| loss.node == Some(declaration))
			.cloned()
			.collect();
		self.b().entry_mut(entry).losses.extend(attributed);
		Some(Some(entry))
	}

	/// `typeLevelReference`: a use of a type-level alias. A use whose arguments hold no free
	/// type variable keeps the checker's resolved spelling unless the alias is declared in this
	/// file and its translation was complete. A generic use is always `Name<Args>`.
	pub fn type_level_reference(
		&mut self,
		t: P<Type>,
		symbol: P<Symbol>,
		declaration: Option<P<Node>>,
		ctx: TypeContext,
	) -> Option<BuiltType> {
		let arguments: Vec<P<Type>> = t
			.alias()
			.filter(|alias| alias.symbol() == Some(symbol))
			.map_or_else(Vec::new, |alias| alias.type_arguments().to_vec());
		let closed = !arguments
			.iter()
			.any(|&argument| self.type_has_free_variables(Some(argument)));
		if closed
			&& declaration
				.is_none_or(|declaration| tsast::get_source_file_of_node(declaration) != Some(self.s.source_file))
		{
			return None;
		}
		let entry = self.alias_for(Some(SymKey::Real(symbol)))?;
		if closed && self.b().entry(entry).type_function.is_none() {
			return None;
		}
		let declared = super::luautypes_builder::declared_arguments(self.b().entry(entry), &arguments);
		let type_arguments = self.build_type_args(&declared, ctx.nested());
		let name = self.b().entry(entry).name.clone();
		let reference = self.ast().type_reference(name, type_arguments);
		self.set_origin(reference, SymKey::Real(symbol));
		Some(self.finish(reference, false))
	}

	/// `typeLevelNodeReference`: the call-site spelling of a written `Name<Args>` whose resolved
	/// type no longer names the alias.
	pub fn type_level_node_reference(&mut self, node: P<Node>, ctx: TypeContext) -> Option<BuiltType> {
		let type_name = node.as_type_reference_node().type_name;
		let mut symbol = self.s.ck().get_symbol_at_location(type_name)?;
		if symbol.flags().intersects(SymbolFlags::Alias) {
			symbol = self.s.ck().get_aliased_symbol(symbol);
		}
		let declaration = alias_declaration_of_symbol(symbol);
		if !is_type_level_alias(symbol, declaration)
			|| declaration.and_then(tsast::get_source_file_of_node) != Some(self.s.source_file)
		{
			return None;
		}
		let t = self.s.ck().get_type_from_type_node(node);
		if t.alias().is_some_and(|alias| alias.symbol() == Some(symbol)) {
			return None;
		}
		let argument_nodes = node.type_arguments();
		for &argument in argument_nodes {
			let argument_type = self.s.ck().get_type_from_type_node(argument);
			if self.type_has_free_variables(Some(argument_type)) {
				return None;
			}
		}
		let entry = self.alias_for(Some(SymKey::Real(symbol)))?;
		let params = self.b().entry(entry).params.clone();
		if self.b().entry(entry).type_function.is_none() || argument_nodes.len() > params.len() {
			return None;
		}
		let nested = ctx.nested();
		let arguments = self.ast().new_list::<element::TypeNode>();
		for &argument in argument_nodes {
			let built = self.build_type_node(argument, nested).node;
			self.ast().list_push(arguments, built);
		}
		for &parameter in &params[argument_nodes.len()..] {
			let parameter = parameter?;
			let default_type = self.s.ck().get_default_from_type_parameter(parameter)?;
			let built = self.build(default_type, nested).node;
			self.ast().list_push(arguments, built);
		}
		let name = self.b().entry(entry).name.clone();
		let result = self.ast().type_reference(name, Some(arguments));
		self.set_origin(result, SymKey::Real(symbol));
		Some(self.finish(result, false))
	}

	/// `typeHasFreeVariables`: a type that mentions an uninstantiated type parameter.
	pub fn type_has_free_variables(&mut self, t: Option<P<Type>>) -> bool {
		self.has_free_variables(t, 0, &mut HashMap::new())
	}

	fn has_free_variables(&mut self, t: Option<P<Type>>, depth: usize, expanded: &mut HashMap<P<Type>, usize>) -> bool {
		let Some(t) = t else {
			return false;
		};
		if depth > 16 {
			return false;
		}
		// Roblox's instance types reach each other through hundreds of properties, so without this
		// the walk is exponential in the depth limit. A type already expanded at this depth or
		// shallower had at least as much budget left, and found nothing.
		if expanded.get(&t).is_some_and(|&seen| seen <= depth) {
			return false;
		}
		expanded.insert(t, depth);
		let flags = t.flags();
		if flags.intersects(TypeFlags::Instantiable) {
			return true;
		}
		if flags.intersects(TypeFlags::UnionOrIntersection) {
			return t
				.types()
				.iter()
				.any(|&member| self.has_free_variables(Some(member), depth + 1, expanded));
		}
		if !flags.intersects(TypeFlags::Object) {
			return false;
		}
		if let Some(alias) = t.alias() {
			return alias
				.type_arguments()
				.iter()
				.any(|&argument| self.has_free_variables(Some(argument), depth + 1, expanded));
		}
		let object_flags = t.object_flags();
		if object_flags.intersects(ObjectFlags::Reference) {
			let arguments = get_type_arguments(self.s, t);
			return arguments
				.iter()
				.any(|&argument| self.has_free_variables(Some(argument), depth + 1, expanded));
		}
		if object_flags.intersects(ObjectFlags::Mapped) {
			return true;
		}
		if object_flags.intersects(ObjectFlags::CouldContainTypeVariablesComputed)
			&& !object_flags.intersects(ObjectFlags::CouldContainTypeVariables)
		{
			return false;
		}
		// An anonymous object type (a type literal, a function type) is only marked as possibly
		// containing type variables, so its members decide.
		let properties = self.s.ck().get_properties_of_type(t);
		for &property in properties {
			let property_type = self.s.ck().get_type_of_symbol(property);
			if self.has_free_variables(Some(property_type), depth + 1, expanded) {
				return true;
			}
		}
		let infos = self.s.ck().get_index_infos_of_type(t);
		for &info in infos {
			if self.has_free_variables(Some(info.key_type()), depth + 1, expanded)
				|| self.has_free_variables(Some(info.value_type()), depth + 1, expanded)
			{
				return true;
			}
		}
		let mut signatures = self.s.ck().get_call_signatures(t).to_vec();
		signatures.extend_from_slice(self.s.ck().get_construct_signatures(t));
		for signature in signatures {
			for &parameter in signature.parameters() {
				let parameter_type = self.s.ck().get_type_of_symbol(parameter);
				if self.has_free_variables(Some(parameter_type), depth + 1, expanded) {
					return true;
				}
			}
			let return_type = self.s.ck().get_return_type_of_signature(signature);
			if self.has_free_variables(Some(return_type), depth + 1, expanded) {
				return true;
			}
		}
		false
	}
}
