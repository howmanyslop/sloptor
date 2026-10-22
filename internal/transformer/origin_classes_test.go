package transformer

import (
	"testing"

	"rotor/internal/luau"
)

func TestClassMemberOrigins(t *testing.T) {
	statements := transformOriginSource(t, `class Point {
	value = 1;
	static origin = 0;
	constructor(public x: number) {
		this.x = x;
	}
	method() {
		return this.x;
	}
	static read(this: void) {
		return 1;
	}
	static {
		this.origin += 1;
	}
}`).ToSlice()
	block := statements[1].(*luau.DoStatement)
	methods := map[string]*luau.MethodDeclaration{}
	var staticMethod *luau.FunctionDeclaration
	var staticProperty *luau.Assignment
	var staticBlock *luau.DoStatement
	block.Statements.ForEach(func(statement luau.Statement) {
		switch node := statement.(type) {
		case *luau.MethodDeclaration:
			methods[node.Name] = node
		case *luau.FunctionDeclaration:
			if property, ok := node.Name.(*luau.PropertyAccessExpression); ok && property.Name == "read" {
				staticMethod = node
			}
		case *luau.Assignment:
			if property, ok := node.Left.(*luau.ComputedIndexExpression); ok {
				if name, ok := property.Index.(*luau.StringLiteral); ok && name.Value == "origin" {
					staticProperty = node
				}
			}
		case *luau.DoStatement:
			staticBlock = node
		}
	})
	if methods["constructor"] == nil || methods["method"] == nil || staticMethod == nil || staticProperty == nil || staticBlock == nil {
		t.Fatal("missing transformed class members")
	}
	for _, test := range []struct {
		name           string
		node           luau.Node
		start, closing luau.Position
	}{
		{"constructor", methods["constructor"], luau.Position{Line: 3, Column: 1}, luau.Position{Line: 5, Column: 1}},
		{"parameter property", methods["constructor"].Statements.Head.Value, luau.Position{Line: 3, Column: 13}, luau.Position{Line: 3, Column: 28}},
		{"instance property", methods["constructor"].Statements.Head.Next.Value, luau.Position{Line: 1, Column: 1}, luau.Position{Line: 1, Column: 10}},
		{"method", methods["method"], luau.Position{Line: 6, Column: 1}, luau.Position{Line: 8, Column: 1}},
		{"static function", staticMethod, luau.Position{Line: 9, Column: 1}, luau.Position{Line: 11, Column: 1}},
		{"static property", staticProperty, luau.Position{Line: 2, Column: 1}, luau.Position{Line: 2, Column: 18}},
		{"static block", staticBlock, luau.Position{Line: 12, Column: 1}, luau.Position{Line: 14, Column: 1}},
	} {
		t.Run(test.name, func(t *testing.T) { assertNodeOrigin(t, test.node, test.start, test.closing) })
	}
	if block.Statements.Head.Value.Origin() != nil {
		t.Error("class boilerplate must inherit the class origin")
	}
}

func TestClassPropertyPrereqOrigins(t *testing.T) {
	function := transformOriginSource(t, "function make(a?: number) {\n\tclass Example {\n\t\tvalue = a ?? 1;\n\t\tstatic value = a ?? 2;\n\t}\n\treturn Example;\n}").Head.Value.(*luau.FunctionDeclaration)
	block := function.Statements.Head.Next.Value.(*luau.DoStatement)
	var constructor *luau.MethodDeclaration
	block.Statements.ForEach(func(statement luau.Statement) {
		if method, ok := statement.(*luau.MethodDeclaration); ok && method.Name == "constructor" {
			constructor = method
		}
	})
	if constructor == nil {
		t.Fatal("missing implicit constructor")
	}
	instance := constructor.Statements.ToSlice()
	classBody := block.Statements.ToSlice()
	static := classBody[len(classBody)-3:]
	for _, test := range []struct {
		name           string
		statements     []luau.Statement
		start, closing luau.Position
	}{
		{"instance initializer", instance, luau.Position{Line: 2, Column: 2}, luau.Position{Line: 2, Column: 16}},
		{"static initializer", static, luau.Position{Line: 3, Column: 2}, luau.Position{Line: 3, Column: 23}},
	} {
		t.Run(test.name, func(t *testing.T) {
			if len(test.statements) != 3 {
				t.Fatalf("initializer statements = %d, want local, if, assignment", len(test.statements))
			}
			for _, statement := range test.statements {
				assertNodeOrigin(t, statement, test.start, test.closing)
				if statement.Origin() != test.statements[0].Origin() {
					t.Error("property prereqs and assignment must share an origin")
				}
			}
			if test.statements[1].(*luau.IfStatement).Statements.Head.Value.Origin() != nil {
				t.Error("nested synthetic assignment must inherit the property origin")
			}
		})
	}
	if constructor.Origin() != nil {
		t.Error("implicit constructor must inherit the class origin")
	}
}
