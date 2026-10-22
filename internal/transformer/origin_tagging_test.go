package transformer

import (
	"testing"

	"rotor/internal/luau"
)

func transformOriginSource(t *testing.T, source string) *luau.List[luau.Statement] {
	t.Helper()
	directory := t.TempDir()
	writeFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"target":"ESNext","strict":true,"types":[],"moduleDetection":"force"}}`)
	writeFixture(t, directory, "input.ts", source)
	state := buildInternalState(t, directory, "input.ts")
	statements := TransformStatementList(state, state.SourceFile.AsNode(), state.SourceFile.Statements.Nodes, nil)
	if diagnostics := state.Diags.Flush(); len(diagnostics) != 0 {
		t.Fatalf("transform diagnostics: %v", diagnostics)
	}
	return statements
}

func assertNodeOrigin(t *testing.T, node luau.Node, start, closing luau.Position) {
	t.Helper()
	origin := node.Origin()
	if origin == nil {
		t.Fatalf("%T has no origin", node)
	}
	if origin.Start != start {
		t.Errorf("%T start = %v, want %v", node, origin.Start, start)
	}
	if origin.Closing == nil || *origin.Closing != closing {
		t.Errorf("%T closing = %v, want %v", node, origin.Closing, closing)
	}
}

func TestFunctionOrigins(t *testing.T) {
	tests := []struct {
		name           string
		source         string
		selectNode     func(luau.Statement) luau.Node
		start, closing luau.Position
	}{
		{
			name:   "function expression",
			source: "const callback = function() {\n\treturn 1;\n};",
			selectNode: func(statement luau.Statement) luau.Node {
				return statement.(*luau.VariableDeclaration).Right.(*luau.FunctionExpression)
			},
			start: luau.Position{Line: 0, Column: 17}, closing: luau.Position{Line: 2, Column: 0},
		},
		{
			name:   "arrow",
			source: "const callback = () => {\n\treturn 1;\n};",
			selectNode: func(statement luau.Statement) luau.Node {
				return statement.(*luau.VariableDeclaration).Right.(*luau.FunctionExpression)
			},
			start: luau.Position{Line: 0, Column: 17}, closing: luau.Position{Line: 2, Column: 0},
		},
		{
			name:   "object method",
			source: "const object = {\n\tmethod() {\n\t\treturn 1;\n\t},\n};",
			selectNode: func(statement luau.Statement) luau.Node {
				return statement.(*luau.VariableDeclaration).Right.(*luau.Map).Fields.Head.Value.Value
			},
			start: luau.Position{Line: 1, Column: 1}, closing: luau.Position{Line: 3, Column: 1},
		},
		{
			name:   "async declaration callback",
			source: "async function task() {\n\treturn 1;\n}",
			selectNode: func(statement luau.Statement) luau.Node {
				return statement.(*luau.VariableDeclaration).Right.(*luau.CallExpression).Args.Head.Value
			},
			start: luau.Position{Line: 0, Column: 0}, closing: luau.Position{Line: 2, Column: 0},
		},
		{
			name:       "named expression declaration",
			source:     "const callback = function named() {\n\treturn 1;\n};",
			selectNode: func(statement luau.Statement) luau.Node { return statement.(*luau.FunctionDeclaration) },
			start:      luau.Position{Line: 0, Column: 17}, closing: luau.Position{Line: 2, Column: 0},
		},
		{
			name:       "matching named expression declaration",
			source:     "export {};\nconst named = function named() {\n\treturn 1;\n};",
			selectNode: func(statement luau.Statement) luau.Node { return statement.(*luau.FunctionDeclaration) },
			start:      luau.Position{Line: 1, Column: 14}, closing: luau.Position{Line: 3, Column: 0},
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			statements := transformOriginSource(t, test.source)
			assertNodeOrigin(t, test.selectNode(statements.Head.Value), test.start, test.closing)
		})
	}
}
