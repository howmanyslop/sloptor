//! `forof_test.go` and `forof_variadic_test.go`. The expected text in every render test is byte
//! for byte what rbxtsc 3.0.0 emits for the same source. Diagnostic probes pin only the diagnostic.

use super::{has_diagnostic, render_file, transform_expecting_diagnostics};
use crate::statementlist::transform_statement_list;
use crate::tests::{render_list, repo_root, with_state};

/// `for (const { x } of objs)` routes through transformBindingName — the loop binding becomes a
/// `_binding` temp and the object destructure statements are unshifted into the body.
#[test]
#[ignore = "needs slice i: transform_binding_name and the binding pattern transforms"]
fn for_of_object_pattern_initializer() {
	let want = "local objs = { {
	x = 1,
}, {
	x = 2,
} }
local sum = 0
for _, _binding in objs do
	local x = _binding.x
	sum += x
end
print(sum)
";
	assert_eq!(render_file("forof", "src/objpattern.ts"), want);
}

/// defaults in the loop-header pattern emit the extract-then-nil-check shape inside the body, for
/// both array and object patterns.
#[test]
#[ignore = "needs slice i: transform_binding_name and the binding pattern transforms"]
fn for_of_pattern_defaults() {
	let want = "local entries = { { 1, nil }, { nil, 4 } }
local total = 0
for _, _binding in entries do
	local a = _binding[1]
	if a == nil then
		a = 10
	end
	local b = _binding[2]
	if b == nil then
		b = 20
	end
	total += a + b
end
local objs = { {
	x = 1,
}, {} }
for _, _binding in objs do
	local x = _binding.x
	if x == nil then
		x = 5
	end
	total += x
end
print(total)
";
	assert_eq!(render_file("forof", "src/defaults.ts"), want);
}

/// prereqs of the iterable expression (`i++` inside the call argument) land BEFORE the loop; the
/// call itself stays inline in the for header.
#[test]
#[ignore = "needs slice i: transform_binding_name (identifier for-of names)"]
fn for_of_expression_prereqs() {
	let want = "local function make(n)
	return { n, n + 1 }
end
local i = 0
local sum = 0
local _original = i
i += 1
for _, v in make(_original) do
	sum += v
end
print(sum, i)
";
	assert_eq!(render_file("forof", "src/prereqs.ts"), want);
}

/// the inner loop's unnamed discard temp dedupes against the outer one (`_`, then `_1`).
#[test]
#[ignore = "needs slice i: transform_binding_name (identifier for-of names)"]
fn nested_for_of() {
	let want = "local grid = { { 1, 2 }, { 3, 4 } }
local sum = 0
for _, row in grid do
	for _1, cell in row do
		sum += cell
	end
end
print(sum)
";
	assert_eq!(render_file("forof", "src/nested.ts"), want);
}

/// `continue` passes straight through the statement list — for-of has no loop finalizers (unlike
/// for-in over keys in older targets), so no finalizer interplay exists.
#[test]
#[ignore = "needs slice i: transform_binding_name (identifier for-of names)"]
fn for_of_continue() {
	let want = "local nums = { 1, 2, 3, 4 }
local odds = 0
for _, n in nums do
	if n % 2 == 0 then
		continue
	end
	odds += n
end
print(odds)
";
	assert_eq!(render_file("forof", "src/continue.ts"), want);
}

/// buildSetLoop — a single generic-for binding directly over the Set expression.
#[test]
#[ignore = "needs slice i: transform_binding_name (identifier for-of names)"]
fn for_of_set() {
	let want = "for x in s do
	print(x)
end
";
	assert_eq!(render_file("forof", "src/set.ts"), want);
}

/// the `[k, v]` inline-destructure fast path — the pattern elements become the generic-for
/// bindings directly, no `_binding` temp.
#[test]
#[ignore = "needs slice i: transform_binding_name (identifier for-of names)"]
fn for_of_map_inline_pattern() {
	let want = "for k, v in m do
	print(k, v)
end
";
	assert_eq!(render_file("forof", "src/map.ts"), want);
}

