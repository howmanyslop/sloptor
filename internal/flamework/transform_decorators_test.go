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

func TestDecoratorLowering_emitsExactAccessorStaticComponentAndImportOutput(t *testing.T) {
	// Given
	source := strings.Join([]string{
		`// @rbxts-transform-debug`,
		`import { Reflect as RuntimeReflect } from "@flamework/core";`,
		`type FW = ClassDecorator & MethodDecorator & PropertyDecorator & { _flamework_Decorator: "Class" };`,
		`namespace Decorators {`,
		` /** @metadata intrinsic-component-decorator */`,
		` export declare const Component: (config?: object) => FW;`,
		` export declare const Member: () => FW;`,
		`}`,
		`class Base<A> {`,
		` /** @metadata intrinsic-component-attributes */`,
		` readonly attributes!: A`,
		`}`,
		`interface Attributes { active: boolean }`,
		`@Decorators.Component({})`,
		`class Counter extends Base<Attributes> {`,
		` @Decorators.Member() static count = 0;`,
		` @Decorators.Member() get value(): number { return 1 }`,
		` @Decorators.Member() set value(next: number) { void next }`,
		`}`,
		"",
	}, "\n")
	state, file, plan := newDecoratorTransformFixture(t, source, ClassPlan{
		InternalID: "fixture:out/service@Counter",
		Decorators: []DecoratorPlan{
			{Name: "Component", InternalID: "fixture:out/service@Component"},
			{Name: "Member", InternalID: "fixture:out/service@Member"},
		},
	})
	classNode := findNamedClass(t, file.AsNode(), "Counter")

	// When
	transformed, err := transformFlameworkClass(state, plan, classNode)
	// Then
	if err != nil {
		t.Fatalf("transformFlameworkClass() error = %v", err)
	}
	statements := replaceClassSyntaxList(file, classNode, transformed)
	outputFile := state.factory.UpdateSourceFile(file, state.factory.NewNodeList(statements), file.EndOfFileToken).AsSourceFile()
	outputFile = prependFlameworkReflectImport(state.factory, outputFile)
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(outputFile)
	wants := []string{
		`// @rbxts-transform-debug`,
		`import { Reflect as RuntimeReflect } from "@flamework/core";`,
		`RuntimeReflect["decorate"](Counter, "fixture:service@Component", Decorators.Component, [{`,
		`"attributes": {`,
		`"active": t["boolean"]`,
		`Decorators.Member, [], "count", true`,
		`Decorators.Member, [], "value", false`,
	}
	for _, want := range wants {
		if !strings.Contains(printed, want) {
			t.Fatalf("transformed output missing %q:\n%s", want, printed)
		}
	}
	if strings.Count(printed, `from "@flamework/core"`) != 1 || strings.Contains(printed, "@Decorators") {
		t.Fatalf("decorator import/stripping mismatch:\n%s", printed)
	}
	reparsed := parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: "/output.ts", Path: tspath.Path("/output.ts")}, printed, core.ScriptKindTS)
	if len(reparsed.Diagnostics()) != 0 {
		t.Fatalf("reparse diagnostics = %v", reparsed.Diagnostics())
	}
	t.Logf("exact transformed TypeScript:\n%s", printed)
}

func TestDecoratorLowering_reportsInvalidComponentConfigSignature(t *testing.T) {
	// Given
	state, file, plan := newDecoratorTransformFixture(t, strings.Join([]string{
		`type FW = ClassDecorator & { _flamework_Decorator: "Class" };`,
		`/** @metadata intrinsic-component-decorator */`,
		`declare const Component: (config: unknown) => FW;`,
		`@Component("invalid") class Counter {}`,
	}, "\n"), ClassPlan{InternalID: "fixture:out/service@Counter", Decorators: []DecoratorPlan{{Name: "Component", InternalID: "fixture:out/service@Component"}}})

	// When
	classNode := findNamedClass(t, file.AsNode(), "Counter")
	transformed, err := transformFlameworkClass(state, plan, classNode)
	// Then
	if err != nil || transformed != classNode || len(state.orderedDiagnostics()) != 1 {
		t.Fatalf("invalid signature result = (%p, %v, %d diagnostics), want original, nil, 1", transformed, err, len(state.orderedDiagnostics()))
	}
}

