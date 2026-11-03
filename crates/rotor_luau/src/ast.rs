//! The per-file node arena.

use crate::list::{ListHeader, ListLink};
use crate::nodes::Payload;
use crate::{AnyList, List, ListId, NodeData, NodeId, NodeOrList, Origin, OriginId, SyntaxKind, TempId};

/// One file's Luau AST: every node, list, and origin, addressed by `u32`
/// indices.
///
/// Node columns are stored struct-of-arrays: kind-specific data, the parent,
/// and the origin each live in their own vector indexed by [`NodeId`].
///
/// Construction keeps the Go semantics: the tree is a DAG while it is built,
/// and attaching a node that already has a parent attaches a shallow clone
/// instead. Clones share child IDs, list handles, and origins with the
/// original.
///
/// Two differences from Go matter to callers:
///
/// - Payload `Vec`s (generics, and a type function's parameter list) are
///   copied on clone, where Go clones share the slice. Editing a clone's
///   generics does not change the original.
/// - Temporary IDs ([`TempId`]) are numbered per `Ast`, not process-wide. A
///   [`RenderState`](crate::render::RenderState) names temporaries by ID, so
///   never reuse one across `Ast`s.
#[derive(Clone, Debug, Default)]
pub struct Ast {
	data: Vec<NodeData>,
	pub(crate) parents: Vec<Option<NodeId>>,
	pub(crate) origins: Vec<Option<OriginId>>,
	pub(crate) origin_table: Vec<Origin>,
	pub(crate) lists: Vec<ListHeader>,
	pub(crate) links: Vec<ListLink>,
	temp_ids: usize,
}

impl Ast {
	/// Creates an empty arena.
	#[must_use]
	pub fn new() -> Self {
		Self::default()
	}

	/// Stores a node without adopting its children: the Go struct literal.
	/// Constructors such as [`Ast::binary`] adopt their children instead.
	pub fn alloc(&mut self, data: impl Into<NodeData>) -> NodeId {
		let id = NodeId::from_index(self.data.len());
		self.data.push(data.into());
		self.parents.push(None);
		self.origins.push(None);
		id
	}

	/// Reserves a slot so a constructor knows its own ID before adopting its
	/// children; [`Ast::fill`] stores the real data.
	pub(crate) fn reserve(&mut self) -> NodeId {
		self.alloc(NodeData::None)
	}

	pub(crate) fn fill(&mut self, node: NodeId, data: impl Into<NodeData>) -> NodeId {
		self.data[node.index()] = data.into();
		node
	}

	/// How many nodes the arena holds. Node indices are dense below this.
	#[must_use]
	pub const fn node_count(&self) -> usize {
		self.data.len()
	}

	/// How many temporary IDs have been issued. Temp indices are dense below
	/// this.
	#[must_use]
	pub const fn temp_id_count(&self) -> usize {
		self.temp_ids
	}

	pub(crate) fn next_temp_id(&mut self) -> TempId {
		let id = TempId::from_index(self.temp_ids);
		self.temp_ids += 1;
		id
	}

	/// The node's kind and fields.
	#[must_use]
	pub fn data(&self, node: NodeId) -> &NodeData {
		&self.data[node.index()]
	}

	/// The node's kind and fields, mutably. Assigning a child here does not
	/// adopt it, like assigning a Go struct field.
	pub fn data_mut(&mut self, node: NodeId) -> &mut NodeData {
		&mut self.data[node.index()]
	}

	/// The node's payload, when it is of kind `T`.
	#[must_use]
	pub fn get<T: Payload>(&self, node: NodeId) -> Option<&T> {
		T::from_data(self.data(node))
	}

	/// The node's payload, mutably, when it is of kind `T`.
	pub fn get_mut<T: Payload>(&mut self, node: NodeId) -> Option<&mut T> {
		T::from_data_mut(self.data_mut(node))
	}

	/// The node's kind.
	#[must_use]
	pub fn kind(&self, node: NodeId) -> SyntaxKind {
		self.data(node).kind()
	}

	/// The node's parent.
	#[must_use]
	pub fn parent(&self, node: NodeId) -> Option<NodeId> {
		self.parents[node.index()]
	}

	/// Reparents `node` without cloning. Upstream transform code mutates
	/// `.parent` directly in places (for example the finalizer clones that
	/// `transformForStatement` splices before `continue`); this is the
	/// surface for those fixups.
	pub fn set_parent(&mut self, node: NodeId, parent: Option<NodeId>) {
		self.parents[node.index()] = parent;
	}

	/// Copies the node into a new slot. The copy shares children, lists,
	/// origin, and parent with the original.
	pub(crate) fn shallow_clone(&mut self, node: NodeId) -> NodeId {
		let data = self.data(node).clone();
		let clone = self.alloc(data);
		self.parents[clone.index()] = self.parents[node.index()];
		self.origins[clone.index()] = self.origins[node.index()];
		clone
	}

	/// Mirrors upstream `create()`: sets `child`'s parent, cloning it first
	/// when it already has one. Returns the node that was attached.
	pub(crate) fn adopt(&mut self, parent: NodeId, child: NodeId) -> NodeId {
		let child = if self.parents[child.index()].is_some() {
			self.shallow_clone(child)
		} else {
			child
		};
		self.parents[child.index()] = Some(parent);
		child
	}

	/// Adopts every value of `list` in place: a value that already has a
	/// parent is replaced by a clone in its link.
	pub(crate) fn adopt_list<T>(&mut self, parent: NodeId, list: List<T>) -> List<T> {
		self.adopt_links(parent, list.id());
		list
	}

	fn adopt_links(&mut self, parent: NodeId, list: ListId) {
		let mut next = self.header(list).head;
		while let Some(link) = next {
			let value = self.link_value(link);
			let adopted = self.adopt(parent, value);
			if adopted != value {
				self.set_link_value(link, adopted);
			}
			next = self.link_next(link);
		}
	}

	pub(crate) fn adopt_any_list(&mut self, parent: NodeId, list: AnyList) -> AnyList {
		self.adopt_links(parent, list.id);
		list
	}

	/// Adopts a union field.
	pub(crate) fn adopt_node_or_list(&mut self, parent: NodeId, value: NodeOrList) -> NodeOrList {
		match value {
			NodeOrList::Node(node) => NodeOrList::Node(self.adopt(parent, node)),
			NodeOrList::List(list) => NodeOrList::List(self.adopt_any_list(parent, list)),
		}
	}
}