/// the non-inline Map shapes — plain id binds `_k, _v` and reconstitutes `local pair = { _k, _v
/// }`; the expression-form initializer assigns `entry = { _k, _v }` through the writable target; a
/// default in the inline pattern nil-checks in the body; a nested pattern in the inline pattern
/// destructures a `_binding` temp.
#[test]
#[ignore = "needs slice i: transform_binding_name and the binding pattern transforms"]
fn for_of_map_fallbacks() {
	let want = "for _k, _v in m do
	local pair = { _k, _v }
	print(pair[1], pair[2])
end
local entry
for _k, _v in m do
	entry = { _k, _v }
	print(entry[1])
end
for k2, v2 in mOpt do
	if v2 == nil then
		v2 = 5
	end
	print(k2, v2)
end
for k3, _binding in nestedMap do
	local a1 = _binding[1]
	local b1 = _binding[2]
	print(k3, a1, b1)
end
";
	assert_eq!(render_file("forof", "src/mapfallback.ts"), want);
}

/// buildGeneratorLoop over a generator-object call — the .next()/.done/.value protocol; the call
/// expression is indexable so `.next` attaches directly.
#[test]
#[ignore = "needs slice i: transform_binding_name (identifier for-of names)"]
fn for_of_generator() {
	let want = "for _result in gen().next do
	if _result.done then
		break
	end
	local n = _result.value
	print(n)
end
";
	assert_eq!(render_file("forof", "src/generator.ts"), want);
}

/// a binding pattern over a generator routes the `_result.value` through a `_binding` temp; an
/// object pattern over a Set routes through transformBindingName the same way.
#[test]
#[ignore = "needs slice i: transform_binding_name and the binding pattern transforms"]
fn for_of_generator_patterns() {
	let want = "for _result in pairsGen.next do
	if _result.done then
		break
	end
	local _binding = _result.value
	local a = _binding[1]
	local b = _binding[2]
	print(a, b)
end
for _binding in objSet do
	local x = _binding.x
	print(x)
end
";
	assert_eq!(render_file("forof", "src/genpattern.ts"), want);
}

/// `IterableFunction<LuaTuple<T>>` tuple-arity introspection with a plain-id initializer — one
/// generic-for temp per tuple slot. The first label `end` is a valid TS identifier but NOT a valid
/// Luau identifier (luau.IsValidIdentifier rejects reserved words), so it falls back to "element";
/// the second label `value` is used.
#[test]
#[ignore = "needs slice i: transform_binding_name (identifier for-of names)"]
fn for_of_tuple_labels() {
	let want = "for _element, _value in it1 do
	local item = { _element, _value }
	print(item[1], item[2])
end
";
	assert_eq!(render_file("forof", "src/tuplelabels.ts"), want);
}

/// the while-true protocol fallbacks — a rest-element tuple (unknown arity; the iterFunc temp
/// takes its name from the expression), and an expression-form initializer over a KNOWN-arity
/// tuple (declaration-list gate fails; valueToIdStr of a call is empty so the temp falls back to
/// "iterFunc").
#[test]
#[ignore = "needs slice i: transform_binding_name (identifier for-of names)"]
fn for_of_tuple_while_loop() {
	let want = "local _restIt = restIt
while true do
	local packed = { _restIt() }
	if #packed == 0 then
		break
	end
	print(packed[1])
end
local tup
local _iterFunc = make()
while true do
	local _v = { _iterFunc() }
	if #_v == 0 then
		break
	end
	tup = _v
	print(tup[2])
end
";
	assert_eq!(render_file("forof", "src/tuplewhile.ts"), want);
}

