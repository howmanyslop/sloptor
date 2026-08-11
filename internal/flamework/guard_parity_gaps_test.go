package flamework

import (
	"errors"
	"testing"

	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/parser"
	"rotor/tsgo/printer"
	"rotor/tsgo/scanner"
	"rotor/tsgo/tspath"
)

func Test_GuardIsDefinedType_matches_upstream_exact_predicate(t *testing.T) {
	// Given
	state, sourceFile := newGuardTestState(t, `
type Empty = {};
type Property = { value: string };
type Callable = { (): void };
type Constructable = { new (): object };
type Indexed = { [key: string]: unknown };
type Union = {} | undefined;
`)
	tests := []struct {
		name string
		want bool
	}{
		{name: "Empty", want: true},
		{name: "Property", want: false},
		{name: "Callable", want: false},
		{name: "Constructable", want: false},
		{name: "Indexed", want: false},
		{name: "Union", want: false},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			// When
			typeValue := state.checker.GetTypeFromTypeNode(guardTestTypeNode(t, sourceFile, test.name))
			got := guardIsDefinedType(typeValue, state.checker)

			// Then
			if got != test.want {
				t.Fatalf("guardIsDefinedType() = %v, want %v", got, test.want)
			}
		})
	}
}

func Test_buildFlameworkGuard_groups_complete_RobloxEnum_union(t *testing.T) {
	// Given
	state, sourceFile := newGuardTestState(t, `
interface Enum {}
declare namespace Enum {
	namespace Material {
		interface Air { readonly Name: "Air" }
		interface Grass { readonly Name: "Grass" }
		function GetEnumItems(): readonly (Air | Grass)[];
	}
}
type EnumComplete = Enum.Material.Air | Enum.Material.Grass;
type EnumPartial = Enum.Material.Air | "fallback";
`)

	for _, test := range []struct {
		name string
		want string
	}{
		{name: "EnumComplete", want: `t["enum"](Enum["Material"])`},
		{name: "EnumPartial", want: `t["literal"]("fallback", Enum["Material"]["Air"])`},
	} {
		t.Run(test.name, func(t *testing.T) {
			// When
			typeNode := guardTestTypeNode(t, sourceFile, test.name)
			guard, err := buildFlameworkGuard(state, state.checker.GetTypeFromTypeNode(typeNode), typeNode)
			// Then
			if err != nil {
				t.Fatalf("buildFlameworkGuard() error = %v", err)
			}
			got := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(guard, sourceFile)
			if got != test.want {
				t.Fatalf("guard = %q, want %q", got, test.want)
			}
			reparsed := parser.ParseSourceFile(ast.SourceFileParseOptions{
				FileName: "/guard-enum-reparse.ts",
				Path:     tspath.Path("/guard-enum-reparse.ts"),
			}, "const guard = "+got+";", core.ScriptKindTS)
			if len(reparsed.Diagnostics()) != 0 {
				t.Fatalf("reparse diagnostics = %v", reparsed.Diagnostics())
			}
		})
	}
}

func Test_buildFlameworkGuard_resolves_nominal_type_at_declaration(t *testing.T) {
	// Given
	state, sourceFile := newGuardTestState(t, `
interface Instance { readonly _nominal_Instance: unique symbol }
interface Part extends Instance { readonly _nominal_Part: unique symbol }
type NominalAlias = Part & { child: string };
`)
	typeNode := guardTestTypeNode(t, sourceFile, "NominalAlias")

	// When
	guard, err := buildFlameworkGuard(state, state.checker.GetTypeFromTypeNode(typeNode), typeNode)
	// Then
	if err != nil {
		t.Fatalf("buildFlameworkGuard() error = %v", err)
	}
	got := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(guard, sourceFile)
	const want = `t["intersection"](t["instanceIsA"]("Part"), t["children"]({ "child": t["string"] }))`
	if got != want {
		t.Fatalf("guard = %q, want %q", got, want)
	}
}

