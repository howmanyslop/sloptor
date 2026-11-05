//! Luau type nodes, the cast expression, and the type statements (rotor
//! extensions).
//!
//! Every production of the Luau type grammar. Constructors adopt their
//! children and never set an origin. The renderer owns parenthesization, so
//! builders never inspect rendered text.

use crate::list::element;
use crate::{Ast, List, NodeData, NodeId};

/// A table property or indexer modifier: `read x: T`, `write [K]: V`,
/// `{ read T }`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TypeAccess {
	/// No modifier.
	#[default]
	None,
	/// `read`
	Read,
	/// `write`
	Write,
}

/// One generic parameter of an alias, a function, or a function type: `T`,
/// `U...` (`is_pack`), `T = number` (`default`). Defaults are legal only on a
/// [`TypeAlias`]; the renderer rejects them anywhere else.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypeGeneric {
	/// The parameter name.
	pub name: String,
	/// Whether this is a generic pack.
	pub is_pack: bool,
	/// The default, legal only on aliases.
	pub default: Option<NodeId>,
}

impl TypeGeneric {
	/// A generic type parameter without a default.
	#[must_use]
	pub fn new(name: impl Into<String>) -> Self {
		Self {
			name: name.into(),
			is_pack: false,
			default: None,
		}
	}

	/// A generic pack parameter without a default.
	#[must_use]
	pub fn pack(name: impl Into<String>) -> Self {
		Self {
			name: name.into(),
			is_pack: true,
			default: None,
		}
	}

	/// Sets the default.
	#[must_use]
	pub const fn with_default(mut self, default: NodeId) -> Self {
		self.default = Some(default);
		self
	}
}

/// One bound parameter of a function type. An empty name renders the type
/// alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeFunctionParameter {
	/// The parameter name, or empty.
	pub name: String,
	/// The parameter type.
	pub ty: NodeId,
}

impl TypeFunctionParameter {
	/// Builds a parameter.
	#[must_use]
	pub fn new(name: impl Into<String>, ty: NodeId) -> Self {
		Self { name: name.into(), ty }
	}
}

/// `Name`, `Module.Name`, or `Name<A, B...>`; `nil` is a reference too. A
/// missing or empty argument list renders the bare name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeReference {
	/// The module qualifier, or empty.
	pub module: String,
	/// The type name.
	pub name: String,
	/// The type arguments.
	pub arguments: Option<List<element::TypeNode>>,
}

/// `"value"`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeStringSingleton {
	/// The unescaped value.
	pub value: String,
}

/// `true` or `false`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeBooleanSingleton {
	/// The value.
	pub value: bool,
}

/// `T?`. The inner type must not be a pack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeOptional {
	/// The wrapped type. `None` is an error at render time.
	pub inner: Option<NodeId>,
}

/// `A | B`. `multiline` (set by [`layout_types`](crate::render::layout_types)
/// on alias bodies) renders one member per line with a leading `|`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeUnion {
	/// The members.
	pub members: List<element::TypeNode>,
	/// Whether to render one member per line.
	pub multiline: bool,
}

/// `A & B`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeIntersection {
	/// The members.
	pub members: List<element::TypeNode>,
}

/// `{ a: number, [string]: boolean }`. `multiline` renders one field per
/// line, like a map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeTable {
	/// Props and at most one indexer.
	pub fields: List<element::TypeTableField>,
	/// Whether to render one field per line.
	pub multiline: bool,
}

/// `[read|write] name: T`; a name that is not a valid identifier (or is a
/// keyword) renders as `["name"]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeTableProp {
	/// The access modifier.
	pub access: TypeAccess,
	/// The property name.
	pub name: String,
	/// The property type.
	pub value: NodeId,
}

/// `[read|write] [K]: V`; a table type has at most one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeTableIndexer {
	/// The access modifier.
	pub access: TypeAccess,
	/// The key type.
	pub key: NodeId,
	/// The value type.
	pub value: NodeId,
}

