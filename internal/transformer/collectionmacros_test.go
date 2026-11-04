package transformer_test

import (
	"path/filepath"
	"testing"

	"rotor/internal/luau/render"
	"rotor/internal/transformer"
)

// TestCollectionMacrosAgainstRbxtsc covers the Set/Map long-tail positions
// the oracle fixture (28_collectionmacros) does not isolate:
//
//   - clear() in VALUE position    -> prereq `table.clear(set)` + `nil` value
//     (single prereq: no comment markers)
//   - forEach() in VALUE position  -> loop prereqs + `nil` value (two
//     prereqs: `-- ▼ ReadonlySet.forEach ▼` markers)
//   - WeakMap.delete via interface inheritance (WeakMap declares no methods;
//     the call resolves to the Map.delete method symbol) in value position
//     -> existence snapshot `local _valueExisted = wm[t] ~= nil`
//   - Map.set CHAINING (`m.set(...).set(...).size()`): each set returns the
//     map for the next link; string-literal keys render as property
//     assignments (`m.a = 1`)
//   - Map.set on a COMPLEX base in value position -> header-exempt
//     `local _exp = getMap2()` push (no markers: single remaining prereq)
//
// The expected text below is byte-for-byte what rbxtsc 3.0.0 emits for this
// source (verified by compiling the same statements through
// testdata/diff/project; header and trailing `return nil` stripped — those
// belong to TransformSourceFile, not the statement list under test).
func TestCollectionMacrosAgainstRbxtsc(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "calls"), "src/collections.ts")

	statements := transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)

	want := `local set = {
	[1] = true,
}
table.clear(set)
local cleared = nil
print(cleared)
-- ▼ ReadonlySet.forEach ▼
local _callback = function(v)
	return print(v)
end
for _v in set do
	_callback(_v, _v, set)
end
-- ▲ ReadonlySet.forEach ▲
local fe = nil
print(fe)
local wm = setmetatable({}, {
	__mode = "k",
})
local t = {
	id = 1,
}
-- ▼ Map.delete ▼
local _valueExisted = wm[t] ~= nil
wm[t] = nil
-- ▲ Map.delete ▲
local deleted = _valueExisted
print(deleted)
local m = {}
m.a = 1
m.b = 2
-- ▼ ReadonlyMap.size ▼
local _size = 0
for _ in m do
	_size += 1
end
-- ▲ ReadonlyMap.size ▲
local msize = _size
print(msize)
local function getMap2()
	return {}
end
local _exp = getMap2()
_exp.k = 1
local chained2 = _exp
print(chained2)
`
	if got := render.RenderAST(statements); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}

	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}

// TestMacroOnlyClassAssertPanics: a method on a MACRO-ONLY class with NO
// registered macro (the macroonly fixture's single global ReadonlySet
// declaration carries an extra `union` method — the
// compiler-types-newer-than-the-compiler scenario) must hit upstream
// getPropertyCallMacro's `assert(false, ...)` — rotor panics with the exact
// upstream text. The CompileFile recover boundary turns this into an
// internal-compiler-error (compile-level coverage with the REAL
// compiler-types package: internal/compile TestMacroOnlyClassAssert).
func TestMacroOnlyClassAssertPanics(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "macroonly"), "src/main.ts")

	defer func() {
		r := recover()
		if r == nil {
			t.Fatal("expected a panic for the unimplemented macro-only method, got none")
		}
		const want = "Macro ReadonlySet.union() is not implemented!"
		if msg, ok := r.(string); !ok || msg != want {
			t.Fatalf("panic = %v, want %q", r, want)
		}
	}()

	transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)
}
