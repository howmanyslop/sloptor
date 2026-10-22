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

func TestSwitchClauseOrigins(t *testing.T) {
	function := transformOriginSource(t, "function f(n: number, a?: number) {\n\tswitch (n) {\n\t\tcase 1:\n\t\t\tn = 2;\n\t\tcase a ?? 2:\n\t\t\tbreak;\n\t}\n}").Head.Value.(*luau.FunctionDeclaration)
	loop := function.Statements.Tail.Value.(*luau.RepeatStatement)
	statements := loop.Statements.ToSlice()
	if len(statements) != 4 {
		t.Fatalf("switch statements = %d, want flag, case, prereq, case", len(statements))
	}
	first := statements[1].(*luau.IfStatement)
	prereq := statements[2].(*luau.IfStatement)
	second := statements[3].(*luau.IfStatement)
	for _, test := range []struct {
		name           string
		node           luau.Node
		start, closing luau.Position
	}{
		{"first case", first, luau.Position{Line: 2, Column: 2}, luau.Position{Line: 3, Column: 8}},
		{"fallthrough update", first.Statements.Tail.Value, luau.Position{Line: 2, Column: 2}, luau.Position{Line: 3, Column: 8}},
		{"second case", second, luau.Position{Line: 4, Column: 2}, luau.Position{Line: 5, Column: 8}},
		{"clause prereq", prereq, luau.Position{Line: 4, Column: 2}, luau.Position{Line: 5, Column: 8}},
		{"fallthrough condition assignment", prereq.Statements.Tail.Value, luau.Position{Line: 4, Column: 2}, luau.Position{Line: 5, Column: 8}},
	} {
		t.Run(test.name, func(t *testing.T) { assertNodeOrigin(t, test.node, test.start, test.closing) })
	}
	if statements[0].Origin() != nil {
		t.Error("initial fallthrough flag must inherit the switch origin")
	}
	if prereq.Statements.Head.Next.Value.(*luau.IfStatement).Statements.Head.Value.Origin() != nil {
		t.Error("nested nullish assignment must inherit its enclosing clause origin")
	}
}

func TestTryCallbackOrigins(t *testing.T) {
	function := transformOriginSource(t, "function attempt() {\n\ttry {\n\t\trisky();\n\t} catch (e) {\n\t\tuse(e);\n\t} finally {\n\t\trisky();\n\t}\n}\ndeclare function risky(): void;\ndeclare function use(value: unknown): void;").Tail.Value.(*luau.FunctionDeclaration)
	call := function.Statements.Head.Value.(*luau.CallStatement).Expression.(*luau.CallExpression)
	callbacks := call.Args.ToSlice()
	for index, test := range []struct {
		name           string
		start, closing luau.Position
	}{
		{"try block", luau.Position{Line: 1, Column: 5}, luau.Position{Line: 3, Column: 1}},
		{"catch clause", luau.Position{Line: 3, Column: 3}, luau.Position{Line: 5, Column: 1}},
		{"finally block", luau.Position{Line: 5, Column: 11}, luau.Position{Line: 7, Column: 1}},
	} {
		t.Run(test.name, func(t *testing.T) { assertNodeOrigin(t, callbacks[index], test.start, test.closing) })
	}
	assertNodeOrigin(t, callbacks[0].(*luau.FunctionExpression).Statements.Head.Value, luau.Position{Line: 2, Column: 2}, luau.Position{Line: 2, Column: 9})
}
