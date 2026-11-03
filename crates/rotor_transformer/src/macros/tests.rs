//! The Go tests of slice (iv), with the expected strings verbatim: `callmacros_test`,
//! `arraymacros_test`, `arraymacros_internal_test`, `collectionmacros_test`, `stringmacros_test`,
//! `filemacro_test`, `newexpression_test`, and the macro tests of `call_test` and
//! `optionalchain_test`.

use super::arraymacros::READONLY_ARRAY_METHODS;
use super::arraymacros2::ARRAY_METHODS;
use super::filemacro::json_bytes_to_luau;
use super::propertycallmacros::wrap_comments;
use super::stringmacros::{ARRAY_LIKE_METHODS, STRING_CALLBACKS, arguments_with_defaults};
use crate::macromanager::PropertyCallMacro;
use crate::prelude::*;
use crate::statementlist::transform_statement_list;
use crate::tests::{
	render_file_statements, render_list, render_node, transformer_testdata, with_state, with_test_state,
};

/// Go's `hasDiagnostic`.
fn has_diagnostic(diagnostics: &[Diagnostic], code: &str, message_substring: &str) -> bool {
	diagnostics
		.iter()
		.any(|diagnostic| diagnostic.code == code && diagnostic.message.contains(message_substring))
}

/// The diagnostics of the file's statement list.
fn transform_diagnostics(dir: &str, rel_path: &str) -> Vec<Diagnostic> {
	with_state(&transformer_testdata(dir), rel_path, |state| {
		let file = state.source_file;
		transform_statement_list(state, Some(file.as_node()), file.statements().nodes(), None);
		state.diags.flush()
	})
}

/// The file's statement list, rendered, with no diagnostics allowed.
fn render_file(dir: &str, rel_path: &str) -> String {
	with_state(&transformer_testdata(dir), rel_path, render_file_statements)
}

/// `renderMacroExpr`: renders `expression` as `local x = <expression>` (so the temp-id solver
/// runs) and strips the declaration.
fn render_macro_expr(state: &mut State<'_>, expression: NodeId) -> String {
	let x = state.ast.id("x");
	let declaration = state.ast.variable_declaration(x, Some(expression.into()));
	let statements = state.ast.list_from([declaration]);
	let out = render_list(state, statements);
	let out = out.strip_prefix("local x = ").unwrap_or(&out);
	out.strip_suffix('\n').unwrap_or(out).to_owned()
}

/// A macro of `table` by method name.
fn lookup(table: &[(&str, PropertyCallMacro)], method: &str) -> PropertyCallMacro {
	table
		.iter()
		.find(|&&(name, _)| name == method)
		.map_or_else(|| panic!("no macro {method}"), |&(_, macro_fn)| macro_fn)
}

// callmacros_test.go

/// `TestRangeMacroOutsideForOf`
#[test]
fn range_macro_outside_for_of() {
	let diagnostics = transform_diagnostics("calls", "src/rangemisuse.ts");
	assert!(
		has_diagnostic(
			&diagnostics,
			"noRangeMacroOutsideForOf",
			"only valid as an expression of a for-of loop"
		),
		"no noRangeMacroOutsideForOf diagnostic; got: {diagnostics:?}"
	);
}

/// `TestTupleMacroOutsideReturn`
#[test]
fn tuple_macro_outside_return() {
	let diagnostics = transform_diagnostics("calls", "src/tuplemisuse.ts");
	assert!(
		has_diagnostic(
			&diagnostics,
			"noTupleMacroOutsideReturn",
			"only valid as an expression of a return statement"
		),
		"no noTupleMacroOutsideReturn diagnostic; got: {diagnostics:?}"
	);
}

/// `TestCallMacroIndexWithoutCall`
#[test]
fn call_macro_index_without_call() {
	let diagnostics = transform_diagnostics("calls", "src/indexwithoutcall.ts");
	assert!(
		has_diagnostic(
			&diagnostics,
			"noIndexWithoutCall",
			"Cannot index a method without calling it"
		),
		"no noIndexWithoutCall diagnostic; got: {diagnostics:?}"
	);
}

