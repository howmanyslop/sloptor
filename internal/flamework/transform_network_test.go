package flamework

import (
	"context"
	"path/filepath"
	"strings"
	"testing"

	"rotor/internal/config"
	"rotor/tsgo/ast"
	"rotor/tsgo/printer"
)

func TestShuffledIndexes_uses_injected_randomness_when_obfuscation_enabled(t *testing.T) {
	// Given
	state := newMacroTestState(t, config.FlameworkConfig{Obfuscation: true, IDGenerationMode: "obfuscated"}, `export {};`)
	draws := []int{0, 1, 0}
	runtime := MacroRuntime{RandomIndex: func(upperBound int) (int, error) {
		value := draws[0]
		draws = draws[1:]
		return value, nil
	}}

	// When
	indexes, err := shuffledIndexes(state, 4, runtime)
	// Then
	if err != nil {
		t.Fatalf("shuffledIndexes() error = %v", err)
	}
	want := []int{2, 3, 1, 0}
	for index := range want {
		if indexes[index] != want[index] {
			t.Fatalf("indexes = %v, want %v", indexes, want)
		}
	}
}

func TestObfuscateMiddlewareObject_preserves_nested_keys_when_obfuscation_disabled(t *testing.T) {
	// Given
	state := newMacroTestState(t, config.FlameworkConfig{}, `const value = { submitted: { audit: handler } };`)
	file := state.program.GetSourceFile(filepath.ToSlash(filepath.Join(state.project.projectDirectory, "src/main.ts")))
	object := firstObjectLiteral(file)
	if object == nil {
		t.Fatal("object literal not found")
	}

	// When
	transformed, err := obfuscateMiddlewareObject(state, object, MacroRuntime{})
	// Then
	if err != nil {
		t.Fatalf("obfuscateMiddlewareObject() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(transformed, file)
	want := `{ ["submitted" as "submitted"]: { ["audit" as "audit"]: handler } }`
	if printed != want {
		t.Fatalf("middleware = %q, want %q", printed, want)
	}
	t.Logf("controlled middleware output: %s", printed)
}

func TestObfuscateMiddlewareObject_stably_hashes_and_shuffles_when_obfuscation_enabled(t *testing.T) {
	// Given
	state := newMacroTestState(t, config.FlameworkConfig{Obfuscation: true, IDGenerationMode: "obfuscated"}, `const value = { submitted: handler, accepted: handler };`)
	file := state.program.GetSourceFile(filepath.ToSlash(filepath.Join(state.project.projectDirectory, "src/main.ts")))
	object := firstObjectLiteral(file)
	runtime := MacroRuntime{RandomIndex: func(upperBound int) (int, error) { return 0, nil }}

	// When
	first, err := obfuscateMiddlewareObject(state, object, runtime)
	if err != nil {
		t.Fatalf("first obfuscateMiddlewareObject() error = %v", err)
	}
	second, err := obfuscateMiddlewareObject(state, object, runtime)
	if err != nil {
		t.Fatalf("second obfuscateMiddlewareObject() error = %v", err)
	}
	printer := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil)
	firstText := printer.Emit(first, file)
	secondText := printer.Emit(second, file)

	// Then
	if firstText != secondText {
		t.Fatalf("controlled obfuscation changed between calls:\n%s\n%s", firstText, secondText)
	}
	if strings.Contains(firstText, `"submitted" as`) || strings.Contains(firstText, `"accepted" as`) {
		t.Fatalf("controlled obfuscation retained a remote key: %s", firstText)
	}
	if !strings.HasSuffix(firstText, `: handler }`) {
		t.Fatalf("controlled obfuscation output = %s", firstText)
	}
	t.Logf("controlled obfuscated middleware output: %s", firstText)
}

func newMacroTestState(t *testing.T, flameworkConfig config.FlameworkConfig, source string) *TransformState {
	t.Helper()
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"strict":true},"include":["src/**/*.ts"]}`)
	writeTransformFixture(t, directory, "src/main.ts", source)
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	t.Cleanup(release)
	file := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/main.ts")))
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: flameworkConfig})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}
	state, err := newTransformState(TransformInput{Program: program, Checker: checker, Files: []*ast.SourceFile{file}, Project: project}, nil)
	if err != nil {
		t.Fatalf("newTransformState() error = %v", err)
	}
	return state
}

func firstObjectLiteral(file *ast.SourceFile) *ast.Node {
	var object *ast.Node
	var visit func(*ast.Node) bool
	visit = func(node *ast.Node) bool {
		if ast.IsObjectLiteralExpression(node) && object == nil {
			object = node
		}
		node.ForEachChild(visit)
		return false
	}
	file.AsNode().ForEachChild(visit)
	return object
}
