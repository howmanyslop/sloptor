package flamework

import (
	"errors"
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
