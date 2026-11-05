//! A builder DSL for test ASTs: nested calls build children before parents,
//! in argument order, like the Go constructors.

use crate::{Ast, BinaryOperator, NodeId, TypeAccess, TypeFunctionParameter, TypeGeneric, UnaryOperator, element};

/// A node to build.
pub struct N(Box<dyn FnOnce(&mut Ast) -> NodeId>);

impl N {
	pub fn build(self, ast: &mut Ast) -> NodeId {
		(self.0)(ast)
	}

	pub fn from_fn(build: impl FnOnce(&mut Ast) -> NodeId + 'static) -> Self {
		Self(Box::new(build))
	}
}

impl From<NodeId> for N {
	fn from(node: NodeId) -> Self {
		Self(Box::new(move |_| node))
	}
}

fn n(build: impl FnOnce(&mut Ast) -> NodeId + 'static) -> N {
	N(Box::new(build))
}

fn build_all(ast: &mut Ast, nodes: Vec<N>) -> Vec<NodeId> {
	nodes.into_iter().map(|node| node.build(ast)).collect()
}

/// A generic parameter to build.
pub struct G {
	name: &'static str,
	is_pack: bool,
	default: Option<N>,
}

pub const fn generic(name: &'static str) -> G {
	G {
		name,
		is_pack: false,
		default: None,
	}
}

pub const fn generic_pack(name: &'static str) -> G {
	G {
		name,
		is_pack: true,
		default: None,
	}
}

impl G {
	pub fn default(mut self, default: impl Into<N>) -> Self {
		self.default = Some(default.into());
		self
	}

	fn build(self, ast: &mut Ast) -> TypeGeneric {
		TypeGeneric {
			name: self.name.to_owned(),
			is_pack: self.is_pack,
			default: self.default.map(|default| default.build(ast)),
		}
	}
}

pub fn build_generics(ast: &mut Ast, generics: Vec<G>) -> Vec<TypeGeneric> {
	generics.into_iter().map(|generic| generic.build(ast)).collect()
}

pub fn id(name: &'static str) -> N {
	n(move |a| a.id(name))
}

pub fn num(value: f64) -> N {
	n(move |a| a.num(value))
}

pub fn string(value: &'static str) -> N {
	n(move |a| a.str(value))
}

pub fn nil() -> N {
	n(Ast::nil)
}

pub fn bin(left: impl Into<N>, operator: BinaryOperator, right: impl Into<N>) -> N {
	let (left, right) = (left.into(), right.into());
	n(move |a| {
		let left = left.build(a);
		let right = right.build(a);
		a.binary(left, operator, right)
	})
}

pub fn un(operator: UnaryOperator, expression: impl Into<N>) -> N {
	let expression = expression.into();
	n(move |a| {
		let expression = expression.build(a);
		a.unary(operator, expression)
	})
}

pub fn if_expr(condition: impl Into<N>, expression: impl Into<N>, alternative: impl Into<N>) -> N {
	let (condition, expression, alternative) = (condition.into(), expression.into(), alternative.into());
	n(move |a| {
		let condition = condition.build(a);
		let expression = expression.build(a);
		let alternative = alternative.build(a);
		a.if_expression(condition, expression, alternative)
	})
}

pub fn call(callee: impl Into<N>, args: Vec<N>) -> N {
	let callee = callee.into();
	n(move |a| {
		let callee = callee.build(a);
		let args = build_all(a, args);
		let args = a.list_from::<element::Expression>(args);
		a.call(callee, args)
	})
}

pub fn property(expression: impl Into<N>, name: &'static str) -> N {
	let expression = expression.into();
	n(move |a| {
		let expression = expression.build(a);
		a.property_access(expression, name)
	})
}

pub fn paren(expression: impl Into<N>) -> N {
	let expression = expression.into();
	n(move |a| {
		let expression = expression.build(a);
		a.parenthesized(expression)
	})
}

pub fn array(members: Vec<N>) -> N {
	n(move |a| {
		let members = build_all(a, members);
		let members = a.list_from::<element::Expression>(members);
		a.array(members)
	})
}

pub fn map(fields: Vec<N>) -> N {
	n(move |a| {
		let fields = build_all(a, fields);
		let fields = a.list_from::<element::MapField>(fields);
		a.map(fields)
	})
}

pub fn cast(expression: impl Into<N>, ty: impl Into<N>) -> N {
	let (expression, ty) = (expression.into(), ty.into());
	n(move |a| {
		let expression = expression.build(a);
		let ty = ty.build(a);
		a.cast(expression, ty)
	})
}

pub fn tn(name: &'static str) -> N {
	n(move |a| a.type_name(name))
}

