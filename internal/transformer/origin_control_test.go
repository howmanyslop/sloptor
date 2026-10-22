package transformer

import (
	"fmt"
	"testing"

	"rotor/internal/luau"
)

func TestElseIfOrigins(t *testing.T) {
	for _, condition := range []string{"a === 1", "(a ?? 0) === 1"} {
		t.Run(condition, func(t *testing.T) {
			source := fmt.Sprintf("function f(a?: number) {\n\tif (a === 0) {\n\t\treturn 0;\n\t} else if (%s) {\n\t\treturn 1;\n\t}\n\treturn 2;\n}", condition)
			function := transformOriginSource(t, source).Head.Value.(*luau.FunctionDeclaration)
			outer := function.Statements.Head.Value.(*luau.IfStatement)
			assertNodeOrigin(t, outer, luau.Position{Line: 1, Column: 1}, luau.Position{Line: 5, Column: 1})
			if linked, ok := outer.ElseBody.(*luau.IfStatement); ok {
				assertNodeOrigin(t, linked, luau.Position{Line: 3, Column: 8}, luau.Position{Line: 5, Column: 1})
			} else {
				statements := outer.ElseBody.(*luau.List[luau.Statement]).ToSlice()
				if len(statements) != 3 {
					t.Fatalf("else statements = %d, want local, if, if", len(statements))
				}
				for _, statement := range statements {
					assertNodeOrigin(t, statement, luau.Position{Line: 3, Column: 8}, luau.Position{Line: 5, Column: 1})
				}
				if statements[0].Origin() != statements[1].Origin() {
					t.Error("else-if prereqs must share their origin")
				}
				if statements[1].(*luau.IfStatement).Statements.Head.Value.Origin() != nil {
					t.Error("nested nullish assignment must inherit the inner if origin")
				}
			}
		})
	}
}
