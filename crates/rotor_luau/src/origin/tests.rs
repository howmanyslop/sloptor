//! Port of `internal/luau/origin_test.go`.

use crate::{
	AssignmentOperator, Ast, CallExpression, NodeId, Origin, Position, PropertyAccessExpression, TemporaryIdentifier,
	element,
};

#[test]
fn origin_survives_cloning() {
	type Clone = fn(&mut Ast, NodeId) -> NodeId;
	let tests: [(&str, Clone); 3] = [
		("adopt", |a, node| {
			let access = a.property_access(node, "second");
			a.get::<PropertyAccessExpression>(access).expect("access").expression
		}),
		("adopt list", |a, node| {
			let callee = a.id("use");
			let args = a.list_from::<element::Expression>([node]);
			let call = a.call(callee, args);
			let args = a.get::<CallExpression>(call).expect("call").args;
			a.link_value(a.list_head(args).expect("head"))
		}),
		("list clone", |a, node| {
			let list = a.list_from::<element::Expression>([node]);
			let clone = a.list_clone(list);
			a.link_value(a.list_head(clone).expect("head"))
		}),
	];
	for (name, clone) in tests {
		let mut a = Ast::new();
		let node = a.id("value");
		assert_eq!(a.origin(node), None, "{name}: new node origin");
		let origin = a.add_origin(Origin {
			start: Position::new(2, 3),
			closing: Some(Position::new(5, 1)),
			..Origin::default()
		});
		a.set_origin(node, origin);
		let first_parent = a.property_access(node, "first");
		let cloned = clone(&mut a, node);
		assert_ne!(cloned, node, "{name}: expected a cloned node");
		assert_eq!(
			a.parent(node),
			Some(first_parent),
			"{name}: cloning changed the original node's parent"
		);
		assert_eq!(a.origin(cloned), Some(origin), "{name}: clone must share the origin");
		let later = a.add_origin(Origin {
			start: Position::new(9, 0),
			..Origin::default()
		});
		a.set_origin(cloned, later);
		assert!(
			a.origin(node) == Some(origin) && a.origin(cloned) == Some(origin),
			"{name}: setting an origin must preserve an existing origin"
		);
	}
}

#[test]
fn with_origin_clones_without_parent() {
	let mut a = Ast::new();
	let id = a.temp_id("value");
	let parent = a.return_statement(id);
	let origin = a.add_origin(Origin {
		start: Position::new(2, 4),
		..Origin::default()
	});
	let clone = a.with_origin(id, Some(origin));
	let temp_id = |a: &Ast, node| a.get::<TemporaryIdentifier>(node).expect("temp").id;
	assert!(
		clone != id
			&& temp_id(&a, clone) == temp_id(&a, id)
			&& a.parent(clone).is_none()
			&& a.origin(clone) == Some(origin),
		"invalid occurrence clone"
	);
	assert!(
		a.parent(id) == Some(parent) && a.origin(id).is_none(),
		"cloning mutated the original node"
	);
}

#[test]
fn origin_fields_fill_independently() {
	let mut a = Ast::new();
	let (x, one) = (a.id("x"), a.num(1.0));
	let node = a.assignment(x, AssignmentOperator::Add, one);
	let operator = Position::new(0, 2);
	let partial = a.add_origin(Origin {
		operator: Some(operator),
		start_unset: true,
		..Origin::default()
	});
	a.set_origin(node, partial);
	let closing = Position::new(0, 6);
	let with_closing = a.add_origin(Origin {
		start: Position::default(),
		closing: Some(closing),
		..Origin::default()
	});
	a.set_origin(node, with_closing);
	let later = a.add_origin(Origin {
		start: Position::new(9, 0),
		operator: Some(Position::new(9, 0)),
		..Origin::default()
	});
	a.set_origin(node, later);
	let got = *a.origin_data(a.origin(node).expect("origin"));
	assert!(
		!got.start_unset
			&& got.start == Position::default()
			&& got.closing == Some(closing)
			&& got.operator == Some(operator),
		"per-field anchors = {got:?}; want start 0:0 and original operator/closing"
	);
	let partial = a.origin_data(partial);
	assert!(
		partial.start_unset && partial.closing.is_none(),
		"filling mutated a shared partial origin"
	);
}
