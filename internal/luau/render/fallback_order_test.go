package render

import (
	"testing"

	"rotor/internal/luau"
)

func TestRenderForIteratorFallbackTempOrder(t *testing.T) {
	callback := func(authoredLocal bool, fallback float64) *luau.FunctionExpression {
		temp := luau.TempID("condition")
		body := stmts()
		if authoredLocal {
			body.Push(luau.NewVariableDeclaration(luau.ID("_0"), luau.Num(10)))
		}
		body.Push(luau.NewVariableDeclaration(temp, luau.ID("input")))
		body.Push(luau.NewIf(luau.NewBinary(temp, "==", luau.Nil()),
			stmts(luau.NewAssignment(temp, "=", luau.Num(fallback))), nil))
		body.Push(luau.NewVariableDeclaration(luau.ID("n"), temp))
		var result luau.Expression = luau.ID("n")
		if authoredLocal {
			result = luau.NewBinary(luau.ID("_0"), "+", result)
		}
		body.Push(luau.NewReturn(result))
		return luau.NewFunctionExpression(luau.NewList[luau.AnyIdentifier](), false, body)
	}
	field := luau.NewMapField(callback(true, 1), callback(false, 2))
	loop := luau.NewFor(luau.NewList[luau.AnyIdentifier](luau.ID("key"), luau.ID("value")),
		luau.NewMap(luau.NewList(field)),
		stmts(luau.NewCallStatement(luau.NewCall(luau.ID("print"), exprList(
			luau.NewCall(luau.ID("key"), exprList()), luau.NewCall(luau.ID("value"), exprList()))))))
	// The legacy renderer evaluates the value callback before the key callback.
	// Its fallback _0 belongs to the value; the key must use _1 so that the
	// authored local _0 survives and key() returns 11 when input is nil.
	want := "for key, value in {\n" +
		"\t[function()\n\t\tlocal _0 = 10\n\t\tlocal _1 = input\n" +
		"\t\tif _1 == nil then\n\t\t\t_1 = 1\n\t\tend\n" +
		"\t\tlocal n = _1\n\t\treturn _0 + n\n\tend] = function()\n" +
		"\t\tlocal _0 = input\n\t\tif _0 == nil then\n\t\t\t_0 = 2\n\t\tend\n" +
		"\t\tlocal n = _0\n\t\treturn n\n\tend,\n} do\n\tprint(key(), value())\nend\n"
	for name, render := range map[string]func() string{
		"RenderAST": func() string { return RenderAST(stmts(loop)) },
		"RenderASTWithPositions": func() string {
			text, _ := RenderASTWithPositions(stmts(loop))
			return text
		},
		"Render": func() string { return Render(NewRenderState(), loop) },
	} {
		t.Run(name, func(t *testing.T) {
			if got := render(); got != want {
				t.Fatalf("iterator callback names changed:\n%s\nwant:\n%s", got, want)
			}
		})
	}
}