func TestDecoratorLowering_preservesOriginalClassAndAddsLocatedDiagnostic_whenConfigInvalid(t *testing.T) {
	// Given
	state, file, plan := newDecoratorTransformFixture(t, strings.Join([]string{
		`type FW = ClassDecorator & { _flamework_Decorator: "Class" };`,
		`/** @metadata intrinsic-component-decorator */`,
		`declare const Component: (config: unknown) => FW;`,
		`@Component("invalid") class Counter {}`,
	}, "\n"), ClassPlan{InternalID: "fixture:out/service@Counter", Decorators: []DecoratorPlan{{Name: "Component", InternalID: "fixture:out/service@Component"}}})
	classNode := findNamedClass(t, file.AsNode(), "Counter")

	// When
	transformed, err := transformFlameworkClass(state, plan, classNode)
	// Then
	if err != nil {
		t.Fatalf("transformFlameworkClass() error = %v", err)
	}
	if transformed != classNode {
		t.Fatalf("transformFlameworkClass() fallback = %p, want original %p", transformed, classNode)
	}
	diagnostics := state.orderedDiagnostics()
	if len(diagnostics) != 1 || diagnostics[0].File() != file || diagnostics[0].Pos() < classNode.Pos() || diagnostics[0].End() > classNode.End() {
		t.Fatalf("located diagnostics = %#v, want one diagnostic within original class", diagnostics)
	}
	if diagnostics[0].String() != "Decorators are not valid here." {
		t.Fatalf("diagnostic = %q", diagnostics[0].String())
	}
}

