package transformer_test

import (
	"path/filepath"
	"testing"

	"rotor/internal/luau"
	"rotor/internal/luau/render"
	"rotor/internal/transformer"
)

func TestVoidExpressionPrereqsOperandAndYieldsNil(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "operators"), "src/void.ts")
	statements := s.SourceFile.Statements.Nodes

	valueVoid := statements[3].AsVariableStatement().DeclarationList.AsVariableDeclarationList().Declarations.Nodes[0].AsVariableDeclaration().Initializer
	result, prereqs := s.Capture(func() luau.Expression {
		return transformer.TransformExpression(s, valueVoid)
	})
	if got, want := render.Render(render.NewRenderState(), result), "nil"; got != want {
		t.Fatalf("value void result = %q, want %q", got, want)
	}
	if got, want := render.RenderAST(prereqs), "sideEffect()\n"; got != want {
		t.Fatalf("value void prereqs = %q, want %q", got, want)
	}

	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Fatalf("unexpected diagnostics: %v", ds)
	}
}

func TestVoidStatementListAgainstRbxtsc(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "operators"), "src/void.ts")
	got := render.RenderAST(transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil))
	want := "sideEffect()\nlocal _ = nil\nsideEffect()\nlocal value = nil\nlocal _1 = flag\nlocal dropped = nil\n"
	if got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}
