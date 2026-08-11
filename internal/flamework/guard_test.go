package flamework

import (
	"context"
	"errors"
	"path/filepath"
	"testing"

	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/parser"
	"rotor/tsgo/printer"
	"rotor/tsgo/tspath"
)

func Test_buildFlameworkGuard_emits_upstream_guards_for_representative_types(t *testing.T) {
	// Given
	state, sourceFile := newGuardTestState(t, `
type StringValue = string;
type NumberValue = number;
type BooleanValue = boolean;
type LiteralValue = "ready" | 42;
type OptionalValue = string | undefined;
type TupleValue = [string, number, boolean];
type ArrayValue = string[];
type MapValue = Map<string, number>;
type SetValue = Set<boolean>;
type ObjectValue = { name: string; count?: number };
type CallbackValue = (value: string) => number;
`)
	tests := []struct {
		name string
		want string
	}{
		{"StringValue", `t["string"]`},
		{"NumberValue", `t["number"]`},
		{"BooleanValue", `t["boolean"]`},
		{"LiteralValue", `t["literal"]("ready", 42)`},
		{"OptionalValue", `t["optional"](t["string"])`},
		{"TupleValue", `t["strictArray"](t["string"], t["number"], t["boolean"])`},
		{"ArrayValue", `t["array"](t["string"])`},
		{"MapValue", `t["map"](t["string"], t["number"])`},
		{"SetValue", `t["set"](t["boolean"])`},
		{"ObjectValue", `t["interface"]({ "name": t["string"], "count": t["optional"](t["number"]) })`},
		{"CallbackValue", `t["callback"]`},
	}

	for _, test := range tests {
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
				FileName: "/guard-reparse.ts",
				Path:     tspath.Path("/guard-reparse.ts"),
			}, "const guard = "+got+";", core.ScriptKindTS)
			if len(reparsed.Diagnostics()) != 0 {
				t.Fatalf("reparse diagnostics = %v", reparsed.Diagnostics())
			}
			t.Logf("printed guard: %s", got)
		})
	}
}

func Test_buildConfiguredFlameworkGuard_deduplicates_repeated_object_guard_at_configured_limit(t *testing.T) {
	// Given
	state, sourceFile := newGuardTestState(t, `
interface Leaf { id: string; value: number }
interface Repeated { first: Leaf; second: Leaf; third: Leaf }
type Target = Repeated;
`)
	limit := 2
	state.project.config.Optimizations.GuardGenerationDedupLimit = &limit
	typeNode := guardTestTypeNode(t, sourceFile, "Target")

	// When
	result, err := buildConfiguredFlameworkGuard(state, state.checker.GetTypeFromTypeNode(typeNode), typeNode)
	// Then
	if err != nil {
		t.Fatalf("buildConfiguredFlameworkGuard() error = %v", err)
	}
	printerInstance := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil)
	if len(result.Statements) != 1 {
		t.Fatalf("dedup statement count = %d, want 1", len(result.Statements))
	}
	statement := printerInstance.Emit(result.Statements[0], sourceFile)
	wantStatement := `const dedup = t["interface"]({ "id": t["string"], "value": t["number"] });`
	if statement != wantStatement {
		t.Fatalf("dedup statement = %q, want %q", statement, wantStatement)
	}
	guard := printerInstance.Emit(result.Expression, sourceFile)
	wantGuard := `t["interface"]({ "first": dedup, "second": dedup, "third": dedup })`
	if guard != wantGuard {
		t.Fatalf("guard = %q, want %q", guard, wantGuard)
	}
	t.Logf("dedup declaration: %s\nprinted guard: %s", statement, guard)
}

func Test_buildConfiguredFlameworkGuard_preserves_option_presence_and_zero_threshold(t *testing.T) {
	// Given
	state, sourceFile := newGuardTestState(t, `
interface Leaf { value: string }
interface Container { leaf: Leaf }
type Target = Container;
`)
	typeNode := guardTestTypeNode(t, sourceFile, "Target")
	typeValue := state.checker.GetTypeFromTypeNode(typeNode)

	// When: the optimization is absent.
	absent, err := buildConfiguredFlameworkGuard(state, typeValue, typeNode)
	if err != nil {
		t.Fatalf("absent buildConfiguredFlameworkGuard() error = %v", err)
	}

	// Then: upstream does not calculate or emit dedup declarations.
	if len(absent.Statements) != 0 {
		t.Fatalf("absent optimization emitted %d statements, want 0", len(absent.Statements))
	}

	// When: explicit zero is clamped to upstream's minimum threshold of one.
	zero := 0
	state.project.config.Optimizations.GuardGenerationDedupLimit = &zero
	explicitZero, err := buildConfiguredFlameworkGuard(state, typeValue, typeNode)
	if err != nil {
		t.Fatalf("zero buildConfiguredFlameworkGuard() error = %v", err)
	}

	// Then: both one-use object guards receive reusable declarations.
	if len(explicitZero.Statements) != 2 {
		t.Fatalf("zero optimization emitted %d statements, want 2", len(explicitZero.Statements))
	}
}

