package flamework

import (
	"context"
	"path/filepath"
	"strings"
	"testing"

	"rotor/internal/config"
	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/parser"
	"rotor/tsgo/printer"
	"rotor/tsgo/tspath"
)

func TestTransform_discoversAndTransformsClassesAndMacros_whenAnalysesAreOmitted(t *testing.T) {
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
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	t.Cleanup(release)
	sourceFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/service.ts")))
	project, err := OpenProject(ProjectOptions{
		ProjectDir: directory,
		RootDir:    "src",
		OutDir:     "out",
		Config:     config.FlameworkConfig{HashPrefix: "fixture"},
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
	})
	// Then
	if err != nil {
		t.Fatalf("Transform() error = %v", err)
	}
	if len(result.Files) != 1 || len(result.Plans) != 1 || len(result.Plans[0].Classes()) != 1 {
		t.Fatalf("Transform() files/plans = %d/%#v, want one discovered class", len(result.Files), result.Plans)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(result.Files[0])
	for _, want := range []string{
		`import { Reflect as Reflect } from "@flamework/core";`,
		`Reflect["defineMetadata"](Consumer, "identifier", "fixture:service@Consumer");`,
		`Reflect["decorate"](Consumer, "fixture:service@Service", Service, []);`,
		`export const text = textMacro("\"payload\"" as never);`,
	} {
		if !strings.Contains(printed, want) {
			t.Fatalf("transformed source missing %q:\n%s", want, printed)
		}
	}
	if strings.Contains(printed, "@Service()") {
		t.Fatalf("Flamework decorator was not stripped:\n%s", printed)
	}
	reparsed := parser.ParseSourceFile(ast.SourceFileParseOptions{
		FileName: "/transformed.ts",
		Path:     tspath.Path("/transformed.ts"),
	}, printed, core.ScriptKindTS)
	if len(reparsed.Diagnostics()) != 0 {
		t.Fatalf("reparse diagnostics = %v", reparsed.Diagnostics())
	}
	if len(result.Sources) != 1 || !result.Sources[0].Changed() || result.Sources[0].Original() != sourceFile || result.Sources[0].Transformed() != result.Files[0] {
		t.Fatalf("source metadata = %#v, want exact original and transformed identities", result.Sources)
	}
	t.Logf("transformed TypeScript:\n%s", printed)
}

func TestTransform_resolvesNestedClassesByFullInternalID_whenSimpleNamesRepeat(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"experimentalDecorators":true,"rootDir":"src","outDir":"out"},"files":["src/nested.ts"]}`)
	writeTransformFixture(t, directory, "src/nested.ts", strings.Join([]string{
		`type FW = ClassDecorator & { _flamework_Decorator: "Class" };`,
		`declare const Service: () => FW;`,
		`namespace Left {`,
		` @Service() export class Same {}`,
		`}`,
		`namespace Right {`,
		` @Service() export class Same {}`,
		`}`,
	}, "\n"))
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	t.Cleanup(release)
	sourceFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/nested.ts")))
	project, err := OpenProject(ProjectOptions{
		ProjectDir: directory,
		RootDir:    "src",
		OutDir:     "out",
		Config:     config.FlameworkConfig{HashPrefix: "fixture"},
	})
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
	for _, want := range []string{
		`Reflect["defineMetadata"](Same, "identifier", "fixture:nested@Left.Same");`,
		`Reflect["defineMetadata"](Same, "identifier", "fixture:nested@Right.Same");`,
	} {
		if strings.Count(printed, want) != 1 {
			t.Fatalf("nested class output count for %q != 1:\n%s", want, printed)
		}
	}
	if len(result.Plans) != 1 || len(result.Plans[0].Classes()) != 2 {
		t.Fatalf("nested class plans = %#v, want two unique full IDs", result.Plans)
	}
}

func TestTransform_mergesExplicitCharacterizationWithoutDuplicatePlans(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}`)
	writeTransformFixture(t, directory, "src/service.ts", `/** @metadata reflect */ export class Service {}`)
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	t.Cleanup(release)
	sourceFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/service.ts")))
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{}})
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
			FileID:  "src/service.ts",
			Classes: []ClassPlan{{InternalID: "fixture-game:out/service@Service"}},
		}},
	})
	// Then
	if err != nil {
		t.Fatalf("Transform() error = %v", err)
	}
	if len(result.Plans) != 1 || len(result.Plans[0].Classes()) != 1 {
		t.Fatalf("merged plans = %#v, want one file and one class", result.Plans)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(result.Files[0])
	if !strings.Contains(printed, `Reflect["defineMetadata"](Service, "identifier", "service@Service");`) {
		t.Fatalf("explicit characterization identifier missing:\n%s", printed)
	}
}