/// The array shorthand `{ T }`, or `{ read T }` with an access modifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeArray {
	/// The access modifier.
	pub access: TypeAccess,
	/// The element type.
	pub element: NodeId,
}

/// `<T, U...>(name: T, ...V) -> R`. `var_args` is `None`, a
/// [`TypeVariadic`] (`...V`), or a [`TypeGenericPack`] (`U...`). The return is
/// required at render time: an empty [`TypePack`] is `()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeFunction {
	/// The generic parameters.
	pub generics: Vec<TypeGeneric>,
	/// The parameters.
	pub parameters: Vec<TypeFunctionParameter>,
	/// The variadic tail.
	pub var_args: Option<NodeId>,
	/// The return type.
	pub ret: Option<NodeId>,
}

/// `typeof(expression)`; the expression is never evaluated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeTypeof {
	/// The expression.
	pub expression: NodeId,
}

/// The explicit pack `(A, B)`, or `()` when empty. A pack-shaped element is
/// legal only in last position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypePack {
	/// The elements.
	pub elements: List<element::TypeNode>,
}

/// `...T`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeVariadic {
	/// The element type.
	pub inner: NodeId,
}

/// `T...`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeGenericPack {
	/// The pack name.
	pub name: String,
}

/// `expression :: Type` (Luau `asexp`). It is not indexable: wrap it in a
/// parenthesized expression to call or index the result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CastExpression {
	/// The operand.
	pub expression: NodeId,
	/// The target type.
	pub ty: NodeId,
}

/// `[export] type Name<K, V = number> = Body`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeAlias {
	/// Whether the alias is exported.
	pub export: bool,
	/// The alias name.
	pub name: String,
	/// The generic parameters, which may have defaults.
	pub generics: Vec<TypeGeneric>,
	/// The body; `None` until [`Ast::set_type_alias_body`] fills it.
	pub body: Option<NodeId>,
}

/// `[export] type function Name(params) ... end`. Its body is ordinary Luau
/// run by the type checker (new solver only).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeFunctionDeclaration {
	/// Whether the type function is exported.
	pub export: bool,
	/// The name.
	pub name: String,
	/// The parameters.
	pub parameters: List<element::AnyIdentifier>,
	/// Whether it takes `...`.
	pub has_dot_dot_dot: bool,
	/// The body.
	pub statements: List<element::Statement>,
}

impl Ast {
	pub(crate) fn adopt_generics(&mut self, parent: NodeId, mut generics: Vec<TypeGeneric>) -> Vec<TypeGeneric> {
		for generic in &mut generics {
			if let Some(default) = generic.default {
				generic.default = Some(self.adopt(parent, default));
			}
		}
		generics
	}

	/// `name` or `name<arguments>`.
	pub fn type_reference(&mut self, name: impl Into<String>, arguments: Option<List<element::TypeNode>>) -> NodeId {
		self.type_qualified_reference("", name, arguments)
	}

	/// `module.name` or `module.name<arguments>`.
	pub fn type_qualified_reference(
		&mut self,
		module: impl Into<String>,
		name: impl Into<String>,
		arguments: Option<List<element::TypeNode>>,
	) -> NodeId {
		let node = self.reserve();
		let arguments = arguments.map(|list| self.adopt_list(node, list));
		self.fill(
			node,
			TypeReference {
				module: module.into(),
				name: name.into(),
				arguments,
			},
		)
	}

	/// A bare reference: `number`, `nil`, `Part`, `T`.
	pub fn type_name(&mut self, name: impl Into<String>) -> NodeId {
		self.alloc(TypeReference {
			module: String::new(),
			name: name.into(),
			arguments: None,
		})
	}

	/// `"value"`
	pub fn type_string_singleton(&mut self, value: impl Into<String>) -> NodeId {
		self.alloc(TypeStringSingleton { value: value.into() })
	}

