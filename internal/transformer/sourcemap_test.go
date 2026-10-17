package transformer

import (
	"encoding/json"
	"testing"

	"rotor/internal/luau"
	"rotor/internal/luau/render"
	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/parser"
	"rotor/tsgo/sourcemap"
	"rotor/tsgo/tspath"
)

func TestSourcePositionAnchorsPrereqsAndStatements(t *testing.T) {
	sourceFile := parser.ParseSourceFile(ast.SourceFileParseOptions{
		FileName: "/test.ts",
		Path:     tspath.Path("/test.ts"),
	}, "first();\nsecond();\n", core.ScriptKindTS)
	state := NewState(nil, nil, sourceFile, NewDiagService(), NewMultiState())

	previousDispatch := TransformStatement
	TransformStatement = func(state *State, _ *ast.Node) *luau.List[luau.Statement] {
		state.Prereq(luau.NewVariableDeclaration(luau.ID("value"), luau.Num(1)))
		return luau.NewList[luau.Statement](luau.NewCallStatement(luau.NewCall(luau.ID("call"), luau.NewList[luau.Expression]())))
	}
	t.Cleanup(func() { TransformStatement = previousDispatch })

	statements := TransformStatementList(state, sourceFile.AsNode(), sourceFile.Statements.Nodes, nil).ToSlice()
	if len(statements) != 4 {
		t.Fatalf("statement count = %d, want 4", len(statements))
	}

	tests := []struct {
		name      string
		statement luau.Statement
		start     SourcePosition
		end       SourcePosition
	}{
		{name: "first prereq", statement: statements[0], start: SourcePosition{Line: 0, Column: 0}, end: SourcePosition{Line: 0, Column: 7}},
		{name: "first transformed", statement: statements[1], start: SourcePosition{Line: 0, Column: 0}, end: SourcePosition{Line: 0, Column: 7}},
		{name: "second prereq", statement: statements[2], start: SourcePosition{Line: 1, Column: 0}, end: SourcePosition{Line: 1, Column: 8}},
		{name: "second transformed", statement: statements[3], start: SourcePosition{Line: 1, Column: 0}, end: SourcePosition{Line: 1, Column: 8}},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			if got := state.sourcePositionMap[sourcePositionKey(test.statement)]; got == nil || *got != test.start {
				t.Errorf("start = %#v, want %#v", got, test.start)
			}
			if got := state.sourceEndPositionMap[sourcePositionKey(test.statement)]; got == nil || *got != test.end {
				t.Errorf("end = %#v, want %#v", got, test.end)
			}
		})
	}
}

func TestSourcePositionUsesUTF16Columns(t *testing.T) {
	sourceFile := parser.ParseSourceFile(ast.SourceFileParseOptions{
		FileName: "/test.ts",
		Path:     tspath.Path("/test.ts"),
	}, "const value = \"😀\"; value;", core.ScriptKindTS)
	state := NewState(nil, nil, sourceFile, NewDiagService(), NewMultiState())
	secondStatement := sourceFile.Statements.Nodes[1]

	if got := state.getOriginalSourcePosition(secondStatement, false); got == nil || *got != (SourcePosition{Line: 0, Column: 20}) {
		t.Errorf("position = %#v, want %#v", got, SourcePosition{Line: 0, Column: 20})
	}
}

func TestSourcePositionExportSyntaxAnchors(t *testing.T) {
	sourceFile := parser.ParseSourceFile(ast.SourceFileParseOptions{
		FileName: "/test.ts",
		Path:     tspath.Path("/test.ts"),
	}, "export const value = 1;\nexport { value as alias };\nexport default value;\n", core.ScriptKindTS)
	variableStatement := sourceFile.Statements.Nodes[0]
	exportDeclaration := sourceFile.Statements.Nodes[1]
	exportAssignment := sourceFile.Statements.Nodes[2]
	exportSpecifier := exportDeclaration.AsExportDeclaration().ExportClause.AsNamedExports().Elements.Nodes[0]
	variableDeclaration := variableStatement.AsVariableStatement().DeclarationList.AsVariableDeclarationList().Declarations.Nodes[0]

	tests := []struct {
		name   string
		symbol *ast.Symbol
		want   *ast.Node
	}{
		{name: "export specifier", symbol: &ast.Symbol{Declarations: []*ast.Node{exportSpecifier}}, want: exportSpecifier},
		{name: "export assignment", symbol: &ast.Symbol{Declarations: []*ast.Node{exportAssignment}}, want: exportAssignment},
		{name: "export declaration", symbol: &ast.Symbol{Declarations: []*ast.Node{variableDeclaration}}, want: variableStatement},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			if got := getExportSyntaxAnchor(test.symbol, sourceFile); got != test.want {
				t.Errorf("anchor = %p, want %p", got, test.want)
			}
		})
	}
}