func Test_buildConfiguredFlameworkGuard_preserves_dedup_traversal_order(t *testing.T) {
	// Given: declaration order deliberately differs from first traversal order.
	state, sourceFile := newGuardTestState(t, `
interface DeclaredFirst { declared: boolean }
interface TraversedFirst { traversed: string }
interface Target {
	a: TraversedFirst;
	b: TraversedFirst;
	c: DeclaredFirst;
	d: DeclaredFirst;
}
type DedupOrder = Target;
`)
	limit := 2
	state.project.config.Optimizations.GuardGenerationDedupLimit = &limit
	typeNode := guardTestTypeNode(t, sourceFile, "DedupOrder")

	// When
	result, err := buildConfiguredFlameworkGuard(state, state.checker.GetTypeFromTypeNode(typeNode), typeNode)
	// Then
	if err != nil {
		t.Fatalf("buildConfiguredFlameworkGuard() error = %v", err)
	}
	if len(result.Statements) != 2 {
		for index, statement := range result.Statements {
			t.Logf("statement %d: %s", index, printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(statement, sourceFile))
		}
		t.Fatalf("statement count = %d, want 2", len(result.Statements))
	}
	printerInstance := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil)
	wants := []string{
		`const dedup = t["interface"]({ "traversed": t["string"] });`,
		`const dedup_1 = t["interface"]({ "declared": t["boolean"] });`,
	}
	for index, statement := range result.Statements {
		if got := printerInstance.Emit(statement, sourceFile); got != wants[index] {
			t.Fatalf("statement %d = %q, want %q", index, got, wants[index])
		}
	}
}

func Test_buildFlameworkGuard_reports_primary_and_related_AST_locations(t *testing.T) {
	// Given
	state, sourceFile := newGuardTestState(t, `
interface Nested { broken: `+"`unsupported-${string}`"+` }
type DiagnosticTarget = { nested: Nested };
`)
	typeNode := guardTestTypeNode(t, sourceFile, "DiagnosticTarget")

	// When
	_, err := buildFlameworkGuard(state, state.checker.GetTypeFromTypeNode(typeNode), typeNode)

	// Then
	if !errors.Is(err, ErrUnsupportedGuardType) {
		t.Fatalf("error = %v, want ErrUnsupportedGuardType", err)
	}
	var generationError *GuardGenerationError
	if !errors.As(err, &generationError) {
		t.Fatalf("error = %#v, want GuardGenerationError", err)
	}
	semanticStart := scanner.GetTokenPosOfNode(typeNode, sourceFile, false)
	if generationError.FileName != sourceFile.FileName() || generationError.Start != semanticStart || generationError.End != typeNode.End() {
		t.Fatalf("primary location = %q:%d-%d, want %q:%d-%d", generationError.FileName, generationError.Start, generationError.End, sourceFile.FileName(), semanticStart, typeNode.End())
	}
	if len(generationError.RelatedInformation) < 2 {
		t.Fatalf("related information = %#v, want at least type and property locations", generationError.RelatedInformation)
	}
	for _, related := range generationError.RelatedInformation {
		if related.FileName == "" || related.Start >= related.End || related.TypeName == "" {
			t.Fatalf("invalid related information = %#v", related)
		}
	}
}

func Test_buildFlameworkGuard_rejects_recursive_cycle_with_typed_location(t *testing.T) {
	// Given
	state, sourceFile := newGuardTestState(t, `
interface Recursive { next: Recursive }
type RecursiveTarget = Recursive;
`)
	typeNode := guardTestTypeNode(t, sourceFile, "RecursiveTarget")
	limit := 2
	state.project.config.Optimizations.GuardGenerationDedupLimit = &limit

	// When
	_, err := buildConfiguredFlameworkGuard(state, state.checker.GetTypeFromTypeNode(typeNode), typeNode)

	// Then
	var generationError *GuardGenerationError
	if !errors.As(err, &generationError) || generationError.FileName == "" {
		t.Fatalf("error = %#v, want located GuardGenerationError", err)
	}
}
