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

func TestAnalyzeFlameworkClasses_preservesPlannedClassTransformOutput(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}`)
	writeTransformFixture(t, directory, "src/service.ts", "export class Service {}\n")
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	defer release()
	file := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/service.ts")))
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}

	// When
	result, err := Transform(TransformInput{
		Program: program, Checker: checker, Files: []*ast.SourceFile{file}, Project: project,
		Analyses: []FileAnalysis{{FileID: "src/service.ts", Classes: []ClassPlan{{InternalID: "fixture-game:out/service@Service"}}}},
	})
	// Then
	if err != nil {
		t.Fatalf("Transform() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(result.Files[0])
	for _, fragment := range []string{"export class Service", `Reflect["defineMetadata"](Service, "identifier", "service@Service");`} {
		if !strings.Contains(printed, fragment) {
			t.Fatalf("planned transform missing %q:\n%s", fragment, printed)
		}
	}
}

func TestDiscoverFlameworkClasses_transformsDiscoveredClassAfterExpressionVisitorClonesIt(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"experimentalDecorators":true,"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}`)
	writeTransformFixture(t, directory, "src/service.ts", strings.Join([]string{
		`type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };`,
		`declare const Service: () => FlameworkDecorator;`,
		`type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };`,
		`declare function textMacro(value?: Generic<"payload", "text">): string;`,
		`@Service() export class Consumer {}`,
		`export const text = textMacro();`,
	}, "\n"))
	input := newClassAnalysisInput(t, directory, "src/service.ts")

	// When
	result, err := Transform(input)
	// Then
	if err != nil {
		t.Fatalf("Transform() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(result.Files[0])
	if !strings.Contains(printed, `Reflect["defineMetadata"](Consumer, "identifier", "service@Consumer");`) {
		t.Fatalf("transformed source missing discovered identifier:\n%s", printed)
	}
}

func TestDiscoverFlameworkClasses_discoversClassWithOnlyMemberDecorator(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"experimentalDecorators":true,"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}`)
	writeTransformFixture(t, directory, "src/service.ts", strings.Join([]string{
		`type FlameworkDecorator = MethodDecorator & { _flamework_Decorator: "Method" };`,
		`declare const Observe: () => FlameworkDecorator;`,
		`export class Consumer { @Observe() run(): void {} }`,
	}, "\n"))
	input := newClassAnalysisInput(t, directory, "src/service.ts")

	// When
	analysis, err := AnalyzeFlameworkClasses(input)
	// Then
	if err != nil {
		t.Fatalf("AnalyzeFlameworkClasses() error = %v", err)
	}
	if classes := analysis.Classes(); len(classes) != 1 || classes[0].InternalID != "fixture-game:out/service@Consumer" {
		t.Fatalf("discovered classes = %#v", classes)
	}
	if files := analysis.Files(); len(files) != 1 || !files[0].Classes[0].containsLegacyDecorator {
		t.Fatalf("member decorator provenance = %#v", files)
	}
}

func TestDiscoverFlameworkClasses_discoversNamespacedDecorator(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"experimentalDecorators":true,"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}`)
	writeTransformFixture(t, directory, "src/service.ts", strings.Join([]string{
		`type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };`,
		`namespace Decorators { export declare const Service: () => FlameworkDecorator }`,
		`@Decorators.Service() export class Consumer {}`,
	}, "\n"))
	input := newClassAnalysisInput(t, directory, "src/service.ts")

	// When
	analysis, err := AnalyzeFlameworkClasses(input)
	// Then
	if err != nil {
		t.Fatalf("AnalyzeFlameworkClasses() error = %v", err)
	}
	classes := analysis.Classes()
	if len(classes) != 1 || strings.Join(classes[0].DecoratorIDs, ",") != "fixture-game:out/service@Decorators.Service" {
		t.Fatalf("discovered namespaced decorator classes = %#v", classes)
	}
	if files := analysis.Files(); len(files) != 1 || !files[0].Classes[0].containsLegacyDecorator {
		t.Fatalf("namespaced decorator provenance = %#v", files)
	}
}

func TestDiscoverFlameworkClasses_discoversDecoratorAndInferredTypeIDs(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"experimentalDecorators":true,"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}`)
	writeTransformFixture(t, directory, "src/service.ts", strings.Join([]string{
		`type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };`,
		`/** @metadata reflect */`,
		`declare const Service: () => FlameworkDecorator;`,
		`class Dependency {}`,
		`@Service() export class Consumer { constructor(dependency = new Dependency()) {} }`,
	}, "\n"))
	input := newClassAnalysisInput(t, directory, "src/service.ts")

	// When
	analysis, err := AnalyzeFlameworkClasses(input)
	// Then
	if err != nil {
		t.Fatalf("AnalyzeFlameworkClasses() error = %v", err)
	}
	files := analysis.Files()
	if len(files) != 1 || len(files[0].Classes) != 1 {
		t.Fatalf("discovered files = %#v, want one class", files)
	}
	class := analysis.Classes()[0]
	if class.InternalID != "fixture-game:out/service@Consumer" {
		t.Fatalf("internal ID = %q", class.InternalID)
	}
	if got := strings.Join(class.DecoratorIDs, ","); got != "fixture-game:out/service@Service" {
		t.Fatalf("decorator IDs = %q", got)
	}
	if got := strings.Join(class.ConstructorTypeIDs, ","); got != "service@Dependency" {
		t.Fatalf("constructor type IDs = %q", got)
	}
}

func TestDiscoverFlameworkClasses_usesPackageBuildFallback(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-package","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}`)
	writeTransformFixture(t, directory, "src/service.ts", `export class Service {}`)
	input := newClassAnalysisInput(t, directory, "src/service.ts")
	if err := input.Project.AddClass(BuildClass{
		FilePath: "out/service.lua", InternalID: "fixture-package:out/service@Service",
		Decorators: []BuildDecorator{{Name: "Service", InternalID: "@flamework/core:out/services@Service"}},
	}); err != nil {
		t.Fatalf("AddClass() error = %v", err)
	}

	// When
	analysis, err := AnalyzeFlameworkClasses(input)
	// Then
	if err != nil {
		t.Fatalf("AnalyzeFlameworkClasses() error = %v", err)
	}
	files := analysis.Files()
	if len(files) != 1 || len(files[0].Classes) != 1 || len(files[0].Classes[0].Decorators) != 1 {
		t.Fatalf("fallback analysis = %#v", files)
	}
	if files[0].Classes[0].containsLegacyDecorator {
		t.Fatalf("build fallback incorrectly marked source decorator provenance: %#v", files)
	}
}

func TestDiscoverFlameworkClasses_recordsPackageBuildClassOnce(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"@scope/fixture","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}`)
	writeTransformFixture(t, directory, "src/service.ts", `/** @metadata reflect */ export class Service {}`)
	input := newClassAnalysisInput(t, directory, "src/service.ts")

	// When
	first, firstErr := AnalyzeFlameworkClasses(input)
	second, secondErr := AnalyzeFlameworkClasses(input)

	// Then
	if firstErr != nil || secondErr != nil || len(first.Classes()) != 1 || len(second.Classes()) != 1 {
		t.Fatalf("first = %#v/%v second = %#v/%v", first, firstErr, second, secondErr)
	}
	if first.Files()[0].Classes[0].containsLegacyDecorator || second.Files()[0].Classes[0].containsLegacyDecorator {
		t.Fatal("reflect-only discovery incorrectly marked legacy decorator provenance")
	}
	classes := input.Project.BuildInfoSnapshot().Classes
	if len(classes) != 1 || classes[0].InternalID != "@scope/fixture:service@Service" {
		t.Fatalf("build classes = %#v", classes)
	}
}

