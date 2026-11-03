//! A pre/post-order walk over every node the renderer can reach.

use crate::types::TypeGeneric;
use crate::{Ast, NodeData, NodeId, NodeOrList};

pub trait Visitor {
	/// Rendering historically evaluated map-field values first and allocated
	/// fallback temporary names in that order, including in unsolved
	/// iterators. Render-order walks also enter `for` iterators.
	const RENDER_ORDER: bool = false;

	fn before(&mut self, _ast: &Ast, _node: NodeId) {}

	fn after(&mut self, _ast: &Ast, _node: NodeId) {}
}

pub fn visit_node_or_list<V: Visitor>(ast: &Ast, value: NodeOrList, vis: &mut V) {
	match value {
		NodeOrList::Node(node) => visit_node(ast, node, vis),
		NodeOrList::List(list) => {
			for node in ast.any_list_iter(list.id) {
				visit_node(ast, node, vis);
			}
		}
	}
}

fn visit_list<V: Visitor, T>(ast: &Ast, list: crate::List<T>, vis: &mut V) {
	for node in ast.list_iter(list) {
		visit_node(ast, node, vis);
	}
}

/// Visits an optional type child.
fn visit_type<V: Visitor>(ast: &Ast, node: Option<NodeId>, vis: &mut V) {
	if let Some(node) = node {
		visit_node(ast, node, vis);
	}
}

fn visit_generics<V: Visitor>(ast: &Ast, generics: &[TypeGeneric], vis: &mut V) {
	for generic in generics {
		visit_type(ast, generic.default, vis);
	}
}

/// Visits the type children of a function-like node in render order: generic
/// defaults, parameters (with their binding annotations), the varargs
/// annotation, then the return annotation.
fn visit_signature<V: Visitor>(ast: &Ast, generics: &[TypeGeneric], node: NodeId, vis: &mut V) {
	visit_generics(ast, generics, vis);
	if let Some(params) = ast.param_data(node) {
		visit_list(ast, params.params, vis);
		visit_type(ast, params.var_args_type, vis);
	}
	visit_type(ast, ast.return_type_data(node), vis);
}

