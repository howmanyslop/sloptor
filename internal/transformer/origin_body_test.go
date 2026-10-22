package transformer

import (
	"testing"

	"rotor/internal/luau"
)

func TestParameterPrologueOrigins(t *testing.T) {
	tests := []struct {
		name, source   string
		start, closing luau.Position
		prologues      int
	}{
		{"default", "function f(value = 1) { return value; }", luau.Position{Line: 0, Column: 11}, luau.Position{Line: 0, Column: 19}, 1},
		{"destructuring", "function f([x, y]: number[]) { return x + y; }", luau.Position{Line: 0, Column: 11}, luau.Position{Line: 0, Column: 26}, 2},
		{"rest capture", "function f(...rest: number[]) { return rest; }", luau.Position{Line: 0, Column: 11}, luau.Position{Line: 0, Column: 27}, 1},
		{"flattened rest default", "function f(...[x = 1]: [number?]) { return x; }", luau.Position{Line: 0, Column: 11}, luau.Position{Line: 0, Column: 31}, 1},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			function := transformOriginSource(t, test.source).Head.Value.(*luau.FunctionDeclaration)
			statements := function.Statements.ToSlice()
			if len(statements) != test.prologues+1 {
				t.Fatalf("body statements = %d, want %d", len(statements), test.prologues+1)
			}
			for _, statement := range statements[:test.prologues] {
				assertNodeOrigin(t, statement, test.start, test.closing)
				if statement.Origin() != statements[0].Origin() {
					t.Error("prologues for one parameter must share an origin")
				}
				if conditional, ok := statement.(*luau.IfStatement); ok && conditional.Statements.Head.Value.Origin() != nil {
					t.Error("synthetic initializer assignment must inherit the parameter origin")
				}
			}
		})
	}
}
