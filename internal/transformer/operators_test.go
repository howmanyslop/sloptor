package transformer_test

import (
	"path/filepath"
	"testing"

	"rotor/internal/luau/render"
	"rotor/internal/transformer"
)

// TestOperatorsAgainstRbxtsc covers the Task 9 operator paths the diff
// fixtures don't isolate:
//
//   - `in`               -> right[left] ~= nil
//   - `&=`               -> read-modify-write fallback (no simple Luau form):
//     x = bit32.band(x, y)
//   - `!` on number      -> truthiness-inverted not (x ~= 0 and x == x and x)
//   - `??` possibly-false -> inline `or` refused; `_condition` temp + if-chain
//     (for boolean|undefined, a fresh false const, and an undefined-narrowed
//     boolean|undefined — all three refused)
//   - `+=` with string LHS -> `..=` with getAssignableValue tostring() around
//     a non-string RHS
//   - `instanceof`       -> TS.instanceof(left, right) + UsesRuntimeLib
//   - `~`                -> bit32.bnot(x)
//   - `**=`              -> simple-map compound `^=` (stays an assignment)
//   - `>>=`              -> read-modify-write x = bit32.arshift(x, y)
//
// The expected text below is byte-for-byte what rbxtsc 3.0.0 emits for
// testdata/operators/src/operators.ts (verified by compiling the same source
// through testdata/diff/project; header, runtime-lib require, and the
// trailing `return nil` stripped — those belong to TransformSourceFile, not
// the statement list under test).
func TestOperatorsAgainstRbxtsc(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "operators"), "src/operators.ts")

	statements := transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)

	want := `local hasKey = obj[key] ~= nil
flags = bit32.band(flags, mask)
local notNum = not (num ~= 0 and num == num and num)
local _condition = maybeBool
if _condition == nil then
	_condition = true
end
local coalesced = _condition
str ..= tostring(num)
local isFoo = TS.instanceof(inst, Foo)
local fresh = false
local _condition_1 = fresh
if _condition_1 == nil then
	_condition_1 = true
end
local coalesced2 = _condition_1
local cond
local _condition_2 = cond
if _condition_2 == nil then
	_condition_2 = false
end
local coalesced3 = _condition_2
local inverted = bit32.bnot(num)
num ^= mask
num = bit32.arshift(num, mask)
`
	if got := render.RenderAST(statements); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}

	// instanceof is the only construct above that touches the runtime lib.
	if !s.UsesRuntimeLib {
		t.Errorf("UsesRuntimeLib = false, want true (instanceof emits TS.instanceof)")
	}

	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}
