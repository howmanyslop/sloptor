//! Port of `internal/luau/list_test.go`.

use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::{Ast, Identifier, List, NodeId, element};

fn id(a: &mut Ast, name: &str) -> NodeId {
	a.id(name)
}

fn values<T>(a: &Ast, list: List<T>) -> Vec<String> {
	a.list_iter(list)
		.map(|node| a.get::<Identifier>(node).expect("identifier").name.clone())
		.collect()
}

fn list(a: &mut Ast, names: &[&str]) -> List<element::AnyIdentifier> {
	let nodes: Vec<NodeId> = names.iter().map(|name| id(a, name)).collect();
	a.list_from(nodes)
}

#[test]
fn list_push_shift_unshift() {
	let mut a = Ast::new();
	let l = a.new_list::<element::AnyIdentifier>();
	assert!(a.list_is_empty(l), "new list not empty");
	let (two, three, one) = (id(&mut a, "2"), id(&mut a, "3"), id(&mut a, "1"));
	a.list_push(l, two);
	a.list_push(l, three);
	a.list_unshift(l, one);
	assert_eq!(values(&a, l), ["1", "2", "3"]);
	assert_eq!(a.list_size(l), 3);
	let first = a.list_shift(l);
	assert_eq!(first, Some(one), "shift");
	assert_eq!(values(&a, l), ["2", "3"], "after shift");
}

#[test]
fn list_push_list_marks_readonly() {
	let mut a = Ast::new();
	let first = list(&mut a, &["1", "2"]);
	let second = list(&mut a, &["3", "4"]);
	a.list_push_list(first, second);
	assert_eq!(values(&a, first), ["1", "2", "3", "4"]);
	assert!(
		a.list_is_read_only(second),
		"source list must be marked readonly after PushList"
	);
	let five = id(&mut a, "5");
	let pushed = catch_unwind(AssertUnwindSafe(|| a.list_push(second, five)));
	assert!(pushed.is_err(), "pushing to readonly list must panic");
}

#[test]
fn list_unshift_list() {
	let mut a = Ast::new();
	let first = list(&mut a, &["3", "4"]);
	let second = list(&mut a, &["1", "2"]);
	a.list_unshift_list(first, second);
	assert_eq!(values(&a, first), ["1", "2", "3", "4"]);
}

#[test]
fn list_splice_empty_source() {
	// Splicing in an empty list is a no-op, but the source must still be
	// marked readonly.
	let mut a = Ast::new();
	let first = list(&mut a, &["1", "2"]);
	let empty_push = a.new_list::<element::AnyIdentifier>();
	a.list_push_list(first, empty_push);
	assert_eq!(values(&a, first), ["1", "2"], "after PushList(empty)");
	assert!(
		a.list_is_read_only(empty_push),
		"empty source must be marked readonly after PushList"
	);

	let second = list(&mut a, &["1", "2"]);
	let empty_unshift = a.new_list::<element::AnyIdentifier>();
	a.list_unshift_list(second, empty_unshift);
	assert_eq!(values(&a, second), ["1", "2"], "after UnshiftList(empty)");
	assert!(
		a.list_is_read_only(empty_unshift),
		"empty source must be marked readonly after UnshiftList"
	);
}

#[test]
fn list_splice_onto_empty_destination() {
	let mut a = Ast::new();
	let destination = a.new_list::<element::AnyIdentifier>();
	let source = list(&mut a, &["1", "2"]);
	a.list_push_list(destination, source);
	assert_eq!(values(&a, destination), ["1", "2"], "PushList onto empty");
	let tail = a.list_tail(destination).map(|link| a.link_value(link));
	assert_eq!(
		tail.and_then(|tail| a.get::<Identifier>(tail))
			.map(|id| id.name.as_str()),
		Some("2"),
		"PushList onto empty must set Tail"
	);

	let destination = a.new_list::<element::AnyIdentifier>();
	let source = list(&mut a, &["1", "2"]);
	a.list_unshift_list(destination, source);
	assert_eq!(values(&a, destination), ["1", "2"], "UnshiftList onto empty");
	let tail = a.list_tail(destination).map(|link| a.link_value(link));
	assert_eq!(
		tail.and_then(|tail| a.get::<Identifier>(tail))
			.map(|id| id.name.as_str()),
		Some("2"),
		"UnshiftList onto empty must set Tail"
	);
}

#[test]
fn list_some_every() {
	let mut a = Ast::new();
	let l = list(&mut a, &["1", "2", "3"]);
	let name = |node| a.get::<Identifier>(node).expect("identifier").name.clone();
	assert!(a.list_some(l, |node| name(node) == "2"), "Some failed");
	assert!(!a.list_every(l, |node| name(node).as_str() < "3"), "Every failed");
}

/// `addFinalizers` (`transformForStatement.ts`) splices before the head and before a middle
/// link; the spliced list's head keeps no back link, as upstream leaves it.
#[test]
fn list_splice_before_head_and_middle() {
	let mut a = Ast::new();
	let l = list(&mut a, &["a", "b"]);
	let head = a.list_head(l).expect("a head");
	let front = list(&mut a, &["x", "y"]);
	a.list_splice_before(l, head, front);
	assert_eq!(values(&a, l), ["x", "y", "a", "b"], "before the head");
	assert_eq!(
		a.link_prev(head),
		a.list_tail(front),
		"the link points back at the spliced tail"
	);

	let tail = a.list_tail(l).expect("a tail");
	let middle = list(&mut a, &["m"]);
	a.list_splice_before(l, tail, middle);
	assert_eq!(values(&a, l), ["x", "y", "a", "m", "b"], "before a middle link");
	let spliced_head = a.list_head(middle).expect("a head");
	assert_eq!(
		a.link_prev(spliced_head),
		None,
		"upstream leaves the back link dangling"
	);

	let empty = a.new_list::<element::AnyIdentifier>();
	a.list_splice_before(l, tail, empty);
	assert_eq!(values(&a, l), ["x", "y", "a", "m", "b"], "an empty splice is a no-op");
}
