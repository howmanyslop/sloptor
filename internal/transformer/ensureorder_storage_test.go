package transformer

import (
	"strconv"
	"testing"

	"rotor/internal/luau"
	"rotor/internal/luau/render"
	"rotor/tsgo/ast"
)

func TestEnsureTransformOrderCaptureAndSpliceOrder(t *testing.T) {
	s := buildVarArgsOptimizationState(t, `
let changing = 1;
const fixed = 2;
let primitive = 7;
let prepared = 8;
let tail = 3;
[changing, fixed, primitive, prepared, 9, 10, tail];
`)
	statements := s.SourceFile.Statements.Nodes
	nodes := statements[len(statements)-1].AsExpressionStatement().Expression.AsArrayLiteralExpression().Elements.Nodes
	call := func(name string) *luau.CallExpression {
		return luau.NewCall(luau.GlobalID(name), luau.NewList[luau.Expression]())
	}
	var captured []*luau.List[luau.Statement]
	var result []luau.Expression
	prereqs := s.CaptureStatements(func() {
		s.Prereq(luau.NewCallStatement(call("before")))
		outer := s.prereqStack[len(s.prereqStack)-1]
		result = ensureTransformOrderWith(s, nodes, func(s *State, node *ast.Node) luau.Expression {
			i := len(captured)
			if i >= len(nodes) || node != nodes[i] {
				t.Fatal("transforms must run once each in source order")
			}
			if outer.Size() != 1 || len(s.Multi.IsDefinedAsLetCache) != 0 {
				t.Fatal("prerequisite splicing and mutability queries must follow all captures")
			}
			for _, list := range captured {
				if list.ReadOnly {
					t.Fatal("a captured list was consumed before all transforms finished")
				}
			}
			captured = append(captured, s.prereqStack[len(s.prereqStack)-1])
			s.Diags.Add(DiagNoAny(node))
			switch i {
			case 0:
				return luau.ID("changing")
			case 1:
				s.Prereq(luau.NewCallStatement(call("first")))
				return luau.ID("fixed")
			case 2:
				return luau.Num(7)
			case 3:
				return s.PushToVar(call("prepare"), "exp")
			case 4:
				return call("read")
			case 5:
				s.Prereq(luau.NewCallStatement(call("mutate")))
				s.Prereq(luau.NewCallStatement(call("after")))
				return luau.ID("changed")
			default:
				return luau.ID("tail")
			}
		})
	})
	if len(result) != len(nodes) || len(captured) != len(nodes) || len(s.prereqStack) != 0 {
		t.Fatal("lost expressions or captures, or leaked prerequisite stack")
	}
	for i, list := range captured {
		if !list.ReadOnly {
			t.Errorf("captured list %d was not consumed", i)
		}
	}
	// The transform-created temp precedes both ordering temps, even though
	// its declaration is emitted between theirs.
	first, firstOK := result[0].(*luau.TemporaryIdentifier)
	prepared, preparedOK := result[3].(*luau.TemporaryIdentifier)
	second, secondOK := result[4].(*luau.TemporaryIdentifier)
	if !firstOK || !preparedOK || !secondOK || prepared.ID >= first.ID || first.ID >= second.ID {
		t.Fatal("temporary creation order changed")
	}
	for _, check := range []struct {
		index   int
		mutable bool
	}{{0, true}, {1, false}, {2, true}, {3, true}, {6, true}} {
		symbol := s.Checker.GetSymbolAtLocation(nodes[check.index])
		mutable, queried := s.Multi.IsDefinedAsLetCache[symbol]
		if symbol == nil || !queried || mutable != check.mutable {
			t.Errorf("identifier %d: mutability = %v, queried = %v", check.index, mutable, queried)
		}
	}
	diags := s.Diags.Flush()
	if len(diags) != len(nodes) {
		t.Fatalf("got %d diagnostics, want %d", len(diags), len(nodes))
	}
	for i, diag := range diags {
		if diag.Node != nodes[i] || diag.Code != "noAny" {
			t.Errorf("diagnostic %d lost its source order or identity", i)
		}
	}
	prereqs.Push(luau.NewCallStatement(luau.NewCall(luau.GlobalID("use"), luau.NewList(result...))))
	want := `before()
local _exp = changing
first()
local _exp_1 = prepare()
local _exp_2 = read()
mutate()
after()
use(_exp, fixed, 7, _exp_1, _exp_2, changed, tail)
`
	if got := render.RenderAST(prereqs); got != want {
		t.Errorf("rendered:\n%s\nwant:\n%s", got, want)
	}
}

func TestEnsureTransformOrderSmallInputs(t *testing.T) {
	for _, tt := range []struct {
		name   string
		source string
		prereq bool
		want   string
	}{
		{"empty", "[];", false, "return \n"},
		{"one", "[1];", false, "return 1\n"},
		{"one_with_prereq", "[1];", true, "prepare()\nreturn read()\n"},
		{"many_without_prereqs", "[1, 2, 3];", false, "return 1, 2, 3\n"},
	} {
		t.Run(tt.name, func(t *testing.T) {
			s, expression := parseFirstExpression(t, tt.source)
			nodes := expression.AsArrayLiteralExpression().Elements.Nodes
			var result []luau.Expression
			prereqs := s.CaptureStatements(func() {
				result = ensureTransformOrderWith(s, nodes, func(s *State, node *ast.Node) luau.Expression {
					if tt.prereq {
						s.Prereq(luau.NewCallStatement(luau.NewCall(luau.GlobalID("prepare"), luau.NewList[luau.Expression]())))
						return luau.NewCall(luau.GlobalID("read"), luau.NewList[luau.Expression]())
					}
					return TransformExpression(s, node)
				})
			})
			if result == nil || len(result) != len(nodes) || cap(result) != len(nodes) {
				t.Fatalf("result shape: nil = %v, len = %d, cap = %d", result == nil, len(result), cap(result))
			}
			prereqs.Push(luau.NewReturn(luau.NewList(result...)))
			if got := render.RenderAST(prereqs); got != tt.want {
				t.Errorf("rendered = %q, want %q", got, tt.want)
			}
			if diags := s.Diags.Flush(); len(diags) != 0 {
				t.Errorf("unexpected diagnostics: %v", diags)
			}
		})
	}
}

// Compare this operation on both revisions with the same test file. Capture
// lists and literal allocations remain included; B/op discriminates the
// sibling slice storage without attributing child-transform time to it.
func BenchmarkEnsureTransformOrderStorage(b *testing.B) {
	for _, count := range []int{1, 8, 64} {
		b.Run(strconv.Itoa(count), func(b *testing.B) {
			s := NewTestState()
			nodes := make([]*ast.Node, count)
			for i := range nodes {
				nodes[i] = &ast.Node{Kind: ast.KindNumericLiteral}
			}
			transform := func(*State, *ast.Node) luau.Expression { return luau.Num(1) }
			s.CaptureStatements(func() { ensureTransformOrderWith(s, nodes, transform) })
			b.ReportAllocs()
			b.ResetTimer()
			for range b.N {
				var result []luau.Expression
				prereqs := s.CaptureStatements(func() {
					result = ensureTransformOrderWith(s, nodes, transform)
				})
				if len(result) != count || prereqs.IsNonEmpty() {
					b.Fatal("unexpected expressions or prerequisites")
				}
			}
		})
	}
}
