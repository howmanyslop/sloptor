package transformer_test

import (
	"path/filepath"
	"testing"

	"rotor/internal/luau"
	"rotor/internal/luau/render"
	"rotor/internal/transformer"
)

// All expected text in this file is byte-for-byte what rbxtsc 3.0.0 emits for
// the same source (compiled through testdata/diff/project on 2026-06-07;
// header and trailing return stripped — those belong to TransformSourceFile,
// not the statement list under test).

func renderAsyncFixture(t *testing.T, relPath string) (string, *transformer.State) {
	t.Helper()
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "async"), relPath)
	if symbol := s.Checker.GetSymbolAtLocation(s.SourceFile.AsNode()); symbol != nil {
		s.SetModuleIDBySymbol(symbol, luau.GlobalID("exports"))
	}
	statements := transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)
	return render.RenderAST(statements), s
}

// TestAsyncGeneratorDeclarations pins the declaration-site shapes (digest
// §1.3a, §2.2-2.3, §6.1):
//
//   - async function declarations NEVER emit function statements: localized
//     -> `local f = TS.async(function() ... end)`; hoisted (fetchValue
//     self-references — async functions are hoist-SENSITIVE, unlike plain
//     declarations) -> `local fetchValue` header + `fetchValue = TS.async(...)`;
//   - async arrows / `await x` -> `TS.await(x)` one-liners; statement-position
//     awaits survive as call statements;
//   - generator DECLARATIONS stay real function declarations whose body is
//     swapped for `return TS.generator(function() <body> end)`;
//   - bare `yield` -> `coroutine.yield()` (zero args);
//   - `yield* gen()` used as a VALUE: `local _returnValue` (no initializer),
//     generic for over `gen().next` re-yielding values, `_returnValue =
//     _result.value` captured at `done` before break (temp creation order:
//     _result before _returnValue);
//   - for-of over a generator call consumes the same `.next` protocol.
func TestAsyncGeneratorDeclarations(t *testing.T) {
	got, s := renderAsyncFixture(t, "src/decls.ts")

	want := `local fetchValue
fetchValue = TS.async(function(x)
	local a = TS.await(TS.Promise.resolve(x))
	local b = TS.await(fetchValue(a))
	return a + b
end)
local asyncArrow = TS.async(function(n)
	return n * 2
end)
local exported = TS.async(function()
	TS.await(asyncArrow(1))
end)
local function gen()
	return TS.generator(function()
		coroutine.yield(1)
		coroutine.yield(2)
		return "done"
	end)
end
local function gen2()
	return TS.generator(function()
		coroutine.yield()
		local _returnValue
		for _result in gen().next do
			if _result.done then
				_returnValue = _result.value
				break
			end
			coroutine.yield(_result.value)
		end
		local v = _returnValue
		print(v)
	end)
end
local function useGen()
	for _result in gen2().next do
		if _result.done then
			break
		end
		local x = _result.value
		print(x)
	end
end
`
	if got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}

// TestAsyncGeneratorMethods pins the method shapes (digest §1.3c, §6.3):
//
//   - async methods drop the colon shape (the !isAsync gate) and fall to the
//     map-pointer path with `self` KEPT in parameters: `Foo.work =
//     TS.async(function(self, n) ...)` — instance and static alike;
//   - generator methods KEEP the colon shape: `function Foo:counter()` whose
//     body is the TS.generator return (the wrap composes with the shape);
//   - computed-key async methods use the bracket assignment form;
//   - object-literal async/generator methods land as map fields;
//   - a use-before-declaration async function hoists: `local lateAsync`
//     header + `lateAsync = TS.async(...)` assignment;
//   - call sites use `:` for both async methods and statics (isMethod is
//     type-driven).
func TestAsyncGeneratorMethods(t *testing.T) {
	got, s := renderAsyncFixture(t, "src/methods.ts")

	want := `local lateAsync
local function early()
	return lateAsync()
end
lateAsync = TS.async(function()
	return 1
end)
local Foo
do
	Foo = setmetatable({}, {
		__tostring = function()
			return "Foo"
		end,
	})
	Foo.__index = Foo
	function Foo.new(...)
		local self = setmetatable({}, Foo)
		return self:constructor(...) or self
	end
	function Foo:constructor()
	end
	Foo.work = TS.async(function(self, n)
		return (TS.await(TS.Promise.resolve(n))) + 1
	end)
	Foo.make = TS.async(function(self)
		return Foo.new()
	end)
	function Foo:counter()
		return TS.generator(function()
			coroutine.yield(1)
			coroutine.yield(2)
		end)
	end
	function Foo:names()
		return TS.generator(function()
			coroutine.yield("a")
		end)
	end
	Foo["computed " .. "key"] = TS.async(function(self)
		return 1
	end)
end
local obj = {
	work = TS.async(function(self, n)
		return (TS.await(TS.Promise.resolve(n))) + 1
	end),
	counter = function(self)
		return TS.generator(function()
			coroutine.yield(1)
		end)
	end,
}
local function use()
	local f = Foo.new()
	print(f:work(1), Foo:make(), f:counter(), Foo:names())
end
`
	if got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}

// TestAsyncGeneratorExpressions pins the expression-site shapes (digest
// §1.3b, §6.4):
//
//   - async function expressions / arrows with expression bodies wrap as
//     `TS.async(function(...) ...)`;
//   - generator function expressions keep the function-expression shape with
//     the generator body swap;
//   - `yield* inner()` used as a STATEMENT: no `_returnValue` temp — the
//     `done` arm is a bare break;
//   - await inside binary/condition positions parenthesizes exactly as
//     upstream: `1 + (TS.await(a()))`, `(TS.await(a())) > 0`;
//   - resume arguments surface as coroutine.yield's return value:
//     `local got = coroutine.yield(1)`; an IIFE generator keeps its parens.
func TestAsyncGeneratorExpressions(t *testing.T) {
	got, s := renderAsyncFixture(t, "src/exprs.ts")

	want := `local fexpr = TS.async(function(x)
	return x
end)
local genExpr = function()
	return TS.generator(function()
		coroutine.yield(5)
	end)
end
local function inner()
	return TS.generator(function()
		coroutine.yield(1)
		return "r"
	end)
end
local function outerStmt()
	return TS.generator(function()
		for _result in inner().next do
			if _result.done then
				break
			end
			coroutine.yield(_result.value)
		end
		coroutine.yield()
	end)
end
local chainBin = TS.async(function(a)
	local x = 1 + (TS.await(a()))
	if (TS.await(a())) > 0 then
		TS.await(a())
	end
	TS.await(a())
	return x
end)
local arrowExprBody = TS.async(function(n)
	return n + (TS.await(TS.Promise.resolve(1)))
end)
local function resume()
	local g = (function()
		return TS.generator(function()
			local got = coroutine.yield(1)
			print(got)
		end)
	end)()
	g.next()
	g.next(5)
end
local function use()
	print(fexpr(2), genExpr(), outerStmt(), arrowExprBody(1))
end
`
	if got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}
