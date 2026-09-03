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

func transformSectionCommentFixture(t *testing.T, source string) string {
	t.Helper()
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"experimentalDecorators":true,"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}`)
	writeTransformFixture(t, directory, "src/service.ts", source)

	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	t.Cleanup(release)
	sourceFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/service.ts")))
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
	return printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, result.Sources[0].EmitContext()).EmitSourceFile(result.Files[0])
}

// Section comments belong to one emitted group each: the class's own metadata
// and decorators are labelled with the class name, and every member's with
// `Class.member`. Labelling only the first statement of the whole class drops
// the member name and swallows the later groups entirely.
func TestTransform_labelsEverySectionWithItsOwnDeclaration(t *testing.T) {
	// Given
	source := strings.Join([]string{
		`type FW = ClassDecorator & MethodDecorator & PropertyDecorator & { _flamework_Decorator: "Class" };`,
		`declare const Bound: () => FW;`,
		`/** @metadata flamework:type */`,
		`declare const Typed: () => FW;`,
		`@Bound()`,
		`export class Counter {`,
		` @Typed() label = "idle";`,
		` @Bound() sprint(): void {}`,
		`}`,
		"",
	}, "\n")

	// When
	printed := transformSectionCommentFixture(t, source)

	// Then
	wants := []string{
		"// (Flamework) Counter metadata",
		"// (Flamework) Counter.label metadata",
		"// (Flamework) Counter decorators",
		"// (Flamework) Counter.label decorators",
		"// (Flamework) Counter.sprint decorators",
	}
	for _, want := range wants {
		if strings.Count(printed, want) != 1 {
			t.Fatalf("expected exactly one %q:\n%s", want, printed)
		}
	}
}

// With no class decorator there is no class-level decorator section, so the
// first member's decorators must not inherit the bare class label.
func TestTransform_omitsClassDecoratorSection_whenOnlyMembersAreDecorated(t *testing.T) {
	// Given
	source := strings.Join([]string{
		`type FW = MethodDecorator & PropertyDecorator & { _flamework_Decorator: "Class" };`,
		`declare const Bound: () => FW;`,
		`export class Counter {`,
		` @Bound() count = 0;`,
		` @Bound() sprint(): void {}`,
		`}`,
		"",
	}, "\n")

	// When
	printed := transformSectionCommentFixture(t, source)

	// Then
	if strings.Contains(printed, "// (Flamework) Counter decorators") {
		t.Fatalf("class decorator section emitted for an undecorated class:\n%s", printed)
	}
	for _, want := range []string{"// (Flamework) Counter.count decorators", "// (Flamework) Counter.sprint decorators"} {
		if strings.Count(printed, want) != 1 {
			t.Fatalf("expected exactly one %q:\n%s", want, printed)
		}
	}
}