func TestTransformFlameworkClass_omitsIdentifierMetadata_whenOnlyReflectIsRequested(t *testing.T) {
	// Given
	state, file, plan := newDecoratorTransformFixture(t, `/** @metadata reflect */ class Counter {}`, ClassPlan{InternalID: "fixture:out/service@Counter"})
	classNode := findNamedClass(t, file.AsNode(), "Counter")

	// When
	transformed, err := transformFlameworkClass(state, plan, classNode)
	// Then
	if err != nil {
		t.Fatalf("transformFlameworkClass() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(
		state.factory.UpdateSourceFile(file, state.factory.NewNodeList(replaceClassSyntaxList(file, classNode, transformed)), file.EndOfFileToken).AsSourceFile(),
	)
	if strings.Contains(printed, `"identifier"`) {
		t.Fatalf("reflection-only identifier parity mismatch:\n%s", printed)
	}
}

func TestTransform_omitsIdentifierMetadataForDiscoveredReflectOnlyClass(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}`)
	writeTransformFixture(t, directory, "src/service.ts", `/** @metadata reflect */ export class Counter {}`)
	input := newClassAnalysisInput(t, directory, "src/service.ts")

	// When
	result, err := Transform(input)
	// Then
	if err != nil {
		t.Fatalf("Transform() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(result.Files[0])
	if strings.Contains(printed, `"identifier"`) {
		t.Fatalf("discovered reflect-only class emitted identifier metadata:\n%s", printed)
	}
}

func TestDecoratorLowering_preservesOriginalClassAndReportsInvalidPlacement(t *testing.T) {
	// Given
	state, file, plan := newDecoratorTransformFixture(t, strings.Join([]string{
		`type FW = PropertyDecorator & MethodDecorator & { _flamework_Decorator: "Class" };`,
		`/** @metadata intrinsic-component-decorator */`,
		`declare const Component: (config?: object) => FW;`,
		`class Counter {`,
		` @Component({}) first = 1;`,
		` @Component({}) second() {}`,
		`}`,
	}, "\n"), ClassPlan{InternalID: "fixture:out/service@Counter", Decorators: []DecoratorPlan{{Name: "Component", InternalID: "fixture:out/service@Component"}}})

	// When
	classNode := findNamedClass(t, file.AsNode(), "Counter")
	transformed, err := transformFlameworkClass(state, plan, classNode)
	// Then
	diagnostics := state.orderedDiagnostics()
	if err != nil || transformed != classNode || len(diagnostics) != 1 {
		t.Fatalf("invalid placement result = (%p, %v, %d diagnostics), want original, nil, 1", transformed, err, len(diagnostics))
	}
	if diagnostics[0].File() != file || diagnostics[0].Pos() != classNode.Pos() || diagnostics[0].End() != classNode.End() || diagnostics[0].String() != "Decorators are not valid here." {
		t.Fatalf("invalid placement diagnostic = %#v, want located class diagnostic", diagnostics[0])
	}
}

func TestDecoratorLowering_preservesParameterDecoratorsBecauseUpstreamDoesNotEmitThem(t *testing.T) {
	// Given
	state, file, plan := newDecoratorTransformFixture(t, strings.Join([]string{
		`type FW = ParameterDecorator & { _flamework_Decorator: "Class" };`,
		`declare const Parameter: () => FW;`,
		`class Counter { method(@Parameter() value: string) { void value } }`,
	}, "\n"), ClassPlan{InternalID: "fixture:out/service@Counter", Decorators: []DecoratorPlan{{Name: "Parameter", InternalID: "fixture:out/service@Parameter"}}})
	classNode := findNamedClass(t, file.AsNode(), "Counter")

	// When
	transformed, err := transformFlameworkClass(state, plan, classNode)
	// Then
	if err != nil {
		t.Fatalf("transformFlameworkClass() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(
		state.factory.UpdateSourceFile(file, state.factory.NewNodeList(replaceClassSyntaxList(file, classNode, transformed)), file.EndOfFileToken).AsSourceFile(),
	)
	if !strings.Contains(printed, `@Parameter()`) || strings.Contains(printed, `"fixture:service@Parameter"`) {
		t.Fatalf("parameter decorator parity mismatch:\n%s", printed)
	}
}

func TestDecoratorLowering_emitsMacroPrerequisitesBeforeClassAndDecoratorCall(t *testing.T) {
	// Given
	state, file, plan := newDecoratorTransformFixture(t, strings.Join([]string{
		`type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };`,
		`interface Leaf { id: string; value: number }`,
		`interface Repeated { first: Leaf; second: Leaf; third: Leaf }`,
		`declare function guardMacro(value?: Generic<Repeated, "guard">): unknown;`,
		`type FW = ClassDecorator & { _flamework_Decorator: "Class" };`,
		`declare const Decorate: (guard: unknown) => FW;`,
		`@Decorate(guardMacro()) class Counter {}`,
	}, "\n"), ClassPlan{InternalID: "fixture:out/service@Counter", Decorators: []DecoratorPlan{{Name: "Decorate", InternalID: "fixture:out/service@Decorate"}}})
	limit := 2
	state.project.config.Optimizations.GuardGenerationDedupLimit = &limit
	classNode := findNamedClass(t, file.AsNode(), "Counter")

	// When
	transformed, err := transformFlameworkClass(state, plan, classNode)
	// Then
	if err != nil {
		t.Fatalf("transformFlameworkClass() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(
		state.factory.UpdateSourceFile(file, state.factory.NewNodeList(replaceClassSyntaxList(file, classNode, transformed)), file.EndOfFileToken).AsSourceFile(),
	)
	importStatement := `import { t } from "@flamework/core/out/prelude";`
	importIndex := strings.Index(printed, importStatement)
	prerequisite := strings.Index(printed, `const dedup =`)
	classDeclaration := strings.Index(printed, `class Counter`)
	decoratorCall := strings.Index(printed, `Reflect["decorate"]`)
	if importIndex < 0 || prerequisite <= importIndex || classDeclaration <= prerequisite || decoratorCall <= classDeclaration {
		t.Fatalf("decorator prerequisite/class/call order mismatch:\n%s", printed)
	}
	if strings.Count(printed, importStatement) != 1 {
		t.Fatalf("decorator macro runtime import was not deduplicated:\n%s", printed)
	}
	if !strings.Contains(printed, `guardMacro(t["interface"]({ "first": dedup, "second": dedup, "third": dedup }) as never)`) {
		t.Fatalf("decorator macro argument was not transformed:\n%s", printed)
	}
}

func newDecoratorTransformFixture(t *testing.T, source string, classPlan ClassPlan) (*TransformState, *ast.SourceFile, FilePlan) {
	t.Helper()
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"experimentalDecorators":true,"rootDir":"src","outDir":"out"},"include":["src/**/*.ts"]}`)
	writeTransformFixture(t, directory, "src/service.ts", source)
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	t.Cleanup(release)
	file := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/service.ts")))
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{HashPrefix: "fixture"}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}
	plans, err := project.Analyze([]FileAnalysis{{FileID: "src/service.ts", Classes: []ClassPlan{classPlan}}})
	if err != nil {
		t.Fatalf("Analyze() error = %v", err)
	}
	state, err := newTransformState(TransformInput{Program: program, Checker: checker, Project: project, Files: []*ast.SourceFile{file}}, plans)
	if err != nil {
		t.Fatalf("newTransformState() error = %v", err)
	}
	return state, file, plans[0]
}

func replaceClassSyntaxList(file *ast.SourceFile, classNode, transformed *ast.Node) []*ast.Node {
	statements := make([]*ast.Node, 0, len(file.Statements.Nodes)+len(transformed.AsSyntaxList().Children))
	for _, statement := range file.Statements.Nodes {
		if statement == classNode {
			statements = append(statements, transformed.AsSyntaxList().Children...)
		} else {
			statements = append(statements, statement)
		}
	}
	return statements
}
