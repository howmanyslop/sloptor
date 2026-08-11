package flamework

import (
	"path/filepath"
	"testing"

	"rotor/internal/config"
	"rotor/tsgo/ast"
	"rotor/tsgo/printer"
)

func TestGenerateUniversalTypeNode_emits_literal_union_object_and_call_signature(t *testing.T) {
	// Given
	state := newMacroTestState(t, config.FlameworkConfig{}, `declare const input: { readonly value: "a" | 2; callback(text: string): boolean; optional?: number };`)
	file := state.program.GetSourceFile(filepath.ToSlash(filepath.Join(state.project.projectDirectory, "src/main.ts")))
	var declaration *ast.Node
	var visit func(*ast.Node) bool
	visit = func(node *ast.Node) bool {
		if ast.IsVariableDeclaration(node) {
			declaration = node
		}
		node.ForEachChild(visit)
		return false
	}
	file.AsNode().ForEachChild(visit)
	if declaration == nil {
		t.Fatal("variable declaration not found")
	}
	target := state.checker.GetTypeAtLocation(declaration.Name())

	// When
	typeNode, prerequisites, ok := generateUniversalTypeNode(state, declaration, target)

	// Then
	if !ok || typeNode == nil {
		t.Fatal("generateUniversalTypeNode() did not generate a type")
	}
	if len(prerequisites) != 0 {
		t.Fatalf("len(prerequisites) = %d, want 0", len(prerequisites))
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(typeNode, file)
	want := "{\n    readonly value: \"a\" | 2;\n    callback(text: string): boolean;\n    optional?: number;\n}"
	if printed != want {
		t.Fatalf("universal type = %q, want %q", printed, want)
	}
}