// arraymacros_test.go

/// `TestArrayMacrosAgainstRbxtsc`
#[test]
fn array_macros_against_rbxtsc() {
	let want = std::fs::read_to_string(transformer_testdata("arraymacros").join("expected.luau"))
		.expect("the expected output reads");
	assert_eq!(render_file("arraymacros", "src/arraymacros.ts"), want);
}

// arraymacros_internal_test.go

/// `TestArrayMacroTableInventory`
#[test]
fn array_macro_table_inventory() {
	let want_readonly = [
		"isEmpty",
		"join",
		"move",
		"includes",
		"indexOf",
		"every",
		"some",
		"forEach",
		"map",
		"mapFiltered",
		"filterUndefined",
		"filter",
		"reduce",
		"find",
		"findIndex",
	];
	let want_array = [
		"push",
		"pop",
		"shift",
		"unshift",
		"insert",
		"remove",
		"unorderedRemove",
		"sort",
		"clear",
	];
	for (name, table, want) in [
		("READONLY_ARRAY_METHODS", READONLY_ARRAY_METHODS, &want_readonly[..]),
		("ARRAY_METHODS", ARRAY_METHODS, &want_array[..]),
	] {
		assert_eq!(table.len(), want.len(), "{name} entry count");
		for key in want {
			assert!(
				table.iter().any(|&(method, _)| method == *key),
				"{name}[{key:?}] missing"
			);
		}
	}
}

/// `TestArrayMacroOffsets`
#[test]
fn array_macro_offsets() {
	type Args = fn(&mut Ast) -> Vec<NodeId>;
	type Table = &'static [(&'static str, PropertyCallMacro)];
	let tests: [(Table, &str, Args, &str); 12] = [
		(READONLY_ARRAY_METHODS, "isEmpty", |_| vec![], "#arr == 0"),
		(
			READONLY_ARRAY_METHODS,
			"move",
			|ast| vec![ast.num(0.0), ast.num(2.0), ast.num(1.0)],
			"table.move(arr, 1, 3, 2)",
		),
		(
			READONLY_ARRAY_METHODS,
			"move",
			|ast| vec![ast.num(0.0), ast.num(2.0), ast.num(1.0), ast.id("dst")],
			"table.move(arr, 1, 3, 2, dst)",
		),
		(
			READONLY_ARRAY_METHODS,
			"includes",
			|ast| vec![ast.num(7.0)],
			"table.find(arr, 7) ~= nil",
		),
		(
			READONLY_ARRAY_METHODS,
			"includes",
			|ast| vec![ast.num(7.0), ast.id("from")],
			"table.find(arr, 7, from + 1) ~= nil",
		),
		(
			READONLY_ARRAY_METHODS,
			"indexOf",
			|ast| vec![ast.num(7.0)],
			"(table.find(arr, 7) or 0) - 1",
		),
		(
			READONLY_ARRAY_METHODS,
			"indexOf",
			|ast| vec![ast.num(7.0), ast.num(1.0)],
			"(table.find(arr, 7, 2) or 0) - 1",
		),
		(ARRAY_METHODS, "shift", |_| vec![], "table.remove(arr, 1)"),
		(
			ARRAY_METHODS,
			"insert",
			|ast| vec![ast.num(0.0), ast.str("x")],
			r#"table.insert(arr, 1, "x")"#,
		),
		(
			ARRAY_METHODS,
			"insert",
			|ast| vec![ast.id("i"), ast.str("x")],
			r#"table.insert(arr, i + 1, "x")"#,
		),
		(
			ARRAY_METHODS,
			"insert",
			|ast| {
				let i = ast.id("i");
				let one = ast.num(1.0);
				vec![ast.binary(i, BinaryOperator::Subtract, one), ast.str("x")]
			},
			r#"table.insert(arr, i, "x")"#,
		),
		(
			ARRAY_METHODS,
			"remove",
			|ast| vec![ast.num(4.0)],
			"table.remove(arr, 5)",
		),
	];
	with_test_state(|state| {
		for (table, method, args, want) in tests {
			let macro_fn = lookup(table, method);
			let node = state.source_file.as_node();
			let args = args(&mut state.ast);
			let (expression, prereqs) = state.capture(|state| {
				let arr = state.ast.id("arr");
				macro_fn(state, node, arr, args)
			});
			assert!(
				state.ast.list_is_empty(prereqs),
				"{method}: pure-expression macro must not produce prereqs"
			);
			assert_eq!(render_macro_expr(state, expression), want, "{method}");
		}
	});
}

