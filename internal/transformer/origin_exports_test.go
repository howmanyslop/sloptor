package transformer

import (
	"testing"

	"rotor/internal/luau"
)

func TestExportReturnMapFieldOrigins(t *testing.T) {
	directory := t.TempDir()
	writeFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"target":"ESNext","types":[]}}`)
	writeFixture(t, directory, "input.ts", "export const value = 1;\nexport { value as alias };\nexport default value;\n")
	state := buildInternalState(t, directory, "input.ts")
	statements := TransformSourceFile(state)
	if diagnostics := state.Diags.Flush(); len(diagnostics) != 0 {
		t.Fatalf("transform diagnostics: %v", diagnostics)
	}
	returned := statements.Tail.Value.(*luau.ReturnStatement)
	table := returned.Expression.(*luau.Map)
	fields := map[string]*luau.MapField{}
	table.Fields.ForEach(func(field *luau.MapField) { fields[field.Index.(*luau.StringLiteral).Value] = field })
	if len(fields) != 3 {
		t.Fatalf("export fields = %d, want 3", len(fields))
	}
	for _, test := range []struct {
		name           string
		start, closing luau.Position
	}{
		{"value", luau.Position{Line: 0, Column: 0}, luau.Position{Line: 0, Column: 22}},
		{"alias", luau.Position{Line: 1, Column: 9}, luau.Position{Line: 1, Column: 22}},
		{"default", luau.Position{Line: 2, Column: 0}, luau.Position{Line: 2, Column: 20}},
	} {
		t.Run(test.name, func(t *testing.T) { assertNodeOrigin(t, fields[test.name], test.start, test.closing) })
	}
	if returned.Origin() != nil || table.Origin() != nil {
		t.Error("export return wrapper must remain untagged")
	}
}
