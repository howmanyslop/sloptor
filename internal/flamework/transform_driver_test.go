package flamework

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"rotor/internal/config"
	"rotor/tsgo/ast"
	"rotor/tsgo/bundled"
	"rotor/tsgo/compiler"
	"rotor/tsgo/core"
	"rotor/tsgo/diagnostics"
	"rotor/tsgo/parser"
	"rotor/tsgo/printer"
	"rotor/tsgo/tsoptions"
	"rotor/tsgo/tspath"
	"rotor/tsgo/vfs/osvfs"
)

func TestTransform_emitsExactClassIdentifierMetadata_whenClassIsPlanned(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"rootDir":"src","outDir":"out"},"include":["src/**/*.ts"]}`)
	writeTransformFixture(t, directory, "src/service.ts", "export class Service {}\n")

	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	defer release()
	sourceFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/service.ts")))
	if sourceFile == nil {
		t.Fatal("source file not found")
	}
	project, err := OpenProject(ProjectOptions{
		ProjectDir: directory,
		RootDir:    "src",
		OutDir:     "out",
		Config:     config.FlameworkConfig{},
	})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}

	// When
	result, err := Transform(TransformInput{
		Program: program,
		Checker: checker,
		Files:   []*ast.SourceFile{sourceFile},
		Project: project,
		Analyses: []FileAnalysis{{
			FileID: "src/service.ts",
			Classes: []ClassPlan{{
				InternalID: "fixture-game:out/service@Service",
			}},
		}},
	})
	// Then
	if err != nil {
		t.Fatalf("Transform() error = %v", err)
	}
	if len(result.Files) != 1 {
		t.Fatalf("len(result.Files) = %d, want 1", len(result.Files))
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(result.Files[0])
	want := strings.Join([]string{
		`import { Reflect as Reflect } from "@flamework/core";`,
		"export class Service {",
		"    static {",
		`        Reflect["defineMetadata"](Service, "identifier", "service@Service");`,
		"    }",
		"}",
		"",
	}, "\n")
	if printed != want {
		t.Fatalf("transformed source =\n%s\nwant:\n%s", printed, want)
	}
	t.Logf("transformed source:\n%s", printed)
	if len(result.Diagnostics) != 0 {
		t.Fatalf("len(result.Diagnostics) = %d, want 0", len(result.Diagnostics))
	}
	if len(result.Sources) != 1 || !result.Sources[0].Changed() {
		t.Fatalf("source metadata = %#v, want one changed source", result.Sources)
	}
	trace := result.Sources[0].Trace()
	if trace.OriginalFileName() != sourceFile.FileName() || trace.OriginalText() != sourceFile.Text() {
		t.Fatalf("source trace = %#v, want original file identity", trace)
	}
	if got := result.Plans[0].Classes()[0].InternalID; got != "fixture-game:out/service@Service" {
		t.Fatalf("planned class internal ID = %q", got)
	}
	result.Plans[0].classes[0].InternalID = "mutated"
	if got := project.Plans()[0].Classes()[0].InternalID; got != "fixture-game:out/service@Service" {
		t.Fatalf("project plan changed through result: %q", got)
	}
}

func TestTransformDiagnostics_ordersByFilePositionAndCode_whenAddedOutOfOrder(t *testing.T) {
	// Given
	firstFile := parser.ParseSourceFile(ast.SourceFileParseOptions{
		FileName: "/a.ts",
		Path:     tspath.Path("/a.ts"),
	}, "class A {}", core.ScriptKindTS)
	secondFile := parser.ParseSourceFile(ast.SourceFileParseOptions{
		FileName: "/z.ts",
		Path:     tspath.Path("/z.ts"),
	}, "class Z {}", core.ScriptKindTS)
	state := &TransformState{}
	state.AddDiagnostic(ast.NewDiagnosticFromSerialized(
		secondFile, core.NewTextRange(2, 3), 20, diagnostics.CategoryError, "second", nil, nil, nil, false, false, false,
	))
	state.AddDiagnostic(ast.NewDiagnosticFromSerialized(
		firstFile, core.NewTextRange(4, 5), 30, diagnostics.CategoryError, "later", nil, nil, nil, false, false, false,
	))
	state.AddDiagnostic(ast.NewDiagnosticFromSerialized(
		firstFile, core.NewTextRange(1, 2), 10, diagnostics.CategoryError, "first", nil, nil, nil, false, false, false,
	))

	// When
	ordered := state.orderedDiagnostics()

	// Then
	wantPositions := []int{1, 4, 2}
	for index, want := range wantPositions {
		if ordered[index].Pos() != want {
			t.Fatalf("diagnostic %d position = %d, want %d", index, ordered[index].Pos(), want)
		}
	}
}

func TestTransform_returnsInvalidInputError_whenConcreteCompilerInputsAreNil(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{},"files":["src/main.ts"]}`)
	writeTransformFixture(t, directory, "src/main.ts", "export {};\n")
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	defer release()
	sourceFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/main.ts")))
	if sourceFile == nil {
		t.Fatal("source file not found")
	}
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", Config: config.FlameworkConfig{}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}
	valid := TransformInput{
		Program: program,
		Checker: checker,
		Files:   []*ast.SourceFile{sourceFile},
		Project: project,
	}
	tests := []struct {
		name  string
		input TransformInput
	}{
		{name: "program", input: TransformInput{Checker: checker, Files: valid.Files, Project: project}},
		{name: "checker", input: TransformInput{Program: program, Files: valid.Files, Project: project}},
		{name: "project", input: TransformInput{Program: program, Checker: checker, Files: valid.Files}},
		{name: "source file", input: TransformInput{Program: program, Checker: checker, Files: []*ast.SourceFile{nil}, Project: project}},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			// When
			_, transformErr := Transform(test.input)

			// Then
			if !errors.Is(transformErr, ErrInvalidTransformInput) {
				t.Fatalf("Transform() error = %v, want ErrInvalidTransformInput", transformErr)
			}
		})
	}
}