#[expect(clippy::too_many_lines, reason = "one arm per node kind")]
pub fn visit_node<V: Visitor>(ast: &Ast, node: NodeId, vis: &mut V) {
	vis.before(ast, node);
	match ast.data(node) {
		NodeData::Identifier(n) => visit_type(ast, n.annotation, vis),
		NodeData::TemporaryIdentifier(n) => visit_type(ast, n.annotation, vis),
		NodeData::ComputedIndexExpression(n) => {
			visit_node(ast, n.expression, vis);
			visit_node(ast, n.index, vis);
		}
		NodeData::PropertyAccessExpression(n) => visit_node(ast, n.expression, vis),
		NodeData::CallExpression(n) => {
			visit_node(ast, n.expression, vis);
			visit_list(ast, n.args, vis);
		}
		NodeData::MethodCallExpression(n) => {
			visit_node(ast, n.expression, vis);
			visit_list(ast, n.args, vis);
		}
		NodeData::ParenthesizedExpression(n) => visit_node(ast, n.expression, vis),
		NodeData::FunctionExpression(n) => {
			visit_signature(ast, &n.generics, node, vis);
			visit_list(ast, n.statements, vis);
		}
		NodeData::BinaryExpression(n) => {
			visit_node(ast, n.left, vis);
			visit_node(ast, n.right, vis);
		}
		NodeData::UnaryExpression(n) => visit_node(ast, n.expression, vis),
		NodeData::IfExpression(n) => {
			visit_node(ast, n.condition, vis);
			visit_node(ast, n.expression, vis);
			visit_node(ast, n.alternative, vis);
		}
		NodeData::InterpolatedString(n) => visit_list(ast, n.parts, vis),
		NodeData::Array(n) => visit_list(ast, n.members, vis),
		NodeData::Map(n) => visit_list(ast, n.fields, vis),
		NodeData::Set(n) => visit_list(ast, n.members, vis),
		NodeData::MixedTable(n) => visit_list(ast, n.fields, vis),
		NodeData::CastExpression(n) => {
			visit_node(ast, n.expression, vis);
			visit_node(ast, n.ty, vis);
		}
		NodeData::Assignment(n) => {
			visit_node_or_list(ast, n.left, vis);
			visit_node_or_list(ast, n.right, vis);
		}
		NodeData::CallStatement(n) => visit_node(ast, n.expression, vis),
		NodeData::DoStatement(n) => visit_list(ast, n.statements, vis),
		NodeData::WhileStatement(n) => {
			visit_node(ast, n.condition, vis);
			visit_list(ast, n.statements, vis);
		}
		NodeData::RepeatStatement(n) => {
			visit_list(ast, n.statements, vis);
			visit_node(ast, n.condition, vis);
		}
		NodeData::IfStatement(n) => {
			visit_node(ast, n.condition, vis);
			visit_list(ast, n.statements, vis);
			visit_node_or_list(ast, n.else_body, vis);
		}
		NodeData::NumericForStatement(n) => {
			visit_node(ast, n.id, vis);
			visit_node(ast, n.start, vis);
			visit_node(ast, n.end, vis);
			visit_type(ast, n.step, vis);
			visit_list(ast, n.statements, vis);
		}
		NodeData::ForStatement(n) => {
			visit_list(ast, n.ids, vis);
			if V::RENDER_ORDER {
				visit_node(ast, n.expression, vis);
			}
			visit_list(ast, n.statements, vis);
		}
		NodeData::FunctionDeclaration(n) => {
			visit_node(ast, n.name, vis);
			visit_signature(ast, &n.generics, node, vis);
			visit_list(ast, n.statements, vis);
		}
		NodeData::MethodDeclaration(n) => {
			visit_node(ast, n.expression, vis);
			visit_signature(ast, &n.generics, node, vis);
			visit_list(ast, n.statements, vis);
		}
		NodeData::VariableDeclaration(n) => {
			visit_node_or_list(ast, n.left, vis);
			if let Some(right) = n.right {
				visit_node_or_list(ast, right, vis);
			}
		}
		NodeData::ReturnStatement(n) => visit_node_or_list(ast, n.expression, vis),
		NodeData::TypeAlias(n) => {
			visit_generics(ast, &n.generics, vis);
			visit_type(ast, n.body, vis);
		}
		NodeData::TypeFunctionDeclaration(n) => {
			visit_list(ast, n.parameters, vis);
			visit_list(ast, n.statements, vis);
		}
		NodeData::MapField(n) => {
			if V::RENDER_ORDER {
				visit_node(ast, n.value, vis);
				visit_node(ast, n.index, vis);
			} else {
				visit_node(ast, n.index, vis);
				visit_node(ast, n.value, vis);
			}
		}
		NodeData::TypeReference(n) => {
			if let Some(arguments) = n.arguments {
				visit_list(ast, arguments, vis);
			}
		}
		NodeData::TypeOptional(n) => visit_type(ast, n.inner, vis),
		NodeData::TypeUnion(n) => visit_list(ast, n.members, vis),
		NodeData::TypeIntersection(n) => visit_list(ast, n.members, vis),
		NodeData::TypeTable(n) => visit_list(ast, n.fields, vis),
		NodeData::TypeTableProp(n) => visit_node(ast, n.value, vis),
		NodeData::TypeTableIndexer(n) => {
			visit_node(ast, n.key, vis);
			visit_node(ast, n.value, vis);
		}
		NodeData::TypeArray(n) => visit_node(ast, n.element, vis),
		NodeData::TypeFunction(n) => {
			visit_generics(ast, &n.generics, vis);
			for parameter in &n.parameters {
				visit_node(ast, parameter.ty, vis);
			}
			visit_type(ast, n.var_args, vis);
			visit_type(ast, n.ret, vis);
		}
		NodeData::TypeTypeof(n) => visit_node(ast, n.expression, vis),
		NodeData::TypePack(n) => visit_list(ast, n.elements, vis),
		NodeData::TypeVariadic(n) => visit_node(ast, n.inner, vis),
		NodeData::None
		| NodeData::NilLiteral
		| NodeData::FalseLiteral
		| NodeData::TrueLiteral
		| NodeData::NumberLiteral(_)
		| NodeData::StringLiteral(_)
		| NodeData::VarArgsLiteral
		| NodeData::BreakStatement
		| NodeData::ContinueStatement
		| NodeData::Comment(_)
		| NodeData::InterpolatedStringPart(_)
		| NodeData::TypeStringSingleton(_)
		| NodeData::TypeBooleanSingleton(_)
		| NodeData::TypeGenericPack(_) => {}
	}
	vis.after(ast, node);
}
