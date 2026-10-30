package transformer_test

import "testing"

// These outputs intentionally improve on rbxtsc: the object keyword cannot
// be nil, so its spreads need no truthiness guard (issue #180).
func TestNonPrimitiveObjectSpread(t *testing.T) {
	got, s := renderSpreadFixture(t, "src/nonprimitive.ts")
	want := `local _object = table.clone(source)
setmetatable(_object, nil)
local first = _object
local _object_1 = {
	n = 1,
}
for _k, _v in source do
	_object_1[_k] = _v
end
local last = _object_1
local _object_2 = {}
if maybeObject then
	for _k, _v in maybeObject do
		_object_2[_k] = _v
	end
end
local optional = _object_2
print(first, last, optional)
`
	if got != want {
		t.Errorf("rendered output differs:\ngot:\n%s\nwant:\n%s", got, want)
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}

func TestNonPrimitiveJsxSpread(t *testing.T) {
	got, s := transformJsxFile(t, "src/nonprimitive.tsx")
	want := `local _attributes = table.clone(source)
setmetatable(_attributes, nil)
local first = React.createElement("frame", _attributes)
local _attributes_1 = {
	Visible = true,
}
for _k, _v in source do
	_attributes_1[_k] = _v
end
local last = React.createElement("frame", _attributes_1)
local _attributes_2 = {}
if maybeObject then
	for _k, _v in maybeObject do
		_attributes_2[_k] = _v
	end
end
local optional = React.createElement("frame", _attributes_2)
print(first, last, optional)
`
	if got != want {
		t.Errorf("rendered output differs:\ngot:\n%s\nwant:\n%s", got, want)
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}
