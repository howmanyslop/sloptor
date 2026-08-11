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

func TestTransform_emitsConstructorReflection_whenProgramHasImportedInterface(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"experimentalDecorators":true,"strict":true,"rootDir":"src","outDir":"out"},"files":["src/core.ts","src/service.ts"]}`)
	writeTransformFixture(t, directory, "src/core.ts", strings.Join([]string{
		`export type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };`,
		`export declare const Service: () => FlameworkDecorator;`,
		`/** @metadata flamework:parameters */`,
		`export interface OnStart { onStart(): void; }`,
	}, "\n"))
	writeTransformFixture(t, directory, "src/service.ts", strings.Join([]string{
		`import { OnStart, Service } from "./core";`,
		`declare function print(value: unknown): void;`,
		`@Service() export class LoggerService {}`,
		`@Service() export class ConsumerService implements OnStart {`,
		`  constructor(private readonly logger: LoggerService) {}`,
		`  onStart(): void { print(this.logger); }`,
		`}`,
	}, "\n"))
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	t.Cleanup(release)
	service := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/service.ts")))
	for _, statement := range service.Statements.Nodes {
		if ast.IsClassDeclaration(statement) && statement.Name().Text() == "ConsumerService" {
			checker.GetTypeAtLocation(statement)
			if statement.Symbol() == nil {
				t.Fatal("ConsumerService was not bound in the original Program")
			}
		}
	}
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
		Files:   []*ast.SourceFile{service},
		Project: project,
	})
	// Then
	if err != nil {
		t.Fatalf("Transform() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(result.Files[0])
	if !strings.Contains(printed, `Reflect["defineMetadata"](ConsumerService, "flamework:parameters", ["fixture:service@LoggerService"]);`) {
		t.Fatalf("constructor reflection missing from transformed source:\n%s", printed)
	}
}
