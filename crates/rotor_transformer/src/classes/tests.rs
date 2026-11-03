//! `classes_test.go`, `enums_test.go` and `namespaces_test.go`, with the expected strings
//! verbatim. The expected text is byte for byte what rbxtsc 3.0.0 emits for the same source
//! (header and trailing export return stripped: those belong to `TransformSourceFile`, not the
//! statement list under test).

use rotor_tsrs::ast::{self as tsast, Kind};

use crate::prelude::*;
use crate::statementlist::transform_statement_list;
use crate::tests::{render_list, transformer_testdata, with_state};

/// Go's `renderClassFixture` / `renderModuleFixture`: the file's statement list, rendered, with
/// the file's module symbol mapped to `exports` as `TransformSourceFile` does first.
fn render_module_fixture(dir: &str, rel_path: &str) -> (String, Vec<Diagnostic>) {
	with_state(&transformer_testdata(dir), rel_path, |state| {
		let file = state.source_file;
		if let Some(symbol) = state.ck().get_symbol_at_location(file.as_node()) {
			let exports = state.ast.global_id("exports");
			state.set_module_id_by_symbol(symbol, exports);
		}
		let statements = transform_statement_list(state, Some(file.as_node()), file.statements().nodes(), None);
		assert!(
			state.internal_error().is_none(),
			"internal error: {:?}",
			state.internal_error()
		);
		(render_list(state, statements), state.diags.flush())
	})
}

/// The diagnostics of the file's statement list (no module symbol mapping, as in Go).
fn transform_diagnostics(dir: &str, rel_path: &str) -> Vec<Diagnostic> {
	with_state(&transformer_testdata(dir), rel_path, |state| {
		let file = state.source_file;
		transform_statement_list(state, Some(file.as_node()), file.statements().nodes(), None);
		state.diags.flush()
	})
}

fn assert_renders(dir: &str, rel_path: &str, want: &str) {
	let (got, diagnostics) = render_module_fixture(dir, rel_path);
	assert_eq!(got, want, "rendered output differs from rbxtsc");
	assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
}

/// The plain-class shape: setmetatable boilerplate, `.new` returning
/// `self:constructor(...) or self`, property initializers running before the constructor body,
/// no-initializer properties emitting nothing, static FIELDS as plain assignments, and the static
/// METHOD quirk (`static create()` emits as a COLON method).
#[test]
fn class_basic() {
	let want = r#"local Animal
do
	Animal = setmetatable({}, {
		__tostring = function()
			return "Animal"
		end,
	})
	Animal.__index = Animal
	function Animal.new(...)
		local self = setmetatable({}, Animal)
		return self:constructor(...) or self
	end
	function Animal:constructor(name)
		self.legs = 4
		self.name = name
	end
	function Animal:walk(dist)
		print(self.name, dist)
	end
	function Animal:create()
		return Animal.new("a")
	end
	Animal.VERSION = 1
end
local Plain
do
	Plain = setmetatable({}, {
		__tostring = function()
			return "Plain"
		end,
	})
	Plain.__index = Plain
	function Plain.new(...)
		local self = setmetatable({}, Plain)
		return self:constructor(...) or self
	end
	function Plain:constructor()
	end
end
local p = Plain.new()
local a = Animal:create()
a:walk(Animal.VERSION)
print(p)
"#;
	assert_renders("classes", "src/basic.ts", want);
}

