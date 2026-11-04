//! `binding_test.go`, `spreaddestruct_test.go`, `spread_test.go`, `nonprimitive_spread_test.go`
//! and `TestBigIntPropertyDiagnostic` from `bitwise_test.go`, with the expected strings verbatim.
//! The rest-diagnostics case waits on slice (iv) (constructor macros) and the JSX case on slice
//! (vi); they are ignored until those slices merge.

use super::binding::object_accessor;
use crate::prelude::*;
use crate::statementlist::transform_statement_list;
use crate::tests::{
	render_file_statements, render_list, repo_root, transformer_testdata, with_parsed_expression, with_state,
};

fn render_destructuring_file(rel_path: &str) -> String {
	with_state(&transformer_testdata("destructuring"), rel_path, render_file_statements)
}

fn destructuring_diagnostics(rel_path: &str) -> Vec<Diagnostic> {
	with_state(&transformer_testdata("destructuring"), rel_path, |s| {
		let file = s.source_file;
		transform_statement_list(s, Some(file.as_node()), file.statements().nodes(), None);
		s.diags.flush()
	})
}

/// Go's `renderSpreadFixture`: the file's module symbol is registered as `exports` first.
fn render_spread_fixture(rel_path: &str) -> (String, Vec<Diagnostic>) {
	with_state(&transformer_testdata("spread"), rel_path, |s| {
		let file = s.source_file;
		if let Some(symbol) = s.ck().get_symbol_at_location(file.as_node()) {
			let exports = s.ast.global_id("exports");
			s.set_module_id_by_symbol(symbol, exports);
		}
		let statements = transform_statement_list(s, Some(file.as_node()), file.statements().nodes(), None);
		(render_list(s, statements), s.diags.flush())
	})
}

/// The statements of a one-file project holding `source`, over the diff project's `@rbxts`
/// types, rendered with no diagnostics allowed.
fn render_source(source: &str) -> String {
	let directory = tempfile::tempdir().expect("a temporary directory");
	let type_roots = repo_root().join("testdata/diff/project/node_modules/@rbxts");
	let type_roots = rotor_support::fsutil::real_path(&type_roots).expect("the type roots exist");
	let type_roots = type_roots.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
	let tsconfig = format!(
		r#"{{
	"compilerOptions": {{
		"downlevelIteration": true,
		"module": "commonjs",
		"moduleResolution": "Node",
		"noLib": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"typeRoots": ["{type_roots}"],
		"rootDir": "src"
	}},
	"include": ["src"]
}}"#
	);
	std::fs::write(directory.path().join("tsconfig.json"), tsconfig).expect("the tsconfig writes");
	std::fs::create_dir(directory.path().join("src")).expect("the source directory");
	std::fs::write(directory.path().join("src/test.ts"), source).expect("the source writes");
	let root = rotor_support::fsutil::real_path(directory.path()).expect("the directory resolves");
	with_state(&root, "src/test.ts", render_file_statements)
}

fn transform_jsx_file(rel_path: &str) -> (String, Vec<Diagnostic>) {
	with_state(&transformer_testdata("jsx"), rel_path, |s| {
		let file = s.source_file;
		let statements = transform_statement_list(s, Some(file.as_node()), file.statements().nodes(), None);
		(render_list(s, statements), s.diags.flush())
	})
}

// -------------------------------------------------------------------------------------------------
// binding_test.go
// -------------------------------------------------------------------------------------------------

/// Array-in-object and object-in-array patterns: one `_binding` temp per nesting level.
#[test]
fn nested_destructuring() {
	let want = r#"local data = {
	list = { 1, 2 },
	info = {
		name = "n",
	},
}
local _binding = data.list
local u = _binding[1]
local v = _binding[2]
local _binding_1 = data.info
local name = _binding_1.name
local records = { {
	id = 1,
}, {
	id = 2,
} }
local _binding_2 = records[1]
local firstId = _binding_2.id
print(u, v, name, firstId)
"#;
	assert_eq!(render_destructuring_file("src/nested.ts"), want);
}

/// A default with its own prereqs: extract first, then nil-check, with the prereqs inside the if.
#[test]
fn destructuring_defaults_with_prereqs() {
	let want = "local b = 1
local source = {}
local _binding = source
local a = _binding[1]
if a == nil then
	local _original = b
	b += 1
	a = _original
end
local _binding_1 = {}
local c = _binding_1.m
if c == nil then
	local _original = b
	b += 1
	c = _original
end
print(a, b, c)
";
	assert_eq!(render_destructuring_file("src/defaults.ts"), want);
}

