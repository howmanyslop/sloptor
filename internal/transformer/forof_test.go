package transformer_test

import (
	"path/filepath"
	"testing"

	"rotor/internal/luau/render"
	"rotor/internal/transformer"
)

// The expected text in every render test below is byte-for-byte what rbxtsc
// 3.0.0 emits for the same source (verified via scratch files compiled
// through testdata/diff/project — see tools/oracle/oracle.ps1 for the
// technique). Diagnostic probes have no goldens: rbxtsc aborts compilation on
// them, so only the diagnostic itself is pinned.

func renderForOfFile(t *testing.T, relPath string) string {
	t.Helper()
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "forof"), relPath)
	statements := transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Fatalf("unexpected diagnostics: %v", ds)
	}
	return render.RenderAST(statements)
}

// forOfDiagnostics transforms a probe file and returns rotor's diagnostics.
// The probes iterate non-array types (Set/Map/generators, Set unions,
// Iterable<T>, $range's Iterable<number> return) — valid rbxtsc 3.0.0 input
// that historically tripped a TS5->TS7 checker divergence: tsgo resolves the
// global `Iterable`/`IterableIterator` types with arity 3 while
// @rbxts/compiler-types declares the TS5 arity-1 shape, producing spurious
// "can only be iterated through when using '--downlevelIteration'"
// diagnostics. compile.SanitizeFS now rewrites the compiler-types interfaces
// to arity 3 (RewriteIterableArity), so the checker computes real iteration
// types and the probes must build with ZERO checker diagnostics — buildState
// is strict; any regression of the overlay resurfaces here.
func forOfDiagnostics(t *testing.T, relPath string) []transformer.Diagnostic {
	t.Helper()
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "forof"), relPath)
	transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)
	return s.Diags.Flush()
}

