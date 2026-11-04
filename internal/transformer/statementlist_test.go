package transformer_test

import (
	"path/filepath"
	"testing"

	"rotor/internal/luau/render"
	"rotor/internal/transformer"
	"rotor/tsgo/ast"
)

func TestBlockTrailingCommentsArePreserved(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "control"), "src/trailingcomment.ts")

	declaration := s.SourceFile.Statements.Nodes[0]
	if !ast.IsFunctionDeclaration(declaration) {
		t.Fatalf("expected FunctionDeclaration first, got %v", declaration.Kind)
	}
	body := declaration.AsFunctionDeclaration().Body
	statements := transformer.TransformStatementList(s, body, body.AsBlock().Statements.Nodes, nil)
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Fatalf("unexpected diagnostics: %v", ds)
	}

	want := "print(\"body\")\n-- trailing block comment\n"
	if got := render.RenderAST(statements); got != want {
		t.Fatalf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}