/// Holes advance the accessor index; the optimized literal form binds a `_` placeholder.
#[test]
fn destructuring_omitted_elements() {
	let want = "local arr = { 1, 2, 3, 4, 5 }
local second = arr[2]
local fourth = arr[4]
local _, y = 10, 20
print(second, fourth, y)
";
	assert_eq!(render_destructuring_file("src/omitted.ts"), want);
}

/// Binding-pattern parameters become a `_param` temp destructured at the top of the body.
#[test]
fn parameter_destructuring() {
	let want = "local function f(_param)
	local a = _param[1]
	local b = _param[2]
	return a + b
end
local function g(_param, scale)
	local x = _param.x
	local y = _param.y
	if scale == nil then
		scale = 2
	end
	return (x + y) * scale
end
local function h(p, q)
	return p * q
end
print(f({ 1, 2 }), g({
	x = 1,
	y = 2,
}), h(3, 4))
";
	assert_eq!(render_destructuring_file("src/params.ts"), want);
}

/// A non-`LuaTuple` call right side lands in a `_binding` temp exactly once.
#[test]
fn destructuring_from_call_rhs() {
	let want = "local function make()
	return { 1, 2 }
end
local _binding = make()
local p = _binding[1]
local q = _binding[2]
local m = 0
local n = 0
local _binding_1 = make()
m = _binding_1[1]
n = _binding_1[2]
print(p, q, m, n)
";
	assert_eq!(render_destructuring_file("src/callrhs.ts"), want);
}

/// A `LuaTuple` call right side unpacks directly, with a `_` placeholder for an omitted slot.
#[test]
fn lua_tuple_destructuring() {
	let want = "local d1, d2 = multi()
local e1 = 0
e1, _ = multi()
print(d1, d2, e1)
";
	assert_eq!(render_destructuring_file("src/luatuple.ts"), want);
}

/// `IterableFunction<LuaTuple<T>>` packs each read in `{ _binding() }`; an omitted
/// `IterableFunction<T>` element calls the function as a statement to advance.
#[test]
fn iteration_accessors() {
	let want = "local p1 = { pairsIter() }
local p2 = { pairsIter() }
print(p1[1], p2[2])
nums()
local second = nums()
print(second)
";
	assert_eq!(render_destructuring_file("src/iteraccessors.ts"), want);
}

// -------------------------------------------------------------------------------------------------
// spreaddestruct_test.go
// -------------------------------------------------------------------------------------------------

#[test]
fn unsupported_function_rest_destructuring_reports_diagnostics() {
	let diagnostics = destructuring_diagnostics("src/unsupportedrest.ts");
	assert_eq!(diagnostics.len(), 2, "diagnostic count, want 2: {diagnostics:?}");
	for diagnostic in diagnostics {
		assert_eq!(diagnostic.code, "noNestedSpreadsInAssignmentPatterns");
	}
}

#[test]
fn object_rest() {
	let want = r#"local _binding = {
	x = 1,
	y = 2,
}
local x = _binding.x
local _extracted = {
	["x"] = true,
}
local _rest = {}
for _k, _v in _binding do
	if not _extracted[_k] then
		_rest[_k] = _v
	end
end
local restObject = _rest
print(x, restObject)
"#;
	assert_eq!(render_destructuring_file("src/restobject.ts"), want);
}

#[test]
fn object_rest_assignment() {
	let want = r#"local x = 0
local restObject = {}
local object = {
	x = 1,
	y = 2,
}
local _binding = object
x = _binding.x
local _extracted = {
	["x"] = true,
}
local _rest = {}
for _k, _v in _binding do
	if not _extracted[_k] then
		_rest[_k] = _v
	end
end
restObject = _rest
print(x, restObject)
"#;
	assert_eq!(render_destructuring_file("src/restobjectassign.ts"), want);
}