func TestRenderSourceMap(t *testing.T) {
	sourceFile := parser.ParseSourceFile(ast.SourceFileParseOptions{
		FileName: "/test.ts",
		Path:     tspath.Path("/test.ts"),
	}, "const a = 1;\nconst b = 2;\n", core.ScriptKindTS)
	state := NewState(nil, nil, sourceFile, NewDiagService(), NewMultiState())
	first := luau.NewVariableDeclaration(luau.ID("a"), luau.Num(1))
	second := luau.NewVariableDeclaration(luau.ID("b"), luau.Num(2))
	statements := luau.NewList[luau.Statement](first, second)
	state.sourcePositionMap[sourcePositionKey(first)] = &SourcePosition{Line: 0, Column: 0}
	state.sourceEndPositionMap[sourcePositionKey(first)] = &SourcePosition{Line: 0, Column: 11}
	state.sourcePositionMap[sourcePositionKey(second)] = &SourcePosition{Line: 1, Column: 0}
	state.sourceEndPositionMap[sourcePositionKey(second)] = &SourcePosition{Line: 1, Column: 11}

	var sourceMap struct {
		Version        int      `json:"version"`
		File           string   `json:"file"`
		Sources        []string `json:"sources"`
		SourcesContent []string `json:"sourcesContent"`
		Mappings       string   `json:"mappings"`
	}
	if err := json.Unmarshal([]byte(state.RenderSourceMap(statements, sourceFile)), &sourceMap); err != nil {
		t.Fatal(err)
	}

	if sourceMap.Version != 3 {
		t.Errorf("version = %d, want 3", sourceMap.Version)
	}
	if sourceMap.File != "/test.luau" {
		t.Errorf("file = %q, want /test.luau", sourceMap.File)
	}
	if len(sourceMap.Sources) != 1 || sourceMap.Sources[0] != "/test.ts" {
		t.Errorf("sources = %v, want [/test.ts]", sourceMap.Sources)
	}
	if len(sourceMap.SourcesContent) != 1 || sourceMap.SourcesContent[0] != sourceFile.Text() {
		t.Errorf("sourcesContent = %v, want source text", sourceMap.SourcesContent)
	}
	if sourceMap.Mappings != "AAAA;AACA" {
		t.Errorf("mappings = %q, want %q", sourceMap.Mappings, "AAAA;AACA")
	}
	decoder := sourcemap.DecodeMappings(sourceMap.Mappings)
	for range decoder.Values() {
	}
	if err := decoder.Error(); err != nil {
		t.Errorf("mappings must be valid VLQ: %v", err)
	}
}

func TestRenderSourceMapUsesEndAnchorForBlockEnd(t *testing.T) {
	sourceFile := parser.ParseSourceFile(ast.SourceFileParseOptions{
		FileName: "/test.ts",
		Path:     tspath.Path("/test.ts"),
	}, "if (true) {\n\tconst value = 1;\n}\n", core.ScriptKindTS)
	state := NewState(nil, nil, sourceFile, NewDiagService(), NewMultiState())
	inner := luau.NewVariableDeclaration(luau.ID("value"), luau.Num(1))
	condition := luau.NewIf(luau.Bool(true), luau.NewList[luau.Statement](inner), nil)
	statements := luau.NewList[luau.Statement](condition)
	state.sourcePositionMap[sourcePositionKey(condition)] = &SourcePosition{Line: 0, Column: 0}
	state.sourcePositionMap[sourcePositionKey(inner)] = &SourcePosition{Line: 1, Column: 1}
	state.sourceEndPositionMap[sourcePositionKey(condition)] = &SourcePosition{Line: 2, Column: 0}

	var sourceMap struct {
		Mappings string `json:"mappings"`
	}
	if err := json.Unmarshal([]byte(state.RenderSourceMap(statements, sourceFile)), &sourceMap); err != nil {
		t.Fatal(err)
	}
	if sourceMap.Mappings != "AAAA;AACC;AACD" {
		t.Errorf("mappings = %q, want %q", sourceMap.Mappings, "AAAA;AACC;AACD")
	}
}

