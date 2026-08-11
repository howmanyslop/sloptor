package flamework

import (
	"path/filepath"
	"strings"
	"testing"

	"rotor/internal/config"
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
	"rotor/tsgo/printer"
)

func TestBuildTupleGuardsIntrinsic_splits_fixed_and_rest_guards(t *testing.T) {
	// Given
	state := newMacroTestState(t, config.FlameworkConfig{}, `type Input = [name: string, ...values: number[]];`)
	file := state.program.GetSourceFile(filepath.ToSlash(filepath.Join(state.project.projectDirectory, "src/main.ts")))
	target := firstTypeAliasType(state, file)
	runtime := namedGuardRuntime(state)

	// When
	expression, prerequisites, err := buildTupleGuardsIntrinsic(state, file.AsNode(), target, runtime)
	// Then
	if err != nil {
		t.Fatalf("buildTupleGuardsIntrinsic() error = %v", err)
	}
	if len(prerequisites) != 0 {
		t.Fatalf("len(prerequisites) = %d, want 0", len(prerequisites))
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(expression, file)
	want := "[\n    [\n        guard_string\n    ],\n    guard_number\n]"
	if printed != want {
		t.Fatalf("tuple guards =\n%s\nwant:\n%s", printed, want)
	}
}

func TestBuildTupleGuardsIntrinsic_emits_array_as_only_rest_guard(t *testing.T) {
	// Given
	state := newMacroTestState(t, config.FlameworkConfig{}, `type Input = number[];`)
	file := state.program.GetSourceFile(filepath.ToSlash(filepath.Join(state.project.projectDirectory, "src/main.ts")))
	target := firstTypeAliasType(state, file)

	// When
	expression, _, err := buildTupleGuardsIntrinsic(state, file.AsNode(), target, namedGuardRuntime(state))
	// Then
	if err != nil {
		t.Fatalf("buildTupleGuardsIntrinsic() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(expression, file)
	want := "[\n    [],\n    guard_number\n]"
	if printed != want {
		t.Fatalf("array-only rest guard =\n%s\nwant:\n%s", printed, want)
	}
}

func firstTypeAliasType(state *TransformState, file *ast.SourceFile) *checker.Type {
	for _, statement := range file.Statements.Nodes {
		if ast.IsTypeAliasDeclaration(statement) {
			return state.checker.GetTypeFromTypeNode(statement.Type())
		}
	}
	return nil
}

func namedGuardRuntime(state *TransformState) MacroRuntime {
	return MacroRuntime{BuildGuard: func(_ *TransformState, _ *ast.Node, target *checker.Type) (GuardBuildResult, error) {
		name := strings.NewReplacer("[]", "_array", " ", "_").Replace(state.checker.TypeToString(target))
		return GuardBuildResult{Expression: state.factory.NewIdentifier("guard_" + name)}, nil
	}}
}
