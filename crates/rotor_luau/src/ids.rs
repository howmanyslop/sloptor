//! Typed `u32` indices into the [`Ast`](crate::Ast) arenas.

use core::num::NonZeroU32;

macro_rules! arena_id {
	($(#[$meta:meta])* $name:ident) => {
		$(#[$meta])*
		#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
		pub struct $name(NonZeroU32);

		impl $name {
			/// Names the arena slot at `index`.
			pub(crate) fn from_index(index: usize) -> Self {
				match u32::try_from(index).ok().and_then(|value| value.checked_add(1)).and_then(NonZeroU32::new) {
					Some(raw) => Self(raw),
					None => arena_overflow(),
				}
			}

			/// The arena slot this index names.
			#[must_use]
			pub const fn index(self) -> usize {
				(self.0.get() - 1) as usize
			}
		}
	};
}

#[cold]
fn arena_overflow() -> ! {
	unreachable!("an Ast arena outgrew u32 indices")
}

arena_id! {
	/// A node in an [`Ast`](crate::Ast). Shallow clones get a new `NodeId`, so
	/// equality is the Go port's pointer identity.
	NodeId
}

arena_id! {
	/// A list header. Lists are shared by handle: a shallow clone of a node
	/// shares its lists, exactly like the Go `*List` pointers.
	ListId
}

arena_id! {
	/// One link of a list, the Go `*ListNode`.
	ListNodeId
}

arena_id! {
	/// An entry in the origin table. Shallow clones share the same `OriginId`.
	OriginId
}

arena_id! {
	/// The rendering identity of a temporary identifier. Clones keep it, so
	/// every occurrence renders the same name.
	TempId
}