func TestTransform_usesSuppliedMacroRuntime_forControlledUUIDAndShuffle(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"strict":true},"files":["src/main.ts"]}`)
	writeTransformFixture(t, directory, "src/main.ts", strings.Join([]string{
		`type Caller<M extends string> = { _flamework_macro_caller: M };`,
		`type Intrinsic<N extends string, T> = { _flamework_intrinsic: [N, T] };`,
		`declare function inspect<T>(value?: T): T;`,
		`inspect<Caller<"uuid">>();`,
		`inspect<Intrinsic<"shuffle-array", ["left", "right"]>>();`,
	}, "\n"))
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	defer release()
	sourceFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/main.ts")))
	if sourceFile == nil {
		t.Fatal("source file not found")
	}
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", Config: config.FlameworkConfig{Obfuscation: true}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}
	uuidCalls, randomCalls := 0, 0
	runtime := MacroRuntime{
		UUID: func() (string, error) {
			uuidCalls++
			return "11111111-1111-4111-8111-111111111111", nil
		},
		RandomIndex: func(upperBound int) (int, error) {
			randomCalls++
			return 0, nil
		},
	}
	// When
	result, err := Transform(TransformInput{
		Program:      program,
		Checker:      checker,
		Files:        []*ast.SourceFile{sourceFile},
		Project:      project,
		MacroRuntime: &runtime,
	})
	// Then
	if err != nil {
		t.Fatalf("Transform() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(result.Files[0])
	if !strings.Contains(printed, `"11111111-1111-4111-8111-111111111111" as never`) {
		t.Fatalf("controlled UUID missing from transformed source:\n%s", printed)
	}
	right, left := strings.Index(printed, `"right" as never`), strings.Index(printed, `"left" as never`)
	if right < 0 || left < 0 || right > left {
		t.Fatalf("controlled shuffle order missing from transformed source:\n%s", printed)
	}
	if uuidCalls != 1 || randomCalls != 1 {
		t.Fatalf("runtime calls = uuid:%d random:%d, want 1 each", uuidCalls, randomCalls)
	}
	t.Logf("controlled transformed source:\n%s", printed)
}

func TestNewTransformState_preservesDefaults_whenMacroRuntimeOverrideIsPartial(t *testing.T) {
	// Given
	base, sourceFile := newExpressionTransformFixture(t, "export {};\n")
	override := MacroRuntime{UUID: func() (string, error) { return "controlled", nil }}

	// When
	state, err := newTransformState(TransformInput{Program: base.program, Checker: base.checker, Files: []*ast.SourceFile{sourceFile}, Project: base.project, MacroRuntime: &override}, nil)
	// Then
	if err != nil {
		t.Fatalf("newTransformState() error = %v", err)
	}
	override.UUID = func() (string, error) { return "mutated", nil }
	runtime := state.MacroRuntime()
	uuid, uuidErr := runtime.UUID()
	if uuidErr != nil || uuid != "controlled" || runtime.RandomIndex == nil || runtime.BuildGuard == nil {
		t.Fatalf("snapshotted runtime = uuid:%q error:%v random:%v guard:%v", uuid, uuidErr, runtime.RandomIndex != nil, runtime.BuildGuard != nil)
	}
}

func newTransformProgram(t *testing.T, directory string) *compiler.Program {
	t.Helper()
	directory = filepath.ToSlash(directory)
	host := compiler.NewCompilerHost(directory, bundled.WrapFS(osvfs.FS()), bundled.LibPath(), nil, nil)
	parsed, diagnostics := tsoptions.GetParsedCommandLineOfConfigFile(directory+"/tsconfig.json", nil, nil, host, nil)
	if len(diagnostics) != 0 {
		t.Fatalf("config diagnostics = %v", diagnostics)
	}
	return compiler.NewProgram(compiler.ProgramOptions{Host: host, Config: parsed})
}

func writeTransformFixture(t *testing.T, directory, name, contents string) {
	t.Helper()
	path := filepath.Join(directory, filepath.FromSlash(name))
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("create fixture directory: %v", err)
	}
	if err := os.WriteFile(path, []byte(contents), 0o644); err != nil {
		t.Fatalf("write fixture %s: %v", name, err)
	}
}
