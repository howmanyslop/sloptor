//! Port of `internal/luau/create_test.go`.

use crate::{Ast, BinaryOperator, Identifier, NodeData, TemporaryIdentifier, UnaryOperator, element};

#[test]
fn adopt_sets_parent() {
	let mut a = Ast::new();
	let (left, right) = (a.id("a"), a.id("b"));
	let binary = a.binary(left, BinaryOperator::Add, right);
	assert!(
		a.parent(left) == Some(binary) && a.parent(right) == Some(binary),
		"children must be adopted"
	);
}

#[test]
fn adopt_clones_when_already_parented() {
	let mut a = Ast::new();
	let shared = a.id("x");
	let y = a.id("y");
	let first = a.binary(shared, BinaryOperator::Add, y);
	let z = a.id("z");
	let second = a.binary(shared, BinaryOperator::Subtract, z);
	let NodeData::BinaryExpression(first_data) = a.data(first) else {
		panic!("first is binary")
	};
	assert_eq!(first_data.left, shared, "first use must keep the original node");
	let NodeData::BinaryExpression(second_data) = a.data(second) else {
		panic!("second is binary")
	};
	let clone = second_data.left;
	assert_ne!(clone, shared, "second use must be a clone, not the original");
	assert_eq!(
		a.get::<Identifier>(clone).map(|id| id.name.as_str()),
		Some("x"),
		"clone must preserve fields"
	);
	assert_eq!(a.parent(clone), Some(second), "clone must be adopted by the new parent");
	assert_eq!(
		a.parent(shared),
		Some(first),
		"original node's parent must be untouched"
	);
}

#[test]
fn list_adoption() {
	let mut a = Ast::new();
	let (x, y) = (a.id("a"), a.id("b"));
	let list = a.list_from::<element::Expression>([x, y]);
	let array = a.array(list);
	assert!(
		a.parent(x) == Some(array) && a.parent(y) == Some(array),
		"list elements must be adopted"
	);
	// Reuse x in another list: the link's value gets replaced by a clone.
	let l2 = a.list_from::<element::Expression>([x]);
	let array2 = a.array(l2);
	let head = a.link_value(a.list_head(l2).expect("head"));
	assert_ne!(head, x, "already-parented list element must be cloned");
	assert_eq!(a.parent(head), Some(array2), "cloned element must be adopted");
}

#[test]
fn num_handles_negatives() {
	let mut a = Ast::new();
	let five = a.num(5.0);
	assert!(
		matches!(a.data(five), NodeData::NumberLiteral(literal) if literal.value == "5"),
		"num(5) = {:?}",
		a.data(five)
	);
	let negative = a.num(-3.0);
	let NodeData::UnaryExpression(unary) = a.data(negative) else {
		panic!("num(-3) = {:?}", a.data(negative))
	};
	assert_eq!(unary.operator, UnaryOperator::Negate);
	assert!(
		matches!(a.data(unary.expression), NodeData::NumberLiteral(literal) if literal.value == "3"),
		"inner literal {:?}",
		a.data(unary.expression)
	);
}

#[test]
fn temp_ids_unique() {
	let mut a = Ast::new();
	let (first, second) = (a.temp_id(""), a.temp_id("foo"));
	let first = a.get::<TemporaryIdentifier>(first).expect("temp").clone();
	let second = a.get::<TemporaryIdentifier>(second).expect("temp").clone();
	assert_ne!(first.id, second.id, "temp ids must be unique");
	assert_eq!(second.name, "foo", "temp name");
}
