package transformer

import (
	"fmt"
	"testing"

	"rotor/internal/luau"
	"rotor/internal/luau/render"
	"rotor/tsgo/ast"
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

func TestCachedObjectKeyOriginsArePerOccurrence(t *testing.T) {
	state := expressionOriginState(t, "\"key\";\n\"key\";", true)
	first := state.SourceFile.Statements.Nodes[0].AsExpressionStatement().Expression
	second := state.SourceFile.Statements.Nodes[1].AsExpressionStatement().Expression
	key := luau.Str("key")
	state.SetClassElementObjectKey(first, key)
	for line, node := range []*ast.Node{first, second} {
		occurrence := state.tagExpressionOrigin(state.GetClassElementObjectKey(first), node)
		if occurrence == key || occurrence.Origin() == nil || occurrence.Origin().Start != (luau.Position{Line: line}) {
			t.Fatalf("cached key occurrence %d has wrong identity or origin: %+v", line, occurrence.Origin())
		}
		if key.Origin() != nil || key.Parent() != nil {
			t.Fatal("tagging an occurrence mutated the cached key")
		}
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

func TestCompoundAssignmentExpressionOperator(t *testing.T) {
	state := expressionOriginState(t, "let x = 0;\nconst value = (x += 1);", true)
	statements := TransformStatementList(state, state.SourceFile.AsNode(), state.SourceFile.Statements.Nodes, nil)
	statement := statements.Head.Next.Value.(*luau.Assignment)
	origin := statement.Origin()
	want := luau.Position{Line: 1, Column: 17}
	if origin == nil || origin.Operator == nil || *origin.Operator != want {
		t.Fatalf("compound assignment expression origin = %+v, want operator %v", origin, want)
	}
}

func TestLogicalAssignmentOperatorOrigins(t *testing.T) {
	for _, operator := range []string{"??=", "&&=", "||="} {
		t.Run(operator, func(t *testing.T) {
			state := expressionOriginState(t, "let value: boolean | undefined = true;\nvalue "+operator+" false;", true)
			statements := TransformStatementList(state, state.SourceFile.AsNode(), state.SourceFile.Statements.Nodes, nil)
			found := false
			statements.ForEach(func(statement luau.Statement) {
				if branch, ok := statement.(*luau.IfStatement); ok {
					found = true
					want := luau.Position{Line: 1, Column: 6}
					origin := branch.Condition.Origin()
					if origin == nil || origin.Start != want || origin.Operator == nil || *origin.Operator != want {
						t.Errorf("condition origin = %+v, want start and operator %v", origin, want)
					}
				}
			})
			if !found {
				t.Fatal("logical assignment has no condition")
			}
		})
	}
}

func TestOptionalChainOperatorOrigins(t *testing.T) {
	state := expressionOriginState(t, `declare const object: { value: number; method(): number } | undefined;
const first = object?.value;
const second = object?.method?.();
`, true)
	statements := TransformStatementList(state, state.SourceFile.AsNode(), state.SourceFile.Statements.Nodes, nil)
	_, ranges := render.RenderASTWithPositions(statements)
	want := []luau.Position{{Line: 1, Column: 20}, {Line: 2, Column: 21}, {Line: 2, Column: 29}}
	var got []luau.Position
	for _, r := range ranges {
		if binary, ok := r.Node.(*luau.BinaryExpression); ok && binary.Operator == "~=" {
			if origin := binary.Origin(); origin != nil && origin.Operator != nil {
				got = append(got, *origin.Operator)
			}
		}
	}
	if fmt.Sprint(got) != fmt.Sprint(want) {
		t.Errorf("optional chain operators = %v, want %v", got, want)
	}
}

func TestTernaryOriginsDoNotInventOperators(t *testing.T) {
	state := expressionOriginState(t, "declare const test: number;\nconst value = test ? 1 : 2;", true)
	statements := TransformStatementList(state, state.SourceFile.AsNode(), state.SourceFile.Statements.Nodes, nil)
	_, ranges := render.RenderASTWithPositions(statements)
	var ternary *luau.IfExpression
	for _, r := range ranges {
		if expression, ok := r.Node.(*luau.IfExpression); ok {
			ternary = expression
		}
		if origin := r.Node.Origin(); origin != nil && origin.Operator != nil {
			t.Errorf("ternary or truthiness node %T has an invented operator", r.Node)
		}
	}
	if ternary == nil {
		t.Fatal("missing ternary expression")
	}
	for _, expression := range []luau.Expression{ternary.Condition, ternary.Expression, ternary.Alternative} {
		if expression.Origin() == nil {
			t.Errorf("ternary operand %T has no source start", expression)
		}
	}
}

func TestInOperatorOriginOnContinuationLine(t *testing.T) {
	state := expressionOriginState(t, "declare const object: { key: number };\nconst present = \"key\"\n\tin object;", true)
	statements := TransformStatementList(state, state.SourceFile.AsNode(), state.SourceFile.Statements.Nodes, nil)
	expression := statements.Head.Value.(*luau.VariableDeclaration).Right.(*luau.BinaryExpression)
	origin := expression.Origin()
	want := luau.Position{Line: 2, Column: 1}
	if origin == nil || origin.Operator == nil || *origin.Operator != want {
		t.Fatalf("in comparison origin = %+v, want operator %v", origin, want)
	}
	_, ranges := render.RenderASTWithPositions(statements)
	for _, r := range ranges {
		if r.Node == expression && r.Operator != nil {
			return
		}
	}
	t.Fatal("in comparison has no generated operator marker")
}

func TestArrayIndexAdjustmentKeepsOperatorOrigin(t *testing.T) {
	state := expressionOriginState(t, "declare const array: number[];\ndeclare const index: number;\nconst value = array[index\n\t+ 2];", true)
	statements := TransformStatementList(state, state.SourceFile.AsNode(), state.SourceFile.Statements.Nodes, nil)
	_, ranges := render.RenderASTWithPositions(statements)
	for _, r := range ranges {
		if binary, ok := r.Node.(*luau.BinaryExpression); ok && binary.Operator == "+" {
			origin := binary.Origin()
			want := luau.Position{Line: 3, Column: 1}
			if origin == nil || origin.Operator == nil || *origin.Operator != want {
				t.Fatalf("adjusted index origin = %+v, want operator %v", origin, want)
			}
			return
		}
	}
	t.Fatal("array index has no addition operator")
}
