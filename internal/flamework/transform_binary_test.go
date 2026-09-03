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

// writeDependencyMacroFixture lays out a project whose only Flamework surface is
// `Dependency<T>()`, the macro that rewrites to `Flamework.resolveDependency`
// and therefore needs a synthesized `Flamework` binding.
func writeDependencyMacroFixture(t *testing.T, directory, source string) {
	t.Helper()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"moduleResolution":"node","rootDir":"src","outDir":"out"},"files":["src/consumer.ts"]}`)
	writeTransformFixture(t, directory, "node_modules/@flamework/core/package.json", `{"name":"@flamework/core","types":"index.d.ts"}`)
	writeTransformFixture(t, directory, "node_modules/@flamework/core/index.d.ts", strings.Join([]string{
		`export type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };`,
		`export declare namespace Flamework {`,
		` function resolveDependency<T>(id: string): T;`,
		`}`,
		`/** @metadata macro {@link Flamework.resolveDependency intrinsic-flamework-rewrite} */`,
		`export declare function Dependency<T>(id?: Generic<T, "id">): T;`,
	}, "\n"))
	writeTransformFixture(t, directory, "src/consumer.ts", source)
}

func transformDependencyMacroFixture(t *testing.T, directory string) string {
	t.Helper()
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	t.Cleanup(release)
	sourceFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/consumer.ts")))
	if sourceFile == nil {
		t.Fatal("source file not found")
	}
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}
	result, err := Transform(TransformInput{Program: program, Checker: checker, Files: []*ast.SourceFile{sourceFile}, Project: project})
	if err != nil {
		t.Fatalf("Transform() error = %v", err)
	}
	return printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(result.Files[0])
}

// A macro reached through an assignment used to lose the imports its rewrite
// depends on, emitting `Flamework["resolveDependency"]` with no `Flamework`
// binding in scope.
func TestTransform_importsRewriteNamespace_whenMacroIsUnderAnAssignment(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeDependencyMacroFixture(t, directory, strings.Join([]string{
		`import { Dependency } from "@flamework/core";`,
		`interface Controller { value: number }`,
		`let controller: Controller | undefined;`,
		`task.spawn(() => {`,
		`	controller = Dependency<Controller>();`,
		`});`,
		`export = controller;`,
	}, "\n"))

	// When
	printed := transformDependencyMacroFixture(t, directory)

	// Then
	if !strings.Contains(printed, `Flamework["resolveDependency"]`) {
		t.Fatalf("assignment macro was not rewritten:\n%s", printed)
	}
	if !strings.Contains(printed, `import { Flamework as Flamework } from "@flamework/core";`) {
		t.Fatalf("rewrite namespace was not imported:\n%s", printed)
	}
}

// The same loss applied to every expression form that owns its rewrite:
// compound assignment and the unary update operators.
func TestTransform_importsRewriteNamespace_whenMacroIsUnderACompoundAssignment(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeDependencyMacroFixture(t, directory, strings.Join([]string{
		`import { Dependency } from "@flamework/core";`,
		`let total = 0;`,
		`total += Dependency<number>();`,
		`export = total;`,
	}, "\n"))

	// When
	printed := transformDependencyMacroFixture(t, directory)

	// Then
	if !strings.Contains(printed, `Flamework["resolveDependency"]`) {
		t.Fatalf("compound assignment macro was not rewritten:\n%s", printed)
	}
	if !strings.Contains(printed, `import { Flamework as Flamework } from "@flamework/core";`) {
		t.Fatalf("rewrite namespace was not imported:\n%s", printed)
	}
}
