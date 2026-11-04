package transformer_test

import (
	"path/filepath"
	"testing"

	"rotor/internal/luau/render"
	"rotor/internal/transformer"
)

// All expected text in this file is byte-for-byte what rbxtsc 3.0.0 emits for
// the same source (verified by compiling the identical statements through
// testdata/diff/project; header and trailing `return nil` stripped — those
// belong to TransformSourceFile, not the statement list under test).

// TestOptionalChainShapes pins the worked block shapes of
// transformOptionalChainInner (digest §5.3) against the oracle:
//
//   - a?.b               temp named from the VariableDeclaration parent,
//     reassigned inside the nil check
//   - a?.fn()            optional access + non-optional call: the call folds
//     into the nil-check block via transformChainItem
//   - o.cb?.()           callOptional on a callback: statement position emits
//     a bare CallStatement, expression position reassigns the temp
//   - om.m?.(1)          callOptional on a real method: `local _self = om`
//     BEFORE the nil check, explicit self argument, never `:` sugar
//   - arr?.pop()         property-call macro runs normally inside the
//     nil-check block (optional access, non-optional call)
func TestOptionalChainShapes(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "calls"), "src/optchain.ts")

	statements := transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)

	want := `local _r1 = a
if _r1 ~= nil then
	_r1 = _r1.b
end
local r1 = _r1
local _r2 = a
if _r2 ~= nil then
	_r2 = _r2.fn()
end
local r2 = _r2
local _result = o.cb
if _result ~= nil then
	_result()
end
local _r3 = o.cb
if _r3 ~= nil then
	_r3 = _r3()
end
local r3 = _r3
local _self = om
local _result_1 = _self.m
if _result_1 ~= nil then
	_result_1(_self, 1)
end
local _self_1 = om
local _r4 = _self_1.m
if _r4 ~= nil then
	_r4 = _r4(_self_1, 2)
end
local r4 = _r4
local _result_2 = arr
if _result_2 ~= nil then
	_result_2[#_result_2] = nil
end
local _r5 = arr
if _r5 ~= nil then
	-- ▼ Array.pop ▼
	local _length = #_r5
	local _result_3 = _r5[_length]
	_r5[_length] = nil
	-- ▲ Array.pop ▲
	_r5 = _result_3
end
local r5 = _r5
print(r1, r2, r3, r4, r5)
`
	if got := render.RenderAST(statements); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}

	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}

// TestOptionalChainAccess: `holder.value?.num` — optional access in the
// middle of a chain whose result feeds a VariableDeclaration (temp named
// `_n`).
func TestOptionalChainAccess(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "calls"), "src/optional.ts")

	statements := transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)

	want := `local _n = holder.value
if _n ~= nil then
	_n = _n.num
end
local n = _n
print(n)
`
	if got := render.RenderAST(statements); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}

	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}

// TestNoOptionalMacroCall: optionally CALLING a property-call macro
// (`nums.map?.(...)`) is upstream's noOptionalMacroCall error — never a
// silent emit. (`nums?.map(...)` — optional ACCESS — is fine and covered by
// TestOptionalChainShapes' arr?.pop().)
func TestNoOptionalMacroCall(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "calls"), "src/optmacro.ts")

	transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)

	ds := s.Diags.Flush()
	found := false
	for _, d := range ds {
		if d.Code == "noOptionalMacroCall" {
			found = true
		}
	}
	if !found {
		t.Errorf("expected noOptionalMacroCall diagnostic; got: %v", ds)
	}
}

// TestOptionalChainMacroInterplay: a macro call as the BASE of an optional
// chain (`tbl.get(k)?.x`) and a macro inside an optional link (`s?.add(1)`)
// both work — the macro emits inside the nil-check block.
func TestOptionalChainMacroInterplay(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "calls"), "src/optmacrobase.ts")

	statements := transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)

	want := `local tbl = {}
local _gx = tbl.k
if _gx ~= nil then
	_gx = _gx.x
end
local gx = _gx
local _result = s
if _result ~= nil then
	_result[1] = true
end
print(gx, s)
`
	if got := render.RenderAST(statements); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}

	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}
