//! `jsx_test.go`. The expected text follows rbxtsc 3.0.0, except identifier tag names follow
//! TypeScript's intrinsic-name rule. The test project declares its own ambient React and JSX
//! namespace (`testdata/jsx/src/jsx.d.ts`) so text children flow through the full pipeline.

use super::{assert_renders, render_plain_fixture};

/// The `_k`/`_v` merge loop for non-first spreads, the `table.clone` + `setmetatable(_, nil)`
/// fast path for a first definitely-object spread, and the truthiness-guarded loop for
/// possibly-undefined spreads (even at `properties[0]`).
#[test]
fn jsx_spread_attributes() {
	let want = r#"local _attributes = {
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
"#;
	assert_renders(&render_plain_fixture("jsx", "src/spread.tsx"), want);
}

/// An attribute initializer with prereqs (the `Array.pop` macro) disables the inline map before
/// the prereqs run; later attributes become `_attributes.X = ...` assignments.
#[test]
#[ignore = "needs slice iv: the `Array.pop` macro"]
fn jsx_attribute_prereqs() {
	let want = r#"local _attributes = {}
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
"#;
	assert_renders(&render_plain_fixture("jsx", "src/attrprereq.tsx"), want);
}

/// ensureTransformOrder pins the earlier complex child into `local _exp = ...` when a later
/// child carries prereqs, and the nil placeholder appears (children, no attributes).
#[test]
#[ignore = "needs slice iv: the `Array.pop` macro"]
fn jsx_child_ordering() {
	let want = r#"local _exp = getEl()
-- ▼ Array.pop ▼
local _length = #els
local _result = els[_length]
els[_length] = nil
-- ▲ Array.pop ▲
local d = React.createElement("frame", nil, _exp, _result)
print(d)
"#;
	assert_renders(&render_plain_fixture("jsx", "src/children.tsx"), want);
}

/// `JsxText` fixup and whitespace-only filtering, `&&`/ternary/array expression children, entity
/// decoding (`&amp;` to `&`, `&nbsp;` to a raw U+00A0), and backslash doubling.
#[test]
fn jsx_text_children() {
	let want = concat!(
		r#"local d = (React.createElement("frame", nil, "hello world", cond and React.createElement("frame"), if cond then React.createElement("frame") else React.createElement("textlabel"), items))
local i = (React.createElement("frame", nil, "one & two"#,
		"\u{a0}",
		r#"three line2"))
local e = React.createElement("frame", nil, "back\\slash")
print(d, i, e)
"#
	);
	assert_renders(&render_plain_fixture("jsx", "src/text.tsx"), want);
}

/// Component bindings, dotted component tags, namespaced tag and attribute names, `{}` child
/// dropped, `{...arr}` last child to `unpack`, empty fragment, and fragment with children
/// (always the nil placeholder).
#[test]
fn jsx_tag_shapes() {
	let want = r#"local function _Comp()
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
"#;
	assert_renders(&render_plain_fixture("jsx", "src/tags.tsx"), want);
}

/// A `{...arr}` child before the last significant child raises `noPrecedingJsxSpreadElement`.
#[test]
fn jsx_spread_child_not_last() {
	let (_, diagnostics) = render_plain_fixture("jsx", "src/spreadlast.tsx");
	assert!(
		diagnostics
			.iter()
			.any(|diagnostic| diagnostic.code == "noPrecedingJsxSpreadElement"),
		"expected noPrecedingJsxSpreadElement diagnostic, got {diagnostics:?}"
	);
}
