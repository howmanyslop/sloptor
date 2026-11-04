package transformer_test

import (
	"path/filepath"
	"strings"
	"testing"

	"rotor/internal/luau/render"
	"rotor/internal/transformer"
)

// TestCallsAgainstRbxtsc covers the Task 10 call/access paths the diff
// fixtures don't isolate:
//
//   - interface method (`this` param / method signature) -> `obj:method()`
//   - callback property                                   -> `obj.callback(1)`
//   - method named with a Luau keyword (`then`)           -> `obj["then"](obj)`
//     (PropertyAccessExpression + explicit self, no `:` sugar)
//   - element call on a callback                          -> `lookup["a b"](5)`
//   - compound assignment to element access with the +1
//     array offset (constant-folded and dynamic)          -> `arr[1] += 1`,
//     `arr[i + 1] *= 2`
//   - offset folding of an existing `- 1`                 -> `arr[i - 1]` read
//     becomes `arr[i]`
//   - ++ on element access in statement position          -> `arr[i + 1] += 1`
//   - argument prereqs (i++) pin a mutable callee base    -> `local _mutObj = ...`
//
// The expected text below is byte-for-byte what rbxtsc 3.0.0 emits for this
// source (verified by compiling the same statements through
// testdata/diff/project; header and trailing `return nil` stripped — those
// belong to TransformSourceFile, not the statement list under test).
func TestCallsAgainstRbxtsc(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "calls"), "src/calls.ts")

	statements := transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)

	want := `obj:method()
obj.callback(1)
obj["then"](obj)
lookup["a b"](5)
local arr = { 10, 20, 30 }
arr[1] += 1
local i = 2
arr[i + 1] *= 2
local prev = arr[i]
arr[i + 1] += 1
local _mutObj = mutObj
local _original = i
i += 1
_mutObj.cb(_original)
print(obj, lookup, arr, prev, i, mutObj)
`
	if got := render.RenderAST(statements); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}

	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}

	// isMethod consults the program-wide per-symbol cache; the method-bearing
	// types above must have left entries behind.
	if len(s.Multi.IsMethodCache) == 0 {
		t.Errorf("IsMethodCache is empty, want per-symbol isMethod entries")
	}
}

// TestPropertyCallMacro: `bag.add(...)` resolves to the compiler-types
// Set.add method symbol and runs the Set.add property-call macro (Phase 3b
// Task 5) — in statement position that is the bare assignment
// `bag[4] = true`, never a silently-wrong method call `bag:add(4)` (which is
// what this site detected via a rotorNotYetSupported diagnostic before the
// Set/Map tables landed).
func TestPropertyCallMacro(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "calls"), "src/macro.ts")

	statements := transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)

	want := "local bag = {}\nbag[4] = true\n"
	if got := render.RenderAST(statements); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}

	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}

// TestNoMixedTypeCall: one function type declaring both a callback signature
// (`this: void`) and a method signature (`this: Mixed`) is upstream's
// noMixedTypeCall error; the call still emits as a method (`mixed:fn(1)`).
func TestNoMixedTypeCall(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "calls"), "src/mixed.ts")

	statements := transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)

	want := "mixed:fn(1)\n"
	if got := render.RenderAST(statements); got != want {
		t.Errorf("rendered output:\ngot:\n%s\nwant:\n%s", got, want)
	}

	ds := s.Diags.Flush()
	found := false
	for _, d := range ds {
		if d.Code == "noMixedTypeCall" {
			found = true
		}
	}
	if !found {
		t.Errorf("expected noMixedTypeCall diagnostic; got: %v", ds)
	}
}

func TestContextualMethodArrayCallbacksInjectSelf(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "calls"), "src/contextual_method.ts")
	got := render.RenderAST(transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil))
	for _, want := range []string{"function(self)", "function(self, arg)"} {
		if !strings.Contains(got, want) {
			t.Fatalf("rendered output missing %q:\n%s", want, got)
		}
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}