/// The derived-class shapes: `local super = Base`, explicit constructor ordering, `super` calls,
/// the variadic implicit derived constructor, the re-emitted `__tostring` wrapper, and the
/// abstract forms.
#[test]
#[expect(clippy::too_many_lines, reason = "the expected text is verbatim")]
fn class_inheritance() {
	let want = r#"local Base
do
	Base = setmetatable({}, {
		__tostring = function()
			return "Base"
		end,
	})
	Base.__index = Base
	function Base.new(...)
		local self = setmetatable({}, Base)
		return self:constructor(...) or self
	end
	function Base:constructor(x, y)
		if y == nil then
			y = 2
		end
		self.x = x
		self.y = y
		print("base", x, y)
	end
	function Base:toString()
		return "Base"
	end
	function Base:m()
		return self.x
	end
	function Base:__tostring()
		return self:toString()
	end
end
local Derived
do
	local super = Base
	Derived = setmetatable({}, {
		__tostring = function()
			return "Derived"
		end,
		__index = super,
	})
	Derived.__index = Derived
	function Derived.new(...)
		local self = setmetatable({}, Derived)
		return self:constructor(...) or self
	end
	function Derived:constructor()
		print("before super")
		super.constructor(self, 1)
		self.z = 3
		print("after super")
	end
	function Derived:m()
		return super.m(self) + 1
	end
	function Derived:__tostring()
		return self:toString()
	end
end
local NoCtor
do
	local super = Base
	NoCtor = setmetatable({}, {
		__tostring = function()
			return "NoCtor"
		end,
		__index = super,
	})
	NoCtor.__index = NoCtor
	function NoCtor.new(...)
		local self = setmetatable({}, NoCtor)
		return self:constructor(...) or self
	end
	function NoCtor:constructor(...)
		super.constructor(self, ...)
	end
	function NoCtor:__tostring()
		return self:toString()
	end
end
local Abs
do
	Abs = {}
	function Abs:constructor()
	end
	function Abs:b()
		print("b")
	end
end
local AbsExtends
do
	local super = Base
	AbsExtends = setmetatable({}, {
		__tostring = function()
			return "AbsExtends"
		end,
		__index = super,
	})
	AbsExtends.__index = AbsExtends
	function AbsExtends:constructor(...)
		super.constructor(self, ...)
	end
	function AbsExtends:__tostring()
		return self:toString()
	end
end
local FromAbs
do
	local super = Abs
	FromAbs = setmetatable({}, {
		__tostring = function()
			return "FromAbs"
		end,
		__index = super,
	})
	FromAbs.__index = FromAbs
	function FromAbs.new(...)
		local self = setmetatable({}, FromAbs)
		return self:constructor(...) or self
	end
	function FromAbs:constructor(...)
		super.constructor(self, ...)
	end
	function FromAbs:a()
	end
end
print(Derived.new(), NoCtor.new(2), FromAbs.new(), tostring(Base.new(1)))
"#;
	assert_renders("classes", "src/inherit.ts", want);
}

/// Class expressions, statics, computed member names and export-default classes.
#[test]
fn class_expressions() {
	let want = r#"local _class
do
	local Inner = setmetatable({}, {
		__tostring = function()
			return "Inner"
		end,
	})
	Inner.__index = Inner
	function Inner.new(...)
		local self = setmetatable({}, Inner)
		return self:constructor(...) or self
	end
	function Inner:constructor()
	end
	function Inner:m()
		return Inner
	end
	_class = Inner
end
local Foo = _class
local _class_1
do
	_class_1 = setmetatable({}, {
		__tostring = function()
			return "Anonymous"
		end,
	})
	_class_1.__index = _class_1
	function _class_1.new(...)
		local self = setmetatable({}, _class_1)
		return self:constructor(...) or self
	end
	function _class_1:constructor()
	end
	function _class_1:m()
		return 1
	end
end
local Bar = _class_1
local Statics
do
	Statics = setmetatable({}, {
		__tostring = function()
			return "Statics"
		end,
	})
	Statics.__index = Statics
	function Statics.new(...)
		local self = setmetatable({}, Statics)
		return self:constructor(...) or self
	end
	function Statics:constructor()
	end
	Statics.counter = 0
	do
		Statics.counter = 5
		print(Statics.counter)
	end
end
local KEY = "dyn"
local Computed
do
	Computed = setmetatable({}, {
		__tostring = function()
			return "Computed"
		end,
	})
	Computed.__index = Computed
	function Computed.new(...)
		local self = setmetatable({}, Computed)
		return self:constructor(...) or self
	end
	function Computed:constructor()
	end
	Computed[KEY] = function(self)
		return 1
	end
end
local default
do
	default = setmetatable({}, {
		__tostring = function()
			return "default"
		end,
	})
	default.__index = default
	function default.new(...)
		local self = setmetatable({}, default)
		return self:constructor(...) or self
	end
	function default:constructor()
	end
	function default:m()
	end
end
print(Foo, Bar, Statics, Computed)
"#;
	assert_renders("classes", "src/exprs.ts", want);
}