	/// `true` or `false`
	pub fn type_boolean_singleton(&mut self, value: bool) -> NodeId {
		self.alloc(TypeBooleanSingleton { value })
	}

	/// `inner?`
	pub fn type_optional(&mut self, inner: NodeId) -> NodeId {
		let node = self.reserve();
		let inner = self.adopt(node, inner);
		self.fill(node, TypeOptional { inner: Some(inner) })
	}

	/// [`Ast::type_optional`] with normalization: an optional stays as it is
	/// (`T??` is `T?`), and so do `nil`, `any`, and `unknown`, which already
	/// admit nil.
	pub fn optional_type(&mut self, inner: NodeId) -> NodeId {
		match self.data(inner) {
			NodeData::TypeOptional(_) => return inner,
			NodeData::TypeReference(reference)
				if reference.module.is_empty()
					&& reference
						.arguments
						.is_none_or(|arguments| self.list_is_empty(arguments))
					&& matches!(reference.name.as_str(), "nil" | "any" | "unknown") =>
			{
				return inner;
			}
			_ => {}
		}
		self.type_optional(inner)
	}

	/// `A | B`
	pub fn type_union(&mut self, members: List<element::TypeNode>) -> NodeId {
		let node = self.reserve();
		let members = self.adopt_list(node, members);
		self.fill(
			node,
			TypeUnion {
				members,
				multiline: false,
			},
		)
	}

	/// `A & B`
	pub fn type_intersection(&mut self, members: List<element::TypeNode>) -> NodeId {
		let node = self.reserve();
		let members = self.adopt_list(node, members);
		self.fill(node, TypeIntersection { members })
	}

	/// `{ fields }`
	pub fn type_table(&mut self, fields: List<element::TypeTableField>) -> NodeId {
		let node = self.reserve();
		let fields = self.adopt_list(node, fields);
		self.fill(
			node,
			TypeTable {
				fields,
				multiline: false,
			},
		)
	}

	/// `[read|write] name: value`
	pub fn type_table_prop(&mut self, access: TypeAccess, name: impl Into<String>, value: NodeId) -> NodeId {
		let node = self.reserve();
		let value = self.adopt(node, value);
		self.fill(
			node,
			TypeTableProp {
				access,
				name: name.into(),
				value,
			},
		)
	}

	/// `[read|write] [key]: value`
	pub fn type_table_indexer(&mut self, access: TypeAccess, key: NodeId, value: NodeId) -> NodeId {
		let node = self.reserve();
		let key = self.adopt(node, key);
		let value = self.adopt(node, value);
		self.fill(node, TypeTableIndexer { access, key, value })
	}

	/// `{ element }`
	pub fn type_array(&mut self, element: NodeId) -> NodeId {
		let node = self.reserve();
		let element = self.adopt(node, element);
		self.fill(
			node,
			TypeArray {
				access: TypeAccess::None,
				element,
			},
		)
	}

	/// `<generics>(parameters, var_args) -> ret`. Use an empty
	/// [`Ast::type_pack`] for `()`.
	pub fn type_function(
		&mut self,
		generics: Vec<TypeGeneric>,
		mut parameters: Vec<TypeFunctionParameter>,
		var_args: Option<NodeId>,
		ret: NodeId,
	) -> NodeId {
		let node = self.reserve();
		let generics = self.adopt_generics(node, generics);
		for parameter in &mut parameters {
			parameter.ty = self.adopt(node, parameter.ty);
		}
		let var_args = var_args.map(|var_args| self.adopt(node, var_args));
		let ret = self.adopt(node, ret);
		self.fill(
			node,
			TypeFunction {
				generics,
				parameters,
				var_args,
				ret: Some(ret),
			},
		)
	}

