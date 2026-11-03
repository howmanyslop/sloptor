//! Global names.

use crate::{Ast, NodeId};

/// Exactly the top-level keys of upstream `globals.ts`
/// (`LuauAST/impl/globals.ts`). [`is_reserved_identifier`](crate::is_reserved_identifier)
/// depends on it.
pub const RESERVED_GLOBAL_NAMES: &[&str] = &[
	"_G",
	"TS",
	"assert",
	"bit32",
	"coroutine",
	"error",
	"exports",
	"getmetatable",
	"ipairs",
	"next",
	"pairs",
	"pcall",
	"require",
	"script",
	"select",
	"self",
	"setmetatable",
	"string",
	"super",
	"table",
	"utf8",
	"math",
	"tostring",
	"type",
	"typeof",
	"unpack",
	"game",
];

impl Ast {
	/// A global identifier.
	pub fn global_id(&mut self, name: impl Into<String>) -> NodeId {
		self.id(name)
	}

	/// `object.name` on a global.
	pub fn global_property(&mut self, object: impl Into<String>, name: impl Into<String>) -> NodeId {
		let object = self.id(object);
		self.property_access(object, name)
	}
}
