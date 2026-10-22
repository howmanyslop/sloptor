package transformer

import (
	"testing"

	"rotor/internal/luau"
)

func TestConciseArrowBodyOrigins(t *testing.T) {
	statements := transformOriginSource(t, "const callback = (a?: number) =>\n\ta ?? 1;")
	function := statements.Head.Value.(*luau.VariableDeclaration).Right.(*luau.FunctionExpression)
	body := function.Statements.ToSlice()
	if len(body) != 3 {
		t.Fatalf("body statements = %d, want local, if, return", len(body))
	}
	for _, statement := range body {
		assertNodeOrigin(t, statement, luau.Position{Line: 1, Column: 1}, luau.Position{Line: 1, Column: 6})
		if statement.Origin() != body[0].Origin() {
			t.Error("concise-arrow prereqs and return must share an origin")
		}
	}
	conditional := body[1].(*luau.IfStatement)
	if conditional.Statements.Head.Value.Origin() != nil {
		t.Error("nested synthetic nullish assignment must inherit the body origin")
	}
}
