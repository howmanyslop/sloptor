package transformer_test

import (
	"path/filepath"
	"testing"

	"rotor/internal/luau/render"
	"rotor/internal/transformer"
)

// TestConditionalAgainstRbxtsc covers the transformConditionalExpression
// paths the diff fixtures don't isolate (13_mixed only hits the inline
// IfExpression form):
//
//   - statement position (isUsedAsStatement)   -> plain if/else statement
//   - branch prereqs (i++ in whenTrue)         -> undeclared `_result` temp
//     assigned in both branches
//   - TS-truthy condition (n: number)          -> `n ~= 0 and n == n and n`
//     wrap inside the inline IfExpression
//   - conditional inside a for-incrementor     -> IfExpression on the RHS of
//     the shouldIncrement assignment (parent is the assignment, NOT the for)
//
// The expected text below is byte-for-byte what rbxtsc 3.0.0 emits for this
// source (verified by compiling the same statements through
// testdata/diff/project; header and trailing `return nil` stripped — those
// belong to TransformSourceFile, not the statement list under test).
func TestConditionalAgainstRbxtsc(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "conditional"), "src/conditional.ts")

	statements := transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)

	want := `local i = 5
if flag then
	print("a")
else
	print("b")
end
local _result
if flag then
	local _original = i
	i += 1
	_result = _original
else
	_result = 0
end
local x = _result
local y = if n ~= 0 and n == n and n then 1 else 2
do
	local j = 0
	local _shouldIncrement = false
	while true do
		if _shouldIncrement then
			j = if flag then j + 1 else j + 2
		else
			_shouldIncrement = true
		end
		if not (j < 3) then
			break
		end
		print(j)
	end
end
print(i, x, y)
`
	if got := render.RenderAST(statements); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}

	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}