// TestForOfObjectPatternInitializer: `for (const { x } of objs)` routes
// through transformBindingName — the loop binding becomes a `_binding` temp
// and the object destructure statements are unshifted into the body.
func TestForOfObjectPatternInitializer(t *testing.T) {
	want := `local objs = { {
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
`
	if got := renderForOfFile(t, "src/objpattern.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestForOfPatternDefaults: defaults in the loop-header pattern emit the
// extract-then-nil-check shape inside the body, for both array and object
// patterns.
func TestForOfPatternDefaults(t *testing.T) {
	want := `local entries = { { 1, nil }, { nil, 4 } }
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
`
	if got := renderForOfFile(t, "src/defaults.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestForOfExpressionPrereqs: prereqs of the iterable expression (`i++`
// inside the call argument) land BEFORE the loop; the call itself stays
// inline in the for header.
func TestForOfExpressionPrereqs(t *testing.T) {
	want := `local function make(n)
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
`
	if got := renderForOfFile(t, "src/prereqs.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestNestedForOf: the inner loop's unnamed discard temp dedupes against the
// outer one (`_`, then `_1`).
func TestNestedForOf(t *testing.T) {
	want := `local grid = { { 1, 2 }, { 3, 4 } }
local sum = 0
for _, row in grid do
	for _1, cell in row do
		sum += cell
	end
end
print(sum)
`
	if got := renderForOfFile(t, "src/nested.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestForOfContinue: `continue` passes straight through the statement list —
// for-of has no loop finalizers (unlike for-in over keys in older targets),
// so no finalizer interplay exists.
func TestForOfContinue(t *testing.T) {
	want := `local nums = { 1, 2, 3, 4 }
local odds = 0
for _, n in nums do
	if n % 2 == 0 then
		continue
	end
	odds += n
end
print(odds)
`
	if got := renderForOfFile(t, "src/continue.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestForOfNoMacroUnionDiagnostic: a union iterable that is not definitely
// any single builder type hits upstream's `type.isUnion()` arm. rbxtsc 3.0.0
// reports exactly "Macro cannot be applied to a union type!".
func TestForOfNoMacroUnionDiagnostic(t *testing.T) {
	ds := forOfDiagnostics(t, "src/union.ts")
	if !hasDiagnostic(ds, "noMacroUnion", "Macro cannot be applied to a union type!") {
		t.Errorf("no noMacroUnion diagnostic; got: %v", ds)
	}
}

// TestForOfNoIterableIterationDiagnostic: iterating a bare Iterable<T> is
// upstream's own error. rbxtsc 3.0.0 reports exactly "Iterating on
// Iterable<T> is not supported! You must use a more specific type.".
func TestForOfNoIterableIterationDiagnostic(t *testing.T) {
	ds := forOfDiagnostics(t, "src/iterable.ts")
	if !hasDiagnostic(ds, "noIterableIteration", "Iterating on Iterable<T> is not supported!") {
		t.Errorf("no noIterableIteration diagnostic; got: %v", ds)
	}
}

// TestForOfSet: buildSetLoop — a single generic-for binding directly over the
// Set expression.
func TestForOfSet(t *testing.T) {
	want := `for x in s do
	print(x)
end
`
	if got := renderForOfFile(t, "src/set.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestForOfMapInlinePattern: the `[k, v]` inline-destructure fast path — the
// pattern elements become the generic-for bindings directly, no `_binding`
// temp.
func TestForOfMapInlinePattern(t *testing.T) {
	want := `for k, v in m do
	print(k, v)
end
`
	if got := renderForOfFile(t, "src/map.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestForOfMapFallbacks: the non-inline Map shapes — plain id binds `_k, _v`
// and reconstitutes `local pair = { _k, _v }`; the expression-form initializer
// assigns `entry = { _k, _v }` through the writable target; a default in the
// inline pattern nil-checks in the body; a nested pattern in the inline
// pattern destructures a `_binding` temp.
func TestForOfMapFallbacks(t *testing.T) {
	want := `for _k, _v in m do
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
`
	if got := renderForOfFile(t, "src/mapfallback.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestForOfGenerator: buildGeneratorLoop over a generator-object call — the
// .next()/.done/.value protocol; the call expression is indexable so `.next`
// attaches directly.
func TestForOfGenerator(t *testing.T) {
	want := `for _result in gen().next do
	if _result.done then
		break
	end
	local n = _result.value
	print(n)
end
`
	if got := renderForOfFile(t, "src/generator.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestForOfGeneratorPatterns: a binding pattern over a generator routes the
// `_result.value` through a `_binding` temp; an object pattern over a Set
// routes through transformBindingName the same way.
func TestForOfGeneratorPatterns(t *testing.T) {
	want := `for _result in pairsGen.next do
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
`
	if got := renderForOfFile(t, "src/genpattern.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestForOfTupleLabels: IterableFunction<LuaTuple<T>> tuple-arity
// introspection with a plain-id initializer — one generic-for temp per tuple
// slot. The first label `end` is a valid TS identifier but NOT a valid Luau
// identifier (luau.IsValidIdentifier rejects reserved words), so it falls
// back to "element"; the second label `value` is used.
func TestForOfTupleLabels(t *testing.T) {
	want := `for _element, _value in it1 do
	local item = { _element, _value }
	print(item[1], item[2])
end
`
	if got := renderForOfFile(t, "src/tuplelabels.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestForOfTupleWhileLoop: the while-true protocol fallbacks — a rest-element
// tuple (unknown arity; the iterFunc temp takes its name from the
// expression), and an expression-form initializer over a KNOWN-arity tuple
// (declaration-list gate fails; valueToIdStr of a call is empty so the temp
// falls back to "iterFunc").
func TestForOfTupleWhileLoop(t *testing.T) {
	want := `local _restIt = restIt
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
`
	if got := renderForOfFile(t, "src/tuplewhile.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestForOfRangeMacro: `for (const i of $range(1, 3))` builds a Luau numeric
// for. findRangeMacro must hit BEFORE the expression is transformed — if the
// order were wrong, the $range call macro would raise
// noRangeMacroOutsideForOf and the Iterable<number> return type would raise
// noIterableIteration from the builder dispatch.
func TestForOfRangeMacro(t *testing.T) {
	want := `for i = 1, 3 do
	print(i)
end
`
	if got := renderForOfFile(t, "src/range.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestForOfRangeMacroPrereqs: $range argument prereqs (`i++` needs an
// `_original` temp) land BEFORE the numeric for.
func TestForOfRangeMacroPrereqs(t *testing.T) {
	want := `local i = 0
local _original = i
i += 1
for n = _original, 5 do
	print(n)
end
`
	if got := renderForOfFile(t, "src/rangeprereq.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}
