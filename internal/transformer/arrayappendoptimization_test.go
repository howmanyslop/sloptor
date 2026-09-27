package transformer_test

import (
	"path/filepath"
	"testing"

	"rotor/internal/luau/render"
	"rotor/internal/transformer"
)

func TestOptimizedArrayAppendsEmitsLoopCarriedIndex(t *testing.T) {
	s := buildState(t, filepath.Join("testdata", "arrayappendoptimization"), "src/basic.ts")
	s.OptimizedArrayAppends = true

	statements := transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)

	want := `local result = {}
local _resultLength = 0
for _, value in source do
	-- ▼ Array.push ▼
	_resultLength += 1
	result[_resultLength] = value
	-- ▲ Array.push ▲
end
print(result)
`
	if got := render.RenderAST(statements); got != want {
		t.Errorf("rendered output differs from optimized append form:\ngot:\n%s\nwant:\n%s", got, want)
	}

	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}
