package render

import (
	"reflect"
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

func TestRenderNestedForIteratorFallbackPositions(t *testing.T) {
	callback := func(value luau.Expression) *luau.FunctionExpression {
		return luau.NewFunctionExpression(luau.NewList[luau.AnyIdentifier](), false, stmts(luau.NewReturn(value)))
	}
	innerKey := callback(luau.TempID("innerKey"))
	innerValue := callback(luau.TempID("innerValue"))
	innerField := luau.NewMapField(innerKey, innerValue)
	key := callback(luau.TempID("key"))
	value := luau.NewFunctionExpression(luau.NewList[luau.AnyIdentifier](luau.TempID("parameter")), false,
		stmts(luau.NewReturn(luau.NewMap(luau.NewList(innerField)))))
	field := luau.NewMapField(key, value)
	iterator := luau.NewCall(luau.ID("iter"), exprList(luau.TempID("first"), luau.NewMap(luau.NewList(field))))
	nestedLoop := luau.NewFor(luau.NewList[luau.AnyIdentifier](luau.ID("nested")), luau.TempID("last"), stmts())
	loop := luau.NewFor(luau.NewList[luau.AnyIdentifier](luau.ID("key")), iterator, stmts(nestedLoop))
	want := "for key in iter(_0, {\n\t[function()\n\t\treturn _4\n\tend] = function(_1)\n" +
		"\t\treturn {\n\t\t\t[function()\n\t\t\t\treturn _3\n\t\t\tend] = function()\n" +
		"\t\t\t\treturn _2\n\t\t\tend,\n\t\t}\n\tend,\n}) do\n\tfor nested in _5 do\n\tend\nend\n"
	if got := RenderAST(stmts(loop)); got != want {
		t.Fatalf("nested iterator = %q, want %q", got, want)
	}
	if got := Render(NewRenderState(), loop); got != want {
		t.Fatalf("standalone nested iterator = %q, want %q", got, want)
	}
	text, ranges := RenderASTWithPositions(stmts(loop))
	if text != want {
		t.Fatalf("positioned nested iterator = %q, want %q", text, want)
	}
	// Ranges remain in emitted key-before-value order, even though fallback
	// allocation visits each value first. Every rendered occurrence is recorded.
	wantRanges := []Range{
		closedRange(loop, 0, 0, 15, 3, 15, 0),
		nodeRange(loop.IDs.Head.Value, 0, 4, 0, 7),
		nodeRange(iterator, 0, 11, 12, 2),
		nodeRange(iterator.Expression, 0, 11, 0, 15),
		nodeRange(iterator.Args.Head.Value, 0, 16, 0, 18),
		nodeRange(iterator.Args.Tail.Value, 0, 20, 12, 1),
		nodeRange(field, 1, 1, 11, 4),
		closedRange(key, 1, 2, 3, 4, 3, 1),
		nodeRange(key.Statements.Head.Value, 2, 2, 2, 11),
		nodeRange(key.Statements.Head.Value.(*luau.ReturnStatement).Expression.(luau.Node), 2, 9, 2, 11),
		closedRange(value, 3, 8, 11, 4, 11, 1),
		nodeRange(value.Parameters.Head.Value, 3, 17, 3, 19),
		nodeRange(value.Statements.Head.Value, 4, 2, 10, 3),
		nodeRange(value.Statements.Head.Value.(*luau.ReturnStatement).Expression.(luau.Node), 4, 9, 10, 3),
		nodeRange(innerField, 5, 3, 9, 6),
		closedRange(innerKey, 5, 4, 7, 6, 7, 3),
		nodeRange(innerKey.Statements.Head.Value, 6, 4, 6, 13),
		nodeRange(innerKey.Statements.Head.Value.(*luau.ReturnStatement).Expression.(luau.Node), 6, 11, 6, 13),
		closedRange(innerValue, 7, 10, 9, 6, 9, 3),
		nodeRange(innerValue.Statements.Head.Value, 8, 4, 8, 13),
		nodeRange(innerValue.Statements.Head.Value.(*luau.ReturnStatement).Expression.(luau.Node), 8, 11, 8, 13),
		closedRange(nestedLoop, 13, 1, 14, 4, 14, 1),
		nodeRange(nestedLoop.IDs.Head.Value, 13, 5, 13, 11),
		nodeRange(nestedLoop.Expression, 13, 15, 13, 17),
	}
	if !reflect.DeepEqual(ranges, wantRanges) {
		t.Fatalf("ranges = %+v, want %+v", ranges, wantRanges)
	}
}

func TestRenderFallbackOrderAcrossMapCallbacks(t *testing.T) {
	callback := func() *luau.FunctionExpression {
		loop := luau.NewFor(luau.NewList[luau.AnyIdentifier](luau.ID("item")), luau.TempID("iterator"), stmts())
		return luau.NewFunctionExpression(luau.NewList[luau.AnyIdentifier](), false, stmts(loop))
	}
	// The callbacks are scoped, but each nested iterator still uses fallback
	// names. The map's value must allocate before its key across those subtrees.
	field := luau.NewMapField(callback(), callback())
	ret := luau.NewReturn(luau.NewMap(luau.NewList(field)))
	want := "return {\n\t[function()\n\t\tfor item in _1 do\n\t\tend\n" +
		"\tend] = function()\n\t\tfor item in _0 do\n\t\tend\n\tend,\n}\n"
	if got := RenderAST(stmts(ret)); got != want {
		t.Errorf("map callback iterators = %q, want %q", got, want)
	}
	if got, _ := RenderASTWithPositions(stmts(ret)); got != want {
		t.Errorf("positioned map callback iterators = %q, want %q", got, want)
	}
	if got := Render(NewRenderState(), ret); got != want {
		t.Errorf("standalone map callback iterators = %q, want %q", got, want)
	}
}
