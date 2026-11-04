package transformer_test

import (
	"path/filepath"
	"testing"

	"rotor/internal/luau"
	"rotor/internal/luau/render"
	"rotor/internal/transformer"
	"rotor/tsgo/ast"
)

// All expected text in this file is byte-for-byte what rbxtsc 3.0.0 emits for
// the same source (compiled through testdata/diff/project on 2026-06-07;
// header and trailing return stripped — those belong to TransformSourceFile,
// not the statement list under test).

func renderClassFixture(t *testing.T, relPath string) (string, *transformer.State) {
	t.Helper()
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "classes"), relPath)
	// TransformSourceFile maps the file's module symbol to `exports` before
	// transforming statements; reads of exported symbols consult it.
	if symbol := s.Checker.GetSymbolAtLocation(s.SourceFile.AsNode()); symbol != nil {
		s.SetModuleIDBySymbol(symbol, luau.GlobalID("exports"))
	}
	statements := transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)
	return render.RenderAST(statements), s
}

// TestClassBasic pins the plain-class shape (digest §5.1): setmetatable
// boilerplate, `.new` returning `self:constructor(...) or self`, property
// initializers running before the constructor body, no-initializer properties
// emitting nothing, static FIELDS as plain assignments — and the static
// METHOD quirk: `static create()` emits as a COLON method and call sites use
// `Animal:create()` (isMethod is type-driven — a static's `this` is the class
// type — not staticness-driven).
func TestClassBasic(t *testing.T) {
	got, s := renderClassFixture(t, "src/basic.ts")

	want := `local Animal
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
`
	if got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}

// TestClassInheritance pins the derived-class shapes (digest §5.2):
//
//   - `local super = Base` + `__index = super` as the second metatable field;
//   - explicit constructor ordering: param defaults -> body up to+incl
//     super() -> parameter properties -> property initializers -> rest;
//   - super() -> `super.constructor(self, 1)`; super.m() -> `super.m(self)`
//     (dot-call with explicit self, never `:`);
//   - implicit derived constructor is variadic even when the base takes args;
//   - the `__tostring` wrapper re-emitted per subclass inheriting a toString
//     METHOD (Derived/NoCtor/AbsExtends carry it; Abs/FromAbs don't);
//   - abstract no-extends classes are a bare `{}` — no metatable, no
//     `__index`, no `.new` — but still get constructor and methods;
//   - abstract WITH extends keeps the full metatable boilerplate, just no
//     `.new`.
func TestClassInheritance(t *testing.T) {
	got, s := renderClassFixture(t, "src/inherit.ts")

	want := `local Base
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
`
	if got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}

// TestClassExpressions pins class expressions, statics, computed member
// names, and export-default classes (digest §5.3):
//
//   - named class expression: pre-declared `_class` temp, inner local
//     `local Inner = setmetatable(...)` (the only local-declaration boilerplate
//     form), trailing `_class = Inner`; self-reference `return Inner` uses the
//     INTERNAL name;
//   - anonymous class expression: `_class_1`, `__tostring` name "Anonymous";
//   - static property then static block in source order, block as a nested
//     do-block; `this` in a static block resolves to the class identifier;
//   - computed method name: `Computed[KEY] = function(self) ... end` with
//     explicit self parameter;
//   - export-default unnamed class is literally named `default`.
func TestClassExpressions(t *testing.T) {
	got, s := renderClassFixture(t, "src/exprs.ts")

	want := `local _class
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
`
	if got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}

// TestClassHoisting pins the use-before-declare interplay (digest §5.5): the
// premature `new Late()` registers a hoist, TransformStatementList emits
// `local Late` before the function, and the class transform SKIPS its own
// `local Late` (isClassHoisted).
func TestClassHoisting(t *testing.T) {
	got, s := renderClassFixture(t, "src/hoist.ts")

	want := `local Late
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
`
	if got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}

// TestClassPrivateIdentifierDiagnostics: a `#field` PROPERTY raises
// noPrivateIdentifier spanning the WHOLE CLASS (upstream
// transformPropertyInitializers passes the class node — quirk ported
// verbatim); a `#name` METHOD raises it on the name node instead.
func TestClassPrivateIdentifierDiagnostics(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "classes"), "src/diagprivate.ts")
	transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)

	ds := s.Diags.Flush()
	if len(ds) != 2 {
		t.Fatalf("expected 2 diagnostics, got %d: %v", len(ds), ds)
	}
	for i, d := range ds {
		if d.Code != "noPrivateIdentifier" {
			t.Errorf("diagnostic %d: code = %q, want noPrivateIdentifier", i, d.Code)
		}
	}
	// property diagnostic spans the whole class declaration
	if !ast.IsClassDeclaration(ds[0].Node) {
		t.Errorf("property #field diagnostic node = %s, want the ClassDeclaration", ds[0].Node.Kind)
	}
	// method diagnostic spans the private identifier itself
	if ds[1].Node.Kind != ast.KindPrivateIdentifier {
		t.Errorf("method #name diagnostic node = %s, want PrivateIdentifier", ds[1].Node.Kind)
	}
}

// TestClassMemberDiagnostics covers the member-validation diagnostics:
// noReservedClassFields (`new`), noClassMetamethods (`__add`), noGetterSetter
// (get AND set accessors each), noInstanceMethodCollisions /
// noStaticMethodCollisions (`m` declared both ways).
func TestClassMemberDiagnostics(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "classes"), "src/diagmembers.ts")
	transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)

	var codes []string
	for _, d := range s.Diags.Flush() {
		codes = append(codes, d.Code)
	}
	want := []string{
		"noReservedClassFields",
		"noGetterSetter",
		"noGetterSetter",
		"noClassMetamethods",
		"noStaticMethodCollisions",
		"noInstanceMethodCollisions",
	}
	if len(codes) != len(want) {
		t.Fatalf("diagnostics = %v, want %v", codes, want)
	}
	for i := range want {
		if codes[i] != want[i] {
			t.Fatalf("diagnostics = %v, want %v", codes, want)
		}
	}
}