// These exact outputs were captured before reusing source-map layout renders.
// In particular, callback matching keeps its existing first-substring behavior.
func TestRenderSourceMapLayoutParity(t *testing.T) {
	tests := []struct {
		name    string
		text    string
		mapJSON string
	}{
		{name: "blocks", text: "do\n\tif true then\n\t\t--[[\n\t\t\tfirst\n\t\t\tsecond\n\t\t\tthird\n\t\t]]\n\t\tlocal value = [[😀\nsecond]]\n\telseif false then\n\telse\n\t\tfallback()\n\tend\n\twhile true do\n\t\trepeat\n\t\t\tstep()\n\t\tuntil false\n\tend\n\tfor i = 1, 3 do\n\tend\n\tfor item in items do\n\t\tvisit()\n\tend\n\tlocal function work()\n\t\trun()\n\tend\n\tfunction object:work(...)\n\tend\nend\nlocal fn = work;\n(fn or work)()\n", mapJSON: "{\"version\":3,\"file\":\"/layout.luau\",\"sources\":[\"/layout.ts\"],\"sourcesContent\":[\"// layout oracle 😀\\n\"],\"mappings\":\"AA4BC;AApBA;AARA;;;;;AAEA;;AAIA;;AAFA;AAKC;AAKD;AAFA;AAFA;;AAKC;AACD;AACC;AAGD;AAFA;AAGC;AAGD;AAFA;AAGC;AACD;AACC;AAEA;AACD;AAEA\"}"},
		{name: "callbacks", text: "consume(function()\n\tsame()\nend, function()\n\tsame()\nend, function() end)\nconsume(function()\n\tlocal _value = 1\n\tlocal _value_1 = 2\n\tuse(_value, _value_1)\nend)\ndo\n\tconsume(function()\n\t\tnested()\n\tend)\nend\nobject:consume(function()\n\tmethod()\nend)\n", mapJSON: "{\"version\":3,\"file\":\"/layout.luau\",\"sources\":[\"/layout.ts\"],\"sourcesContent\":[\"// layout oracle 😀\\n\"],\"mappings\":\"AAIC;AAJA,AAEA;AAGC,AAAA;;;AAOD;;;;;AAMA;AAFA;AAFA;AAGC;AAEA;AAGD\"}"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			state, sourceFile, statements := sourceMapLayoutFixture(test.name)
			text := render.RenderAST(statements)
			mapJSON := state.RenderSourceMap(statements, sourceFile)
			if text != test.text || mapJSON != test.mapJSON {
				t.Fatalf("%s text=%q map=%q", test.name, text, mapJSON)
			}
			if got := state.RenderSourceMap(statements, sourceFile); got != mapJSON {
				t.Fatalf("repeated source map = %q, want %q", got, mapJSON)
			}
		})
	}
}

