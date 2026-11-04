package transformer_test

import (
	"path/filepath"
	"testing"

	"rotor/internal/luau"
	"rotor/internal/luau/render"
	"rotor/internal/transformer"
)

func TestDeleteExpressionEmitsNilAssignmentAndStatementValueSemantics(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "operators"), "src/delete.ts")
	statements := s.SourceFile.Statements.Nodes

	stmtDelete := statements[2].AsExpressionStatement().Expression
	stmtResult, stmtPrereqs := s.Capture(func() luau.Expression {
		return transformer.TransformExpression(s, stmtDelete)
	})
	if !luau.IsNone(stmtResult) {
		t.Fatalf("statement delete result = %v, want None", stmtResult.Kind())
	}
	if got, want := render.RenderAST(stmtPrereqs), "obj[key] = nil\n"; got != want {
		t.Fatalf("statement delete prereqs = %q, want %q", got, want)
	}

	valueDelete := statements[3].AsVariableStatement().DeclarationList.AsVariableDeclarationList().Declarations.Nodes[0].AsVariableDeclaration().Initializer
	valueResult, valuePrereqs := s.Capture(func() luau.Expression {
		return transformer.TransformExpression(s, valueDelete)
	})
	if got, want := render.Render(render.NewRenderState(), valueResult), "true"; got != want {
		t.Fatalf("value delete result = %q, want %q", got, want)
	}
	if got, want := render.RenderAST(valuePrereqs), "obj.foo = nil\n"; got != want {
		t.Fatalf("value delete prereqs = %q, want %q", got, want)
	}

	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Fatalf("unexpected diagnostics: %v", ds)
	}
}

func TestDeleteStatementListAgainstRbxtsc(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "operators"), "src/delete.ts")
	got := render.RenderAST(transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil))
	want := "obj[key] = nil\nobj.foo = nil\nlocal deleted = true\n"
	if got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}
