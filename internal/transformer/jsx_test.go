package transformer_test

import (
	"path/filepath"
	"testing"

	"rotor/internal/luau/render"
	"rotor/internal/transformer"
)

// Expected text below follows rbxtsc 3.0.0, except identifier tag names follow
// TypeScript's intrinsic-name rule. The recordings cover the
// same constructs (phase3c jsx digest §3, oracle-verified 2026-06-07; header
// and import lines stripped — those belong to TransformSourceFile). The test
// project declares its own ambient React + JSX namespace (see
// testdata/jsx/src/jsx.d.ts) so that text children — type-illegal under
// @rbxts/react — flow through the full pipeline; the call shapes are
// identical to the digest's TextHolder recordings with `"frame"` tags.

func transformJsxFile(t *testing.T, relPath string) (string, *transformer.State) {
	t.Helper()
	s := buildState(t, filepath.Join("testdata", "jsx"), relPath)
	statements := transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)
	return render.RenderAST(statements), s
}

// TestJsxSpreadAttributes pins digest §3 cases 5/e2/A/B: the `_k`/`_v`
// for-loop merge for non-first spreads, the `table.clone` + `setmetatable(_, nil)`
// fast path for a first definitely-object spread, and the truthiness-guarded
// loop for possibly-undefined spreads (even at properties[0]).
func TestJsxSpreadAttributes(t *testing.T) {
	got, s := transformJsxFile(t, "src/spread.tsx")

	want := `local _attributes = {
	BackgroundTransparency = 1,
}
for _k, _v in extra do
	_attributes[_k] = _v
end
_attributes.Visible = false
local e = React.createElement("frame", _attributes)
local _attributes_1 = table.clone(extra)
setmetatable(_attributes_1, nil)
_attributes_1.Visible = false
local e2 = React.createElement("frame", _attributes_1)
local _attributes_2 = {}
if maybe then
	for _k, _v in maybe do
		_attributes_2[_k] = _v
	end
end
local a = React.createElement("frame", _attributes_2)
local _attributes_3 = {
	BackgroundTransparency = 1,
}
if maybe then
	for _k, _v in maybe do
		_attributes_3[_k] = _v
	end
end
local b = React.createElement("frame", _attributes_3)
print(e, e2, a, b)
`
	if got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}

// TestJsxAttributePrereqs pins digest §3 case C: an attribute initializer
// with prereqs (Array.pop macro) disables the inline map BEFORE the prereqs
// run, then later attributes become `_attributes.X = ...` assignments.
func TestJsxAttributePrereqs(t *testing.T) {
	got, s := transformJsxFile(t, "src/attrprereq.tsx")

	want := `local _attributes = {}
-- ▼ Array.pop ▼
local _length = #flags
local _result = flags[_length]
flags[_length] = nil
-- ▲ Array.pop ▲
_attributes.Visible = _result
-- ▼ Array.pop ▼
local _length_1 = #flags
local _result_1 = flags[_length_1]
flags[_length_1] = nil
-- ▲ Array.pop ▲
_attributes.Active = _result_1
local c = React.createElement("frame", _attributes)
print(c)
`
	if got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}

// TestJsxChildOrdering pins digest §3 case D: ensureTransformOrder pins the
// earlier complex child into `local _exp = ...` when a later child carries
// prereqs, and the nil placeholder appears (children, no attributes).
func TestJsxChildOrdering(t *testing.T) {
	got, s := transformJsxFile(t, "src/children.tsx")

	want := `local _exp = getEl()
-- ▼ Array.pop ▼
local _length = #els
local _result = els[_length]
els[_length] = nil
-- ▲ Array.pop ▲
local d = React.createElement("frame", nil, _exp, _result)
print(d)
`
	if got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}

// TestJsxTextChildren pins digest §3 cases 4/9/E through the full pipeline:
// JsxText fixup + whitespace-only filtering, `&&`/ternary/array expression
// children, entity decoding (`&amp;` -> `&`, `&nbsp;` -> raw U+00A0), and
// backslash doubling.
func TestJsxTextChildren(t *testing.T) {
	got, s := transformJsxFile(t, "src/text.tsx")

	want := `local d = (React.createElement("frame", nil, "hello world", cond and React.createElement("frame"), if cond then React.createElement("frame") else React.createElement("textlabel"), items))
local i = (React.createElement("frame", nil, "one & two` + " " + `three line2"))
local e = React.createElement("frame", nil, "back\\slash")
print(d, i, e)
`
	if got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}

// TestJsxTagShapes covers component bindings, dotted component tags, namespaced
// tag/attribute names, `{}` child dropped, `{...arr}` last child -> unpack,
// empty fragment, and fragment-with-children (always nil placeholder).
func TestJsxTagShapes(t *testing.T) {
	got, s := transformJsxFile(t, "src/tags.tsx")

	want := `local function _Comp()
	return React.createElement("frame")
end
local a = React.createElement(_Comp)
local function Item(props)
	return React.createElement("frame")
end
local NS = {
	Item = Item,
}
local Nested = {
	Deep = {
		Comp = Item,
	},
}
local h = React.createElement(NS.Item, {
	text = "3",
})
local i = React.createElement(Nested.Deep.Comp)
local n = React.createElement("a:b", {
	["c:d"] = "x",
})
local k = React.createElement("frame")
local j = React.createElement("frame", nil, unpack(arr))
local f = React.createElement(React.Fragment)
local kk = React.createElement(React.Fragment, nil, children)
print(a, h, i, n, k, j, f, kk)
`
	if got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}

// TestJsxSpreadChildNotLast pins digest §3 case M: a `{...arr}` child before
// the last significant child raises noPrecedingJsxSpreadElement.
func TestJsxSpreadChildNotLast(t *testing.T) {
	_, s := transformJsxFile(t, "src/spreadlast.tsx")

	found := false
	for _, d := range s.Diags.Flush() {
		if d.Code == "noPrecedingJsxSpreadElement" {
			found = true
		}
	}
	if !found {
		t.Errorf("expected noPrecedingJsxSpreadElement diagnostic")
	}
}
