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

func TestUpdateFlameworkComponentConfig_generatesImportedComponentPayloadAndPreludeImport(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"experimentalDecorators":true,"strict":true,"rootDir":"src","outDir":"out","moduleResolution":"node"},"files":["src/globals.d.ts","src/component.ts"]}`)
	writeTransformFixture(t, directory, "node_modules/@flamework/components/package.json", `{"name":"@flamework/components","version":"1.3.2","types":"out/index.d.ts"}`)
	writeTransformFixture(t, directory, "node_modules/@flamework/components/out/index.d.ts", `export { BaseComponent } from "./baseComponent"; export { Component } from "./components";`)
	writeTransformFixture(t, directory, "node_modules/@flamework/components/out/baseComponent.d.ts", strings.Join([]string{
		`export declare class BaseComponent<A = {}, I extends Instance = Instance> {`,
		`  /** @metadata intrinsic-component-attributes */`,
		`  attributes: A;`,
		`  /** @metadata intrinsic-component-instance */`,
		`  instance: I;`,
		`}`,
	}, "\n"))
	writeTransformFixture(t, directory, "node_modules/@flamework/components/out/components.d.ts", strings.Join([]string{
		`export interface ComponentConfig { tag?: string; attributes?: object; instanceGuard?: unknown; }`,
		`/** @metadata intrinsic-component-decorator */`,
		`export declare const Component: (opts?: ComponentConfig) => ClassDecorator;`,
	}, "\n"))
	writeTransformFixture(t, directory, "src/globals.d.ts", strings.Join([]string{
		`interface Instance { readonly _nominal_Instance: unique symbol; }`,
		`interface Part extends Instance { readonly _nominal_Part: unique symbol; }`,
	}, "\n"))
	writeTransformFixture(t, directory, "src/component.ts", strings.Join([]string{
		`import { BaseComponent as ImportedBaseComponent, Component as ImportedComponent } from "@flamework/components";`,
		`interface Attributes { active: boolean; label: string; retries: number; }`,
		`declare const customGuard: unknown;`,
		`@ImportedComponent({ tag: "Fixture", attributes: { active: customGuard } })`,
		`export class FixtureComponent extends ImportedBaseComponent<Attributes, Part> {}`,
		`export class DefaultInstanceComponent extends ImportedBaseComponent<Attributes> {}`,
	}, "\n"))
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	t.Cleanup(release)
	sourceFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/component.ts")))
	if sourceFile == nil {
		t.Fatal("component source file not found")
	}
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}
	state, err := newTransformState(TransformInput{Program: program, Checker: checker, Files: []*ast.SourceFile{sourceFile}, Project: project}, nil)
	if err != nil {
		t.Fatalf("newTransformState() error = %v", err)
	}
	classDeclaration := findNamedClass(t, sourceFile.AsNode(), "FixtureComponent")
	config := findExpressionKind(t, sourceFile.AsNode(), ast.KindObjectLiteralExpression).AsObjectLiteralExpression()

	// When
	properties, err := updateFlameworkComponentConfig(state, classDeclaration, config.Properties.Nodes)
	object := state.factory.NewObjectLiteralExpression(state.factory.NewNodeList(properties), true)
	imports := decoratorRuntimeImports([]*ast.Node{object}, nil, nil)
	defaultProperties, defaultErr := updateFlameworkComponentConfig(state, findNamedClass(t, sourceFile.AsNode(), "DefaultInstanceComponent"), nil)

	// Then
	if err != nil {
		t.Fatalf("updateFlameworkComponentConfig() error = %v", err)
	}
	if defaultErr != nil {
		t.Fatalf("updateFlameworkComponentConfig() default instance error = %v", defaultErr)
	}
	if len(defaultProperties) != 1 || defaultProperties[0].Name().Text() != "attributes" {
		t.Fatalf("default instance component config = %#v, want only attributes guards", defaultProperties)
	}
	wantImport := MacroImport{Module: flameworkPreludeModule, Export: "t", Local: "t"}
	if len(imports) != 1 || imports[0] != wantImport {
		t.Fatalf("macro imports = %#v, want []MacroImport{%#v}", imports, wantImport)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(object, sourceFile)
	wants := []string{`tag: "Fixture"`, `active: customGuard`, `"label": t["string"]`, `"retries": t["number"]`, `"instanceGuard": t["instanceIsA"]("Part")`}
	for _, want := range wants {
		if !strings.Contains(printed, want) {
			t.Fatalf("component config missing %q:\n%s", want, printed)
		}
	}
	if strings.Contains(printed, `"active": t["boolean"]`) {
		t.Fatalf("component config overwrote custom attribute guard:\n%s", printed)
	}
	imported := macroImportStatements(state.factory, imports)
	configSource := strings.TrimSuffix(printed, ";")
	reparsed := parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: "/component-config.ts", Path: tspath.Path("/component-config.ts")}, printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(imported[0], sourceFile)+"\nconst config = "+configSource+";\n", core.ScriptKindTS)
	if len(reparsed.Diagnostics()) != 0 {
		t.Fatalf("component config reparse diagnostics = %v", reparsed.Diagnostics())
	}
	t.Logf("component config TypeScript:\n%s", printed)
	t.Logf("macro import: %#v", imports[0])
	t.Logf("component config reparse diagnostics: %d", len(reparsed.Diagnostics()))
}

func TestUpdateFlameworkComponentConfig_mergesGeneratedAttributeGuards_withCustomOverrides(t *testing.T) {
	// Given
	state, sourceFile := newExpressionTransformFixture(t, `