	/// `typeof(expression)`
	pub fn type_typeof(&mut self, expression: NodeId) -> NodeId {
		let node = self.reserve();
		let expression = self.adopt(node, expression);
		self.fill(node, TypeTypeof { expression })
	}

	/// `(elements)`
	pub fn type_pack(&mut self, elements: List<element::TypeNode>) -> NodeId {
		let node = self.reserve();
		let elements = self.adopt_list(node, elements);
		self.fill(node, TypePack { elements })
	}

	/// `...inner`
	pub fn type_variadic(&mut self, inner: NodeId) -> NodeId {
		let node = self.reserve();
		let inner = self.adopt(node, inner);
		self.fill(node, TypeVariadic { inner })
	}

	/// `name...`
	pub fn type_generic_pack(&mut self, name: impl Into<String>) -> NodeId {
		self.alloc(TypeGenericPack { name: name.into() })
	}

	/// `expression :: ty`
	pub fn cast(&mut self, expression: NodeId, ty: NodeId) -> NodeId {
		let node = self.reserve();
		let expression = self.adopt(node, expression);
		let ty = self.adopt(node, ty);
		self.fill(node, CastExpression { expression, ty })
	}

	/// `type name<generics> = body`. The body may be `None` so a recursive
	/// alias can be registered before its body is built;
	/// [`Ast::set_type_alias_body`] fills it in, and rendering an alias that
	/// never got one is an error.
	pub fn type_alias(&mut self, name: impl Into<String>, generics: Vec<TypeGeneric>, body: Option<NodeId>) -> NodeId {
		let node = self.reserve();
		let generics = self.adopt_generics(node, generics);
		let body = body.map(|body| self.adopt(node, body));
		self.fill(
			node,
			TypeAlias {
				export: false,
				name: name.into(),
				generics,
				body,
			},
		)
	}

	/// Adopts `body` as the alias body, replacing any previous one. Does
	/// nothing when `alias` is not a type alias.
	pub fn set_type_alias_body(&mut self, alias: NodeId, body: NodeId) {
		if self.get::<TypeAlias>(alias).is_none() {
			return;
		}
		let body = self.adopt(alias, body);
		if let Some(payload) = self.get_mut::<TypeAlias>(alias) {
			payload.body = Some(body);
		}
	}

	/// `type function name(params) ... end`
	pub fn type_function_declaration(
		&mut self,
		name: impl Into<String>,
		params: List<element::AnyIdentifier>,
		has_dot_dot_dot: bool,
		statements: List<element::Statement>,
	) -> NodeId {
		let node = self.reserve();
		let parameters = self.adopt_list(node, params);
		let statements = self.adopt_list(node, statements);
		self.fill(
			node,
			TypeFunctionDeclaration {
				export: false,
				name: name.into(),
				parameters,
				has_dot_dot_dot,
				statements,
			},
		)
	}

	/// Annotates an identifier where it is bound: a parameter or the left
	/// side of a `local`. The annotation lives on the identifier because
	/// constructors clone nodes that already have a parent, and a clone keeps
	/// the annotation. `None` removes it. Does nothing for other kinds.
	pub fn set_binding_type(&mut self, identifier: NodeId, ty: Option<NodeId>) {
		if !self.kind(identifier).is_any_identifier() {
			return;
		}
		let annotation = ty.map(|ty| self.adopt(identifier, ty));
		match self.data_mut(identifier) {
			NodeData::Identifier(payload) => payload.annotation = annotation,
			NodeData::TemporaryIdentifier(payload) => payload.annotation = annotation,
			_ => {}
		}
	}

	/// The annotation set by [`Ast::set_binding_type`], if any.
	#[must_use]
	pub fn binding_type(&self, identifier: NodeId) -> Option<NodeId> {
		match self.data(identifier) {
			NodeData::Identifier(payload) => payload.annotation,
			NodeData::TemporaryIdentifier(payload) => payload.annotation,
			_ => None,
		}
	}
}

#[cfg(test)]
mod tests;
