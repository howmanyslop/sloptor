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

func TestTransformFlameworkCall_builds_generic_literal_caller_and_tuple_macros(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"strict":true},"include":["src/**/*.ts"]}`)
	writeTransformFixture(t, directory, "src/main.ts", strings.Join([]string{
		`type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };`,
		`type Caller<M extends string> = { _flamework_macro_caller: M };`,
		`type Many<T> = { _flamework_macro_many: T };`,
		`declare function inspect<T>(value?: T): T;`,
		`inspect<Generic<"payload", "text">>();`,
		`inspect<Many<["left", 7, true, undefined]>>();`,
		`inspect<Many<Array<"left" | 7>>>();`,
		`inspect<Caller<"line">>();`,
	}, "\n"))

	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	defer release()
	file := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/main.ts")))
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}
	state, err := newTransformState(TransformInput{Program: program, Checker: checker, Files: []*ast.SourceFile{file}, Project: project}, nil)
	if err != nil {
		t.Fatalf("newTransformState() error = %v", err)
	}
	calls := collectCallExpressions(file)
	if len(calls) != 4 {
		t.Fatalf("len(calls) = %d, want 4", len(calls))
	}
	runtime := MacroRuntime{UUID: func() (string, error) { return "00000000-0000-4000-8000-000000000000", nil }}

	// When
	printed := make([]string, len(calls))
	for index, call := range calls {
		result, transformErr := transformFlameworkCall(state, call, runtime)
		if transformErr != nil {
			t.Fatalf("transformFlameworkCall(%d) error = %v", index, transformErr)
		}
		printed[index] = printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(result.Expression, file)
	}

	// Then
	want := []string{
		`inspect<Generic<"payload", "text">>("\"payload\"" as never)`,
		"inspect<Many<[\n    \"left\",\n    7,\n    true,\n    undefined\n]>>([\n    \"left\" as never,\n    7 as never,\n    true as never,\n    undefined as never\n] as never)",
		"inspect<Many<Array<\"left\" | 7>>>([\n    \"left\" as never,\n    7 as never\n] as never)",
		`inspect<Caller<"line">>(8 as never)`,
	}
	for index := range want {
		if printed[index] != want[index] {
			t.Fatalf("printed[%d] = %q, want %q", index, printed[index], want[index])
		}
	}
}

func TestTransformFlameworkCall_buildsMappedGenericManyFromOriginalProgramCall(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"noLib":true,"strict":true},"files":["src/main.ts"]}`)
	writeTransformFixture(t, directory, "src/main.ts", strings.Join([]string{
		`interface GenericMetadata<T> { id: string; text: string; }`,
		`type Many<T> = T & { _flamework_macro_many: T };`,
		`type Generic<T, M extends keyof GenericMetadata<T>> = GenericMetadata<T>[M] & { _flamework_macro_generic: [T, M] };`,
		`type GenericMany<T, M extends keyof GenericMetadata<T>> = Many<{ [K in M]: Generic<T, K> }>;`,
		`declare function inspect<T>(value?: Many<T>): T;`,
		`interface Payload { count: number; enabled: boolean; label: string; }`,
		`inspect<GenericMany<Payload, "id" | "text">>();`,
	}, "\n"))
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	defer release()
	file := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/main.ts")))
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}
	state, err := newTransformState(TransformInput{Program: program, Checker: checker, Files: []*ast.SourceFile{file}, Project: project}, nil)
	if err != nil {
		t.Fatalf("newTransformState() error = %v", err)
	}
	calls := collectCallExpressions(file)
	if len(calls) != 1 {
		t.Fatalf("call count = %d, want 1", len(calls))
	}

	// When
	result, err := transformFlameworkCall(state, calls[0], MacroRuntime{})
	// Then
	if err != nil {
		t.Fatalf("transformFlameworkCall() error = %v", err)
	}
	got := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(result.Expression, file)
	if !strings.Contains(got, `"id":`) || !strings.Contains(got, `"text":`) {
		t.Fatalf("mapped GenericMany output = %q, want id and text members", got)
	}
}

func TestTransformFlameworkCall_preservesCallerManyLiteralCreationOrder(t *testing.T) {
	// Given
	state := newMacroTestState(t, config.FlameworkConfig{}, strings.Join([]string{
		`interface GenericMetadata<T> { id: string; text: string; }`,
		`interface Payload { value: string; }`,
		`type Many<T> = T & { _flamework_macro_many: T };`,
		`type Generic<T, M extends keyof GenericMetadata<T>> = GenericMetadata<T>[M] & { _flamework_macro_generic: [T, M] };`,
		`type GenericMany<T, M extends keyof GenericMetadata<T>> = Many<{ [K in M]: Generic<T, K> }>;`,
		`type Caller<M extends string> = { _flamework_macro_caller: M };`,
		`type CallerMany<M extends string> = Many<{ [K in M]: Caller<K> }>;`,
		`declare function inspect<T>(value?: Many<T>): T;`,
		`inspect<GenericMany<Payload, "id" | "text">>();`,
		`inspect<CallerMany<"line" | "character" | "text" | "uuid">>();`,
	}, "\n"))
	file := state.program.GetSourceFile(filepath.ToSlash(filepath.Join(state.project.projectDirectory, "src/main.ts")))
	calls := collectCallExpressions(file)
	if len(calls) != 2 {
		t.Fatalf("call count = %d, want 2", len(calls))
	}
	runtime := MacroRuntime{UUID: func() (string, error) { return "00000000-0000-4000-8000-000000000004", nil }}
	if _, err := transformFlameworkCall(state, calls[0], runtime); err != nil {
		t.Fatalf("prime transformFlameworkCall() error = %v", err)
	}

	// When
	result, err := transformFlameworkCall(state, calls[1], runtime)
	// Then
	if err != nil {
		t.Fatalf("transformFlameworkCall() error = %v", err)
	}
	got := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(result.Expression, file)
	want := `inspect<CallerMany<"line" | "character" | "text" | "uuid">>({ "text": "inspect<CallerMany<\"line\" | \"character\" | \"text\" | \"uuid\">>()" as never, "line": 10 as never, "character": 1 as never, "uuid": "00000000-0000-4000-8000-000000000004" as never } as never)`
	if got != want {
		t.Fatalf("CallerMany output = %q, want literal-creation property order %q", got, want)
	}
}

func collectCallExpressions(file *ast.SourceFile) []*ast.Node {
	calls := make([]*ast.Node, 0)
	var visit func(*ast.Node) bool
	visit = func(node *ast.Node) bool {
		if ast.IsCallExpression(node) {
			calls = append(calls, node)
		}
		node.ForEachChild(visit)
		return false
	}
	file.AsNode().ForEachChild(visit)
	return calls
}
