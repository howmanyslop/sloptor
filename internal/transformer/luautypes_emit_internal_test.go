package transformer

import (
	"strings"
	"testing"

	"rotor/internal/luau"
	"rotor/internal/luau/render"
	"rotor/tsgo/ast"
)

// objectLiteralMethod is the `inc` method of a top-level `const` whose
// initializer is an object literal.
func (f *typeFixture) objectLiteralMethod(name string) *ast.Node {
	f.t.Helper()
	declaration := f.declaration(name)
	initializer := declaration.AsVariableDeclaration().Initializer
	if initializer == nil || !ast.IsObjectLiteralExpression(initializer) {
		f.t.Fatalf("%q is not initialized with an object literal", name)
	}
	for _, property := range initializer.AsObjectLiteralExpression().Properties.Nodes {
		if ast.IsMethodDeclaration(property) {
			return property
		}
	}
	f.t.Fatalf("%q has no method", name)
	return nil
}

// A method whose object was spilled to a temp is emitted in dot form with
// an explicit `self`: `any` for an unnamed literal, the receiver the class
// transform passes, and a written `this` parameter over either.
func TestLuauTypesMethodReceiver(t *testing.T) {
	fixture := loadTypeFixture(t, "src/emission.ts")
	state := fixture.state
	state.LuauTypes, state.IsLuauTyped = true, true

	cases := []struct {
		name     string
		receiver luau.TypeNode
		want     string
	}{
		{name: "anonymous", receiver: nil, want: "function _object.inc(self: any): ()"},
		{name: "anonymous", receiver: luau.TypeName("Counter"), want: "function _object.inc(self: Counter): ()"},
		{name: "explicit", receiver: luau.TypeName("Other"), want: "function _object.inc(self: Counter): ()"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			ptr := &MapPointer{Name: "object", Value: luau.TempID("object")}
			statements := transformMethodDeclarationWithReceiver(state, fixture.objectLiteralMethod(c.name), ptr, c.receiver)
			got := render.RenderAST(statements)
			if !strings.HasPrefix(got, c.want+"\n") {
				t.Errorf("rendered:\n%s\nwant first line %q", got, c.want)
			}
			if strings.Contains(got, "_object:inc") {
				t.Errorf("luauTypes must never emit a colon-form method:\n%s", got)
			}
		})
	}
}

// Without luauTypes the same method keeps rbxtsc's colon form.
func TestMethodReceiverIgnoredWithoutLuauTypes(t *testing.T) {
	fixture := loadTypeFixture(t, "src/emission.ts")
	ptr := &MapPointer{Name: "object", Value: luau.TempID("object")}
	statements := transformMethodDeclarationWithReceiver(fixture.state, fixture.objectLiteralMethod("anonymous"), ptr, luau.TypeName("Counter"))
	if got := render.RenderAST(statements); !strings.HasPrefix(got, "function _object:inc()\n") {
		t.Errorf("rendered:\n%s", got)
	}
}