func sourceMapLayoutFixture(name string) (*State, *ast.SourceFile, *luau.List[luau.Statement]) {
	sourceFile := parser.ParseSourceFile(ast.SourceFileParseOptions{
		FileName: "/layout.ts", Path: tspath.Path("/layout.ts"),
	}, "// layout oracle 😀\n", core.ScriptKindTS)
	state := NewState(nil, nil, sourceFile, NewDiagService(), NewMultiState())
	nextLine := 0
	anchor := func(statement luau.Statement) luau.Statement {
		state.sourcePositionMap[sourcePositionKey(statement)] = &SourcePosition{Line: nextLine, Column: 1}
		state.sourceEndPositionMap[sourcePositionKey(statement)] = &SourcePosition{Line: nextLine + 1, Column: 2}
		nextLine += 2
		return statement
	}
	list := func(statements ...luau.Statement) *luau.List[luau.Statement] { return luau.NewList(statements...) }
	call := func(name string) luau.Statement {
		return anchor(luau.NewCallStatement(luau.NewCall(luau.ID(name), luau.NewList[luau.Expression]())))
	}
	empty := func() *luau.List[luau.Statement] { return list() }
	if name == "blocks" {
		comment := anchor(luau.NewComment("first\r\nsecond\rthird"))
		value := anchor(luau.NewVariableDeclaration(luau.ID("value"), luau.Str("😀\nsecond")))
		elseBody := luau.NewIf(luau.Bool(false), empty(), list(call("fallback")))
		anchor(elseBody)
		condition := anchor(luau.NewIf(luau.Bool(true), list(comment, value), elseBody))
		loop := anchor(luau.NewWhile(luau.Bool(true), list(anchor(luau.NewRepeat(luau.Bool(false), list(call("step")))))))
		numeric := anchor(luau.NewNumericFor(luau.ID("i"), luau.Num(1), luau.Num(3), luau.Num(1), empty()))
		generic := anchor(luau.NewFor(luau.NewList[luau.AnyIdentifier](luau.ID("item")), luau.ID("items"), list(call("visit"))))
		function := anchor(luau.NewFunctionDeclaration(true, luau.ID("work"), luau.NewList[luau.AnyIdentifier](), false, list(call("run"))))
		method := anchor(luau.NewMethodDeclaration(luau.ID("object"), "work", luau.NewList[luau.AnyIdentifier](), true, empty()))
		block := anchor(luau.NewDo(list(condition, loop, numeric, generic, function, method)))
		// A following parenthesized call adds a semicolon to the full render;
		// the source-map line counter still renders each statement independently.
		declaration := anchor(luau.NewVariableDeclaration(luau.ID("fn"), luau.ID("work")))
		parenthesized := anchor(luau.NewCallStatement(luau.NewCall(luau.NewParenthesized(luau.NewBinary(luau.ID("fn"), "or", luau.ID("work"))), luau.NewList[luau.Expression]())))
		return state, sourceFile, list(block, declaration, parenthesized)
	}
	callback := func(body *luau.List[luau.Statement]) *luau.FunctionExpression {
		return luau.NewFunctionExpression(luau.NewList[luau.AnyIdentifier](), false, body)
	}
	first := callback(list(call("same")))
	second := callback(list(call("same")))
	duplicate := anchor(luau.NewCallStatement(luau.NewCall(luau.ID("consume"), luau.NewList[luau.Expression](first, second, callback(empty())))))
	temp := luau.TempID("value")
	otherTemp := luau.TempID("value")
	tempBody := list(
		anchor(luau.NewVariableDeclaration(temp, luau.Num(1))),
		anchor(luau.NewVariableDeclaration(otherTemp, luau.Num(2))),
		anchor(luau.NewCallStatement(luau.NewCall(luau.ID("use"), luau.NewList[luau.Expression](temp, otherTemp)))),
	)
	// Standalone fallback names fail the legacy callback substring match.
	tempCall := anchor(luau.NewCallStatement(luau.NewCall(luau.ID("consume"), luau.NewList[luau.Expression](callback(tempBody)))))
	nestedBody := callback(list(call("nested")))
	nestedCall := anchor(luau.NewCallStatement(luau.NewCall(luau.ID("consume"), luau.NewList[luau.Expression](nestedBody))))
	nested := anchor(luau.NewDo(list(nestedCall)))
	methodCall := anchor(luau.NewCallStatement(luau.NewMethodCall("consume", luau.ID("object"), luau.NewList[luau.Expression](callback(list(call("method")))))))
	return state, sourceFile, list(duplicate, tempCall, nested, methodCall)
}

func TestRenderSourceMapRecomputesMutatedLayout(t *testing.T) {
	state, sourceFile, statements := sourceMapLayoutFixture("blocks")
	before := state.RenderSourceMap(statements, sourceFile)
	statements.Head.Value.(*luau.DoStatement).Statements.Unshift(luau.NewComment("added\nlines"))
	got := state.RenderSourceMap(statements, sourceFile)
	if got == before {
		t.Fatal("adding multiline output must move the following source-map anchors")
	}
	freshState, freshSourceFile, freshStatements := sourceMapLayoutFixture("blocks")
	freshStatements.Head.Value.(*luau.DoStatement).Statements.Unshift(luau.NewComment("added\nlines"))
	if want := freshState.RenderSourceMap(freshStatements, freshSourceFile); got != want {
		t.Fatalf("mutated map = %q, fresh map = %q", got, want)
	}
}

func TestRenderSourceMapNestedAllocationBudget(t *testing.T) {
	state, sourceFile, _ := sourceMapLayoutFixture("blocks")
	var statement luau.Statement = luau.NewVariableDeclaration(luau.ID("value"), luau.Num(1))
	for range 24 {
		statement = luau.NewDo(luau.NewList(statement))
	}
	statements := luau.NewList(statement)
	allocations := testing.AllocsPerRun(3, func() { state.RenderSourceMap(statements, sourceFile) })
	// The duplicate subtree renders required 5,048 allocations at this depth.
	// Leave room for runtime changes while still detecting their return.
	t.Logf("nested source-map allocations = %.0f", allocations)
	if allocations > 3200 {
		t.Fatalf("nested source-map allocations = %.0f, want at most 3200", allocations)
	}
}
