package transformer

import (
	"encoding/json"
	"fmt"
	"strings"
	"testing"

	"rotor/internal/luau"
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
		start     luau.Position
		end       luau.Position
	}{
		{name: "first prereq", statement: statements[0], start: luau.Position{Line: 0, Column: 0}, end: luau.Position{Line: 0, Column: 7}},
		{name: "first transformed", statement: statements[1], start: luau.Position{Line: 0, Column: 0}, end: luau.Position{Line: 0, Column: 7}},
		{name: "second prereq", statement: statements[2], start: luau.Position{Line: 1, Column: 0}, end: luau.Position{Line: 1, Column: 8}},
		{name: "second transformed", statement: statements[3], start: luau.Position{Line: 1, Column: 0}, end: luau.Position{Line: 1, Column: 8}},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			origin := test.statement.Origin()
			if origin == nil {
				t.Fatal("statement has no origin")
			}
			if got := origin.Start; got != test.start {
				t.Errorf("start = %#v, want %#v", got, test.start)
			}
			if got := origin.Closing; got == nil || *got != test.end {
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
	directory := t.TempDir()
	writeFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"target":"ESNext"}}`)
	// The mutable export selects exports-table assignments, which already have
	// source anchors. Return-map field tagging belongs to the later gap work.
	writeFixture(t, directory, "input.ts", "export const value = 1;\nexport { value as alias };\nexport default value;\nexport let mutable = 0;\n")
	state := buildInternalState(t, directory, "input.ts")
	origins := map[string]*luau.Origin{}
	TransformSourceFile(state).ForEach(func(statement luau.Statement) {
		if assignment, ok := statement.(*luau.Assignment); ok {
			if property, ok := assignment.Left.(*luau.PropertyAccessExpression); ok {
				origins[property.Name] = assignment.Origin()
			}
		}
	})

	tests := []struct {
		name       string
		exportName string
		start      luau.Position
		closing    luau.Position
	}{
		{name: "export specifier", exportName: "alias", start: luau.Position{Line: 1, Column: 9}, closing: luau.Position{Line: 1, Column: 22}},
		{name: "export assignment", exportName: "default", start: luau.Position{Line: 2, Column: 0}, closing: luau.Position{Line: 2, Column: 20}},
		{name: "export declaration", exportName: "value", start: luau.Position{Line: 0, Column: 0}, closing: luau.Position{Line: 0, Column: 22}},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			origin := origins[test.exportName]
			if origin == nil {
				t.Fatalf("export %s has no origin", test.exportName)
			}
			if got := origin.Start; got != test.start {
				t.Errorf("start = %#v, want %#v", got, test.start)
			}
			if got := origin.Closing; got == nil || *got != test.closing {
				t.Errorf("closing = %#v, want %#v", got, test.closing)
			}
		})
	}
}

func TestRenderWithSourceMap(t *testing.T) {
	sourceFile := parser.ParseSourceFile(ast.SourceFileParseOptions{
		FileName: "/test.ts",
		Path:     tspath.Path("/test.ts"),
	}, "const a = 1;\r\nconst b = 2;\r\n", core.ScriptKindTS)
	state := NewState(nil, nil, sourceFile, NewDiagService(), NewMultiState())
	first := luau.NewVariableDeclaration(luau.ID("a"), luau.Num(1))
	second := luau.NewVariableDeclaration(luau.ID("b"), luau.Num(2))
	statements := luau.NewList[luau.Statement](first, second)
	luau.SetOrigin(first, &luau.Origin{Start: luau.Position{Line: 0, Column: 0}, Closing: &luau.Position{Line: 0, Column: 11}})
	luau.SetOrigin(second, &luau.Origin{Start: luau.Position{Line: 1, Column: 0}, Closing: &luau.Position{Line: 1, Column: 11}})

	var sourceMap struct {
		Version        int      `json:"version"`
		File           string   `json:"file"`
		Sources        []string `json:"sources"`
		SourcesContent []string `json:"sourcesContent"`
		Mappings       string   `json:"mappings"`
	}
	text, mapJSON := state.RenderWithSourceMap(statements, sourceFile)
	if text != "local a = 1\nlocal b = 2\n" {
		t.Errorf("text = %q, want two declarations", text)
	}
	if err := json.Unmarshal([]byte(mapJSON), &sourceMap); err != nil {
		t.Fatal(err)
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal([]byte(mapJSON), &fields); err != nil {
		t.Fatal(err)
	}
	if len(fields) != 5 {
		t.Errorf("map fields = %v, want exactly version, file, sources, sourcesContent, mappings", fields)
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

func TestRenderWithSourceMapUsesEndAnchorForBlockEnd(t *testing.T) {
	sourceFile := parser.ParseSourceFile(ast.SourceFileParseOptions{
		FileName: "/test.ts",
		Path:     tspath.Path("/test.ts"),
	}, "if (true) {\n\tconst value = 1;\n}\n", core.ScriptKindTS)
	state := NewState(nil, nil, sourceFile, NewDiagService(), NewMultiState())
	inner := luau.NewVariableDeclaration(luau.ID("value"), luau.Num(1))
	condition := luau.NewIf(luau.Bool(true), luau.NewList[luau.Statement](inner), nil)
	statements := luau.NewList[luau.Statement](condition)
	luau.SetOrigin(condition, &luau.Origin{Start: luau.Position{Line: 0, Column: 0}, Closing: &luau.Position{Line: 2, Column: 0}})
	luau.SetOrigin(inner, &luau.Origin{Start: luau.Position{Line: 1, Column: 1}})

	var sourceMap struct {
		Mappings string `json:"mappings"`
	}
	text, mapJSON := state.RenderWithSourceMap(statements, sourceFile)
	if text != "if true then\n\tlocal value = 1\nend\n" {
		t.Errorf("text = %q, want if block", text)
	}
	if err := json.Unmarshal([]byte(mapJSON), &sourceMap); err != nil {
		t.Fatal(err)
	}
	if sourceMap.Mappings != "AAAA;AACC;AACD" {
		t.Errorf("mappings = %q, want %q", sourceMap.Mappings, "AAAA;AACC;AACD")
	}
}

func TestRenderWithSourceMapCoversNestedSyntheticGaps(t *testing.T) {
	state, sourceFile, _ := sourceMapFixture()
	call := func() luau.Statement {
		return luau.NewCallStatement(luau.NewCall(luau.ID("fallback"), luau.NewList[luau.Expression]()))
	}
	table := luau.NewMap(luau.NewList(luau.NewMapField(luau.Str("x"), luau.Num(1))))
	condition := luau.NewIf(luau.Bool(true), luau.NewList[luau.Statement](
		luau.NewVariableDeclaration(luau.ID("record"), table),
		luau.NewVariableDeclaration(luau.ID("text"), luau.Str("first\ncontinuation")),
		luau.NewDo(luau.NewList(call())),
	), luau.NewList(call()))
	luau.SetOrigin(condition, &luau.Origin{Start: luau.Position{Line: 20, Column: 4}})
	outer := luau.NewDo(luau.NewList[luau.Statement](luau.NewDo(luau.NewList[luau.Statement](condition))))
	luau.SetOrigin(outer, &luau.Origin{Start: luau.Position{Line: 10, Column: 2}, Closing: &luau.Position{Line: 90, Column: 1}})
	statements := luau.NewList[luau.Statement](outer, luau.NewReturn(luau.Nil()))
	text, mapJSON := state.RenderWithSourceMap(statements, sourceFile)
	wantText := "do\n\tdo\n\t\tif true then\n\t\t\tlocal record = {\n\t\t\t\tx = 1,\n\t\t\t}\n\t\t\tlocal text = [[first\ncontinuation]]\n\t\t\tdo\n\t\t\t\tfallback()\n\t\t\tend\n\t\telse\n\t\t\tfallback()\n\t\tend\n\tend\nend\nreturn nil\n"
	if text != wantText {
		t.Fatalf("text = %q, want %q", text, wantText)
	}
	assertSourceMapLines(t, text, mapJSON, []string{
		"10:2", "10:2", "20:4", "20:4", "20:4", "20:4", "20:4", "20:4",
		"20:4", "20:4", "20:4", "20:4", "20:4", "20:4", "10:2", "90:1", "-",
	})
}

func TestRenderWithSourceMapOtherStarts(t *testing.T) {
	state, sourceFile, _ := sourceMapFixture()
	value := luau.Num(1)
	luau.SetOrigin(value, &luau.Origin{Start: luau.Position{Line: 33, Column: 2}})
	nested := luau.NewMap(luau.NewList(luau.NewMapField(luau.Str("x"), value)))
	luau.SetOrigin(nested, &luau.Origin{Start: luau.Position{Line: 30, Column: 3}})
	key := luau.Str("first")
	luau.SetOrigin(key, &luau.Origin{Start: luau.Position{Line: 21, Column: 1}})
	first := luau.NewMapField(key, nested)
	luau.SetOrigin(first, &luau.Origin{Start: luau.Position{Line: 20, Column: 1}})
	secondKey, secondValue := luau.Str("second"), luau.Num(2)
	luau.SetOrigin(secondKey, &luau.Origin{Start: luau.Position{Line: 41, Column: 4}})
	luau.SetOrigin(secondValue, &luau.Origin{Start: luau.Position{Line: 42, Column: 4}})
	thirdValue := luau.Num(3)
	luau.SetOrigin(thirdValue, &luau.Origin{Start: luau.Position{Line: 43, Column: 5}})
	table := luau.NewMap(luau.NewList(first,
		luau.NewMapField(secondKey, secondValue), luau.NewMapField(luau.Str("third"), thirdValue)))
	text, mapJSON := state.RenderWithSourceMap(luau.NewList[luau.Statement](luau.NewReturn(table)), sourceFile)
	wantText := "return {\n\tfirst = {\n\t\tx = 1,\n\t},\n\tsecond = 2,\n\tthird = 3,\n}\n"
	if text != wantText {
		t.Fatalf("text = %q, want %q", text, wantText)
	}
	// Field and key share a start column: the outer field wins. The key on
	// the second line beats its later value; any other start beats coverage.
	assertSourceMapLines(t, text, mapJSON, []string{"-", "20:1", "33:2", "30:3", "41:4", "43:5", "-"})
}

func sourceMapFixture() (*State, *ast.SourceFile, *luau.List[luau.Statement]) {
	sourceFile := parser.ParseSourceFile(ast.SourceFileParseOptions{
		FileName: "/test.ts", Path: tspath.Path("/test.ts"),
	}, "if (true) {\n\tuse(1);\n}\n", core.ScriptKindTS)
	state := NewState(nil, nil, sourceFile, NewDiagService(), NewMultiState())
	inner := luau.NewCallStatement(luau.NewCall(luau.ID("use"), luau.NewList[luau.Expression](luau.Num(1))))
	luau.SetOrigin(inner, &luau.Origin{Start: luau.Position{Line: 1, Column: 1}})
	block := luau.NewIf(luau.Bool(true), luau.NewList[luau.Statement](inner), nil)
	luau.SetOrigin(block, &luau.Origin{Start: luau.Position{}, Closing: &luau.Position{Line: 2}})
	return state, sourceFile, luau.NewList[luau.Statement](block)
}

func TestRenderWithSourceMapRecomputesMutatedLayout(t *testing.T) {
	state, sourceFile, statements := sourceMapFixture()
	_, before := state.RenderWithSourceMap(statements, sourceFile)
	statements.Head.Value.(*luau.IfStatement).Statements.Unshift(luau.NewComment("added\nlines"))
	text, got := state.RenderWithSourceMap(statements, sourceFile)
	if got == before {
		t.Fatal("adding multiline output must move the following source-map anchors")
	}
	freshState, freshSourceFile, freshStatements := sourceMapFixture()
	freshStatements.Head.Value.(*luau.IfStatement).Statements.Unshift(luau.NewComment("added\nlines"))
	wantText, want := freshState.RenderWithSourceMap(freshStatements, freshSourceFile)
	if text != wantText || got != want {
		t.Fatalf("mutated render = %q, map = %q; fresh render = %q, map = %q", text, got, wantText, want)
	}
	assertSourceMapLines(t, text, got, []string{"0:0", "-", "-", "-", "-", "1:1", "2:0"})
}

func TestRenderWithSourceMapNestedAllocationBudget(t *testing.T) {
	state, sourceFile, _ := sourceMapFixture()
	var statement luau.Statement = luau.NewVariableDeclaration(luau.ID("value"), luau.Num(1))
	for range 24 {
		statement = luau.NewDo(luau.NewList(statement))
	}
	luau.SetOrigin(statement, &luau.Origin{Start: luau.Position{Line: 1}})
	statements := luau.NewList(statement)
	allocations := testing.AllocsPerRun(3, func() { state.RenderWithSourceMap(statements, sourceFile) })
	// The old subtree re-renders required 5,048 allocations at this depth.
	// Leave room for runtime changes while still detecting their return.
	t.Logf("nested source-map allocations = %.0f", allocations)
	if allocations > 3200 {
		t.Fatalf("nested source-map allocations = %.0f, want at most 3200", allocations)
	}
}

// Expected coordinates use zero-based lines and UTF-16 columns, like Origin.
func assertSourceMapLines(t *testing.T, text, mapJSON string, want []string) {
	t.Helper()
	var sourceMap struct {
		Mappings string `json:"mappings"`
	}
	if err := json.Unmarshal([]byte(mapJSON), &sourceMap); err != nil {
		t.Fatal(err)
	}
	lines := strings.Split(strings.TrimSuffix(text, "\n"), "\n")
	if len(lines) != len(want) {
		t.Fatalf("rendered %d lines, want %d:\n%s", len(lines), len(want), text)
	}
	got := make([]string, len(lines))
	for i := range got {
		got[i] = "-"
	}
	decoder := sourcemap.DecodeMappings(sourceMap.Mappings)
	for mapping := range decoder.Values() {
		if mapping.GeneratedLine >= len(lines) {
			t.Fatalf("mapping beyond rendered text: %#v", mapping)
		}
		if mapping.GeneratedCharacter != 0 {
			t.Errorf("generated column = %d, want 0", mapping.GeneratedCharacter)
		}
		if !mapping.IsSourceMapping() {
			t.Errorf("unexpected generated-only mapping: %#v", mapping)
		}
		if got[mapping.GeneratedLine] != "-" {
			t.Errorf("multiple segments on line %d", mapping.GeneratedLine)
		}
		got[mapping.GeneratedLine] = fmt.Sprintf("%d:%d", mapping.SourceLine, mapping.SourceCharacter)
	}
	if err := decoder.Error(); err != nil {
		t.Fatal(err)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Errorf("line %d %q: source = %s, want %s", i+1, lines[i], got[i], want[i])
		}
	}
}
