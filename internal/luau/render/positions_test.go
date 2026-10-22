package render

import (
	"reflect"
	"testing"

	"rotor/internal/luau"
)

type positionCase struct {
	name   string
	ast    *luau.List[luau.Statement]
	text   string
	ranges []Range
}

func nodeRange(node luau.Node, line, column, endLine, endColumn int) Range {
	return Range{Node: node, Start: luau.Position{Line: line, Column: column}, End: luau.Position{Line: endLine, Column: endColumn}}
}

func closedRange(node luau.Node, line, column, endLine, endColumn, closingLine, closingColumn int) Range {
	r := nodeRange(node, line, column, endLine, endColumn)
	r.Closing = &luau.Position{Line: closingLine, Column: closingColumn}
	return r
}

func TestRenderASTWithPositions(t *testing.T) {
	call := luau.NewCall(luau.ID("f"), exprList())
	statement := luau.NewCallStatement(call)
	block := luau.NewDo(stmts(statement))
	closing := luau.Position{Line: 2, Column: 0}
	cases := []positionCase{
		{
			name: "do and call",
			ast:  stmts(block),
			text: "do\n\tf()\nend\n",
			ranges: []Range{
				{Node: block, Start: luau.Position{Line: 0, Column: 0}, End: luau.Position{Line: 2, Column: 3}, Closing: &closing},
				{Node: statement, Start: luau.Position{Line: 1, Column: 1}, End: luau.Position{Line: 1, Column: 4}},
				{Node: call, Start: luau.Position{Line: 1, Column: 1}, End: luau.Position{Line: 1, Column: 4}},
				{Node: call.Expression, Start: luau.Position{Line: 1, Column: 1}, End: luau.Position{Line: 1, Column: 2}},
			},
		},
	}
	cases = append(cases, positionCases()...)
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			text, ranges := RenderASTWithPositions(tc.ast)
			if text != tc.text {
				t.Fatalf("text = %q, want %q", text, tc.text)
			}
			if !reflect.DeepEqual(ranges, tc.ranges) {
				t.Fatalf("ranges = %#v, want %#v", ranges, tc.ranges)
			}
			if plain := RenderAST(tc.ast); text != plain {
				t.Fatalf("position text = %q, plain text = %q", text, plain)
			}
		})
	}
}

func TestOperatorPositions(t *testing.T) {
	sum := luau.NewBinary(luau.ID("a"), "+", luau.ID("b"))
	and := luau.NewBinary(luau.ID("a"), "and", luau.ID("b"))
	or := luau.NewBinary(and, "or", luau.ID("c"))
	assignment := luau.NewAssignment(luau.ID("x"), "+=", luau.Num(1))
	inner := luau.NewBinary(luau.ID("a"), "+", luau.ID("b"))
	outer := luau.NewBinary(inner, "*", luau.ID("c"))
	ast := stmts(luau.NewVariableDeclaration(luau.ID("x"), sum), luau.NewVariableDeclaration(luau.ID("x"), or), assignment, luau.NewReturn(outer))
	text, ranges := RenderASTWithPositions(ast)
	if text != "local x = a + b\nlocal x = a and b or c\nx += 1\nreturn (a + b) * c\n" || text != RenderAST(ast) {
		t.Fatalf("unexpected rendered bytes: %q", text)
	}
	want := map[luau.Node]luau.Position{
		sum: {Line: 0, Column: 12}, and: {Line: 1, Column: 12}, or: {Line: 1, Column: 18},
		assignment: {Line: 2, Column: 2}, inner: {Line: 3, Column: 10}, outer: {Line: 3, Column: 15},
	}
	for _, r := range ranges {
		if position, ok := want[r.Node]; ok {
			if r.Operator == nil || *r.Operator != position {
				t.Errorf("%T operator = %v, want %v", r.Node, r.Operator, position)
			}
			delete(want, r.Node)
		} else if r.Operator != nil {
			t.Errorf("%T has unexpected operator marker", r.Node)
		}
	}
	if len(want) != 0 {
		t.Fatalf("missing ranges: %v", want)
	}
}