class ComponentBase<A> {
	/** @metadata intrinsic-component-attributes */
	readonly attributes!: A;
}
interface Attributes { active: boolean; label: string; retries: number }
class Counter extends ComponentBase<Attributes> {}
declare const customGuard: unknown;
const config = { attributes: { active: customGuard } };
`)
	classDeclaration := findNamedClass(t, sourceFile.AsNode(), "Counter")
	config := findExpressionKind(t, sourceFile.AsNode(), ast.KindObjectLiteralExpression).AsObjectLiteralExpression()

	// When
	properties, err := updateFlameworkComponentConfig(state, classDeclaration, config.Properties.Nodes)
	// Then
	if err != nil {
		t.Fatalf("updateFlameworkComponentConfig() error = %v", err)
	}
	object := state.factory.NewObjectLiteralExpression(state.factory.NewNodeList(properties), true)
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(object, sourceFile)
	wants := []string{`active: customGuard`, `"label": t["string"]`, `"retries": t["number"]`}
	for _, want := range wants {
		if !strings.Contains(printed, want) {
			t.Fatalf("component config missing %q: %s", want, printed)
		}
	}
	if strings.Contains(printed, `"active": t["boolean"]`) {
		t.Fatalf("component config overwrote custom active guard: %s", printed)
	}
	t.Logf("component config TypeScript: %s", printed)
}

func TestUpdateFlameworkComponentConfig_generatesInstanceGuard_whenSubclassNarrowsInstance(t *testing.T) {
	// Given
	state, sourceFile := newExpressionTransformFixture(t, `
declare class Instance { private _nominal_Instance: void }
declare class Part extends Instance { private _nominal_Part: void }
class ComponentBase { readonly instance!: Instance }
class Counter extends ComponentBase {
	/** @metadata intrinsic-component-instance */
	declare readonly instance: Part;
}
`)
	classDeclaration := findNamedClass(t, sourceFile.AsNode(), "Counter")

	// When
	properties, err := updateFlameworkComponentConfig(state, classDeclaration, nil)
	// Then
	if err != nil {
		t.Fatalf("updateFlameworkComponentConfig() error = %v", err)
	}
	object := state.factory.NewObjectLiteralExpression(state.factory.NewNodeList(properties), true)
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(object, sourceFile)
	want := `"instanceGuard": t["instanceIsA"]("Part")`
	if !strings.Contains(printed, want) {
		t.Fatalf("component config = %s, want %q", printed, want)
	}
	t.Logf("component config TypeScript: %s", printed)
}
