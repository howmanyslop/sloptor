package flamework

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"rotor/internal/config"
	"rotor/internal/luau/render"
	"rotor/internal/transformer"
	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/parser"
	"rotor/tsgo/printer"
	"rotor/tsgo/tspath"
)

func TestTask4DirectTransform_preservesOracleIdentifierThroughFinalLuau(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"rotor-flamework-task4-oracle","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"rootDir":"src","outDir":"out"},"files":["src/shared/macros.ts"]}`)
	writeTransformFixture(t, directory, "src/shared/macros.ts", strings.Join([]string{
		`interface Payload { readonly count: number }`,
		`type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };`,
		`declare function inspect(value?: Generic<Payload, "id">): string;`,
		`export const payloadId = inspect();`,
	}, "\n"))
	oracleTypeScript, err := os.ReadFile(filepath.Join("testdata", "task4", "expected", "transformed.ts"))
	if err != nil {
		t.Fatalf("read transformed TypeScript oracle: %v", err)
	}
	oracleLuau, err := os.ReadFile(filepath.Join("testdata", "task4", "expected", "luau", "shared", "macros.luau"))
	if err != nil {
		t.Fatalf("read final Luau oracle: %v", err)
	}
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	sourcePath := filepath.ToSlash(filepath.Join(directory, "src/shared/macros.ts"))
	sourceFile := program.GetSourceFile(sourcePath)
	project, err := OpenProject(ProjectOptions{
		ProjectDir: directory,
		RootDir:    "src",
		OutDir:     "out",
		Config:     config.FlameworkConfig{HashPrefix: "task4"},
	})
	if err != nil {
		release()
		t.Fatalf("OpenProject() error = %v", err)
	}

	// When
	result, err := Transform(TransformInput{Program: program, Checker: checker, Files: []*ast.SourceFile{sourceFile}, Project: project})
	release()
	if err != nil {
		t.Fatalf("Transform() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(result.Files[0])
	reparsed := parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: sourcePath, Path: tspath.Path(sourcePath)}, printed, core.ScriptKindTS)
	if len(reparsed.Diagnostics()) != 0 {
		t.Fatalf("reparse diagnostics = %v", reparsed.Diagnostics())
	}
	writeTransformFixture(t, directory, "src/shared/macros.ts", printed)
	luauProgram := newTransformProgram(t, directory)
	luauChecker, luauRelease := luauProgram.GetTypeChecker(context.Background())
	defer luauRelease()
	luauSource := luauProgram.GetSourceFile(sourcePath)
	state := transformer.NewState(luauProgram, luauChecker, luauSource, transformer.NewDiagService(), transformer.NewMultiState())
	luau := render.RenderAST(transformer.TransformSourceFile(state))

	// Then
	wantIdentifier := "task4:shared/macros@Payload"
	for name, artifact := range map[string]string{
		"native transformed TypeScript": printed,
		"reparsed Rotor final Luau":     luau,
		"upstream transformed oracle":   string(oracleTypeScript),
		"upstream final Luau oracle":    string(oracleLuau),
	} {
		if !strings.Contains(artifact, wantIdentifier) {
			t.Fatalf("%s missing controlled identifier %q:\n%s", name, wantIdentifier, artifact)
		}
	}
	if state.Diags.HasErrors() {
		t.Fatalf("Rotor final-Luau diagnostics = %#v", state.Diags.Flush())
	}
	t.Logf("native transformed TypeScript:\n%s\nRotor final Luau:\n%s", printed, luau)
}