func TestTransform_suppressesNativeOnlyDiscoveryDiagnostics(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}`)
	writeTransformFixture(t, directory, "src/service.ts", strings.Join([]string{
		`interface Required { required(): void }`,
		`/** @metadata reflect {@link Required constraint} */`,
		`interface Marked {}`,
		`export class Invalid implements Marked {}`,
	}, "\n"))
	input := newClassAnalysisInput(t, directory, "src/service.ts")

	// When
	result, err := Transform(input)
	// Then
	if err != nil {
		t.Fatalf("Transform() error = %v", err)
	}
	if len(result.Diagnostics) != 0 {
		t.Fatalf("Transform() diagnostics = %#v, want pinned upstream public-path parity", result.Diagnostics)
	}
}

func TestTransform_placesMacroImportsAndPrerequisitesBeforeOwningClass(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"experimentalDecorators":true,"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}`)
	writeTransformFixture(t, directory, "node_modules/@flamework/core/package.json", `{"name":"@flamework/core","version":"1.3.2"}`)
	writeTransformFixture(t, directory, "node_modules/@flamework/core/node_modules/@rbxts/t/package.json", `{"name":"@rbxts/t","version":"3.2.1"}`)
	writeTransformFixture(t, directory, "src/service.ts", strings.Join([]string{
		`type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };`,
		`interface Leaf { id: string; value: number }`,
		`interface Repeated { first: Leaf; second: Leaf; third: Leaf }`,
		`declare function guardMacro(value?: Generic<Repeated, "guard">): unknown;`,
		`type FW = ClassDecorator & { _flamework_Decorator: "Class" };`,
		`declare const Decorate: (guard: unknown) => FW;`,
		`@Decorate(guardMacro()) export class Counter {}`,
	}, "\n"))
	input := newClassAnalysisInput(t, directory, "src/service.ts")
	limit := 2
	input.Project.config.Optimizations.GuardGenerationDedupLimit = &limit

	// When
	result, err := Transform(input)
	// Then
	if err != nil {
		t.Fatalf("Transform() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(result.Files[0])
	importIndex := strings.Index(printed, `import { t as t } from "@flamework/core/out/prelude";`)
	prerequisiteIndex := strings.Index(printed, `const dedup =`)
	classIndex := strings.Index(printed, `export class Counter`)
	decoratorIndex := strings.Index(printed, `Reflect["decorate"]`)
	if importIndex < 0 || prerequisiteIndex <= importIndex || classIndex <= prerequisiteIndex || decoratorIndex <= classIndex {
		t.Fatalf("macro import/prerequisite/class/decorator order mismatch:\n%s", printed)
	}
	if strings.Count(printed, `from "@flamework/core/out/prelude"`) != 1 {
		t.Fatalf("macro import was not deduplicated:\n%s", printed)
	}
	t.Logf("ordered macro/decorator TypeScript:\n%s", printed)
}

func TestTransform_sharesGuardLibraryImportBetweenMacrosAndComponentDecorators_whenProjectOwnsRbxtsT(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"experimentalDecorators":true,"strict":true,"rootDir":"src","outDir":"out","moduleResolution":"node"},"files":["src/globals.d.ts","src/thing.ts"]}`)
	writeTransformFixture(t, directory, "node_modules/@rbxts/t/package.json", `{"name":"@rbxts/t","version":"3.2.1"}`)
	writeTransformFixture(t, directory, "node_modules/@flamework/core/package.json", `{"name":"@flamework/core","version":"1.3.2","types":"out/index.d.ts"}`)
	writeTransformFixture(t, directory, "node_modules/@flamework/core/out/index.d.ts", strings.Join([]string{
		`type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };`,
		`export declare namespace Flamework {`,
		`  /** @metadata macro */`,
		`  function createGuard<T>(meta?: Generic<T, "guard">): unknown;`,
		`}`,
	}, "\n"))
	writeTransformFixture(t, directory, "node_modules/@flamework/components/package.json", `{"name":"@flamework/components","version":"1.3.2","types":"out/index.d.ts"}`)
	writeTransformFixture(t, directory, "node_modules/@flamework/components/out/index.d.ts", strings.Join([]string{
		`export declare class BaseComponent<A = {}, I extends Instance = Instance> {`,
		`  /** @metadata intrinsic-component-attributes */`,
		`  attributes: A;`,
		`  /** @metadata intrinsic-component-instance */`,
		`  instance: I;`,
		`}`,
		`export interface ComponentConfig { tag?: string; attributes?: object; instanceGuard?: unknown; }`,
		`/** @metadata intrinsic-component-decorator */`,
		`export declare const Component: (opts?: ComponentConfig) => ClassDecorator & { _flamework_Decorator: "Class" };`,
	}, "\n"))
	writeTransformFixture(t, directory, "src/globals.d.ts", strings.Join([]string{
		`interface Instance { readonly _nominal_Instance: unique symbol; }`,
		`interface BasePart extends Instance { readonly _nominal_BasePart: unique symbol; }`,
	}, "\n"))
	writeTransformFixture(t, directory, "src/thing.ts", strings.Join([]string{
		`import { Flamework } from "@flamework/core";`,
		`import { BaseComponent, Component } from "@flamework/components";`,
		`interface Config { speed: number; }`,
		`export const isConfig = Flamework.createGuard<Config>();`,
		`interface Attributes { enabled: boolean; }`,
		`@Component({ tag: "Thing" })`,
		`export class Thing extends BaseComponent<Attributes, BasePart> {}`,
	}, "\n"))
	input := newClassAnalysisInput(t, directory, "src/thing.ts")

	// When
	result, err := Transform(input)
	// Then
	if err != nil {
		t.Fatalf("Transform() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(result.Files[0])
	if !strings.Contains(printed, `"enabled": t["boolean"]`) || !strings.Contains(printed, `"speed": t["number"]`) {
		t.Fatalf("guard macro and component attributes were not both transformed:\n%s", printed)
	}
	if count := strings.Count(printed, `from "@rbxts/t";`); count != 1 {
		t.Fatalf("@rbxts/t import count = %d, want 1:\n%s", count, printed)
	}
	if strings.Contains(printed, `from "@flamework/core/out/prelude"`) {
		t.Fatalf("component decorator imported t from the prelude instead of the resolved guard library:\n%s", printed)
	}
}
