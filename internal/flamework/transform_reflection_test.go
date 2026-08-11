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

func TestMemberReflection_preservesExistingOrderedMetadata(t *testing.T) {
	// Given
	printed := transformReflectionFixture(t, strings.Join([]string{
		`type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };`,
		`/** @metadata flamework:implements flamework:parameters */`,
		`declare const Service: () => FlameworkDecorator;`,
		`interface Lifecycle { start(): Dependency; }`,
		`class Dependency {}`,
		`@Service()`,
		`export class Subject implements Lifecycle {`,
		`  constructor(dependency: Dependency) {}`,
		`  /** @metadata flamework:type */`,
		`  field: Dependency;`,
		`  /** @metadata flamework:return_type */`,
		`  start(): Dependency { return new Dependency(); }`,
		`}`,
	}, "\n"))

	// When
	start := strings.Index(printed, `Reflect["defineMetadata"]`)
	end := strings.Index(printed[start:], "\n    }")
	if start < 0 || end < 0 {
		t.Fatalf("reflection block not found:\n%s", printed)
	}
	reflection := printed[start : start+end]

	// Then
	want := strings.Join([]string{
		`Reflect["defineMetadata"](Subject, "identifier", "fixture:subject@Subject");`,
		`        Reflect["defineMetadata"](Subject, "flamework:parameters", ["fixture:subject@Dependency"]);`,
		`        Reflect["defineMetadata"](Subject, "flamework:implements", ["fixture:subject@Lifecycle"]);`,
		`        Reflect["defineMetadata"](Subject, "flamework:type", "fixture:subject@Dependency", "field");`,
		`        Reflect["defineMetadata"](Subject, "flamework:return_type", "fixture:subject@Dependency", "start");`,
	}, "\n")
	if reflection != want {
		t.Fatalf("reflection metadata =\n%s\nwant:\n%s", reflection, want)
	}
	t.Logf("ordered reflection metadata:\n%s", reflection)
}

func TestMemberReflection_emitsExactMethodAndParameterMetadata(t *testing.T) {
	// Given
	printed := transformReflectionFixture(t, strings.Join([]string{
		`type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };`,
		`declare const Service: () => FlameworkDecorator;`,
		`interface Options { count: number; }`,
		`interface Reflected {`,
		`  /** @metadata flamework:return_type flamework:return_guard flamework:parameters flamework:parameter_names flamework:parameter_guards */`,
		`  execute(value: string, binding: Options): { ok: boolean };`,
		`}`,
		`@Service()`,
		`export class Subject implements Reflected {`,
		`  execute(value: string, { count }: Options) { return { ok: count > value.length }; }`,
		`}`,
	}, "\n"))

	// When
	metadata := reflectionStatementsFor(t, printed, "execute")

	// Then
	want := strings.Join([]string{
		`Reflect["defineMetadata"](Subject, "flamework:return_type", "fixture:subject@Subject.execute", "execute");`,
		`Reflect["defineMetadata"](Subject, "flamework:return_guard", t["interface"]({ "ok": t["boolean"] }), "execute");`,
		`Reflect["defineMetadata"](Subject, "flamework:parameters", ["$p:string", "fixture:subject@Options"], "execute");`,
		`Reflect["defineMetadata"](Subject, "flamework:parameter_names", ["value", "_binding_"], "execute");`,
		`Reflect["defineMetadata"](Subject, "flamework:parameter_guards", [t["string"], t["interface"]({ "count": t["number"] })], "execute");`,
	}, "\n")
	if metadata != want {
		t.Fatalf("method metadata =\n%s\nwant:\n%s\ntransformed TypeScript:\n%s", metadata, want, printed)
	}
	t.Logf("transformed TypeScript:\n%s", printed)
}

func TestFieldReflection_emitsInferredTypeAndGuardRequestedByInterface(t *testing.T) {
	// Given
	printed := transformReflectionFixture(t, strings.Join([]string{
		`type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };`,
		`declare const Service: () => FlameworkDecorator;`,
		`interface Reflected {`,
		`  /** @metadata flamework:type flamework:guard */`,
		`  label: string;`,
		`}`,
		`@Service()`,
		`export class Subject implements Reflected { label = "ready"; }`,
	}, "\n"))

	// When
	metadata := reflectionStatementsFor(t, printed, "label")

	// Then
	want := strings.Join([]string{
		`Reflect["defineMetadata"](Subject, "flamework:type", "$p:string", "label");`,
		`Reflect["defineMetadata"](Subject, "flamework:guard", t["string"], "label");`,
	}, "\n")
	if metadata != want {
		t.Fatalf("field metadata =\n%s\nwant:\n%s\ntransformed TypeScript:\n%s", metadata, want, printed)
	}
}

