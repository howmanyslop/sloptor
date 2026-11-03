package transformer

import (
	"slices"
	"strings"
	"testing"

	"rotor/internal/luau"
	"rotor/internal/luau/render"
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

// The registry API WP7 (class and enum aliases) and WP8 (type functions)
// build on: entry fields, naming, generic defaults, recursion checks and
// the two emission entry points, exercised without rendering a whole file.

func aliasNames(entries []*typeAliasEntry) string {
	names := make([]string, 0, len(entries))
	for _, entry := range entries {
		names = append(names, entry.Name)
	}
	return strings.Join(names, ",")
}

func TestLuauTypesAliasEntryFields(t *testing.T) {
	fixture := loadTypeFixture(t, "src/aliases.ts")

	for _, c := range []struct {
		name      string
		exported  bool
		placement aliasPlacement
	}{
		{"Point", true, atDeclaration},
		{"Hidden", false, atDeclaration},
		{"Listed", true, atDeclaration},
		{"Merged", true, atDeclaration},
		{"Handler", true, atDeclaration},
	} {
		entry := fixture.alias(c.name)
		if entry.Exported != c.exported || entry.Placement != c.placement {
			t.Errorf("%s: Exported=%v Placement=%v, want %v %v", c.name, entry.Exported, entry.Placement, c.exported, c.placement)
		}
		if entry.TypeFunction != nil || entry.Statement == nil || entry.Statement.Body == nil || entry.building {
			t.Errorf("%s: entry must carry a finished statement, got %+v", c.name, entry)
		}
		// The export marker is applied when the statement is placed.
		if entry.Statement.Export {
			t.Errorf("%s: Statement.Export must stay unset until the alias is placed", c.name)
		}
	}

	// A nested namespace member is hoisted under its flattened name and
	// keeps the namespace's export.
	fixture.functionType("size")
	entries := fixture.builder().Aliases()
	index := slices.IndexFunc(entries, func(entry *typeAliasEntry) bool { return entry.Name == "Geometry_Size" })
	if index < 0 {
		t.Fatalf("Geometry.Size was not registered: %s", aliasNames(entries))
	}
	if size := entries[index]; !size.Exported || size.Placement != hoisted {
		t.Errorf("Geometry_Size: Exported=%v Placement=%v, want true hoisted", size.Exported, size.Placement)
	}

	// Declined symbols: a LuaTuple alias has no entry.
	if entry := fixture.state.typeAliasFor(fixture.symbol("Tuple")); entry != nil {
		t.Errorf("a LuaTuple alias must not be registered, got %+v", entry)
	}
}

func TestLuauTypesAliasGenericsDefaults(t *testing.T) {
	fixture := loadTypeFixture(t, "src/aliases.ts")

	spell := func(generics []luau.TypeGeneric) string {
		parts := make([]string, 0, len(generics))
		for _, generic := range generics {
			part := generic.Name
			if generic.Default != nil {
				part += " = " + text(generic.Default)
			}
			parts = append(parts, part)
		}
		return strings.Join(parts, ", ")
	}

	for _, c := range []struct {
		name    string
		want    string
		indexes []int
		params  int
	}{
		{"Boxed", "T = number", []int{0}, 1},
		{"Pair", "A, B = A", []int{0, 1}, 2},
		// `until` is a Luau keyword: left out, and the default that names
		// it widens to any.
		{"Labeled", "T = any", []int{1}, 2},
		{"Chain", "T", []int{0}, 1},
		{"Point", "", nil, 0},
	} {
		entry := fixture.alias(c.name)
		if got := spell(entry.Generics); got != c.want {
			t.Errorf("%s generics %q, want %q", c.name, got, c.want)
		}
		if !slices.Equal(entry.GenericIndexes, c.indexes) || len(entry.Params) != c.params {
			t.Errorf("%s GenericIndexes=%v Params=%d, want %v %d", c.name, entry.GenericIndexes, len(entry.Params), c.indexes, c.params)
		}
		if got, want := spell(entry.Statement.Generics), c.want; got != want {
			t.Errorf("%s statement generics %q, want %q", c.name, got, want)
		}
	}

	// A reference drops the arguments of left-out parameters.
	labeled := fixture.alias("Labeled")
	arguments := []*checker.Type{labeled.Params[0], labeled.Params[1]}
	if got := declaredArguments(labeled, arguments); len(got) != 1 || got[0] != labeled.Params[1] {
		t.Errorf("declaredArguments kept %d arguments, want the second only", len(got))
	}
	pair := fixture.alias("Pair")
	if got := declaredArguments(pair, pair.Params); len(got) != 2 {
		t.Errorf("declaredArguments must keep every argument of a fully declared alias, got %d", len(got))
	}
}

// Defaults are trailing: a hole drops every default before it.
func TestLuauTypesAliasDropLeadingDefaults(t *testing.T) {
	generics := []luau.TypeGeneric{
		{Name: "A", Default: luau.TypeName("number")},
		{Name: "B"},
		{Name: "C", Default: luau.TypeName("string")},
		{Name: "D"},
		{Name: "E", Default: luau.TypeName("boolean")},
	}
	got := dropLeadingDefaults(generics)
	for index, generic := range got {
		hasDefault := generic.Default != nil
		if want := index == 4; hasDefault != want {
			t.Errorf("generic %s: default present = %v, want %v", generic.Name, hasDefault, want)
		}
	}
	trailing := dropLeadingDefaults([]luau.TypeGeneric{{Name: "A"}, {Name: "B", Default: luau.TypeName("number")}})
	if trailing[1].Default == nil {
		t.Errorf("a trailing default must be kept")
	}
}

func TestLuauTypesAliasTypeNodeReferencesName(t *testing.T) {
	names := map[string]struct{}{"U": {}}
	inner := luau.NewTypeFunction(nil, []luau.TypeFunctionParameter{{Name: "value", Type: luau.TypeName("U")}}, nil, luau.NewTypePack(luau.NewList[luau.TypeNode]()))
	table := luau.NewTypeTable(luau.NewList[luau.TypeTableField](luau.NewTypeTableProp(luau.TypeAccessNone, "run", luau.OptionalType(inner))))
	if !typeNodeReferencesName(table, names) {
		t.Errorf("a parameter typed U inside a table member must be found")
	}
	if typeNodeReferencesName(luau.NewTypeReference("Box", luau.NewList[luau.TypeNode](luau.TypeName("T"))), names) {
		t.Errorf("Box<T> does not reference U")
	}
	if typeNodeReferencesName(luau.NewTypeQualifiedReference("M", "U", nil), names) {
		t.Errorf("a qualified M.U is not the bare parameter U")
	}
}

func TestLuauTypesAliasRecursion(t *testing.T) {
	fixture := loadTypeFixture(t, "src/aliases.ts")
	chain := fixture.alias("Chain")
	if !recursiveArgsMatch(chain, chain.Params) {
		t.Errorf("the alias's own parameters must match")
	}
	if recursiveArgsMatch(chain, nil) || recursiveArgsMatch(chain, []*checker.Type{nil}) {
		t.Errorf("other arguments must not match")
	}
	if got, want := strings.TrimSpace(text(chain.Statement)), "type Chain<T> = { value: T, next: Chain<T>? }"; got != want {
		t.Errorf("Chain %q, want %q", got, want)
	}

	from := len(fixture.state.LuauTypeLosses())
	weird := fixture.alias("Weird")
	if got := distinctLosses(fixture.state, from); !hasLoss(got, lossRecursiveAliasArgs) {
		t.Errorf("Weird<T[]> inside Weird<T> must record RecursiveAliasArgs, got %v", got)
	}
	if len(weird.Losses) == 0 || weird.Losses[0].Kind != lossRecursiveAliasArgs {
		t.Errorf("the loss must be attributed to the Weird entry, got %+v", weird.Losses)
	}
	if got, want := weird.lossComments(), []string{" luauTypes: Weird<T[]> spelled as any: a recursive alias must be referenced with its own type arguments"}; !slices.Equal(got, want) {
		t.Errorf("loss comments %q, want %q", got, want)
	}
	if len(chain.Losses) != 0 {
		t.Errorf("Chain records no declaration-level loss, got %+v", chain.Losses)
	}
}

func TestLuauTypesAliasUniqueNames(t *testing.T) {
	fixture := loadTypeFixture(t, "src/aliases.ts")
	builder := fixture.builder()
	for _, c := range []struct{ base, want string }{
		{"Fresh", "Fresh"},
		{"Fresh", "Fresh_2"},
		{"Fresh", "Fresh_3"},
		// Luau builtins and type-function names.
		{"number", "number_2"},
		{"keyof", "keyof_2"},
		{"setmetatable", "setmetatable_2"},
		{"typeof", "typeof_2"},
		// Roblox extern types, resolved through the checker.
		{"Part", "Part_2"},
		{"Instance", "Instance_2"},
		{"Enum", "Enum_2"},
		// Type-parameter names used anywhere in the file.
		{"T", "T_2"},
		{"A", "A_2"},
		{"until", "Alias"},
		{"kebab-case", "Alias_2"},
		// Value names are a separate namespace.
		{"origin", "origin"},
	} {
		if got := fixture.state.uniqueLuauTypeName(c.base); got != c.want {
			t.Errorf("uniqueLuauTypeName(%q) = %q, want %q", c.base, got, c.want)
		}
	}
	// A registered alias holds its name.
	fixture.alias("Point")
	if got := builder.uniqueName("Point"); got != "Point_2" {
		t.Errorf("a registered alias name is taken, got %q", got)
	}
	if got := luauAliasBaseName(fixture.state, fixture.symbol("Part")); got != "Part" {
		t.Errorf("the base name is the TS name before uniqueness, got %q", got)
	}
	fresh := loadTypeFixture(t, "src/aliases.ts")
	if got := fresh.alias("Part").Name; got != "Part_2" {
		t.Errorf("the registered Part alias is %q, want Part_2", got)
	}
}

func TestLuauTypesAliasImportedNames(t *testing.T) {
	fixture := loadTypeFixture(t, "src/imports.ts")
	wrapped := fixture.alias("Wrapped")
	if !wrapped.Exported || wrapped.Placement != atDeclaration {
		t.Errorf("Wrapped must be exported at its declaration, got %+v", wrapped)
	}
	if got, want := aliasNames(fixture.builder().Aliases()), "Wrapped,Shape,ShapeKind,Container,Promise,Promise_Status"; got != want {
		t.Errorf("registration order %q, want %q", got, want)
	}
	for _, entry := range fixture.builder().Aliases() {
		if entry == wrapped {
			continue
		}
		if entry.Exported || entry.Placement != hoisted {
			t.Errorf("%s: a mirror is hoisted and never exported, got Exported=%v Placement=%v", entry.Name, entry.Exported, entry.Placement)
		}
	}
	if !fixture.state.isLuauExportedType(fixture.symbol("Wrapped")) {
		t.Errorf("Wrapped is exported")
	}
}

func TestLuauTypesAliasExportDetection(t *testing.T) {
	fixture := loadTypeFixture(t, "src/aliases.ts")
	for _, c := range []struct {
		name string
		want bool
	}{
		{"Point", true},
		{"Hidden", false},
		{"Listed", true},
		{"Merged", true},
		{"Tuple", true},
	} {
		if got := fixture.state.isLuauExportedType(fixture.symbol(c.name)); got != c.want {
			t.Errorf("isLuauExportedType(%s) = %v, want %v", c.name, got, c.want)
		}
	}
	merged := fixture.symbol("Merged")
	if first := fixture.state.luauFirstDeclaration(merged); first == nil || first != fixture.declaration("Merged") {
		t.Errorf("the first declaration of a merged interface is the fixture's first block")
	}
}

// luauDeclarationStatements places a top-level alias once, at its first
// declaration; luauHoistedTypeStatements then lists only what is left.
func TestLuauTypesAliasEmission(t *testing.T) {
	fixture := loadTypeFixture(t, "src/aliases.ts")
	fixture.state.IsLuauTyped = true
	state := fixture.state

	fixture.functionType("size") // registers Geometry_Size (hoisted)
	fixture.alias("Point")       // registered before its declaration is reached

	point := state.luauDeclarationStatements(fixture.declaration("Point"))
	if got, want := strings.TrimSpace(render.RenderAST(point)), "export type Point = { x: number, y: number }"; got != want {
		t.Errorf("Point at its declaration %q, want %q", got, want)
	}
	if again := state.luauDeclarationStatements(fixture.declaration("Point")); !again.IsEmpty() {
		t.Errorf("a placed alias is not emitted twice")
	}
	hidden := state.luauDeclarationStatements(fixture.declaration("Hidden"))
	if got, want := strings.TrimSpace(render.RenderAST(hidden)), "type Hidden = { secret: number }"; got != want {
		t.Errorf("Hidden at its declaration %q, want %q", got, want)
	}
	if tuple := state.luauDeclarationStatements(fixture.declaration("Tuple")); !tuple.IsEmpty() {
		t.Errorf("a LuaTuple alias emits nothing, got %q", render.RenderAST(tuple))
	}
	weird := state.luauDeclarationStatements(fixture.declaration("Weird"))
	if got := render.RenderAST(weird); !strings.HasPrefix(got, "-- luauTypes: Weird<T[]> spelled as any") || !strings.Contains(got, "\nexport type Weird<T> = ") {
		t.Errorf("the loss comment precedes the alias:\n%s", got)
	}

	hoistedStatements := render.RenderAST(state.luauHoistedTypeStatements())
	if got, want := strings.TrimSpace(hoistedStatements), "export type Geometry_Size = { width: number, height: number }"; got != want {
		t.Errorf("hoisted statements %q, want %q", got, want)
	}
	if !state.luauHoistedTypeStatements().IsEmpty() {
		t.Errorf("hoisted statements are emitted once")
	}

	// A second declaration of a merged symbol emits nothing; the first does.
	var mergedDeclarations []*ast.Node
	for _, statement := range state.SourceFile.AsNode().Statements() {
		if name := statement.Name(); name != nil && name.Text() == "Merged" {
			mergedDeclarations = append(mergedDeclarations, statement)
		}
	}
	if len(mergedDeclarations) != 2 {
		t.Fatalf("fixture has %d Merged declarations, want 2", len(mergedDeclarations))
	}
	if first := state.luauDeclarationStatements(mergedDeclarations[0]); first.IsEmpty() {
		t.Errorf("the first Merged declaration emits the alias")
	}
	if second := state.luauDeclarationStatements(mergedDeclarations[1]); !second.IsEmpty() {
		t.Errorf("the second Merged declaration emits nothing")
	}

	// Off: nothing.
	state.IsLuauTyped = false
	if off := state.luauDeclarationStatements(fixture.declaration("Mode")); !off.IsEmpty() {
		t.Errorf("luauTypes off emits nothing")
	}
}