func Test_buildFlameworkGuard_returns_typed_error_for_impossible_types(t *testing.T) {
	// Given
	state, sourceFile := newGuardTestState(t, `
class UnsupportedClass {}
type UnsupportedClassValue = UnsupportedClass;
type TemplateValue = `+"`prefix-${string}`"+`;
`)
	tests := []struct {
		name string
	}{
		{name: "UnsupportedClassValue"},
		{name: "TemplateValue"},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			// When
			typeNode := guardTestTypeNode(t, sourceFile, test.name)
			_, err := buildFlameworkGuard(state, state.checker.GetTypeFromTypeNode(typeNode), typeNode)

			// Then
			if !errors.Is(err, ErrUnsupportedGuardType) {
				t.Fatalf("error = %v, want ErrUnsupportedGuardType", err)
			}
			var generationError *GuardGenerationError
			if !errors.As(err, &generationError) || generationError.TypeName == "" || generationError.Reason == "" {
				t.Fatalf("error = %#v, want populated GuardGenerationError", err)
			}
		})
	}
}

func Test_buildFlameworkGuard_emits_upstream_guards_for_composite_and_nominal_types(t *testing.T) {
	// Given
	state, sourceFile := newGuardTestState(t, `
enum Direction { Up, Down }
type EnumValue = Direction;
type PromiseValue = Promise<string>;
type UnknownValue = unknown;
type AnyValue = any;
type UndefinedValue = undefined;
type DictionaryValue = { [key: string]: number };
type IntersectionValue = { left: string } & { right: number };
type LargeUnionValue = "a" | "b" | "c";
interface Instance { readonly _nominal_Instance: unique symbol }
interface Part extends Instance { readonly _nominal_Part: unique symbol }
type InstanceValue = Part & { child: string };
`)
	tests := []struct {
		name string
		want string
	}{
		{"EnumValue", `t["literal"](0, 1)`},
		{"PromiseValue", `Promise["is"]`},
		{"UnknownValue", `t["union"](t["any"], t["none"])`},
		{"AnyValue", `t["any"]`},
		{"UndefinedValue", `t["none"]`},
		{"DictionaryValue", `t["map"](t["string"], t["number"])`},
		{"IntersectionValue", `t["intersection"](t["interface"]({ "left": t["string"] }), t["interface"]({ "right": t["number"] }))`},
		{"LargeUnionValue", `t["literalList"](["a", "b", "c"])`},
		{"InstanceValue", `t["intersection"](t["instanceIsA"]("Part"), t["children"]({ "child": t["string"] }))`},
	}

	for _, test := range tests {
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
			t.Logf("printed guard: %s", got)
		})
	}
}

func Test_buildFlameworkGuard_returns_typed_error_for_recursive_object(t *testing.T) {
	// Given
	state, sourceFile := newGuardTestState(t, `
interface Recursive { next: Recursive }
type RecursiveValue = Recursive;
`)
	typeNode := guardTestTypeNode(t, sourceFile, "RecursiveValue")

	// When
	_, err := buildFlameworkGuard(state, state.checker.GetTypeFromTypeNode(typeNode), typeNode)

	// Then
	if !errors.Is(err, ErrUnsupportedGuardType) {
		t.Fatalf("error = %v, want ErrUnsupportedGuardType", err)
	}
}

func newGuardTestState(t *testing.T, source string) (*TransformState, *ast.SourceFile) {
	t.Helper()
	directory := t.TempDir()
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"strict":true,"target":"esnext"},"files":["guards.ts"]}`)
	writeTransformFixture(t, directory, "guards.ts", source)
	program := newTransformProgram(t, directory)
	typeChecker, release := program.GetTypeChecker(context.Background())
	t.Cleanup(release)
	sourceFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "guards.ts")))
	if sourceFile == nil {
		t.Fatal("guard source file was not loaded")
	}
	return &TransformState{
		program: program,
		checker: typeChecker,
		factory: ast.NewNodeFactory(ast.NodeFactoryHooks{}),
		project: &Project{},
	}, sourceFile
}

func guardTestTypeNode(t *testing.T, sourceFile *ast.SourceFile, name string) *ast.Node {
	t.Helper()
	for _, statement := range sourceFile.Statements.Nodes {
		if ast.IsTypeAliasDeclaration(statement) && statement.Name().Text() == name {
			return statement.Type()
		}
	}
	t.Fatalf("type alias %q was not found", name)
	return nil
}
