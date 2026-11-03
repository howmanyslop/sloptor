//! Port of `internal/luau/nodes_test.go`. Go checks the marker interfaces at
//! compile time; here every node is a `NodeId`, so the same memberships are
//! checked on the kind categories that replace the interfaces.

use crate::{
	Ast, CastExpression, Identifier, InterpolatedStringPart, NodeData, SyntaxKind, TypeAccess, TypeFunctionDeclaration,
	element,
};

#[test]
fn category_markers() {
	use SyntaxKind as K;
	for kind in [
		K::Identifier,
		K::TemporaryIdentifier,
		K::ComputedIndexExpression,
		K::PropertyAccessExpression,
		K::CallExpression,
		K::MethodCallExpression,
		K::ParenthesizedExpression,
	] {
		assert!(
			kind.is_indexable_expression() && kind.is_expression(),
			"{kind} is an IndexableExpression"
		);
	}
	for kind in [K::NilLiteral, K::MixedTable, K::CastExpression] {
		assert!(kind.is_expression(), "{kind} is an Expression");
	}
	for kind in [K::Assignment, K::Comment, K::TypeAlias, K::TypeFunctionDeclaration] {
		assert!(kind.is_statement(), "{kind} is a Statement");
	}
	for kind in [K::MapField, K::InterpolatedStringPart] {
		assert!(kind.is_field(), "{kind} is a FieldNode");
	}
	for kind in [K::Identifier, K::TemporaryIdentifier] {
		assert!(kind.is_any_identifier(), "{kind} is an AnyIdentifier");
	}
	for kind in [
		K::Identifier,
		K::TemporaryIdentifier,
		K::PropertyAccessExpression,
		K::ComputedIndexExpression,
	] {
		assert!(kind.is_writable_expression(), "{kind} is a WritableExpression");
	}
	for kind in [
		K::FunctionExpression,
		K::FunctionDeclaration,
		K::MethodDeclaration,
		K::TypeFunctionDeclaration,
	] {
		assert!(kind.is_function_like(), "{kind} has parameters");
	}
	for kind in [
		K::TypeReference,
		K::TypeStringSingleton,
		K::TypeBooleanSingleton,
		K::TypeOptional,
		K::TypeUnion,
		K::TypeIntersection,
		K::TypeTable,
		K::TypeArray,
		K::TypeFunction,
		K::TypeTypeof,
		K::TypePack,
		K::TypeVariadic,
		K::TypeGenericPack,
	] {
		assert!(kind.is_type_node(), "{kind} is a TypeNode");
	}
	for kind in [K::TypeTableProp, K::TypeTableIndexer] {
		assert!(kind.is_type_table_field(), "{kind} is a TypeTableField");
	}

	// Every function-like node exposes its parameter data.
	let mut a = Ast::new();
	let (params, body) = (
		a.new_list::<element::AnyIdentifier>(),
		a.new_list::<element::Statement>(),
	);
	let expression = a.function_expression(params, false, body);
	let (name, params, body) = (
		a.id("f"),
		a.new_list::<element::AnyIdentifier>(),
		a.new_list::<element::Statement>(),
	);
	let declaration = a.function_declaration(true, name, params, false, body);
	let (receiver, params, body) = (
		a.id("C"),
		a.new_list::<element::AnyIdentifier>(),
		a.new_list::<element::Statement>(),
	);
	let method = a.method_declaration(receiver, "m", params, false, body);
	let (params, body) = (
		a.new_list::<element::AnyIdentifier>(),
		a.new_list::<element::Statement>(),
	);
	let type_function = a.type_function_declaration("F", params, false, body);
	for node in [expression, declaration, method, type_function] {
		assert!(a.param_data(node).is_some(), "{} has parameters", a.kind(node));
	}
}

// Type nodes are neither statements nor expressions: the renderer indents
// statements, and the temp solver reserves identifier names as values.
#[test]
fn type_nodes_are_not_statements_or_expressions() {
	let mut a = Ast::new();
	let name = a.type_name("x");
	let value = a.type_name("x");
	let prop = a.type_table_prop(TypeAccess::None, "a", value);
	for node in [name, prop] {
		let kind = a.kind(node);
		assert!(
			!kind.is_statement() && !kind.is_expression() && !kind.is_field(),
			"{kind} must only be a type node"
		);
	}
}

#[test]
fn kinds() {
	let mut a = Ast::new();
	let identifier = NodeData::from(Identifier {
		name: String::new(),
		annotation: None,
	});
	assert_eq!(identifier.kind(), SyntaxKind::Identifier, "Identifier kind");
	let part = NodeData::from(InterpolatedStringPart { text: String::new() });
	assert_eq!(
		part.kind(),
		SyntaxKind::InterpolatedStringPart,
		"InterpolatedStringPart kind"
	);
	let (params, body) = (
		a.new_list::<element::AnyIdentifier>(),
		a.new_list::<element::Statement>(),
	);
	let declaration = NodeData::from(TypeFunctionDeclaration {
		export: false,
		name: String::new(),
		parameters: params,
		has_dot_dot_dot: false,
		statements: body,
	});
	let (expression, ty) = (a.none(), a.none());
	let cast = NodeData::from(CastExpression { expression, ty });
	assert!(
		declaration.kind() == SyntaxKind::TypeFunctionDeclaration && cast.kind() == SyntaxKind::CastExpression,
		"rotor extension kinds"
	);
}