// collectionmacros_test.go

/// `TestCollectionMacrosAgainstRbxtsc`
#[test]
fn collection_macros_against_rbxtsc() {
	let want = r#"local set = {
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
"#;
	assert_eq!(render_file("calls", "src/collections.ts"), want);
}

/// `TestMacroOnlyClassAssertPanics`: Go panics with upstream's assert text; Rust records it as
/// the internal error.
#[test]
fn macro_only_class_assert_panics() {
	let internal_error = with_state(&transformer_testdata("macroonly"), "src/main.ts", |state| {
		let file = state.source_file;
		transform_statement_list(state, Some(file.as_node()), file.statements().nodes(), None);
		state.internal_error().map(str::to_owned)
	});
	assert_eq!(
		internal_error.as_deref(),
		Some("Macro ReadonlySet.union() is not implemented!")
	);
}

// stringmacros_test.go

/// `TestStringMacroEmits`
#[test]
fn string_macro_emits() {
	type Build = fn(&mut Ast) -> (NodeId, Vec<NodeId>);
	let tests: [(&str, Build, &str); 15] = [
		("size", |ast| (ast.id("s"), vec![]), "#s"),
		// method-style call on a string literal: the literal is just the first call argument /
		// unary operand, no parens needed
		("size", |ast| (ast.str("abc"), vec![]), r#"#"abc""#),
		("byte", |ast| (ast.id("s"), vec![]), "string.byte(s)"),
		(
			"byte",
			|ast| (ast.id("s"), vec![ast.num(1.0), ast.num(2.0)]),
			"string.byte(s, 1, 2)",
		),
		(
			"find",
			|ast| (ast.id("s"), vec![ast.str("o"), ast.num(2.0), ast.bool(true)]),
			r#"string.find(s, "o", 2, true)"#,
		),
		(
			"format",
			|ast| (ast.str("%d/%s"), vec![ast.num(5.0), ast.str("x")]),
			r#"string.format("%d/%s", 5, "x")"#,
		),
		(
			"gmatch",
			|ast| (ast.id("s"), vec![ast.str("%a+")]),
			r#"string.gmatch(s, "%a+")"#,
		),
		(
			"gsub",
			|ast| (ast.id("s"), vec![ast.str("l"), ast.str("L"), ast.num(1.0)]),
			r#"string.gsub(s, "l", "L", 1)"#,
		),
		("lower", |ast| (ast.id("s"), vec![]), "string.lower(s)"),
		(
			"match",
			|ast| (ast.id("s"), vec![ast.str("(%a+)")]),
			r#"string.match(s, "(%a+)")"#,
		),
		("rep", |ast| (ast.id("s"), vec![ast.num(3.0)]), "string.rep(s, 3)"),
		("reverse", |ast| (ast.id("s"), vec![]), "string.reverse(s)"),
		(
			"split",
			|ast| (ast.id("s"), vec![ast.str(", ")]),
			r#"string.split(s, ", ")"#,
		),
		(
			"sub",
			|ast| (ast.str("xyz"), vec![ast.num(1.0), ast.num(2.0)]),
			r#"string.sub("xyz", 1, 2)"#,
		),
		("upper", |ast| (ast.id("s"), vec![]), "string.upper(s)"),
	];
	let mut covered = std::collections::BTreeSet::new();
	with_test_state(|state| {
		for (method, build, want) in tests {
			covered.insert(method);
			let macro_fn = lookup(STRING_CALLBACKS, method);
			let node = state.source_file.as_node();
			let (base, args) = build(&mut state.ast);
			let (expression, prereqs) = state.capture(|state| macro_fn(state, node, base, args));
			assert!(
				state.ast.list_is_empty(prereqs),
				"{method}: string macros must not produce prereqs"
			);
			assert_eq!(render_macro_expr(state, expression), want, "{method}");
		}
	});
	assert_eq!(
		covered.len(),
		STRING_CALLBACKS.len(),
		"covered {} of {} STRING_CALLBACKS entries",
		covered.len(),
		STRING_CALLBACKS.len()
	);
}

/// `TestArrayLikeSizeMacro`
#[test]
fn array_like_size_macro() {
	assert_eq!(ARRAY_LIKE_METHODS.len(), 1, "ARRAY_LIKE_METHODS entry count");
	with_test_state(|state| {
		let node = state.source_file.as_node();
		let (expression, prereqs) = state.capture(|state| {
			let arr = state.ast.id("arr");
			lookup(ARRAY_LIKE_METHODS, "size")(state, node, arr, vec![])
		});
		assert!(
			state.ast.list_is_empty(prereqs),
			"ArrayLike.size must not produce prereqs"
		);
		assert_eq!(render_macro_expr(state, expression), "#arr");
	});
}

/// `fakeMacro`: optionally pushes the incoming base to a var first (the header-exempt push),
/// then emits `EXTRA` `doWork(expression)` prereqs.
fn fake_macro<const PUSH_BASE: bool, const EXTRA: usize>(
	state: &mut State<'_>,
	_node: P<Node>,
	mut expression: NodeId,
	_args: Vec<NodeId>,
) -> NodeId {
	if PUSH_BASE {
		expression = state.push_to_var_if_complex(expression, "exp");
	}
	for _ in 0..EXTRA {
		let callee = state.ast.global_id("doWork");
		let args = state.ast.list_from([expression]);
		let call = state.ast.call(callee, args);
		let statement = state.ast.call_statement(call);
		state.prereq(statement);
	}
	expression
}

/// A leading temp declaration whose value is not the incoming base: `wasExpressionPushed`
/// rejects it.
fn other_declaration_macro(state: &mut State<'_>, _node: P<Node>, expression: NodeId, _args: Vec<NodeId>) -> NodeId {
	let c = state.ast.id("c");
	let d = state.ast.id("d");
	let sum = state.ast.binary(c, BinaryOperator::Add, d);
	let id = state.push_to_var(Some(sum), "other");
	let callee = state.ast.global_id("doWork");
	let args = state.ast.list_from([id]);
	let call = state.ast.call(callee, args);
	let statement = state.ast.call_statement(call);
	state.prereq(statement);
	expression
}

/// `TestWrapComments`
#[test]
fn wrap_comments_threshold() {
	fn simple_base(ast: &mut Ast) -> NodeId {
		ast.id("v")
	}
	fn complex_base(ast: &mut Ast) -> NodeId {
		let a = ast.id("a");
		let b = ast.id("b");
		ast.binary(a, BinaryOperator::Add, b)
	}
	type Base = fn(&mut Ast) -> NodeId;
	let tests: [(&str, PropertyCallMacro, Base, &str, &str); 7] = [
		("zero prereqs, no markers", fake_macro::<false, 0>, simple_base, "", "v"),
		(
			"one prereq, no markers",
			fake_macro::<false, 1>,
			simple_base,
			"doWork(v)\n",
			"v",
		),
		(
			"two prereqs, markers",
			fake_macro::<false, 2>,
			simple_base,
			"-- ▼ Test.method ▼
doWork(v)
doWork(v)
-- ▲ Test.method ▲
",
			"v",
		),
		(
			"base push only, exempt, no markers",
			fake_macro::<true, 0>,
			complex_base,
			"local _exp = a + b\n",
			"_exp",
		),
		(
			"base push + one prereq, still below threshold",
			fake_macro::<true, 1>,
			complex_base,
			"local _exp = a + b
doWork(_exp)
",
			"_exp",
		),
		(
			"base push + two prereqs, push stays above header",
			fake_macro::<true, 2>,
			complex_base,
			"local _exp = a + b
-- ▼ Test.method ▼
doWork(_exp)
doWork(_exp)
-- ▲ Test.method ▲
",
			"_exp",
		),
		(
			"leading temp declaration NOT of the base counts toward the threshold",
			other_declaration_macro,
			simple_base,
			"-- ▼ Test.method ▼
local _other = c + d
doWork(_other)
-- ▲ Test.method ▲
",
			"v",
		),
	];
	for (name, macro_fn, base, want, want_expression) in tests {
		let got = with_test_state(|state| {
			let node = state.source_file.as_node();
			let base = base(&mut state.ast);
			let mut expression = None;
			let prereqs = state.capture_statements(|state| {
				expression = Some(wrap_comments("Test.method", macro_fn, state, node, base, vec![]));
			});
			// render expression and prereqs together so the temp-id solver names temps
			// consistently with the declarations
			let callee = state.ast.global_id("use");
			let args = state.ast.list_from(expression);
			let call = state.ast.call(callee, args);
			let statement = state.ast.call_statement(call);
			state.ast.list_push(prereqs, statement);
			render_list(state, prereqs)
		});
		assert_eq!(got, format!("{want}use({want_expression})\n"), "{name}");
	}
}

/// `TestArgumentsWithDefaults`
#[test]
fn arguments_with_defaults_contract() {
	// simple primitive passes through
	with_test_state(|state| {
		let arg = state.ast.str("x");
		let default = state.ast.str(", ");
		let mut args = Vec::new();
		let prereqs = state.capture_statements(|state| {
			args = arguments_with_defaults(state, vec![arg], &[default]);
		});
		assert!(
			state.ast.list_is_empty(prereqs),
			"simple primitive must not produce prereqs"
		);
		assert_eq!(args, [arg], "arg must pass through untouched");
	});

	// non-primitive arg gets nil-guarded temp
	with_test_state(|state| {
		let sep = state.ast.id("sep");
		let default = state.ast.str(", ");
		let mut args = Vec::new();
		let prereqs = state.capture_statements(|state| {
			args = arguments_with_defaults(state, vec![sep], &[default]);
		});
		assert_eq!(
			state.ast.kind(args[0]),
			LuauKind::TemporaryIdentifier,
			"arg must become a temp"
		);
		let callee = state.ast.global_id("use");
		let call_args = state.ast.list_from(args);
		let call = state.ast.call(callee, call_args);
		let statement = state.ast.call_statement(call);
		state.ast.list_push(prereqs, statement);
		let want = r#"local _sep = sep
if _sep == nil then
	_sep = ", "
end
use(_sep)
"#;
		assert_eq!(render_list(state, prereqs), want);
	});

	// missing trailing args filled with defaults literally
	with_test_state(|state| {
		let default = state.ast.str(", ");
		let mut args = Vec::new();
		let prereqs = state.capture_statements(|state| {
			args = arguments_with_defaults(state, vec![], &[default]);
		});
		assert!(
			state.ast.list_is_empty(prereqs),
			"filling missing args must not produce prereqs"
		);
		assert_eq!(args, [default], "missing arg must be the default expression itself");
	});
}

// filemacro_test.go

/// `TestJSONBytesToLuau`
#[test]
fn json_bytes_to_luau_values() {
	let cases = [
		("string", r#""hi""#, r#""hi""#),
		("int", "42", "42"),
		("negative-int", "-7", "-7"),
		("float-precision", "0.30000000000000004", "0.30000000000000004"),
		("big-int-literal", "9007199254740993", "9007199254740993"),
		("bool-true", "true", "true"),
		("bool-false", "false", "false"),
		("null", "null", "nil"),
		("empty-object", "{}", "{}"),
		("empty-array", "[]", "{}"),
		("array", "[1, 2, 3]", "{ 1, 2, 3 }"),
		("array-with-null", "[1, null, 3]", "{ 1, nil, 3 }"),
		// object keys are sorted; the null member is DROPPED entirely.
		(
			"object-sorted-null-dropped",
			r#"{"b":2,"a":1,"c":null}"#,
			"{\n\ta = 1,\n\tb = 2,\n}",
		),
		// nested
		(
			"nested",
			r#"{"outer":{"inner":[true]}}"#,
			"{\n\touter = {\n\t\tinner = { true },\n\t},\n}",
		),
	];
	with_test_state(|state| {
		for (name, json, want) in cases {
			let expression = json_bytes_to_luau(&mut state.ast, json.as_bytes())
				.unwrap_or_else(|error| panic!("jsonBytesToLuau({json:?}): {error}"));
			assert_eq!(render_node(state, expression), want, "{name}");
		}
	});
}

/// `TestJSONBytesToLuauInvalid`: malformed JSON and trailing garbage both error.
#[test]
fn json_bytes_to_luau_invalid() {
	with_test_state(|state| {
		for bad in ["{", r#"{ "x": }"#, "not json", "{} {}", "1 2"] {
			assert!(
				json_bytes_to_luau(&mut state.ast, bad.as_bytes()).is_err(),
				"jsonBytesToLuau({bad:?}) = nil error, want a parse error"
			);
		}
	});
}

/// The `encoding/json` error texts that reach `rotorFileInvalidJSON`, and the trailing-value
/// rule (a second value fails, anything that does not decode is ignored).
#[test]
fn json_bytes_to_luau_go_errors() {
	let cases = [
		("", Err("EOF")),
		("  ", Err("EOF")),
		("{", Err("unexpected EOF")),
		("[1,", Err("unexpected EOF")),
		("-", Err("unexpected EOF")),
		(
			r#"{ "x": }"#,
			Err("invalid character '}' looking for beginning of value"),
		),
		("not json", Err("invalid character 'o' in literal null (expecting 'u')")),
		("[1 2]", Err("invalid character '2' after array element")),
		(r#"{"a" 1}"#, Err("invalid character '1' after object key")),
		(
			r#"{"a":1 "b":2}"#,
			Err("invalid character '\"' after object key:value pair"),
		),
		(
			"{[]}",
			Err("invalid character '[' looking for beginning of object key string"),
		),
		(
			"1.x",
			Err("invalid character 'x' after decimal point in numeric literal"),
		),
		("1ex", Err("invalid character 'x' in exponent of numeric literal")),
		("\"a\nb\"", Err("invalid character '\\n' in string literal")),
		(r#""\q""#, Err("invalid character 'q' in string escape code")),
		("{} {}", Err("unexpected trailing JSON value")),
		("1 2", Err("unexpected trailing JSON value")),
		("1x", Ok("1")),
		("{} ]", Ok("{}")),
		(r#""😀\ud800""#, Ok("\"😀\u{fffd}\"")),
		("1e5", Ok("1e5")),
	];
	with_test_state(|state| {
		for (json, want) in cases {
			let got = json_bytes_to_luau(&mut state.ast, json.as_bytes()).map(|value| render_node(state, value));
			assert_eq!(got.as_deref(), want.map_err(str::to_owned).as_deref(), "{json:?}");
		}
	});
}

// newexpression_test.go

/// `TestNewExpressionAgainstRbxtsc`
#[test]
fn new_expression_against_rbxtsc() {
	let want = r#"local a = {}
local b = table.create(8)
local c = table.create(4, "x")
local s1 = {
	["a"] = true,
	["b"] = true,
}
local s2 = {}
local src = { "x", "y" }
local _set = {}
for _, _v in src do
	_set[_v] = true
end
local s3 = _set
local i = 0
local _original = i
i += 1
local s4 = {
	[_original] = true,
	[i] = true,
}
local m1 = {
	a = 1,
	b = 2,
}
local m2 = {}
local entries = { { "k", 3 } }
local _map = {}
for _, _v in entries do
	_map[_v[1]] = _v[2]
end
local m3 = _map
local m4 = {}
local function makePairs()
	return { { "p", 9 } }
end
local _map_1 = {}
for _, _v in makePairs() do
	_map_1[_v[1]] = _v[2]
end
local m5 = _map_1
local wm = setmetatable({}, {
	__mode = "k",
})
local ws = setmetatable({}, {
	__mode = "k",
})
local rm = {
	a = 1,
}
local rs = {
	["q"] = true,
}
local part = Instance.new("Part")
local t = Thing.new(5)
print(a, b, c, s1, s2, s3, s4, m1, m2, m3, m4, m5, wm, ws, rm, rs, part, t)
"#;
	assert_eq!(render_file("new", "src/constructors.ts"), want);
}

/// `TestConstructorMacroIdentifierWithoutNew`
#[test]
fn constructor_macro_identifier_without_new() {
	let diagnostics = transform_diagnostics("new", "src/macroident.ts");
	assert!(
		has_diagnostic(
			&diagnostics,
			"noConstructorMacroWithoutNew",
			"without using the `new` operator"
		),
		"no noConstructorMacroWithoutNew diagnostic for `Array` without new; got: {diagnostics:?}"
	);
}

/// `TestNewPromise`
#[test]
fn new_promise() {
	let (got, uses_runtime_lib, diagnostics) = with_state(&transformer_testdata("new"), "src/promise.ts", |state| {
		let file = state.source_file;
		let statements = transform_statement_list(state, Some(file.as_node()), file.statements().nodes(), None);
		(
			render_list(state, statements),
			state.uses_runtime_lib,
			state.diags.flush(),
		)
	});
	assert!(
		got.contains("TS.Promise.new("),
		"`new Promise(...)` did not emit through TS.Promise.new:\n{got}"
	);
	assert!(uses_runtime_lib, "identifier macro did not set UsesRuntimeLib");
	assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
}

// call_test.go

/// `TestPropertyCallMacro`
#[test]
fn property_call_macro() {
	assert_eq!(render_file("calls", "src/macro.ts"), "local bag = {}\nbag[4] = true\n");
}

/// `TestContextualMethodArrayCallbacksInjectSelf`
#[test]
fn contextual_method_array_callbacks_inject_self() {
	let got = render_file("calls", "src/contextual_method.ts");
	for want in ["function(self)", "function(self, arg)"] {
		assert!(got.contains(want), "rendered output missing {want:?}:\n{got}");
	}
}

// optionalchain_test.go

/// `TestOptionalChainShapes`
#[test]
fn optional_chain_shapes() {
	let want = "local _r1 = a
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
";
	assert_eq!(render_file("calls", "src/optchain.ts"), want);
}

/// `TestNoOptionalMacroCall`
#[test]
fn no_optional_macro_call() {
	let diagnostics = transform_diagnostics("calls", "src/optmacro.ts");
	assert!(
		diagnostics
			.iter()
			.any(|diagnostic| diagnostic.code == "noOptionalMacroCall"),
		"expected noOptionalMacroCall diagnostic; got: {diagnostics:?}"
	);
}

/// `TestOptionalChainMacroInterplay`
#[test]
fn optional_chain_macro_interplay() {
	let want = "local tbl = {}
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
";
	assert_eq!(render_file("calls", "src/optmacrobase.ts"), want);
}