/// `for (const i of $range(1, 3))` builds a Luau numeric for. findRangeMacro must hit BEFORE the
/// expression is transformed — if the order were wrong, the $range call macro would raise
/// noRangeMacroOutsideForOf and the Iterable<number> return type would raise noIterableIteration
/// from the builder dispatch.
#[test]
#[ignore = "needs slice i: transform_binding_name (identifier for-of names)"]
fn for_of_range_macro() {
	let want = "for i = 1, 3 do
	print(i)
end
";
	assert_eq!(render_file("forof", "src/range.ts"), want);
}

/// $range argument prereqs (`i++` needs an `_original` temp) land BEFORE the numeric for.
#[test]
#[ignore = "needs slice i: transform_binding_name (identifier for-of names)"]
fn for_of_range_macro_prereqs() {
	let want = "local i = 0
local _original = i
i += 1
for n = _original, 5 do
	print(n)
end
";
	assert_eq!(render_file("forof", "src/rangeprereq.ts"), want);
}
/// A variadic `LuaTuple` iterator takes the unknown-arity while loop.
#[test]
#[ignore = "needs slice i: transform_binding_name (identifier for-of names)"]
fn for_of_variadic_lua_tuple_uses_unknown_arity_loop() {
	let want = "local function collect(iterator)
	local _iterator = iterator
	while true do
		local entry = { _iterator() }
		if #entry == 0 then
			break
		end
		print(entry[1])
	end
end
";
	assert_eq!(render_file("forof", "src/tuplevariadic.ts"), want);
}

/// A union iterable that is not definitely any single builder type hits upstream's
/// `type.isUnion()` arm.
#[test]
fn for_of_no_macro_union_diagnostic() {
	let diagnostics = transform_expecting_diagnostics("forof", "src/union.ts");
	assert!(
		has_diagnostic(&diagnostics, "noMacroUnion", "Macro cannot be applied to a union type!"),
		"no noMacroUnion diagnostic; got: {diagnostics:?}"
	);
}

/// Iterating a bare `Iterable<T>` is upstream's own error.
#[test]
fn for_of_no_iterable_iteration_diagnostic() {
	let diagnostics = transform_expecting_diagnostics("forof", "src/iterable.ts");
	assert!(
		has_diagnostic(
			&diagnostics,
			"noIterableIteration",
			"Iterating on Iterable<T> is not supported!"
		),
		"no noIterableIteration diagnostic; got: {diagnostics:?}"
	);
}

/// Diff fixture `29_iteration` is an upstream-corrected ledger row: its golden is frozen, and the
/// corrected iterator-rest destructuring is checked at runtime by Go's
/// `TestPinnedCompatibility/iterator-rest` (slice i and the conformance stage). The for-of half
/// of the fixture is untouched by that correction, so its statements must still compile to the
/// golden's lines byte for byte.
#[test]
#[ignore = "needs slice i: transform_binding_name (identifier for-of names)"]
fn iteration_fixture_for_of_half_matches_the_golden() {
	let golden =
		std::fs::read_to_string(repo_root().join("testdata/diff/golden/29_iteration.luau")).expect("the golden reads");
	let marker = "-- destructuring FROM a Map";
	let start = golden.find('\n').expect("the golden has a header") + 1;
	let end = golden.find(marker).expect("the golden has the destructuring half");
	let want = &golden[start..end];

	let got = with_state(&repo_root().join("testdata/diff/project"), "src/29_iteration.ts", |s| {
		let file = s.source_file;
		let statements = file.statements().nodes();
		let for_of_half = statements
			.iter()
			.position(|&statement| {
				rotor_tsrs::ast::is_variable_statement(statement)
					&& statement
						.as_variable_statement()
						.declaration_list
						.as_variable_declaration_list()
						.declarations
						.nodes()[0]
						.as_variable_declaration()
						.name
						.kind() == crate::prelude::Kind::ArrayBindingPattern
			})
			.expect("the fixture has the destructuring half");
		let list = transform_statement_list(s, Some(file.as_node()), &statements[..for_of_half], None);
		let diagnostics = s.diags.flush();
		assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
		render_list(s, list)
	});
	assert_eq!(got, want);
}
