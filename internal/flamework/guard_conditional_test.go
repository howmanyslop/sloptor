package flamework

import (
	"context"
	"errors"
	"strings"
	"testing"

	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/parser"
	"rotor/tsgo/printer"
	"rotor/tsgo/tspath"
)

func Test_buildFlameworkGuard_emits_resolved_guard_for_instantiatedConditional(t *testing.T) {
	// Given
	state, sourceFile := newGuardTestState(t, `
type InstantiatedConditional = "ready" extends string ? string : number;
`)
	typeNode := guardTestTypeNode(t, sourceFile, "InstantiatedConditional")

	// When
	guard, err := buildFlameworkGuard(state, state.checker.GetTypeFromTypeNode(typeNode), typeNode)
	// Then
	if err != nil {
		t.Fatalf("buildFlameworkGuard() error = %v", err)
	}
	got := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(guard, sourceFile)
	const want = `t["string"]`
	if got != want {
		t.Fatalf("guard = %q, want %q", got, want)
	}
	t.Logf("printed guard: %s", got)
}

func Test_buildFlameworkGuard_emits_union_for_unresolvedGenericConditional(t *testing.T) {
	// Given
	state, sourceFile := newGuardTestState(t, `
type UnresolvedGenericConditional<T> = T extends string ? string : number;
`)
	typeNode := guardTestTypeNode(t, sourceFile, "UnresolvedGenericConditional")

	// When
	guard, err := buildFlameworkGuard(state, state.checker.GetTypeFromTypeNode(typeNode), typeNode)
	// Then
	if err != nil {
		t.Fatalf("buildFlameworkGuard() error = %v", err)
	}
	got := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(guard, sourceFile)
	const want = `t["union"](t["string"], t["number"])`
	if got != want {
		t.Fatalf("guard = %q, want %q", got, want)
	}
	reparsed := parser.ParseSourceFile(ast.SourceFileParseOptions{
		FileName: "/guard-reparse.ts",
		Path:     tspath.Path("/guard-reparse.ts"),
	}, "const guard = "+got+";", core.ScriptKindTS)
	if len(reparsed.Diagnostics()) != 0 {
		t.Fatalf("reparse diagnostics = %v", reparsed.Diagnostics())
	}
	t.Logf("printed guard: %s; reparse diagnostics: %d", got, len(reparsed.Diagnostics()))
}

func Test_buildFlameworkGuard_returns_upstreamDiagnostic_for_unconstrainedGenericConditional(t *testing.T) {
	// Given
	state, sourceFile := newGuardTestState(t, `
type UnconstrainedGenericConditional<T> = T extends string ? T : number;
`)
	typeNode := guardTestTypeNode(t, sourceFile, "UnconstrainedGenericConditional")

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
	const want = "could not find constraint of type parameter"
	if generationError.Reason != want {
		t.Fatalf("reason = %q, want %q", generationError.Reason, want)
	}
	t.Logf("upstream diagnostic: %s", generationError.Reason)
}

func TestChecker_combinedMapperInstantiation(t *testing.T) {
	tests := []struct {
		name         string
		source       string
		propertyPath []string
		badNames     []string
	}{
		{
			name: "nested_object",
			source: `
type Box<T> = { value: T };
type Wrap<U> = Box<{ wrapped: U }>;
type Actual = Wrap<string>;
type Expected = string;
declare const wrongNumber: Wrap<number>;
declare const wrongBoolean: Wrap<boolean>;
const okW: Actual = { value: { wrapped: "a" } };
const badW: Actual = wrongNumber;
const badWAgain: Actual = wrongBoolean;
`,
			propertyPath: []string{"value", "wrapped"},
			badNames:     []string{"badW", "badWAgain"},
		},
		{
			name: "conditional_tail",
			source: `
type Loop<T, N extends unknown[]> = N["length"] extends 3 ? T : Loop<T, [...N, 0]>;
type Actual = Loop<"x", []>;
type Expected = "x";
const okL: Actual = "x";
const badL: Actual = "y";
const badLAgain: Actual = "z";
`,
			badNames: []string{"badL", "badLAgain"},
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			state, sourceFile := newGuardTestState(t, test.source)
			c := state.checker
			node := guardTestTypeNode(t, sourceFile, "Actual")
			want := c.GetTypeFromTypeNode(guardTestTypeNode(t, sourceFile, "Expected"))
			before := c.TotalInstantiationCount
			actual := c.GetTypeFromTypeNode(node)
			leaf := actual
			for _, name := range test.propertyPath {
				property := c.GetPropertyOfType(leaf, name)
				if property == nil {
					t.Fatalf("missing property %q", name)
				}
				leaf = c.GetTypeOfSymbol(property)
			}
			delta := c.TotalInstantiationCount - before
			if leaf != want {
				t.Fatalf("resolved type = %p, want expected type %p", leaf, want)
			}
			if delta == 0 {
				t.Fatal("generic query performed no instantiations")
			}
			if again := c.GetTypeFromTypeNode(node); again != actual {
				t.Fatalf("repeated result = %p, want original %p", again, actual)
			}
			t.Logf("query instantiation delta = %d", delta)

			// Check diagnostics after the query so checking assignments cannot warm it.
			diagnostics := c.GetDiagnostics(context.Background(), sourceFile)
			if len(diagnostics) != len(test.badNames) {
				t.Fatalf("diagnostics = %v, want %d generic assignment errors", diagnostics, len(test.badNames))
			}
			for i, name := range test.badNames {
				declarationPos := strings.Index(test.source, "const "+name+":")
				if declarationPos < 0 {
					t.Fatalf("missing bad assignment %q", name)
				}
				wantPos := declarationPos + len("const ")
				diagnostic := diagnostics[i]
				if diagnostic.Code() != 2322 || diagnostic.File() != sourceFile || diagnostic.Pos() != wantPos || diagnostic.Len() != len(name) {
					t.Fatalf("diagnostic %d = TS%d [%d,%d) %v, want TS2322 on %s [%d,%d)", i, diagnostic.Code(), diagnostic.Pos(), diagnostic.End(), diagnostic, name, wantPos, wantPos+len(name))
				}
				t.Logf("diagnostic[%d] = TS%d [%d,%d) %q", i, diagnostic.Code(), diagnostic.Pos(), diagnostic.End(), diagnostic.String())
			}
		})
	}
}