/// The premature `new Late()` registers a hoist; the class transform skips its own `local Late`.
#[test]
fn class_hoisting() {
	let want = r#"local Late
local function makeInstance()
	return Late.new()
end
do
	Late = setmetatable({}, {
		__tostring = function()
			return "Late"
		end,
	})
	Late.__index = Late
	function Late.new(...)
		local self = setmetatable({}, Late)
		return self:constructor(...) or self
	end
	function Late:constructor()
		self.tag = "late"
	end
end
print(makeInstance())
"#;
	assert_renders("classes", "src/hoist.ts", want);
}

/// A `#field` PROPERTY raises `noPrivateIdentifier` spanning the WHOLE CLASS; a `#name` METHOD
/// raises it on the name node instead.
#[test]
fn class_private_identifier_diagnostics() {
	let diagnostics = transform_diagnostics("classes", "src/diagprivate.ts");
	assert_eq!(diagnostics.len(), 2, "expected 2 diagnostics, got {diagnostics:?}");
	for (i, diagnostic) in diagnostics.iter().enumerate() {
		assert_eq!(
			diagnostic.code, "noPrivateIdentifier",
			"diagnostic {i}: code = {:?}, want noPrivateIdentifier",
			diagnostic.code
		);
	}
	// property diagnostic spans the whole class declaration
	assert!(
		diagnostics[0].node.is_some_and(tsast::is_class_declaration),
		"property #field diagnostic node = {:?}, want the ClassDeclaration",
		diagnostics[0].node.map(|node| node.kind())
	);
	// method diagnostic spans the private identifier itself
	assert_eq!(
		diagnostics[1].node.map(|node| node.kind()),
		Some(Kind::PrivateIdentifier),
		"method #name diagnostic node, want PrivateIdentifier"
	);
}

/// The member-validation diagnostics: `noReservedClassFields`, `noGetterSetter` (get AND set),
/// `noClassMetamethods`, and the static/instance method collisions.
#[test]
fn class_member_diagnostics() {
	let codes: Vec<&str> = transform_diagnostics("classes", "src/diagmembers.ts")
		.iter()
		.map(|diagnostic| diagnostic.code)
		.collect();
	let want = [
		"noReservedClassFields",
		"noGetterSetter",
		"noGetterSetter",
		"noClassMetamethods",
		"noStaticMethodCollisions",
		"noInstanceMethodCollisions",
	];
	assert_eq!(codes, want, "diagnostics = {codes:?}, want {want:?}");
}

/// Every non-hoisted enum shape: numeric, string-only, heterogeneous, folded and unfoldable
/// computed members, const enums, `declare enum`, and non-identifier member names.
#[test]
fn enum_basic() {
	let want = r#"local Fruit
do
	local _inverse = {}
	Fruit = setmetatable({}, {
		__index = _inverse,
	})
	Fruit.Apple = 0
	_inverse[0] = "Apple"
	Fruit.Banana = 1
	_inverse[1] = "Banana"
	Fruit.Cherry = 2
	_inverse[2] = "Cherry"
end
local Mixed
do
	local _inverse = {}
	Mixed = setmetatable({}, {
		__index = _inverse,
	})
	Mixed.A = 5
	_inverse[5] = "A"
	Mixed.B = 6
	_inverse[6] = "B"
	Mixed.C = 10
	_inverse[10] = "C"
end
local Color = {
	Red = "red",
	Green = "green",
}
local Hetero
do
	local _inverse = {}
	Hetero = setmetatable({}, {
		__index = _inverse,
	})
	Hetero.Num = 1
	_inverse[1] = "Num"
	Hetero.Str = "str"
end
local base = 10
local Computed
do
	local _inverse = {}
	Computed = setmetatable({}, {
		__index = _inverse,
	})
	Computed.X = 20
	_inverse[20] = "X"
	local _value = #"y"
	Computed.Y = _value
	_inverse[_value] = "Y"
end
local Exported
do
	local _inverse = {}
	Exported = setmetatable({}, {
		__index = _inverse,
	})
	Exported.E1 = 0
	_inverse[0] = "E1"
	Exported.E2 = 1
	_inverse[1] = "E2"
end
local Weird
do
	local _inverse = {}
	Weird = setmetatable({}, {
		__index = _inverse,
	})
	Weird["a b"] = 1
	_inverse[1] = "a b"
	Weird.ok = 2
	_inverse[2] = "ok"
end
local function uses()
	print(Fruit.Apple, Fruit[1], Mixed.B, Color.Red, Hetero.Str, Computed.X, Computed.Y, Weird.ok)
	print(0, 1)
	print(Exported.E2)
end
"#;
	assert_renders("enums", "src/basic.ts", want);
}