pub fn type_ref(name: &'static str, arguments: Vec<N>) -> N {
	n(move |a| {
		let arguments = build_all(a, arguments);
		let arguments = a.list_from::<element::TypeNode>(arguments);
		a.type_reference(name, Some(arguments))
	})
}

pub fn qualified(module: &'static str, name: &'static str, arguments: Option<Vec<N>>) -> N {
	n(move |a| {
		let arguments = arguments.map(|arguments| {
			let arguments = build_all(a, arguments);
			a.list_from::<element::TypeNode>(arguments)
		});
		a.type_qualified_reference(module, name, arguments)
	})
}

pub fn string_singleton(value: &'static str) -> N {
	n(move |a| a.type_string_singleton(value))
}

pub fn bool_singleton(value: bool) -> N {
	n(move |a| a.type_boolean_singleton(value))
}

pub fn optional(inner: impl Into<N>) -> N {
	let inner = inner.into();
	n(move |a| {
		let inner = inner.build(a);
		a.type_optional(inner)
	})
}

/// `OptionalType`, which normalizes.
pub fn optional_type(inner: impl Into<N>) -> N {
	let inner = inner.into();
	n(move |a| {
		let inner = inner.build(a);
		a.optional_type(inner)
	})
}

pub fn union(members: Vec<N>) -> N {
	n(move |a| {
		let members = build_all(a, members);
		let members = a.list_from::<element::TypeNode>(members);
		a.type_union(members)
	})
}

pub fn intersection(members: Vec<N>) -> N {
	n(move |a| {
		let members = build_all(a, members);
		let members = a.list_from::<element::TypeNode>(members);
		a.type_intersection(members)
	})
}

pub fn pack(elements: Vec<N>) -> N {
	n(move |a| {
		let elements = build_all(a, elements);
		let elements = a.list_from::<element::TypeNode>(elements);
		a.type_pack(elements)
	})
}

pub fn variadic(inner: impl Into<N>) -> N {
	let inner = inner.into();
	n(move |a| {
		let inner = inner.build(a);
		a.type_variadic(inner)
	})
}

pub fn gpack(name: &'static str) -> N {
	n(move |a| a.type_generic_pack(name))
}

pub fn typeof_(expression: impl Into<N>) -> N {
	let expression = expression.into();
	n(move |a| {
		let expression = expression.build(a);
		a.type_typeof(expression)
	})
}

pub fn type_array(element: impl Into<N>) -> N {
	let element = element.into();
	n(move |a| {
		let element = element.build(a);
		a.type_array(element)
	})
}

pub fn prop(name: &'static str, value: impl Into<N>) -> N {
	prop_with(TypeAccess::None, name, value)
}

pub fn prop_with(access: TypeAccess, name: &'static str, value: impl Into<N>) -> N {
	let value = value.into();
	n(move |a| {
		let value = value.build(a);
		a.type_table_prop(access, name, value)
	})
}

pub fn indexer(access: TypeAccess, key: impl Into<N>, value: impl Into<N>) -> N {
	let (key, value) = (key.into(), value.into());
	n(move |a| {
		let key = key.build(a);
		let value = value.build(a);
		a.type_table_indexer(access, key, value)
	})
}

pub fn table(fields: Vec<N>) -> N {
	n(move |a| {
		let fields = build_all(a, fields);
		let fields = a.list_from::<element::TypeTableField>(fields);
		a.type_table(fields)
	})
}

/// A function type parameter.
pub fn param(name: &'static str, ty: impl Into<N>) -> (&'static str, N) {
	(name, ty.into())
}

pub fn fn_type_generic(
	generics: Vec<G>,
	parameters: Vec<(&'static str, N)>,
	var_args: Option<N>,
	ret: impl Into<N>,
) -> N {
	let ret = ret.into();
	n(move |a| {
		let generics = build_generics(a, generics);
		let parameters = parameters
			.into_iter()
			.map(|(name, ty)| TypeFunctionParameter::new(name, ty.build(a)))
			.collect();
		let var_args = var_args.map(|var_args| var_args.build(a));
		let ret = ret.build(a);
		a.type_function(generics, parameters, var_args, ret)
	})
}

pub fn fn_type(parameters: Vec<(&'static str, N)>, var_args: Option<N>, ret: impl Into<N>) -> N {
	fn_type_generic(Vec::new(), parameters, var_args, ret)
}

pub fn alias(name: &'static str, generics: Vec<G>, body: Option<N>) -> N {
	n(move |a| {
		let generics = build_generics(a, generics);
		let body = body.map(|body| body.build(a));
		a.type_alias(name, generics, body)
	})
}
