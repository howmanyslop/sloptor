//! Arena-allocated doubly linked lists with O(1) splicing.
//!
//! This is the Go `List`/`ListNode` pair: a list is a header with head and tail
//! links, and its links live in the [`Ast`]'s link arena. Every walk follows
//! `next` from the head until it runs out, as the Go loops do, so a source list
//! spliced by [`Ast::list_push_list`] shares its links with the destination.

use core::fmt;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;

use crate::{Ast, ListId, ListNodeId, NodeId};

/// Element markers for [`List`]. They are the Go type parameters of
/// `List[T]`, and the renderer dispatches on them the way Go type-switches on
/// `*List[T]`.
pub mod element {
	/// `List[luau.Expression]`
	#[derive(Debug)]
	pub enum Expression {}
	/// `List[luau.Statement]`
	#[derive(Debug)]
	pub enum Statement {}
	/// `List[luau.AnyIdentifier]`
	#[derive(Debug)]
	pub enum AnyIdentifier {}
	/// `List[luau.WritableExpression]`
	#[derive(Debug)]
	pub enum WritableExpression {}
	/// `List[luau.Node]`
	#[derive(Debug)]
	pub enum Node {}
	/// `List[*luau.MapField]`
	#[derive(Debug)]
	pub enum MapField {}
	/// `List[luau.TypeNode]`
	#[derive(Debug)]
	pub enum TypeNode {}
	/// `List[luau.TypeTableField]`
	#[derive(Debug)]
	pub enum TypeTableField {}
}

/// The element type of a list, known at run time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ListKind {
	/// [`element::Expression`]
	Expression,
	/// [`element::Statement`]
	Statement,
	/// [`element::AnyIdentifier`]
	AnyIdentifier,
	/// [`element::WritableExpression`]
	WritableExpression,
	/// [`element::Node`]
	Node,
	/// [`element::MapField`]
	MapField,
	/// [`element::TypeNode`]
	TypeNode,
	/// [`element::TypeTableField`]
	TypeTableField,
}

mod sealed {
	pub trait Sealed {}
}

/// An element marker for [`List`].
pub trait ListElement: sealed::Sealed {
	/// The run-time tag of this element type.
	const KIND: ListKind;
}

macro_rules! list_elements {
	($($marker:ident),*) => {
		$(
			impl sealed::Sealed for element::$marker {}
			impl ListElement for element::$marker {
				const KIND: ListKind = ListKind::$marker;
			}
		)*
	};
}

list_elements!(
	Expression,
	Statement,
	AnyIdentifier,
	WritableExpression,
	Node,
	MapField,
	TypeNode,
	TypeTableField
);

/// A typed handle to a list in an [`Ast`]. Copying the handle shares the
/// list, like copying a Go `*List`.
pub struct List<T> {
	id: ListId,
	marker: PhantomData<fn() -> T>,
}

impl<T> List<T> {
	const fn new(id: ListId) -> Self {
		Self {
			id,
			marker: PhantomData,
		}
	}

	/// The list's arena index.
	#[must_use]
	pub const fn id(self) -> ListId {
		self.id
	}
}

impl<T: ListElement> List<T> {
	/// Erases the element type into a run-time tag.
	#[must_use]
	pub const fn erase(self) -> AnyList {
		AnyList {
			id: self.id,
			kind: T::KIND,
		}
	}
}

impl<T> Clone for List<T> {
	fn clone(&self) -> Self {
		*self
	}
}

impl<T> Copy for List<T> {}

impl<T> PartialEq for List<T> {
	fn eq(&self, other: &Self) -> bool {
		self.id == other.id
	}
}

impl<T> Eq for List<T> {}

impl<T> Hash for List<T> {
	fn hash<H: Hasher>(&self, state: &mut H) {
		self.id.hash(state);
	}
}

impl<T> fmt::Debug for List<T> {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.debug_tuple("List").field(&self.id).finish()
	}
}

/// A list whose element type is a run-time tag: the Go `AnyList`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AnyList {
	/// The list's arena index.
	pub id: ListId,
	/// The element type.
	pub kind: ListKind,
}

impl AnyList {
	/// Recovers the typed handle when the element type matches.
	#[must_use]
	pub fn typed<T: ListElement>(self) -> Option<List<T>> {
		(self.kind == T::KIND).then(|| List::new(self.id))
	}
}

/// A field that holds either one node or a list: the Go `NodeOrList` union,
/// as in `luau.X | luau.List<luau.Y>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeOrList {
	/// A single node.
	Node(NodeId),
	/// A list.
	List(AnyList),
}

impl NodeOrList {
	/// The node, when this is not a list.
	#[must_use]
	pub const fn node(self) -> Option<NodeId> {
		match self {
			Self::Node(node) => Some(node),
			Self::List(_) => None,
		}
	}

	/// The list, when this is one.
	#[must_use]
	pub const fn list(self) -> Option<AnyList> {
		match self {
			Self::Node(_) => None,
			Self::List(list) => Some(list),
		}
	}

