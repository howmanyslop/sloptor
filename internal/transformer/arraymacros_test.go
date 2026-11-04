package transformer_test

import (
	"os"
	"path/filepath"
	"testing"

	"rotor/internal/luau/render"
	"rotor/internal/transformer"
)

// TestArrayMacrosAgainstRbxtsc covers the long tail of the ReadonlyArray and
// Array macro tables (Phase 3b Task 4) — every one of the 24 macros at least
// once, plus the contract points the diff fixture (27_arraymacros) does not
// isolate:
//
//   - the callback invocation shape: user callbacks receive
//     `(value, index, array)` = `cb(_v, _k - 1, exp)` — the 1-based loop key
//     adjusted to the 0-based JS index (reduce: `cb(_result, exp[_i], _i - 1,
//     exp)` from a numeric for)
//   - an identifier callback is NOT pushed (`isBig(_v, _k - 1, arr)`), so
//     filter emits only 2 declarations and the markers still appear
//   - push-return-used: statement-position push emits bare `table.insert`s,
//     value position appends `local len = #list`
//   - reduce without initialValue: empty-array `error(...)` guard, seed from
//     `arr[1]`, numeric for starts at 2; with initialValue: seed from the arg,
//     for starts at 1; callback pushed unconditionally AFTER the result temp
//   - the +1 index adjustments: move(0,1,3) -> `table.move(arr, 1, 2, 4)`
//     (target arg absent), insert(0,..) -> `table.insert(list, 1, ..)`,
//     indexOf -> `(table.find(arr, 20) or 0) - 1`
//   - a mutable-binding index arg is pinned by runCallMacro BEFORE the macro
//     offsets it: `local _arg0 = i - 1` then `table.insert(list, _arg0 + 1, 7)`
//     (the fold does NOT collapse through the temp — upstream identical)
//   - statement-position variants of pop (`list[#list] = nil` one-liner),
//     shift wrapped in parens as a print argument (Roblox void-call fix),
//     unorderedRemove's unconditional `local _value`, comparator-less sort,
//     clear
//
// The expected text (testdata/arraymacros/expected.luau) is byte-for-byte
// what rbxtsc 3.0.0 emits for this source (verified through the
// testdata/diff/project oracle; header and trailing `return nil` stripped —
// those belong to TransformSourceFile, not the statement list under test).
func TestArrayMacrosAgainstRbxtsc(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "arraymacros"), "src/arraymacros.ts")

	statements := transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)

	wantBytes, err := os.ReadFile(filepath.Join("..", "..", "testdata", "transformer-unit", "arraymacros", "expected.luau"))
	if err != nil {
		t.Fatal(err)
	}
	want := string(wantBytes)

	if got := render.RenderAST(statements); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}

	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}
