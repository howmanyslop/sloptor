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

func TestTransformFlameworkClass_emitsReflectionAndDecorators_whenClassIsPlanned(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"experimentalDecorators":true,"rootDir":"src","outDir":"out"},"include":["src/**/*.ts"]}`)
	writeTransformFixture(t, directory, "src/service.ts", strings.Join([]string{
		`type FlameworkDecorator = ClassDecorator & MethodDecorator & { _flamework_Decorator: "Class" };`,
		`/** @metadata flamework:implements flamework:parameters */`,
		`declare const First: (label: string) => FlameworkDecorator;`,
		`/** @metadata flamework:implements flamework:parameters */`,
		`declare const Second: () => FlameworkDecorator;`,
		`interface OnStart { onStart(): Logger; }`,
		`class Logger {}`,
		`@First("fixture")`,
		`@Second()`,
		`export class Consumer implements OnStart {`,
		`	constructor(logger: Logger) {}`,
		`	/** @metadata flamework:type */`,
		`	value: Logger;`,
		`	/** @metadata flamework:return_type */`,
		`	@First("member")`,
		`	onStart(): Logger { return new Logger(); }`,
		`}`,
		"",
	}, "\n"))
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	defer release()
	sourceFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/service.ts")))
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{HashPrefix: "fixture"}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}
	plans, err := project.Analyze([]FileAnalysis{{
		FileID: "src/service.ts",
		Classes: []ClassPlan{
			{InternalID: "fixture-game:out/service@Logger"},
			{InternalID: "fixture-game:out/service@Consumer", Decorators: []DecoratorPlan{
				{Name: "First", InternalID: "fixture-game:out/service@First"},
				{Name: "Second", InternalID: "fixture-game:out/service@Second"},
			}},
		},
	}})
	if err != nil {
		t.Fatalf("Analyze() error = %v", err)
	}
	state, err := newTransformState(TransformInput{Program: program, Checker: checker, Project: project, Files: []*ast.SourceFile{sourceFile}}, plans)
	if err != nil {
		t.Fatalf("newTransformState() error = %v", err)
	}
	var classNode *ast.Node
	for _, statement := range sourceFile.Statements.Nodes {
		if ast.IsClassDeclaration(statement) && statement.Name().Text() == "Consumer" {
			classNode = statement
			break
		}
	}
	if classNode == nil {
		t.Fatal("Consumer class not found")
	}

	// When
	transformed, err := transformFlameworkClass(state, plans[0], classNode)
	// Then
	if err != nil {
		t.Fatalf("transformFlameworkClass() error = %v", err)
	}
	statements := make([]*ast.Node, 0, len(sourceFile.Statements.Nodes)+2)
	for _, statement := range sourceFile.Statements.Nodes {
		if statement == classNode {
			statements = append(statements, transformed.AsSyntaxList().Children...)
			continue
		}
		statements = append(statements, statement)
	}
	transformedFile := state.factory.UpdateSourceFile(sourceFile, state.factory.NewNodeList(statements), sourceFile.EndOfFileToken).AsSourceFile()
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(transformedFile)
	wantFragments := []string{
		`export class Consumer implements OnStart`,
		`Reflect["defineMetadata"](Consumer, "identifier", "fixture:service@Consumer");`,
		`Reflect["defineMetadata"](Consumer, "flamework:parameters", ["fixture:service@Logger"]);`,
		`Reflect["defineMetadata"](Consumer, "flamework:implements", ["fixture:service@OnStart"]);`,
		`Reflect["defineMetadata"](Consumer, "flamework:type", "fixture:service@Logger", "value");`,
		`Reflect["defineMetadata"](Consumer, "flamework:return_type", "fixture:service@Logger", "onStart");`,
		`Reflect["decorate"](Consumer, "fixture:service@Second", Second, []);`,
		`Reflect["decorate"](Consumer, "fixture:service@First", First, ["fixture"]);`,
		`Reflect["decorate"](Consumer, "fixture:service@First", First, ["member"], "onStart", false);`,
	}
	for _, fragment := range wantFragments {
		if !strings.Contains(printed, fragment) {
			t.Fatalf("transformed class missing %q:\n%s", fragment, printed)
		}
	}
	if strings.Contains(printed, "@First(") || strings.Contains(printed, "@Second(") {
		t.Fatalf("Flamework decorators were not stripped:\n%s", printed)
	}
	reparsed := parser.ParseSourceFile(ast.SourceFileParseOptions{
		FileName: "/transformed.ts",
		Path:     tspath.Path("/transformed.ts"),
	}, printed, core.ScriptKindTS)
	if len(reparsed.Diagnostics()) != 0 {
		t.Fatalf("reparse diagnostics = %v", reparsed.Diagnostics())
	}
	t.Logf("transformed class:\n%s", printed)
}