#[test]
fn array_rest_destructuring() {
	let want = "local _binding = { 1, 2, 3 }
local first = _binding[1]
local restBinding = table.move(_binding, 2, #_binding, 1, {})
print(first, restBinding)
";
	assert_eq!(render_destructuring_file("src/restbinding.ts"), want);
}

#[test]
fn rest_destructuring() {
	let want = "local first = 0
local restAssign = {}
local _binding = { 1, 2, 3 }
first = _binding[1]
restAssign = table.move(_binding, 2, #_binding, 1, {})
print(first, restAssign)
";
	assert_eq!(render_destructuring_file("src/restassign.ts"), want);
}

// -------------------------------------------------------------------------------------------------
// spread_test.go
// -------------------------------------------------------------------------------------------------

/// Object-literal spread: the `table.clone` fast path only for a definitely-object spread into
/// an empty inline map; otherwise a copy loop, if-wrapped when possibly undefined.
#[test]
fn object_spread() {
	let (got, diagnostics) = render_spread_fixture("src/object.ts");
	let want = r#"local base = {
	x = 1,
	y = 2,
}
local extra = {
	z = 3,
}
local function getObj()
	return {
		a = "k",
		b = 1,
	}
end
local _object = table.clone(base)
setmetatable(_object, nil)
local spreadOnly = _object
local _object_1 = table.clone(base)
setmetatable(_object_1, nil)
_object_1.z = 3
local spreadFirst = _object_1
local _object_2 = {
	z = 3,
}
for _k, _v in base do
	_object_2[_k] = _v
end
local spreadLast = _object_2
local _object_3 = table.clone(base)
setmetatable(_object_3, nil)
for _k, _v in extra do
	_object_3[_k] = _v
end
local spreadTwice = _object_3
local _object_4 = {}
if maybe then
	for _k, _v in maybe do
		_object_4[_k] = _v
	end
end
_object_4.q = 1
local spreadMaybe = _object_4
local _object_5 = {
	a = 1,
}
local _spread = props.event
if _spread then
	for _k, _v in _spread do
		_object_5[_k] = _v
	end
end
local spreadOptionalProp = _object_5
local _object_6 = table.clone(getObj())
setmetatable(_object_6, nil)
_object_6.b = 2
local spreadCall = _object_6
local _object_7 = table.clone(base)
setmetatable(_object_7, nil)
_object_7[getObj().a] = 9
local computedAfterSpread = _object_7
print(spreadOnly, spreadFirst, spreadLast, spreadTwice, spreadMaybe, spreadOptionalProp, spreadCall, computedAfterSpread)
"#;
	assert_eq!(got, want);
	assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
}

/// Array-literal spread through the shared builders.
#[test]
fn array_spread() {
	let (got, diagnostics) = render_spread_fixture("src/array.ts");
	let want = "local function getObj()
	return {
		b = 1,
	}
end
local arr1 = { 1, 2 }
local arr2 = { 3, 4 }
local _array = {}
local _length = #_array
table.move(arr1, 1, #arr1, _length + 1, _array)
local a1 = _array
local _array_1 = {}
local _length_1 = #_array_1
local _arr1Length = #arr1
table.move(arr1, 1, _arr1Length, _length_1 + 1, _array_1)
_length_1 += _arr1Length
_array_1[_length_1 + 1] = 5
local a2 = _array_1
local _array_2 = { 5 }
local _length_2 = #_array_2
table.move(arr1, 1, #arr1, _length_2 + 1, _array_2)
local a3 = _array_2
local _array_3 = {}
local _length_3 = #_array_3
local _arr1Length_1 = #arr1
table.move(arr1, 1, _arr1Length_1, _length_3 + 1, _array_3)
_length_3 += _arr1Length_1
table.move(arr2, 1, #arr2, _length_3 + 1, _array_3)
local a4 = _array_3
local _array_4 = {}
local _length_4 = #_array_4
local _arr1Length_2 = #arr1
table.move(arr1, 1, _arr1Length_2, _length_4 + 1, _array_4)
_length_4 += _arr1Length_2
_array_4[_length_4 + 1] = 5
local _arr2Length = #arr2
table.move(arr2, 1, _arr2Length, _length_4 + 2, _array_4)
_length_4 += _arr2Length
_array_4[_length_4 + 2] = 6
local a5 = _array_4
local _array_5 = {}
local _length_5 = #_array_5
for _v in nset do
	_length_5 += 1
	_array_5[_length_5] = _v
end
local a6 = _array_5
local _array_6 = {}
local _length_6 = #_array_6
for _k, _v in nmap do
	_length_6 += 1
	_array_6[_length_6] = { _k, _v }
end
local a7 = _array_6
local _array_7 = {}
local _length_7 = #_array_7
for _char in string.gmatch(str, utf8.charpattern) do
	_length_7 += 1
	_array_7[_length_7] = _char
end
local a8 = _array_7
local _array_8 = {}
local _length_8 = #_array_8
local _arr1Length_3 = #arr1
table.move(arr1, 1, _arr1Length_3, _length_8 + 1, _array_8)
_length_8 += _arr1Length_3
_array_8[_length_8 + 1] = getObj().b
local a9 = _array_8
print(a1, a2, a3, a4, a5, a6, a7, a8, a9)
";
	assert_eq!(got, want);
	assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
}

/// Call-argument spread: `unpack(arr1)` for arrays; other iterables collect into
/// `_array`/`_length` (`local _length = 0` here) first.
#[test]
fn call_spread() {
	let (got, diagnostics) = render_spread_fixture("src/call.ts");
	let want = r##"local arr1 = { 1, 2 }
local function takeNums(...)
	return select("#", ...)
end
print(takeNums(unpack(arr1)))
print(takeNums(1, unpack(arr1)))
local _array = {}
local _length = 0
for _v in nset do
	_length += 1
	_array[_length] = _v
end
print(takeNums(unpack(_array)))
"##;
	assert_eq!(got, want);
	assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
}

/// `??=`, `&&=` and `||=`.
#[test]
fn logical_assignments() {
	let (got, diagnostics) = render_spread_fixture("src/logical.ts");
	let want = r#"local v
if v == nil then
	v = 1
end
local holder = {}
local function f()
	return 5
end
if holder.p == nil then
	holder.p = f()
end
local arrIdx = {}
local i = 0
local _index = i + 1
if arrIdx[_index] == nil then
	arrIdx[_index] = f()
end
local function get()
	return holder
end
local _exp = get()
if _exp.p == nil then
	_exp.p = f()
end
local s = "x"
local _condition = s
if s ~= "" and s then
	_condition = "y"
end
s = _condition
local _condition_1 = s
if not (s ~= "" and s) then
	_condition_1 = "z"
end
s = _condition_1
local n = 0
local _condition_2 = n
if not (n ~= 0 and n == n and n) then
	_condition_2 = 7
end
n = _condition_2
local _condition_3 = n
if n ~= 0 and n == n and n then
	_condition_3 = 8
end
n = _condition_3
if v == nil then
	v = 2
end
local useAsExpr = v
print(v, holder.p, arrIdx[i + 1], s, n, useAsExpr, i)
"#;
	assert_eq!(got, want);
	assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
}

/// A call-argument spread that is not the last argument.
#[test]
fn no_preceding_spread_element() {
	let (_, diagnostics) = render_spread_fixture("src/diag.ts");
	assert!(
		diagnostics
			.iter()
			.any(|diagnostic| diagnostic.code == "noPrecedingSpreadElement"),
		"expected noPrecedingSpreadElement diagnostic: {diagnostics:?}"
	);
}

// -------------------------------------------------------------------------------------------------
// nonprimitive_spread_test.go: the object keyword cannot be nil, so its spreads need no
// truthiness guard (issue #180); intentionally better than rbxtsc.
// -------------------------------------------------------------------------------------------------

#[test]
fn non_primitive_object_spread() {
	let (got, diagnostics) = render_spread_fixture("src/nonprimitive.ts");
	let want = "local _object = table.clone(source)
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
";
	assert_eq!(got, want);
	assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
}

#[test]
fn non_primitive_jsx_spread() {
	let (got, diagnostics) = transform_jsx_file("src/nonprimitive.tsx");
	let want = r#"local _attributes = table.clone(source)
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
"#;
	assert_eq!(got, want);
	assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
}

// -------------------------------------------------------------------------------------------------
// bitwise_test.go: TestBigIntPropertyDiagnostic
// -------------------------------------------------------------------------------------------------

/// Go passes a nil parent type, which the bigint branch never reads; the port passes `any`.
#[test]
fn big_int_property_diagnostic() {
	with_parsed_expression("({ 1n: value });", |s, expression| {
		let property = expression
			.as_parenthesized_expression()
			.expression
			.get()
			.as_object_literal_expression()
			.properties
			.nodes()[0];
		let name = property.as_property_assignment().name();
		let parent = s.ast.id("obj");
		let t = s.ck().get_any_type();
		object_accessor(s, parent, t, name);
		let diagnostics = s.diags.flush();
		assert!(
			diagnostics.len() == 1 && diagnostics[0].code == "noBigInt",
			"BigInt diagnostics = {diagnostics:?}, want one noBigInt diagnostic"
		);
	});
}

// -------------------------------------------------------------------------------------------------
// Rust-only cases: paths the Go tests reach only through functions. The expected text is what Go
// rotor's `build` emits for the same source (header and trailing `return nil` stripped).
// -------------------------------------------------------------------------------------------------

/// The iterator-protocol accessors (`Set`, `Map`, generator), their rest destructors, and the
/// string matcher.
#[test]
fn iterator_destructuring() {
	let source = "declare const nset: Set<number>;
declare const nmap: Map<string, number>;
declare const str: string;
declare const gen: Generator<number>;
const [s1, , s3] = nset;
const [m1, ...mRest] = nmap;
const [c1, , c3] = str;
const [g1, ...gRest] = gen;
print(s1, s3, m1, mRest, c1, c3, g1, gRest);
";
	assert_eq!(render_source(source), ITERATOR_DESTRUCTURING_WANT);
}

const ITERATOR_DESTRUCTURING_WANT: &str = "local _key
local _consumed = {}
local _done = false
local _value
if not _done then
	if _key ~= nil and nset[_key] == nil then
		_key = nil
	end
	_key = next(nset, _key)
	while _key ~= nil and _consumed[_key] do
		_key = next(nset, _key)
	end
	_done = _key == nil
	if not _done then
		_consumed[_key] = true
		_value = _key
	end
end
local s1 = _value
if not _done then
	if _key ~= nil and nset[_key] == nil then
		_key = nil
	end
	_key = next(nset, _key)
	while _key ~= nil and _consumed[_key] do
		_key = next(nset, _key)
	end
	_done = _key == nil
	if not _done then
		_consumed[_key] = true
	end
end
local _value_1
if not _done then
	if _key ~= nil and nset[_key] == nil then
		_key = nil
	end
	_key = next(nset, _key)
	while _key ~= nil and _consumed[_key] do
		_key = next(nset, _key)
	end
	_done = _key == nil
	if not _done then
		_consumed[_key] = true
		_value_1 = _key
	end
end
local s3 = _value_1
local _key_1
local _consumed_1 = {}
local _done_1 = false
local _value_2
if not _done_1 then
	local _value_3
	if _key_1 ~= nil and nmap[_key_1] == nil then
		_key_1 = nil
	end
	_key_1, _value_3 = next(nmap, _key_1)
	while _key_1 ~= nil and _consumed_1[_key_1] do
		_key_1, _value_3 = next(nmap, _key_1)
	end
	_done_1 = _key_1 == nil
	if not _done_1 then
		_consumed_1[_key_1] = true
		_value_2 = { _key_1, _value_3 }
	end
end
local m1 = _value_2
local _rest = {}
local _length = 0
while not _done_1 do
	local _value_3
	if _key_1 ~= nil and nmap[_key_1] == nil then
		_key_1 = nil
	end
	_key_1, _value_3 = next(nmap, _key_1)
	while _key_1 ~= nil and _consumed_1[_key_1] do
		_key_1, _value_3 = next(nmap, _key_1)
	end
	_done_1 = _key_1 == nil
	if _done_1 then
		break
	end
	_length += 1
	_rest[_length] = { _key_1, _value_3 }
end
local mRest = _rest
local _matcher = string.gmatch(str, utf8.charpattern)
local c1 = _matcher()
_matcher()
local c3 = _matcher()
local _next = gen.next
local _done_2 = false
local _value_3
if not _done_2 then
	local _result = _next()
	_done_2 = _result.done
	if not _done_2 then
		_value_3 = _result.value
	end
end
local g1 = _value_3
local _rest_1 = {}
local _length_1 = 0
while not _done_2 do
	local _result = _next()
	_done_2 = _result.done
	if _done_2 then
		break
	end
	_length_1 += 1
	_rest_1[_length_1] = _result.value
end
local gRest = _rest_1
print(s1, s3, m1, mRest, c1, c3, g1, gRest)
";

/// Object rest, literal and computed property names (the `+1` quirk), and an object assignment
/// pattern with a default.
#[test]
fn object_pattern_names() {
	let source = r#"declare const obj: { a: number; b: string; c: boolean; 0: number; k: number };
const { a, ...others } = obj;
const { 0: zero, ["k"]: kv, [`c`]: cv } = obj;
declare const tuple: [number, number];
const { 0: t0, [1]: t1 } = tuple;
let x = 0;
let y = "";
({ a: x, b: y = "d" } = obj);
print(a, others, zero, kv, cv, t0, t1, x, y);
"#;
	let want = r#"local a = obj.a
local _extracted = {
	["a"] = true,
}
local _rest = {}
for _k, _v in obj do
	if not _extracted[_k] then
		_rest[_k] = _v
	end
end
local others = _rest
local zero = obj[0]
local kv = obj.k
local cv = obj[`c`]
local t0 = tuple[0]
local t1 = tuple[2]
local x = 0
local y = ""
local _binding = obj
x = _binding.a
y = _binding.b
if y == nil then
	y = "d"
end
print(a, others, zero, kv, cv, t0, t1, x, y)
"#;
	assert_eq!(render_source(source), want);
}

/// Array assignment patterns (swap, pinned targets with a default, nested optimized and
/// fallback forms) and the logical assignments over property targets.
#[test]
fn array_assignment_and_logical_assignment() {
	let source = "let t1 = 0, t2 = 0;
[t1, t2] = [t2, t1];
const holder = { list: [1, 2] as Array<number>, n: 0, m: undefined as number | undefined };
[holder.list[0], holder.n = 5] = holder.list;
let [p, [q, r]] = [1, [2, 3]] as [number, [number, number]];
[p, [q, r]] = [q, [r, p]];
const nested = [[1, 2]] as Array<Array<number>>;
[[t1, t2]] = nested;
holder.m ??= 3;
holder.n ||= 4;
holder.n &&= 6;
const m = (holder.m ??= 9);
print(t1, t2, holder, p, q, r, m);
";
	let want = "local t1 = 0
local t2 = 0
t1, t2 = t2, t1
local holder = {
	list = { 1, 2 },
	n = 0,
	m = nil,
}
local _binding = holder.list
holder.list[1] = _binding[1]
local _exp = holder
local _binding_1 = _binding[2]
if _binding_1 == nil then
	_binding_1 = 5
end
_exp.n = _binding_1
local p, _binding_2 = 1, { 2, 3 }
local q = _binding_2[1]
local r = _binding_2[2]
local _binding_3
p, _binding_3 = q, { r, p }
q = _binding_3[1]
r = _binding_3[2]
local nested = { { 1, 2 } }
local _binding_4 = nested
local _binding_5 = _binding_4[1]
t1 = _binding_5[1]
t2 = _binding_5[2]
if holder.m == nil then
	holder.m = 3
end
local _condition = holder.n
local _value = holder.n
if not (_value ~= 0 and _value == _value and _value) then
	_condition = 4
end
holder.n = _condition
local _condition_1 = holder.n
local _value_1 = holder.n
if _value_1 ~= 0 and _value_1 == _value_1 and _value_1 then
	_condition_1 = 6
end
holder.n = _condition_1
if holder.m == nil then
	holder.m = 9
end
local m = (holder.m)
print(t1, t2, holder, p, q, r, m)
";
	assert_eq!(render_source(source), want);
}

/// The array-literal and call-argument spread builders for every iterable kind, and an object
/// spread after an inline field.
#[test]
fn spread_builders() {
	let source = "declare const nset: Set<number>;
declare const str: string;
declare const nmap: Map<string, number>;
declare const fnIter: IterableFunction<number>;
declare const gen: Generator<number>;
declare const obj: { a: number };
const arr = [...nset, 1, ...str, ...fnIter, ...gen, ...nmap];
print(...nset);
print(...arr);
print(...str);
const o = { z: 1, ...obj };
print(arr, o);
";
	let want = "local _array = {}
local _length = #_array
for _v in nset do
	_length += 1
	_array[_length] = _v
end
_array[_length + 1] = 1
_length += 1
for _char in string.gmatch(str, utf8.charpattern) do
	_length += 1
	_array[_length] = _char
end
_length += 1
for _result in fnIter do
	_length += 1
	_array[_length] = _result
end
_length += 1
for _result in gen.next do
	if _result.done then
		break
	end
	_length += 1
	_array[_length] = _result.value
end
_length += 1
for _k, _v in nmap do
	_length += 1
	_array[_length] = { _k, _v }
end
local arr = _array
local _array_1 = {}
local _length_1 = 0
for _v in nset do
	_length_1 += 1
	_array_1[_length_1] = _v
end
print(unpack(_array_1))
print(unpack(arr))
local _array_2 = {}
local _length_2 = 0
for _char in string.gmatch(str, utf8.charpattern) do
	_length_2 += 1
	_array_2[_length_2] = _char
end
print(unpack(_array_2))
local _object = {
	z = 1,
}
for _k, _v in obj do
	_object[_k] = _v
end
local o = _object
print(arr, o)
";
	assert_eq!(render_source(source), want);
}