/// The hoist machinery emits `local E` at the premature use, so the enum transform skips its own
/// header; hoisted string enums take the assignment branch of the fast path.
#[test]
fn enum_hoisted() {
	let want = r#"local E
local function early()
	return E.A
end
do
	local _inverse = {}
	E = setmetatable({}, {
		__index = _inverse,
	})
	E.A = 0
	_inverse[0] = "A"
end
local SE
local function earlyStr()
	return SE.S
end
SE = {
	S = "s",
}
"#;
	assert_renders("enums", "src/hoist.ts", want);
}

/// Both declarations of a merged enum emit nothing, and `noEnumMerging` is reported exactly once
/// per symbol.
#[test]
fn enum_merging() {
	let (got, diagnostics) = render_module_fixture("enums", "src/merge.ts");
	assert_eq!(got, "", "merged enum should emit nothing");
	assert_eq!(
		diagnostics.len(),
		1,
		"expected exactly 1 diagnostic, got {diagnostics:?}"
	);
	assert_eq!(diagnostics[0].code, "noEnumMerging");
	assert_eq!(diagnostics[0].message, "Enum merging is not supported!");
}

/// The namespace emit shapes: the container, the export mapping, `export let` routing, bare
/// do-blocks, dotted namespaces, type-only and `declare` namespaces, and multi-export statements.
#[test]
fn namespace_basic() {
	let want = r"local Outer = {}
do
	local _container = Outer
	local value = 1
	_container.value = value
	_container.mut = 2
	local function fn()
		return value
	end
	_container.fn = fn
	local function bump()
		_container.mut += 1
		return _container.mut
	end
	_container.bump = bump
	local hidden = 3
	local Inner = {}
	do
		local _container_1 = Inner
		local deep = hidden
		_container_1.deep = deep
	end
	_container.Inner = Inner
end
local function useNs()
	print(Outer.value, Outer.fn(), Outer.Inner.deep, Outer.mut, Outer.bump())
	Outer.mut = 5
end
local NoExports = {}
do
	local x = 1
	print(x)
end
local AB = {}
do
	local _container = AB
	local CD = {}
	do
		local _container_1 = CD
		local v = 1
		_container_1.v = v
	end
	_container.CD = CD
end
local function useAB()
	print(AB.CD.v)
end
local ExpNs = {}
do
	local _container = ExpNs
	local e = 1
	_container.e = e
	local p = 1
	local q = 2
	_container.p = p
	_container.q = q
end
";
	assert_renders("namespaces", "src/basic.ts", want);
}

/// The use-before-declare namespace takes the assignment branch (`NS = {}`).
#[test]
fn namespace_hoisted() {
	let want = r"local NS
local function earlyNs()
	return NS.v
end
NS = {}
do
	local _container = NS
	local v = 7
	_container.v = v
end
";
	assert_renders("namespaces", "src/hoist.ts", want);
}

/// The enum's do-block runs inside the namespace do-block and the export mapping appends
/// `_container.Inner2 = Inner2` after the enum's statements.
#[test]
fn namespace_with_enum() {
	let want = r#"local HasEnum = {}
do
	local _container = HasEnum
	local Inner2
	do
		local _inverse = {}
		Inner2 = setmetatable({}, {
			__index = _inverse,
		})
		Inner2.X = 0
		_inverse[0] = "X"
	end
	_container.Inner2 = Inner2
	local c = Inner2.X
	_container.c = c
end
local function useHasEnum()
	print(HasEnum.c)
end
"#;
	assert_renders("namespaces", "src/hasenum.ts", want);
}

/// Merged namespace declarations emit nothing and report `noNamespaceMerging` once per symbol;
/// namespace+function-with-body counts too (the function itself still emits).
#[test]
fn namespace_merging() {
	let want = r"local function f()
end
";
	let (got, diagnostics) = render_module_fixture("namespaces", "src/merge.ts");
	assert_eq!(got, want, "rendered output differs");
	assert_eq!(
		diagnostics.len(),
		2,
		"expected exactly 2 diagnostics (one per merged symbol), got {diagnostics:?}"
	);
	for diagnostic in &diagnostics {
		assert_eq!(diagnostic.code, "noNamespaceMerging");
		assert_eq!(diagnostic.message, "Namespace merging is not supported!");
	}
}
