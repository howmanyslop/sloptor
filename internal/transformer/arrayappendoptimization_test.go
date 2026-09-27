package transformer_test

import (
	"path/filepath"
	"strings"
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

func TestOptimizedArrayAppendsHandlesLabeledLoops(t *testing.T) {
	s := buildState(t, filepath.Join("testdata", "arrayappendoptimization"), "src/labeled.ts")
	s.OptimizedArrayAppends = true

	statements := transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)
	got := render.RenderAST(statements)

	if strings.Contains(got, "table.insert") {
		t.Errorf("labeled loop retained table.insert:\n%s", got)
	}
	if !strings.Contains(got, "local _resultLength = 0") || !strings.Contains(got, "result[_resultLength] = value") {
		t.Errorf("labeled loop did not use a loop-carried index:\n%s", got)
	}

	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}

func TestOptimizedArrayAppendsSkipsZeroArgumentPush(t *testing.T) {
	s := buildState(t, filepath.Join("testdata", "arrayappendoptimization"), "src/zero.ts")
	s.OptimizedArrayAppends = true

	statements := transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)

	want := `local result = {}
for index = 0, 2 do
	local _ = #result
end
`
	if got := render.RenderAST(statements); got != want {
		t.Errorf("zero-argument push should retain the baseline output:\ngot:\n%s\nwant:\n%s", got, want)
	}

	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}