	/// Whether this is a list (Go `IsList`).
	#[must_use]
	pub const fn is_list(self) -> bool {
		matches!(self, Self::List(_))
	}
}

impl From<NodeId> for NodeOrList {
	fn from(node: NodeId) -> Self {
		Self::Node(node)
	}
}

impl<T: ListElement> From<List<T>> for NodeOrList {
	fn from(list: List<T>) -> Self {
		Self::List(list.erase())
	}
}

impl From<AnyList> for NodeOrList {
	fn from(list: AnyList) -> Self {
		Self::List(list)
	}
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ListHeader {
	pub(crate) head: Option<ListNodeId>,
	pub(crate) tail: Option<ListNodeId>,
	pub(crate) read_only: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ListLink {
	pub(crate) prev: Option<ListNodeId>,
	pub(crate) next: Option<ListNodeId>,
	pub(crate) value: NodeId,
}

/// Iterates a list's values by following `next` links from the head.
#[derive(Clone, Debug)]
pub struct ListIter<'ast> {
	ast: &'ast Ast,
	next: Option<ListNodeId>,
}

impl Iterator for ListIter<'_> {
	type Item = NodeId;

	fn next(&mut self) -> Option<NodeId> {
		let link = self.ast.links[self.next?.index()];
		self.next = link.next;
		Some(link.value)
	}
}

/// Iterates a list's links by following `next` from the head.
#[derive(Clone, Debug)]
pub struct ListLinkIter<'ast> {
	ast: &'ast Ast,
	next: Option<ListNodeId>,
}

impl Iterator for ListLinkIter<'_> {
	type Item = ListNodeId;

	fn next(&mut self) -> Option<ListNodeId> {
		let current = self.next?;
		self.next = self.ast.links[current.index()].next;
		Some(current)
	}
}

impl Ast {
	/// Creates an empty list (Go `NewList[T]()`).
	pub fn new_list<T: ListElement>(&mut self) -> List<T> {
		let id = ListId::from_index(self.lists.len());
		self.lists.push(ListHeader::default());
		List::new(id)
	}

	/// Creates a list of `values` (Go `NewList[T](values...)`). Values are not
	/// adopted until the list is passed to a constructor.
	pub fn list_from<T: ListElement>(&mut self, values: impl IntoIterator<Item = NodeId>) -> List<T> {
		let list = self.new_list();
		for value in values {
			self.list_push(list, value);
		}
		list
	}

	pub(crate) fn header(&self, list: ListId) -> &ListHeader {
		&self.lists[list.index()]
	}

	fn new_link(&mut self, value: NodeId) -> ListNodeId {
		let id = ListNodeId::from_index(self.links.len());
		self.links.push(ListLink {
			prev: None,
			next: None,
			value,
		});
		id
	}

	fn assert_writable(&self, list: ListId) {
		assert!(!self.lists[list.index()].read_only, "list is readonly");
	}

	/// Appends `value`.
	///
	/// # Panics
	///
	/// When the list is read-only, like the Go `assertWritable`.
	pub fn list_push<T>(&mut self, list: List<T>, value: NodeId) {
		self.assert_writable(list.id);
		let link = self.new_link(value);
		let header = self.lists[list.id.index()];
		if let Some(tail) = header.tail {
			self.links[tail.index()].next = Some(link);
			self.links[link.index()].prev = Some(tail);
		} else {
			self.lists[list.id.index()].head = Some(link);
		}
		self.lists[list.id.index()].tail = Some(link);
	}

	/// Splices `other` onto the end in O(1) and marks `other` read-only. Both
	/// lists then share `other`'s links.
	///
	/// # Panics
	///
	/// When either list is read-only.
	pub fn list_push_list<T>(&mut self, list: List<T>, other: List<T>) {
		self.assert_writable(list.id);
		self.assert_writable(other.id);
		self.lists[other.id.index()].read_only = true;
		let source = self.lists[other.id.index()];
		let (Some(other_head), Some(other_tail)) = (source.head, source.tail) else {
			return;
		};
		let destination = self.lists[list.id.index()];
		if let (Some(_), Some(tail)) = (destination.head, destination.tail) {
			self.links[tail.index()].next = Some(other_head);
			self.links[other_head.index()].prev = Some(tail);
		} else {
			self.lists[list.id.index()].head = Some(other_head);
		}
		self.lists[list.id.index()].tail = Some(other_tail);
	}

	/// Removes and returns the first value.
	///
	/// # Panics
	///
	/// When the list is read-only.
	pub fn list_shift<T>(&mut self, list: List<T>) -> Option<NodeId> {
		self.assert_writable(list.id);
		let head = self.lists[list.id.index()].head?;
		let link = self.links[head.index()];
		if let Some(next) = link.next {
			self.lists[list.id.index()].head = Some(next);
			self.links[next.index()].prev = None;
		} else {
			self.lists[list.id.index()].head = None;
			self.lists[list.id.index()].tail = None;
		}
		Some(link.value)
	}