func TestFieldReflection_emitsExplicitTypeAndGuard(t *testing.T) {
	// Given
	printed := transformReflectionFixture(t, strings.Join([]string{
		`type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };`,
		`declare const Service: () => FlameworkDecorator;`,
		`interface Options { count: number; }`,
		`@Service()`,
		`export class Subject {`,
		`  /** @metadata flamework:type flamework:guard */`,
		`  options: Options;`,
		`}`,
	}, "\n"))

	// When
	metadata := reflectionStatementsFor(t, printed, "options")

	// Then
	want := strings.Join([]string{
		`Reflect["defineMetadata"](Subject, "flamework:type", "fixture:subject@Options", "options");`,
		`Reflect["defineMetadata"](Subject, "flamework:guard", t["interface"]({ "count": t["number"] }), "options");`,
	}, "\n")
	if metadata != want {
		t.Fatalf("field metadata =\n%s\nwant:\n%s\ntransformed TypeScript:\n%s", metadata, want, printed)
	}
}

func TestMethodReflection_emitsConstructorParameterMetadata(t *testing.T) {
	// Given
	printed := transformReflectionFixture(t, strings.Join([]string{
		`type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };`,
		`declare const Service: () => FlameworkDecorator;`,
		`interface Dependency { ready: boolean; }`,
		`interface Contract {}`,
		`/** @metadata flamework:parameters flamework:parameter_names flamework:parameter_guards flamework:implements */`,
		`@Service()`,
		`export class Subject implements Contract { constructor(dependency: Dependency, { enabled }: { enabled: boolean }) {} }`,
	}, "\n"))

	// When
	metadata := printed

	// Then
	previous := -1
	for _, want := range []string{
		`Reflect["defineMetadata"](Subject, "identifier", "fixture:subject@Subject");`,
		`Reflect["defineMetadata"](Subject, "flamework:parameters", ["fixture:subject@Dependency", "fixture:subject@Subject"]);`,
		`Reflect["defineMetadata"](Subject, "flamework:parameter_names", ["dependency", "_binding_"]);`,
		`Reflect["defineMetadata"](Subject, "flamework:parameter_guards", [t["interface"]({ "ready": t["boolean"] }), t["interface"]({ "enabled": t["boolean"] })]);`,
		`Reflect["defineMetadata"](Subject, "flamework:implements", ["fixture:subject@Contract"]);`,
	} {
		position := strings.Index(metadata, want)
		if position < 0 {
			t.Fatalf("constructor metadata missing %q:\n%s\ntransformed TypeScript:\n%s", want, metadata, printed)
		}
		if position <= previous {
			t.Fatalf("constructor metadata out of order at %q:\n%s", want, metadata)
		}
		previous = position
	}
}

func reflectionStatementsFor(t *testing.T, printed, property string) string {
	t.Helper()
	lines := strings.Split(printed, "\n")
	statements := make([]string, 0)
	propertySuffix := `, "` + property + `");`
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if !strings.HasPrefix(line, `Reflect["defineMetadata"]`) {
			continue
		}
		if strings.HasSuffix(line, propertySuffix) {
			statements = append(statements, line)
		}
	}
	return strings.Join(statements, "\n")
}

func transformReflectionFixture(t *testing.T, source string) string {
	t.Helper()
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"experimentalDecorators":true,"strict":true,"rootDir":"src","outDir":"out"},"files":["src/subject.ts"]}`)
	writeTransformFixture(t, directory, "src/subject.ts", source)
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	t.Cleanup(release)
	sourceFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/subject.ts")))
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{HashPrefix: "fixture"}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}
	plans, err := project.Analyze([]FileAnalysis{{FileID: "src/subject.ts", Classes: []ClassPlan{
		{InternalID: "fixture:out/subject@Dependency"},
		{InternalID: "fixture:out/subject@Subject", Decorators: []DecoratorPlan{{Name: "Service", InternalID: "fixture:out/subject@Service"}}},
	}}})
	if err != nil {
		t.Fatalf("Analyze() error = %v", err)
	}
	state, err := newTransformState(TransformInput{Program: program, Checker: checker, Project: project, Files: []*ast.SourceFile{sourceFile}}, plans)
	if err != nil {
		t.Fatalf("newTransformState() error = %v", err)
	}
	var classNode *ast.Node
	for _, statement := range sourceFile.Statements.Nodes {
		if ast.IsClassDeclaration(statement) && statement.Name().Text() == "Subject" {
			classNode = statement
			break
		}
	}
	if classNode == nil {
		t.Fatal("Subject class not found")
	}
	transformed, err := transformFlameworkClass(state, plans[0], classNode)
	if err != nil {
		t.Fatalf("transformFlameworkClass() error = %v", err)
	}
	statements := make([]*ast.Node, 0, len(sourceFile.Statements.Nodes)+2)
	for _, statement := range sourceFile.Statements.Nodes {
		if statement == classNode {
			statements = append(statements, transformed.AsSyntaxList().Children...)
		} else {
			statements = append(statements, statement)
		}
	}
	transformedFile := state.factory.UpdateSourceFile(sourceFile, state.factory.NewNodeList(statements), sourceFile.EndOfFileToken).AsSourceFile()
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(transformedFile)
	reparsed := parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: "/reflection.ts", Path: tspath.Path("/reflection.ts")}, printed, core.ScriptKindTS)
	if len(reparsed.Diagnostics()) != 0 {
		t.Fatalf("reparse diagnostics = %v", reparsed.Diagnostics())
	}
	return printed
}