func TestDiscoverFlameworkClasses_resolvesExternalPackageDecoratorAndTypeUIDs(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"experimentalDecorators":true,"rootDir":"src","outDir":"out","moduleResolution":"node"},"files":["src/service.ts"]}`)
	writeTransformFixture(t, directory, "node_modules/@scope/ext/package.json", `{"name":"@scope/ext","version":"1.0.0","types":"index.d.ts"}`)
	writeTransformFixture(t, directory, "node_modules/@scope/ext/flamework.build", `{"version":1,"flameworkVersion":"1.3.2","identifierPrefix":"ext","identifiers":{}}`)
	writeTransformFixture(t, directory, "node_modules/@scope/ext/index.d.ts", strings.Join([]string{
		`export type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };`,
		`/** @metadata reflect */ export declare const ExternalDecorator: () => FlameworkDecorator;`,
		`export declare class ExternalDependency {}`,
	}, "\n"))
	writeTransformFixture(t, directory, "src/service.ts", strings.Join([]string{
		`import { ExternalDecorator, ExternalDependency } from "@scope/ext";`,
		`@ExternalDecorator() export class Consumer { constructor(value: ExternalDependency) {} }`,
	}, "\n"))
	input := newClassAnalysisInput(t, directory, "src/service.ts")

	// When
	analysis, err := AnalyzeFlameworkClasses(input)
	// Then
	if err != nil {
		t.Fatalf("AnalyzeFlameworkClasses() error = %v", err)
	}
	if len(analysis.Classes()) != 1 {
		t.Fatalf("classes = %#v, want one", analysis.Classes())
	}
	class := analysis.Classes()[0]
	if got := strings.Join(class.DecoratorIDs, ","); got != "@scope/ext:index@ExternalDecorator" {
		t.Fatalf("external decorator IDs = %q", got)
	}
	if got := strings.Join(class.ConstructorTypeIDs, ","); got != "ext:index@ExternalDependency" {
		t.Fatalf("external constructor IDs = %q", got)
	}
}

func TestDiscoverFlameworkClasses_ignoresUnsupportedDecoratorAndMalformedMetadata(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"experimentalDecorators":true,"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}`)
	writeTransformFixture(t, directory, "src/service.ts", `/** @metadata {@link Missing constraint} */ declare const ns: { Decorator(): ClassDecorator }; @ns.Decorator() export class Ignored {}`)
	input := newClassAnalysisInput(t, directory, "src/service.ts")

	// When
	analysis, err := AnalyzeFlameworkClasses(input)

	// Then
	if err != nil || len(analysis.Files()) != 0 || len(analysis.Diagnostics()) != 0 {
		t.Fatalf("unsupported analysis = %#v, error = %v", analysis, err)
	}
}

func newClassAnalysisInput(t *testing.T, directory, fileName string) TransformInput {
	t.Helper()
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	t.Cleanup(release)
	file := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, fileName)))
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}
	return TransformInput{Program: program, Checker: checker, Files: []*ast.SourceFile{file}, Project: project}
}
