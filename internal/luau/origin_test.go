package luau

import "testing"

func TestOriginSurvivesCloning(t *testing.T) {
	tests := []struct {
		name  string
		clone func(*Identifier) Node
	}{
		{
			name: "adopt",
			clone: func(node *Identifier) Node {
				return NewPropertyAccess(node, "second").Expression
			},
		},
		{
			name: "adopt list",
			clone: func(node *Identifier) Node {
				return NewCall(ID("use"), NewList[Expression](node)).Args.Head.Value
			},
		},
		{
			name: "list clone",
			clone: func(node *Identifier) Node {
				return NewList[Expression](node).Clone().Head.Value
			},
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			node := ID("value")
			if got := node.Origin(); got != nil {
				t.Fatalf("new node origin = %#v, want nil", got)
			}
			origin := &Origin{
				Start:   Position{Line: 2, Column: 3},
				Closing: &Position{Line: 5, Column: 1},
			}
			SetOrigin(node, origin)
			firstParent := NewPropertyAccess(node, "first")
			clone := test.clone(node)
			if clone == node {
				t.Fatal("expected a cloned node")
			}
			if node.Parent() != firstParent {
				t.Fatal("cloning changed the original node's parent")
			}
			if got := clone.Origin(); got != origin {
				t.Fatalf("clone origin = %#v, want shared origin %#v", got, origin)
			}
			SetOrigin(clone, &Origin{Start: Position{Line: 9}})
			if node.Origin() != origin || clone.Origin() != origin {
				t.Fatal("setting an origin must preserve an existing origin")
			}
		})
	}
}
