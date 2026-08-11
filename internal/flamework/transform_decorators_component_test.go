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

func TestTransform_rewritesImportedComponentConfig_whenAttributesAndInstanceAreTyped(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"experimentalDecorators":true,"strict":true,"rootDir":"src","outDir":"out","moduleResolution":"node"},"files":["src/globals.d.ts","src/component.ts"]}`)
	writeTransformFixture(t, directory, "src/globals.d.ts", `interface Instance { readonly _nominal_Instance: unique symbol; } interface Part extends Instance { readonly _nominal_Part: unique symbol; }`)
	writeTransformFixture(t, directory, "node_modules/@flamework/components/package.json", `{"name":"@flamework/components","version":"1.3.2","types":"out/index.d.ts"}`)
	writeTransformFixture(t, directory, "node_modules/@flamework/components/flamework.build", `{"version":1,"flameworkVersion":"1.3.2","identifierPrefix":"$c","identifiers":{}}`)
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
		`/** @metadata flamework:implements flamework:parameters injectable intrinsic-component-decorator */`,
		`export declare const Component: (opts?: ComponentConfig) => ClassDecorator & { _flamework_Decorator: never };`,
	}, "\n"))
	writeTransformFixture(t, directory, "node_modules/@flamework/core/package.json", `{"name":"@flamework/core","version":"1.3.2","types":"out/index.d.ts"}`)
	writeTransformFixture(t, directory, "node_modules/@flamework/core/flamework.build", `{"version":1,"flameworkVersion":"1.3.2","identifierPrefix":"$","identifiers":{}}`)
	writeTransformFixture(t, directory, "node_modules/@flamework/core/out/index.d.ts", `export { Service } from "./flamework";`)
	writeTransformFixture(t, directory, "node_modules/@flamework/core/out/flamework.d.ts", `export declare const Service: () => ClassDecorator & { _flamework_Decorator: never };`)
	writeTransformFixture(t, directory, "src/component.ts", strings.Join([]string{
		`import { BaseComponent, Component } from "@flamework/components";`,
		`import { Service } from "@flamework/core";`,
		`interface Attributes { active: boolean; label: string; retries: number; }`,
		`@Service()`,
		`@Component({ tag: "Fixture" })`,
		`export class FixtureComponent extends BaseComponent<Attributes, Part> {}`,
	}, "\n"))
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	t.Cleanup(release)
	source := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/component.ts")))
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{HashPrefix: "fixture"}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}

	// When
	result, err := Transform(TransformInput{Program: program, Checker: checker, Files: []*ast.SourceFile{source}, Project: project})
	// Then
	if err != nil {
		t.Fatalf("Transform() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, result.Sources[0].EmitContext()).EmitSourceFile(result.Files[0])
	wants := []string{
		`"attributes": {`, `"active": t["boolean"]`, `"label": t["string"]`, `"retries": t["number"]`,
		`"instanceGuard": t["instanceIsA"]("Part")`, `"$c:components@Component"`, `"$:flamework@Service"`,
		`// (Flamework) FixtureComponent metadata`, `// (Flamework) FixtureComponent decorators`,
	}
	for _, want := range wants {
		if !strings.Contains(printed, want) {
			t.Fatalf("component output missing %q:\n%s", want, printed)
		}
	}
}
