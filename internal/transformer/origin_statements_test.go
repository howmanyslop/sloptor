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

func TestStatementOriginsIncludePrereqsAndHoists(t *testing.T) {
	t.Run("prereqs share the statement origin", func(t *testing.T) {
		function := transformOriginSource(t, "function f(a?: number) {\n\tconst result = a ?? 1;\n\treturn result;\n}").Head.Value.(*luau.FunctionDeclaration)
		body := function.Statements.ToSlice()
		if len(body) != 4 {
			t.Fatalf("body statements = %d, want local, if, local, return", len(body))
		}
		for _, statement := range body[:3] {
			assertNodeOrigin(t, statement, luau.Position{Line: 1, Column: 1}, luau.Position{Line: 1, Column: 22})
			if statement.Origin() != body[0].Origin() {
				t.Error("prereqs and result must share their TS statement's origin")
			}
		}
	})
	t.Run("hoist shares the triggering statement origin", func(t *testing.T) {
		statements := transformOriginSource(t, "function first() {\n\treturn later();\n}\nfunction later() { return 1; }").ToSlice()
		if len(statements) != 3 {
			t.Fatalf("statements = %d, want hoist and two functions", len(statements))
		}
		if _, ok := statements[0].(*luau.VariableDeclaration); !ok {
			t.Fatalf("first statement = %T, want hoist declaration", statements[0])
		}
		for _, statement := range statements[:2] {
			assertNodeOrigin(t, statement, luau.Position{Line: 0, Column: 0}, luau.Position{Line: 2, Column: 0})
		}
		if statements[0].Origin() != statements[1].Origin() {
			t.Error("hoist and function must share the triggering statement's origin")
		}
	})
}
