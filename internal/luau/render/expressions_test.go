package render

import (
	"testing"

	"rotor/internal/luau"
)

func renderExpr(t *testing.T, e luau.Expression) string {
	t.Helper()
	s := NewRenderState()
	solveTempIDs(s, e)
	text := Render(s, e)
	renderAST(t, stmts(luau.NewReturn(e)))
	return text
}

func exprList(es ...luau.Expression) *luau.List[luau.Expression] {
	return luau.NewList[luau.Expression](es...)
}

func TestRenderLiterals(t *testing.T) {
	cases := []struct {
		node luau.Expression
		want string
	}{
		{luau.Nil(), "nil"},
		{luau.Bool(true), "true"},
		{luau.Bool(false), "false"},
		{luau.NewVarArgs(), "..."},
		{luau.Num(5), "5"},
		{luau.NewNumberLiteral("1_000"), "1_000"},
		{luau.NewNumberLiteral("0xFF"), "0xFF"},
		{luau.Str("hi"), `"hi"`},
		{luau.Str(`say "hi"`), `'say "hi"'`},
		{luau.Str("both \" and '"), `[[both " and ']]`},
		{luau.Str("multi\nline"), "[[multi\nline]]"},
	}
	for _, c := range cases {
		if got := renderExpr(t, c.node); got != c.want {
			t.Errorf("got %q, want %q", got, c.want)
		}
	}
}

func TestRenderIndexing(t *testing.T) {
	cases := []struct {
		node luau.Expression
		want string
	}{
		{luau.NewPropertyAccess(luau.ID("a"), "b"), "a.b"},
		{luau.NewPropertyAccess(luau.ID("a"), "not valid"), `a["not valid"]`},
		{luau.NewComputedIndex(luau.ID("a"), luau.Str("b")), "a.b"},
		{luau.NewComputedIndex(luau.ID("a"), luau.Num(1)), "a[1]"},
		{luau.NewCall(luau.ID("f"), exprList(luau.Num(1), luau.Num(2))), "f(1, 2)"},
		{luau.NewMethodCall("m", luau.ID("obj"), exprList()), "obj:m()"},
	}
	for _, c := range cases {
		if got := renderExpr(t, c.node); got != c.want {
			t.Errorf("got %q, want %q", got, c.want)
		}
	}
}

func TestRenderOperators(t *testing.T) {
	cases := []struct {
		node luau.Expression
		want string
	}{
		{luau.NewBinary(luau.NewBinary(luau.ID("a"), "+", luau.ID("b")), "*", luau.ID("c")), "(a + b) * c"},
		{luau.NewBinary(luau.ID("a"), "-", luau.NewBinary(luau.ID("b"), "-", luau.ID("c"))), "a - (b - c)"},
		{luau.NewUnary("not", luau.ID("a")), "not a"},
		{luau.NewUnary("-", luau.NewUnary("-", luau.ID("a"))), "- -a"},
		{luau.NewUnary("#", luau.ID("t")), "#t"},
		{luau.NewIfExpression(luau.ID("c"), luau.Num(1), luau.Num(2)), "if c then 1 else 2"},
	}
	for _, c := range cases {
		if got := renderExpr(t, c.node); got != c.want {
			t.Errorf("got %q, want %q", got, c.want)
		}
	}
}

func TestRenderParenthesized(t *testing.T) {
	// parens around simple expressions are dropped
	if got := renderExpr(t, luau.NewParenthesized(luau.ID("x"))); got != "x" {
		t.Errorf("got %q", got)
	}
	bin := luau.NewBinary(luau.ID("a"), "+", luau.ID("b"))
	if got := renderExpr(t, luau.NewParenthesized(bin)); got != "(a + b)" {
		t.Errorf("got %q", got)
	}
}

func TestRenderTables(t *testing.T) {
	if got := renderExpr(t, luau.NewArray(exprList())); got != "{}" {
		t.Errorf("empty array got %q", got)
	}
	if got := renderExpr(t, luau.NewArray(exprList(luau.Num(1), luau.Num(2)))); got != "{ 1, 2 }" {
		t.Errorf("array got %q", got)
	}
	if got := renderExpr(t, luau.NewSet(exprList(luau.Str("a")))); got != "{\n\t[\"a\"] = true,\n}" {
		t.Errorf("set got %q", got)
	}
	m := luau.NewMap(luau.NewList(
		luau.NewMapField(luau.Str("foo"), luau.Num(1)),
		luau.NewMapField(luau.Num(2), luau.Num(3)),
	))
	if got := renderExpr(t, m); got != "{\n\tfoo = 1,\n\t[2] = 3,\n}" {
		t.Errorf("map got %q", got)
	}
}

func TestRenderMixedTable(t *testing.T) {
	if got := renderExpr(t, luau.NewMixedTable(luau.NewList[luau.Node]())); got != "{}" {
		t.Errorf("empty mixed table got %q", got)
	}
	m := luau.NewMixedTable(luau.NewList[luau.Node](
		luau.Num(1),
		luau.NewMapField(luau.Str("foo"), luau.Num(2)),
		luau.NewMapField(luau.Num(3), luau.Num(4)),
	))
	want := "{\n\t1,\n\tfoo = 2,\n\t[3] = 4,\n}"
	if got := renderExpr(t, m); got != want {
		t.Errorf("mixed table got %q, want %q", got, want)
	}
}

func TestRenderIfExpressionElseifChain(t *testing.T) {
	node := luau.NewIfExpression(luau.ID("a"), luau.Num(1),
		luau.NewIfExpression(luau.ID("b"), luau.Num(2), luau.Num(3)))
	want := "if a then 1 elseif b then 2 else 3"
	if got := renderExpr(t, node); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

func TestRenderInterpolatedString(t *testing.T) {
	parts := luau.NewList[luau.Node](
		luau.NewInterpolatedStringPart("a"),
		luau.ID("b"),
		luau.NewInterpolatedStringPart(" {c} "),
	)
	want := "`a{b} \\{c\\} `"
	if got := renderExpr(t, luau.NewInterpolatedString(parts)); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

func TestRenderInterpolatedStringWithTable(t *testing.T) {
	// table expressions are wrapped in parens inside the braces, since `{{}}`
	// is invalid Luau
	parts := luau.NewList[luau.Node](
		luau.NewInterpolatedStringPart("x="),
		luau.NewArray(exprList(luau.Num(1))),
	)
	want := "`x={({ 1 })}`"
	if got := renderExpr(t, luau.NewInterpolatedString(parts)); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

func TestRenderFunctionExpression(t *testing.T) {
	empty := luau.NewFunctionExpression(luau.NewList[luau.AnyIdentifier](), false, luau.NewList[luau.Statement]())
	if got := renderExpr(t, empty); got != "function() end" {
		t.Errorf("got %q", got)
	}
	body := luau.NewList[luau.Statement](luau.NewReturn(luau.ID("x")))
	params := luau.NewList[luau.AnyIdentifier](luau.ID("x"))
	fn := luau.NewFunctionExpression(params, true, body)
	want := "function(x, ...)\n\treturn x\nend"
	if got := renderExpr(t, fn); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}
