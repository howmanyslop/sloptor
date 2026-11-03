package transformer

import (
	"testing"
)

// The registry entries classes get (luautypes_class.go): a class
// declaration is placed at its declaration and exported with the class, a
// class expression is hoisted under the class name or the assigned
// variable, and a generic class keeps its generics on the alias.
func TestLuauTypesClassAliasEntries(t *testing.T) {
	fixture := loadTypeFixture(t, "src/classes.ts")

	for _, c := range []struct {
		name      string
		exported  bool
		placement aliasPlacement
		generics  int
	}{
		{"Animal", true, atDeclaration, 0},
		{"Dog", true, atDeclaration, 0},
		{"Box", true, atDeclaration, 1},
		{"Late", true, atDeclaration, 0},
	} {
		entry := fixture.alias(c.name)
		if entry.Exported != c.exported || entry.Placement != c.placement || len(entry.Generics) != c.generics {
			t.Errorf("%s: Exported=%v Placement=%v Generics=%d, want %v %v %d", c.name, entry.Exported, entry.Placement, len(entry.Generics), c.exported, c.placement, c.generics)
		}
		if entry.Statement == nil || entry.Statement.Body == nil || entry.building || entry.Kind != nativeKindTable {
			t.Errorf("%s: entry must carry a finished table alias, got %+v", c.name, entry)
		}
	}

	// A class expression assigned to a variable is hoisted under the inner
	// class name when it has one, else the variable's name; neither is
	// exported on its own.
	for _, c := range []struct {
		variable string
		alias    string
	}{
		{"Named", "Inner"},
		{"Anonymous", "Anonymous"},
	} {
		declaration := fixture.declaration(c.variable)
		class := declaration.AsVariableDeclaration().Initializer
		entry := fixture.state.typeAliasFor(class.Symbol())
		if entry == nil {
			t.Fatalf("%s: the class expression was not aliased", c.variable)
		}
		if entry.Name != c.alias || entry.Placement != hoisted || entry.Exported {
			t.Errorf("%s: Name=%q Placement=%v Exported=%v, want %q hoisted unexported", c.variable, entry.Name, entry.Placement, entry.Exported, c.alias)
		}
	}
}
