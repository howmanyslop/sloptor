package transformer_test

import (
	"path/filepath"
	"testing"

	"rotor/internal/luau/render"
	"rotor/internal/transformer"
)

func TestTaggedTemplateAgainstRbxtsc(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "operators"), "src/template.ts")
	got := render.RenderAST(transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil))
	want := "tag({ \"a\", \"b\", \"c\" }, 1, 2)\n"
	if got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}