func TestTransform_mergesOnlyGeneratedNamedImportsAndPreservesAuthoredImport(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"moduleResolution":"node","rootDir":"src","outDir":"out"},"files":["src/macros.ts"]}`)
	writeTransformFixture(t, directory, "node_modules/@flamework/core/package.json", `{"name":"@flamework/core","types":"index.d.ts"}`)
	writeTransformFixture(t, directory, "node_modules/@flamework/core/index.d.ts", strings.Join([]string{
		`type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };`,
		`export declare const Service: () => FlameworkDecorator;`,
		`export declare namespace Dependency {}`,
		`export declare namespace Modding {}`,
		`export declare interface OnStart {}`,
	}, "\n"))
	writeTransformFixture(t, directory, "src/macros.ts", strings.Join([]string{
		`// retained import comment`,
		`import { Dependency, Modding, type OnStart, Service } from "@flamework/core";`,
		`type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };`,
		`declare namespace Flamework {`,
		` function _id<T>(value: string): string;`,
		` /** @metadata macro {@link _id intrinsic-flamework-rewrite} */`,
		` function id<T>(value?: Generic<T, "id">): string;`,
		`}`,
		`interface Payload { value: number }`,
		`@Service() export class Consumer {}`,
		`export const payloadId = Flamework.id<Payload>();`,
	}, "\n"))
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	t.Cleanup(release)
	sourceFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/macros.ts")))
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}

	// When
	result, err := Transform(TransformInput{Program: program, Checker: checker, Files: []*ast.SourceFile{sourceFile}, Project: project})
	// Then
	if err != nil {
		t.Fatalf("Transform() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(result.Files[0])
	generated := `import { Reflect as Reflect, Flamework as Flamework } from "@flamework/core";`
	authored := `import { Dependency, Modding, type OnStart, Service } from "@flamework/core";`
	if strings.Count(printed, `from "@flamework/core"`) != 2 || strings.Count(printed, generated) != 1 || strings.Count(printed, authored) != 1 {
		t.Fatalf("generated imports were not merged separately from the authored import:\n%s", printed)
	}
	generatedIndex, commentIndex, authoredIndex := strings.Index(printed, generated), strings.Index(printed, "// retained import comment"), strings.Index(printed, authored)
	if commentIndex < 0 || generatedIndex <= commentIndex || authoredIndex <= generatedIndex {
		t.Fatalf("generated/authored import order or comment placement mismatch:\n%s", printed)
	}
	reparsed := parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: "/merged.ts", Path: tspath.Path("/merged.ts")}, printed, core.ScriptKindTS)
	if len(reparsed.Diagnostics()) != 0 {
		t.Fatalf("merged import reparse diagnostics = %v", reparsed.Diagnostics())
	}
	t.Logf("merged import TypeScript:\n%s", printed)
}

func TestTransform_preservesEmptyGeneratedImportWhenAuthoredSpecifierWins(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"moduleResolution":"node","rootDir":"src","outDir":"out"},"files":["src/macros.ts"]}`)
	writeTransformFixture(t, directory, "node_modules/@flamework/core/package.json", `{"name":"@flamework/core","types":"index.d.ts"}`)
	writeTransformFixture(t, directory, "node_modules/@flamework/core/index.d.ts", strings.Join([]string{
		`export type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };`,
		`export declare namespace Flamework {`,
		` function _id<T>(value: string): string;`,
		` /** @metadata macro {@link _id intrinsic-flamework-rewrite} */`,
		` function id<T>(value?: Generic<T, "id">): string;`,
		`}`,
		`export declare namespace Modding {}`,
	}, "\n"))
	writeTransformFixture(t, directory, "src/macros.ts", strings.Join([]string{
		`import { Flamework, Modding } from "@flamework/core";`,
		`interface Payload { value: number }`,
		`export const payloadId = Flamework.id<Payload>();`,
	}, "\n"))
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	t.Cleanup(release)
	sourceFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/macros.ts")))
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}

	// When
	result, err := Transform(TransformInput{Program: program, Checker: checker, Files: []*ast.SourceFile{sourceFile}, Project: project})
	// Then
	if err != nil {
		t.Fatalf("Transform() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, result.Sources[0].EmitContext()).EmitSourceFile(result.Files[0])
	if !strings.HasPrefix(printed, "import {} from \"@flamework/core\";\nimport { Flamework, Modding } from \"@flamework/core\";") {
		t.Fatalf("generated empty import and authored import parity mismatch:\n%s", printed)
	}
	reparsed := parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: "/empty-generated.ts", Path: tspath.Path("/empty-generated.ts")}, printed, core.ScriptKindTS)
	if len(reparsed.Diagnostics()) != 0 {
		t.Fatalf("empty generated import reparse diagnostics = %v", reparsed.Diagnostics())
	}
	writeTransformFixture(t, directory, "src/macros.ts", printed)
	semanticProgram := newTransformProgram(t, directory)
	semanticSource := semanticProgram.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/macros.ts")))
	if diagnostics := semanticProgram.GetSemanticDiagnostics(context.Background(), semanticSource); len(diagnostics) != 0 {
		t.Fatalf("empty generated import semantic diagnostics = %v", diagnostics)
	}
	t.Logf("empty generated import TypeScript:\n%s", printed)
}
