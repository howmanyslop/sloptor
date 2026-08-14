package flamework

import (
	"context"
	"path/filepath"
	"regexp"
	"strings"
	"testing"

	"rotor/tsgo/ast"
	"rotor/tsgo/printer"
)

func TestTransformFlameworkCallExpression_injectsCallerLine_forCrossModuleMethodCall(t *testing.T) {
	// Given: a call site that does NOT import Flamework surface, but calls a
	// method whose declaration has an optional Modding.Caller<"line"> parameter.
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"rootDir":"src","outDir":"out","moduleResolution":"node","strict":true},"include":["src/**/*.ts"]}`)
	writeTransformFixture(t, directory, "src/store.ts", `
type CallerMetadata = { line: number; };
declare namespace Modding {
    export type Caller<M extends keyof CallerMetadata> = CallerMetadata[M] & { _flamework_macro_caller: M };
}
export interface Store {
    enqueue(name: string, value: unknown, callsiteId?: Modding.Caller<"line">): void;
}
`)
	writeTransformFixture(t, directory, "src/main.ts", `
import { Store } from "./store";
declare const store: Store;
store.enqueue("settings.userSettings", { value: 1 });
`)

	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	defer release()

	sourceFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/main.ts")))
	if sourceFile == nil {
		t.Fatal("source file not found")
	}

	project, err := OpenProject(ProjectOptions{
		ProjectDir: directory,
		RootDir:    "src",
		OutDir:     "out",
	})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}

	state, err := newTransformState(TransformInput{
		Program: program,
		Checker: checker,
		Files:   []*ast.SourceFile{sourceFile},
		Project: project,
	}, nil)
	if err != nil {
		t.Fatalf("newTransformState() error = %v", err)
	}

	// When: the single call expression in main.ts is transformed.
	calls := collectCallExpressions(sourceFile)
	if len(calls) != 1 {
		t.Fatalf("call count = %d, want 1", len(calls))
	}
	result, err := transformFlameworkCallExpression(state, calls[0], MacroRuntime{})
	if err != nil {
		t.Fatalf("transformFlameworkCallExpression() error = %v", err)
	}

	// Then: the emitted call includes the injected line-number argument.
	got := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(result.expression, sourceFile)
	t.Logf("transformed call: %s", got)
	if !strings.Contains(got, `"settings.userSettings"`) {
		t.Fatalf("transformed call lost the key argument: %s", got)
	}
	if !regexp.MustCompile(`\d+ as never\)$`).MatchString(got) {
		t.Fatalf("transformed call missing injected Caller<\"line\"> line number: %s", got)
	}
}