func positionCases() []positionCase {
	var cases []positionCase
	{
		call := luau.NewCall(luau.ID("f"), exprList())
		statement := luau.NewCallStatement(call)
		loop := luau.NewWhile(luau.ID("live"), stmts(statement))
		repeat := luau.NewRepeat(luau.ID("done"), stmts(loop))
		block := luau.NewDo(stmts(repeat))
		cases = append(cases, positionCase{
			name: "nested blocks and until",
			ast:  stmts(block),
			text: "do\n\trepeat\n\t\twhile live do\n\t\t\tf()\n\t\tend\n\tuntil done\nend\n",
			ranges: []Range{
				closedRange(block, 0, 0, 6, 3, 6, 0),
				closedRange(repeat, 1, 1, 5, 11, 5, 1),
				closedRange(loop, 2, 2, 4, 5, 4, 2),
				nodeRange(loop.Condition, 2, 8, 2, 12),
				nodeRange(statement, 3, 3, 3, 6),
				nodeRange(call, 3, 3, 3, 6),
				nodeRange(call.Expression, 3, 3, 3, 4),
				nodeRange(repeat.Condition, 5, 7, 5, 11),
			},
		})
	}
	{
		ret := luau.NewReturn(luau.ID("p"))
		fn := luau.NewFunctionExpression(luau.NewList[luau.AnyIdentifier](luau.ID("p")), false, stmts(ret))
		call := luau.NewCall(luau.ID("f"), exprList(fn))
		decl := luau.NewVariableDeclaration(luau.ID("x"), call)
		cases = append(cases, positionCase{
			name: "function argument",
			ast:  stmts(decl),
			text: "local x = f(function(p)\n\treturn p\nend)\n",
			ranges: []Range{
				nodeRange(decl, 0, 0, 2, 4),
				nodeRange(decl.Left.(luau.Node), 0, 6, 0, 7),
				nodeRange(call, 0, 10, 2, 4),
				nodeRange(call.Expression, 0, 10, 0, 11),
				closedRange(fn, 0, 12, 2, 3, 2, 0),
				nodeRange(fn.Parameters.Head.Value, 0, 21, 0, 22),
				nodeRange(ret, 1, 1, 1, 9),
				nodeRange(ret.Expression.(luau.Node), 1, 8, 1, 9),
			},
		})
	}
	{
		call := luau.NewCall(luau.ID("f"), exprList())
		statement := luau.NewCallStatement(call)
		last := luau.NewIf(luau.ID("c"), stmts(), stmts(statement))
		link := luau.NewIf(luau.ID("b"), stmts(), last)
		first := luau.NewIf(luau.ID("a"), stmts(), link)
		block := luau.NewDo(stmts(first))
		cases = append(cases, positionCase{
			name: "elseif links contain later links and else body",
			ast:  stmts(block),
			text: "do\n\tif a then\n\telseif b then\n\telseif c then\n\telse\n\t\tf()\n\tend\nend\n",
			ranges: []Range{
				closedRange(block, 0, 0, 7, 3, 7, 0),
				closedRange(first, 1, 1, 6, 4, 6, 1),
				nodeRange(first.Condition, 1, 4, 1, 5),
				closedRange(link, 2, 1, 6, 4, 6, 1),
				nodeRange(link.Condition, 2, 8, 2, 9),
				closedRange(last, 3, 1, 6, 4, 6, 1),
				nodeRange(last.Condition, 3, 8, 3, 9),
				nodeRange(statement, 5, 2, 5, 5),
				nodeRange(call, 5, 2, 5, 5),
				nodeRange(call.Expression, 5, 2, 5, 3),
			},
		})
	}
	{
		fn := luau.NewFunctionExpression(luau.NewList[luau.AnyIdentifier](), false, stmts())
		decl := luau.NewVariableDeclaration(luau.ID("f"), fn)
		cases = append(cases, positionCase{
			name: "empty inline function",
			ast:  stmts(decl),
			text: "local f = function() end\n",
			ranges: []Range{
				nodeRange(decl, 0, 0, 0, 24),
				nodeRange(decl.Left.(luau.Node), 0, 6, 0, 7),
				closedRange(fn, 0, 10, 0, 24, 0, 21),
			},
		})
	}
	{
		comment := luau.NewComment("first\r\n😀")
		block := luau.NewDo(stmts(comment))
		cases = append(cases, positionCase{
			name: "multiline CRLF comment",
			ast:  stmts(block),
			text: "do\n\t--[[\n\t\tfirst\n\t\t😀\n\t]]\nend\n",
			ranges: []Range{
				closedRange(block, 0, 0, 5, 3, 5, 0),
				nodeRange(comment, 1, 1, 4, 3),
			},
		})
	}
	{
		str := luau.Str("a\r\n😀")
		number := luau.Num(2)
		call := luau.NewCall(luau.ID("f"), exprList(str, number))
		statement := luau.NewCallStatement(call)
		cases = append(cases, positionCase{
			name: "long bracket string",
			ast:  stmts(statement),
			text: "f([[a\r\n😀]], 2)\n",
			ranges: []Range{
				nodeRange(statement, 0, 0, 1, 8),
				nodeRange(call, 0, 0, 1, 8),
				nodeRange(call.Expression, 0, 0, 0, 1),
				nodeRange(str, 0, 2, 1, 4),
				nodeRange(number, 1, 6, 1, 7),
			},
		})
	}
	{
		call := luau.NewCall(luau.ID("f"), exprList())
		first := luau.NewCallStatement(call)
		binary := luau.NewBinary(luau.ID("g"), "+", luau.ID("h"))
		parens := luau.NewParenthesized(binary)
		nextCall := luau.NewCall(parens, exprList())
		next := luau.NewCallStatement(nextCall)
		cases = append(cases, positionCase{
			name: "semicolon belongs to statement",
			ast:  stmts(first, next),
			text: "f();\n(g + h)()\n",
			ranges: []Range{
				nodeRange(first, 0, 0, 0, 4),
				nodeRange(call, 0, 0, 0, 3),
				nodeRange(call.Expression, 0, 0, 0, 1),
				nodeRange(next, 1, 0, 1, 9),
				nodeRange(nextCall, 1, 0, 1, 9),
				nodeRange(parens, 1, 0, 1, 7),
				{Node: binary, Start: luau.Position{Line: 1, Column: 1}, End: luau.Position{Line: 1, Column: 6}, Operator: &luau.Position{Line: 1, Column: 3}},
				nodeRange(binary.Left, 1, 1, 1, 2),
				nodeRange(binary.Right, 1, 5, 1, 6),
			},
		})
	}
	{
		str := luau.Str("😀")
		id := luau.ID("x")
		call := luau.NewCall(luau.ID("f"), exprList(str, id))
		statement := luau.NewCallStatement(call)
		cases = append(cases, positionCase{
			name: "non-BMP generated columns",
			ast:  stmts(statement),
			text: "f(\"😀\", x)\n",
			ranges: []Range{
				nodeRange(statement, 0, 0, 0, 10),
				nodeRange(call, 0, 0, 0, 10),
				nodeRange(call.Expression, 0, 0, 0, 1),
				nodeRange(str, 0, 2, 0, 6),
				nodeRange(id, 0, 8, 0, 9),
			},
		})
	}
	{
		str := luau.Str("a\rb")
		id := luau.ID("x")
		call := luau.NewCall(luau.ID("f"), exprList(str, id))
		statement := luau.NewCallStatement(call)
		cases = append(cases, positionCase{
			name: "bare CR does not break a generated line",
			ast:  stmts(statement),
			text: "f(\"a\rb\", x)\n",
			ranges: []Range{
				nodeRange(statement, 0, 0, 0, 11),
				nodeRange(call, 0, 0, 0, 11),
				nodeRange(call.Expression, 0, 0, 0, 1),
				nodeRange(str, 0, 2, 0, 7),
				nodeRange(id, 0, 9, 0, 10),
			},
		})
	}
	{
		call := luau.NewCall(luau.ID("f"), exprList())
		statement := luau.NewCallStatement(call)
		cases = append(cases, positionCase{
			name: "same node rendered more than once",
			ast:  stmts(statement, statement),
			text: "f()\nf()\n",
			ranges: []Range{
				nodeRange(statement, 0, 0, 0, 3),
				nodeRange(call, 0, 0, 0, 3),
				nodeRange(call.Expression, 0, 0, 0, 1),
				nodeRange(statement, 1, 0, 1, 3),
				nodeRange(call, 1, 0, 1, 3),
				nodeRange(call.Expression, 1, 0, 1, 1),
			},
		})
	}
	{
		index := luau.NewCall(luau.ID("key"), exprList())
		value := luau.NewCall(luau.ID("value"), exprList())
		field := luau.NewMapField(index, value)
		table := luau.NewMap(luau.NewList(field))
		ret := luau.NewReturn(table)
		cases = append(cases, positionCase{
			name: "map field index precedes value",
			ast:  stmts(ret),
			text: "return {\n\t[key()] = value(),\n}\n",
			ranges: []Range{
				nodeRange(ret, 0, 0, 2, 1),
				nodeRange(table, 0, 7, 2, 1),
				nodeRange(field, 1, 1, 1, 18),
				nodeRange(index, 1, 2, 1, 7),
				nodeRange(index.Expression, 1, 2, 1, 5),
				nodeRange(value, 1, 11, 1, 18),
				nodeRange(value.Expression, 1, 11, 1, 16),
			},
		})
	}
	{
		field := luau.NewMapField(luau.Str("x"), luau.Num(1))
		table := luau.NewMap(luau.NewList(field))
		ret := luau.NewReturn(table)
		cases = append(cases, positionCase{
			name: "identifier map field key",
			ast:  stmts(ret),
			text: "return {\n\tx = 1,\n}\n",
			ranges: []Range{
				nodeRange(ret, 0, 0, 2, 1),
				nodeRange(table, 0, 7, 2, 1),
				nodeRange(field, 1, 1, 1, 6),
				nodeRange(field.Index, 1, 1, 1, 2),
				nodeRange(field.Value, 1, 5, 1, 6),
			},
		})
	}
	{
		alternative := luau.NewIfExpression(luau.ID("b"), luau.Num(2), luau.Num(3))
		expression := luau.NewIfExpression(luau.ID("a"), luau.Num(1), alternative)
		ret := luau.NewReturn(expression)
		cases = append(cases, positionCase{
			name: "elseif expression link",
			ast:  stmts(ret),
			text: "return if a then 1 elseif b then 2 else 3\n",
			ranges: []Range{
				nodeRange(ret, 0, 0, 0, 41),
				nodeRange(expression, 0, 7, 0, 41),
				nodeRange(expression.Condition, 0, 10, 0, 11),
				nodeRange(expression.Expression, 0, 17, 0, 18),
				nodeRange(alternative, 0, 19, 0, 41),
				nodeRange(alternative.Condition, 0, 26, 0, 27),
				nodeRange(alternative.Expression, 0, 33, 0, 34),
				nodeRange(alternative.Alternative, 0, 40, 0, 41),
			},
		})
	}
	return cases
}

// Every existing AST-based renderer example runs through both public APIs.
func renderAST(t *testing.T, ast *luau.List[luau.Statement]) string {
	t.Helper()
	plain := RenderAST(ast)
	positioned, _ := RenderASTWithPositions(ast)
	if positioned != plain {
		t.Fatalf("position text = %q, plain text = %q", positioned, plain)
	}
	return plain
}

func TestRenderMapFieldFallbackTempOrder(t *testing.T) {
	index, value := luau.TempID("key"), luau.TempID("value")
	field := luau.NewMapField(index, value)
	state := NewRenderState()
	// Standalone Render retains its historical fallback names: the map field's
	// value receives a name before its index, although the index appears first.
	for range 2 {
		if got := Render(state, field); got != "[_1] = _0" {
			t.Fatalf("standalone field = %q, want %q", got, "[_1] = _0")
		}
	}
}
