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

// writeCrossFileDecoratorFixture declares a Flamework decorator in one project
// file and applies it in another, so the declaration site and the use site
// disagree about which UID the decorator should carry.
func writeCrossFileDecoratorFixture(t *testing.T, directory string) {
	t.Helper()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"experimentalDecorators":true,"moduleResolution":"node","rootDir":"src","outDir":"out"},"files":["src/decorators.ts","src/consumer.ts"]}`)
	writeTransformFixture(t, directory, "src/decorators.ts", strings.Join([]string{
		`type MethodDecorator = ((target: object, property: string) => void) & { _flamework_Decorator: "Method" };`,
		`export declare function BindToAxis(action: string): MethodDecorator;`,
		`export declare function BoundClass(): ((target: object) => void) & { _flamework_Decorator: "Class" };`,
	}, "\n"))
	writeTransformFixture(t, directory, "src/consumer.ts", strings.Join([]string{
		`import { BindToAxis, BoundClass } from "./decorators";`,
		`@BoundClass()`,
		`export class Bound {`,
		`	@BindToAxis("pressing")`,
		`	public sprint(): void {}`,
		`}`,
	}, "\n"))
}

func transformCrossFileDecoratorFixture(t *testing.T, directory string) string {
	t.Helper()
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	t.Cleanup(release)
	files := make([]*ast.SourceFile, 0, 2)
	for _, name := range []string{"src/decorators.ts", "src/consumer.ts"} {
		sourceFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, name)))
		if sourceFile == nil {
			t.Fatalf("source file %s not found", name)
		}
		files = append(files, sourceFile)
	}
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}
	result, err := Transform(TransformInput{Program: program, Checker: checker, Files: files, Project: project})
	if err != nil {
		t.Fatalf("Transform() error = %v", err)
	}
	return printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(result.Files[1])
}

// Reflect.decorate must key on the file the decorator is declared in. Keying on
// the file applying it silently breaks Modding.getDecorators and friends, which
// resolve `typeof Decorator` to the declaration UID, and makes two use sites of
// the same decorator disagree with each other.
func TestTransform_keysDecoratorUidOnDeclarationSite_whenAppliedFromAnotherFile(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeCrossFileDecoratorFixture(t, directory)

	// When
	printed := transformCrossFileDecoratorFixture(t, directory)

	// Then
	for _, want := range []string{`"decorators@BindToAxis"`, `"decorators@BoundClass"`} {
		if !strings.Contains(printed, want) {
			t.Fatalf("decorator UID %s missing from transformed consumer:\n%s", want, printed)
		}
	}
	if strings.Contains(printed, "consumer@BindToAxis") || strings.Contains(printed, "consumer@BoundClass") {
		t.Fatalf("decorator UID was keyed on the use site:\n%s", printed)
	}
}
