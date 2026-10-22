package transformer

import (
	"fmt"
	"testing"

	"rotor/internal/luau"
)

func expressionOriginState(t *testing.T, source string, maps bool) *State {
	t.Helper()
	directory := t.TempDir()
	writeFixture(t, directory, "tsconfig.json", fmt.Sprintf(`{"compilerOptions":{"target":"ESNext","strict":true,"types":[],"moduleDetection":"force","sourceMap":%t}}`, maps))
	writeFixture(t, directory, "input.ts", source)
	return buildInternalState(t, directory, "input.ts")
}

func TestExpressionOriginsArePerOccurrence(t *testing.T) {
	for _, cached := range []bool{false, true} {
		t.Run(fmt.Sprintf("cached=%t", cached), func(t *testing.T) {
			state := expressionOriginState(t, "declare const a: number;\na + a;", true)
			node := state.SourceFile.Statements.Nodes[1].AsExpressionStatement().Expression
			binary := node.AsBinaryExpression()
			var id *luau.TemporaryIdentifier
			if cached {
				id = luau.TempID("a")
				state.SymbolToID[state.Checker.GetSymbolAtLocation(binary.Left)] = id
			}
			left := TransformExpression(state, binary.Left)
			if id != nil && id.Origin() != nil {
				t.Fatal("first occurrence tagged the cached identifier")
			}
			right := TransformExpression(state, binary.Right)
			if left == right {
				t.Fatal("occurrences share a node")
			}
			for i, expression := range []luau.Expression{left, right} {
				want := luau.Position{Line: 1, Column: i * 4}
				if origin := expression.Origin(); origin == nil || origin.Start != want {
					t.Errorf("occurrence %d origin = %+v, want %v", i, origin, want)
				}
				if id != nil && expression.(*luau.TemporaryIdentifier).ID != id.ID {
					t.Error("clone changed temporary identity")
				}
			}
		})
	}
}

func TestExpressionOriginsDisabled(t *testing.T) {
	state := expressionOriginState(t, "declare const a: number;\na + 1;", false)
	node := state.SourceFile.Statements.Nodes[1].AsExpressionStatement().Expression
	transformed, _ := state.Capture(func() luau.Expression { return TransformExpression(state, node) })
	expression := transformed.(*luau.BinaryExpression)
	for _, item := range []luau.Expression{expression, expression.Left, expression.Right} {
		if item.Origin() != nil {
			t.Errorf("maps off: %T has origin %+v", item, item.Origin())
		}
	}
}

func TestExpressionWrapperKeepsInnerOrigin(t *testing.T) {
	state := expressionOriginState(t, "(1 as number)!;", true)
	node := state.SourceFile.Statements.Nodes[0].AsExpressionStatement().Expression
	expression := TransformExpression(state, node)
	if origin := expression.Origin(); origin == nil || origin.Start != (luau.Position{Column: 1}) {
		t.Fatalf("wrapper origin = %+v, want inner token at 0:1", origin)
	}
}

func TestAssignmentOperatorKeepsStatementOrigin(t *testing.T) {
	state := expressionOriginState(t, "let x = 0;\nx += 1;\nx++;", true)
	statements := TransformStatementList(state, state.SourceFile.AsNode(), state.SourceFile.Statements.Nodes, nil).ToSlice()
	for i, column := range []int{2, 1} {
		origin := statements[i+1].Origin()
		if origin == nil || origin.Start != (luau.Position{Line: i + 1}) || origin.Closing == nil {
			t.Fatalf("statement %d lost its source span: %+v", i+1, origin)
		}
		want := luau.Position{Line: i + 1, Column: column}
		if origin.Operator == nil || *origin.Operator != want {
			t.Errorf("statement %d operator = %v, want %v", i+1, origin.Operator, want)
		}
	}
}
