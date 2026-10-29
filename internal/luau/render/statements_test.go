package render

import (
	"testing"

	"rotor/internal/luau"
)

func stmts(ss ...luau.Statement) *luau.List[luau.Statement] {
	return luau.NewList[luau.Statement](ss...)
}

func TestRenderReturnWithEmptyExpressionList(t *testing.T) {
	// `return $tuple()` reaches the renderer with an EMPTY expression list;
	// upstream's unconditional `return ${join(", ")}` renders `return ` with
	// a trailing space.
	got := renderAST(t, stmts(luau.NewReturn(luau.NewList[luau.Expression]())))
	if want := "return \n"; got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

func TestRenderVariableDeclarationAndAssignment(t *testing.T) {
	cases := []struct {
		ast  *luau.List[luau.Statement]
		want string
	}{
		{
			stmts(luau.NewVariableDeclaration(luau.ID("x"), luau.Num(5))),
			"local x = 5\n",
		},
		{
			stmts(luau.NewVariableDeclaration(luau.ID("x"), nil)),
			"local x\n",
		},
		{
			stmts(luau.NewVariableDeclaration(
				luau.NewList[luau.AnyIdentifier](luau.ID("a"), luau.ID("b")),
				luau.NewList[luau.Expression](luau.Num(1), luau.Num(2)),
			)),
			"local a, b = 1, 2\n",
		},
		{
			stmts(luau.NewAssignment(luau.ID("x"), "+=", luau.Num(1))),
			"x += 1\n",
		},
	}
	for _, c := range cases {
		if got := renderAST(t, c.ast); got != c.want {
			t.Errorf("got %q, want %q", got, c.want)
		}
	}
}

func TestRenderIfStatement(t *testing.T) {
	inner := stmts(luau.NewCallStatement(luau.NewCall(luau.ID("print"), exprList(luau.Str("big")))))
	elseInner := stmts(luau.NewCallStatement(luau.NewCall(luau.ID("print"), exprList(luau.Str("small")))))
	ast := stmts(luau.NewIf(luau.NewBinary(luau.ID("x"), ">", luau.Num(3)), inner, elseInner))
	want := "if x > 3 then\n\tprint(\"big\")\nelse\n\tprint(\"small\")\nend\n"
	if got := renderAST(t, ast); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

func TestRenderElseIfChain(t *testing.T) {
	elseif := luau.NewIf(luau.ID("b"), stmts(luau.NewCallStatement(luau.NewCall(luau.ID("g"), exprList()))), nil)
	ast := stmts(luau.NewIf(luau.ID("a"), stmts(luau.NewCallStatement(luau.NewCall(luau.ID("f"), exprList()))), elseif))
	want := "if a then\n\tf()\nelseif b then\n\tg()\nend\n"
	if got := renderAST(t, ast); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

func TestRenderLoops(t *testing.T) {
	body := stmts(luau.NewCallStatement(luau.NewCall(luau.ID("f"), exprList())))
	cases := []struct {
		ast  *luau.List[luau.Statement]
		want string
	}{
		{
			stmts(luau.NewNumericFor(luau.ID("i"), luau.Num(1), luau.Num(10), nil, body.Clone())),
			"for i = 1, 10 do\n\tf()\nend\n",
		},
		{
			stmts(luau.NewNumericFor(luau.ID("i"), luau.Num(1), luau.Num(10), luau.Num(1), body.Clone())),
			"for i = 1, 10 do\n\tf()\nend\n", // step of 1 omitted
		},
		{
			stmts(luau.NewNumericFor(luau.ID("i"), luau.Num(1), luau.Num(10), luau.NewNumberLiteral("0x1"), body.Clone())),
			"for i = 1, 10 do\n\tf()\nend\n", // hex step of 1 also omitted (JS Number("0x1") == 1)
		},
		{
			stmts(luau.NewNumericFor(luau.ID("i"), luau.Num(1), luau.Num(10), luau.Num(2), body.Clone())),
			"for i = 1, 10, 2 do\n\tf()\nend\n",
		},
		{
			stmts(luau.NewFor(
				luau.NewList[luau.AnyIdentifier](luau.ID("k"), luau.ID("v")),
				luau.NewCall(luau.ID("pairs"), exprList(luau.ID("t"))),
				body.Clone(),
			)),
			"for k, v in pairs(t) do\n\tf()\nend\n",
		},
		{
			stmts(luau.NewFor(luau.NewList[luau.AnyIdentifier](), luau.ID("it"), body.Clone())),
			"for _ in it do\n\tf()\nend\n", // empty ids render as _
		},
		{
			stmts(luau.NewWhile(luau.Bool(true), body.Clone())),
			"while true do\n\tf()\nend\n",
		},
		{
			stmts(luau.NewRepeat(luau.ID("done"), body.Clone())),
			"repeat\n\tf()\nuntil done\n",
		},
	}
	for _, c := range cases {
		if got := renderAST(t, c.ast); got != c.want {
			t.Errorf("got %q, want %q", got, c.want)
		}
	}
}

func TestRenderFunctions(t *testing.T) {
	body := stmts(luau.NewReturn(luau.ID("x")))
	params := luau.NewList[luau.AnyIdentifier](luau.ID("x"))
	ast := stmts(luau.NewFunctionDeclaration(true, luau.ID("f"), params, false, body))
	want := "local function f(x)\n\treturn x\nend\n"
	if got := renderAST(t, ast); got != want {
		t.Errorf("got %q, want %q", got, want)
	}

	mBody := stmts(luau.NewReturn(luau.Nil()))
	m := luau.NewMethodDeclaration(luau.ID("Class"), "method", luau.NewList[luau.AnyIdentifier](), false, mBody)
	wantM := "function Class:method()\n\treturn nil\nend\n"
	if got := renderAST(t, stmts(m)); got != wantM {
		t.Errorf("got %q, want %q", got, wantM)
	}
}

func TestRenderComment(t *testing.T) {
	if got := renderAST(t, stmts(luau.NewComment("hello"))); got != "--hello\n" {
		t.Errorf("got %q", got)
	}
	want := "--[[\n\tline1\n\tline2\n]]\n"
	if got := renderAST(t, stmts(luau.NewComment("line1\nline2"))); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
	// text containing "]]" forces the bracket padding up to "=" (--[=[ ... ]=])
	wantEq := "--[=[\n\ta]]b\n\tc\n]=]\n"
	if got := renderAST(t, stmts(luau.NewComment("a]]b\nc"))); got != wantEq {
		t.Errorf("got %q, want %q", got, wantEq)
	}
	wantCRLF := "--[[\n\tline1\n\tline2\n]]\n"
	if got := renderAST(t, stmts(luau.NewComment("line1\r\nline2"))); got != wantCRLF {
		t.Errorf("got %q, want %q", got, wantCRLF)
	}
}

func TestSemicolonAmbiguity(t *testing.T) {
	// local a = b followed by a statement starting with parenthesis needs `;`
	stmt1 := luau.NewVariableDeclaration(luau.ID("a"), luau.ID("b"))
	paren := luau.NewParenthesized(luau.NewBinary(luau.ID("c"), "+", luau.ID("d")))
	access := luau.NewPropertyAccess(paren, "e")
	stmt2 := luau.NewAssignment(access, "=", luau.ID("f"))
	want := "local a = b;\n(c + d).e = f\n"
	if got := renderAST(t, stmts(stmt1, stmt2)); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

func TestTempIdsEndToEnd(t *testing.T) {
	tmp := luau.TempID("result")
	ast := stmts(
		luau.NewVariableDeclaration(tmp, luau.Num(1)),
		luau.NewCallStatement(luau.NewCall(luau.ID("print"), exprList(tmp))),
	)
	want := "local _result = 1\nprint(_result)\n"
	if got := renderAST(t, ast); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

func TestStatementAfterFinalPanics(t *testing.T) {
	defer func() {
		if recover() == nil {
			t.Error("must panic on statement after return")
		}
	}()
	RenderAST(stmts(luau.NewReturn(luau.Nil()), luau.NewBreak()))
}

func TestRenderTypedFunctionSignatures(t *testing.T) {
	a, b := luau.ID("a"), luau.ID("b")
	luau.SetParameterType(a, &luau.Type{Name: "vector"})
	luau.SetParameterType(b, &luau.Type{Name: "vector", Optional: true})

	declaration := luau.NewFunctionDeclaration(true, luau.ID("f"), luau.NewList[luau.AnyIdentifier](a, b), false, luau.NewList[luau.Statement]())
	declaration.ReturnType = &luau.Type{Name: "number", Optional: true}

	got := renderAST(t, stmts(declaration))
	if want := "local function f(a: vector, b: vector?): number?\nend\n"; got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}