	/// Prepends `value`.
	///
	/// # Panics
	///
	/// When the list is read-only.
	pub fn list_unshift<T>(&mut self, list: List<T>, value: NodeId) {
		self.assert_writable(list.id);
		let link = self.new_link(value);
		if let Some(head) = self.lists[list.id.index()].head {
			self.links[head.index()].prev = Some(link);
			self.links[link.index()].next = Some(head);
		} else {
			self.lists[list.id.index()].tail = Some(link);
		}
		self.lists[list.id.index()].head = Some(link);
	}

	/// Splices `other` onto the front in O(1) and marks `other` read-only.
	///
	/// # Panics
	///
	/// When either list is read-only.
	pub fn list_unshift_list<T>(&mut self, list: List<T>, other: List<T>) {
		self.assert_writable(list.id);
		self.assert_writable(other.id);
		self.lists[other.id.index()].read_only = true;
		let source = self.lists[other.id.index()];
		let (Some(other_head), Some(other_tail)) = (source.head, source.tail) else {
			return;
		};
		if let Some(head) = self.lists[list.id.index()].head {
			self.links[head.index()].prev = Some(other_tail);
			self.links[other_tail.index()].next = Some(head);
			self.lists[list.id.index()].head = Some(other_head);
		} else {
			self.lists[list.id.index()].head = Some(other_head);
			self.lists[list.id.index()].tail = Some(other_tail);
		}
	}

	/// Whether the list has no values.
	#[must_use]
	pub fn list_is_empty<T>(&self, list: List<T>) -> bool {
		self.header(list.id).head.is_none()
	}

	/// Whether the list has a value.
	#[must_use]
	pub fn list_is_non_empty<T>(&self, list: List<T>) -> bool {
		self.header(list.id).head.is_some()
	}

	/// Whether the list was spliced into another and may no longer change.
	#[must_use]
	pub fn list_is_read_only<T>(&self, list: List<T>) -> bool {
		self.header(list.id).read_only
	}

	/// The first link.
	#[must_use]
	pub fn list_head<T>(&self, list: List<T>) -> Option<ListNodeId> {
		self.header(list.id).head
	}

	/// The last link.
	#[must_use]
	pub fn list_tail<T>(&self, list: List<T>) -> Option<ListNodeId> {
		self.header(list.id).tail
	}

	/// The value stored in `link`.
	#[must_use]
	pub fn link_value(&self, link: ListNodeId) -> NodeId {
		self.links[link.index()].value
	}

	/// The link after `link`.
	#[must_use]
	pub fn link_next(&self, link: ListNodeId) -> Option<ListNodeId> {
		self.links[link.index()].next
	}

	/// The link before `link`.
	#[must_use]
	pub fn link_prev(&self, link: ListNodeId) -> Option<ListNodeId> {
		self.links[link.index()].prev
	}

	pub(crate) fn set_link_value(&mut self, link: ListNodeId, value: NodeId) {
		self.links[link.index()].value = value;
	}

	/// Iterates the list's values (Go `ForEach`).
	#[must_use]
	pub fn list_iter<T>(&self, list: List<T>) -> ListIter<'_> {
		self.any_list_iter(list.id)
	}

	pub(crate) fn any_list_iter(&self, list: ListId) -> ListIter<'_> {
		ListIter {
			ast: self,
			next: self.header(list).head,
		}
	}

	/// Iterates the list's links (Go `ForEachNode`).
	#[must_use]
	pub fn list_links<T>(&self, list: List<T>) -> ListLinkIter<'_> {
		self.any_list_links(list.id)
	}

	pub(crate) fn any_list_links(&self, list: ListId) -> ListLinkIter<'_> {
		ListLinkIter {
			ast: self,
			next: self.header(list).head,
		}
	}

	/// Collects the list's values (Go `ToSlice`).
	#[must_use]
	pub fn list_to_vec<T>(&self, list: List<T>) -> Vec<NodeId> {
		self.list_iter(list).collect()
	}

	/// Whether any value satisfies `predicate`.
	pub fn list_some<T>(&self, list: List<T>, mut predicate: impl FnMut(NodeId) -> bool) -> bool {
		self.list_iter(list).any(&mut predicate)
	}

	/// Whether every value satisfies `predicate`.
	pub fn list_every<T>(&self, list: List<T>, mut predicate: impl FnMut(NodeId) -> bool) -> bool {
		self.list_iter(list).all(&mut predicate)
	}

	/// Counts the values.
	#[must_use]
	pub fn list_size<T>(&self, list: List<T>) -> usize {
		self.list_iter(list).count()
	}

	/// Makes a new list of shallow-cloned values (upstream `list.clone`).
	pub fn list_clone<T: ListElement>(&mut self, list: List<T>) -> List<T> {
		let values = self.list_to_vec(list);
		let out = self.new_list();
		for value in values {
			let clone = self.shallow_clone(value);
			self.list_push(out, clone);
		}
		out
	}
}

#[cfg(test)]
mod tests;
