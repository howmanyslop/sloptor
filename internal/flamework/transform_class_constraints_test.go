package flamework

import (
	"errors"
	"strings"
	"testing"

	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/parser"
	"rotor/tsgo/tspath"
)

func TestConstraintDiagnostic_hasOrderedRelatedTrace(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"experimentalDecorators":true,"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}`)
	writeTransformFixture(t, directory, "src/service.ts", strings.Join([]string{
		`interface Required { required(): void }`,
		`/** @metadata reflect {@link Required constraint} */`,
		`interface Marked {}`,
		`export class Invalid implements Marked {}`,
	}, "\n"))
	input := newClassAnalysisInput(t, directory, "src/service.ts")

	// When
	analysis, err := AnalyzeFlameworkClasses(input)
	// Then
	if err != nil {
		t.Fatalf("AnalyzeFlameworkClasses() error = %v", err)
	}
	diagnostics := analysis.Diagnostics()
	if len(diagnostics) != 1 {
		t.Fatalf("diagnostics = %#v, want one", diagnostics)
	}
	if got := diagnostics[0].String(); got != "Type 'Invalid' does not satisfy the constraint 'Required'." {
		t.Fatalf("diagnostic = %q", got)
	}
	if related := diagnostics[0].RelatedInformation(); len(related) != 1 || related[0].String() != "'The constraint' is declared here." {
		t.Fatalf("related trace = %#v", related)
	}
}

func TestConstraintDiagnostics_preserveFilePositionOrder(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}`)
	writeTransformFixture(t, directory, "src/service.ts", strings.Join([]string{
		`interface Required { required(): void }`,
		`/** @metadata reflect {@link Required constraint} */ interface Marked {}`,
		`export class Zed implements Marked {}`,
		`export class Alpha implements Marked {}`,
	}, "\n"))
	input := newClassAnalysisInput(t, directory, "src/service.ts")

	// When
	analysis, err := AnalyzeFlameworkClasses(input)
	// Then
	if err != nil {
		t.Fatalf("AnalyzeFlameworkClasses() error = %v", err)
	}
	diagnostics := analysis.Diagnostics()
	if len(diagnostics) != 2 || !strings.Contains(diagnostics[0].String(), "Zed") || !strings.Contains(diagnostics[1].String(), "Alpha") {
		t.Fatalf("ordered diagnostics = %#v", diagnostics)
	}
}

func TestConstraintDiagnostic_captureAddsLocatedDecoratorError(t *testing.T) {
	// Given
	file := parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: "/service.ts", Path: tspath.Path("/service.ts")}, "class Service {}", core.ScriptKindTS)
	node := file.Statements.Nodes[0]
	state := &TransformState{}

	// When
	handled := addClassTransformDiagnostic(state, node, errors.Join(ErrInvalidDecorator, errors.New("bad signature")))

	// Then
	if !handled || len(state.diagnostics) != 1 {
		t.Fatalf("handled/diagnostics = %v/%#v", handled, state.diagnostics)
	}
	diagnostic := state.diagnostics[0]
	if diagnostic.File() != file || diagnostic.Pos() != node.Pos() || diagnostic.End() != node.End() || diagnostic.String() != "Decorators are not valid here." {
		t.Fatalf("located diagnostic = file:%v range:%d-%d text:%q", diagnostic.File(), diagnostic.Pos(), diagnostic.End(), diagnostic.String())
	}
}

func TestCircularDependency_rejectsConstructorGraphDeterministically(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}`)
	writeTransformFixture(t, directory, "src/service.ts", strings.Join([]string{
		`/** @metadata reflect */ export class A { constructor(value: B) {} }`,
		`/** @metadata reflect */ export class B { constructor(value: A) {} }`,
	}, "\n"))
	input := newClassAnalysisInput(t, directory, "src/service.ts")

	// When
	_, err := AnalyzeFlameworkClasses(input)

	// Then
	if err == nil || err.Error() != "circular Flamework constructor dependency: fixture-game:out/service@A -> fixture-game:out/service@B -> fixture-game:out/service@A" {
		t.Fatalf("AnalyzeFlameworkClasses() error = %v", err)
	}
}

func TestAnalyzeFlameworkClasses_inheritsMemberMetadataFromImplementedInterface(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}`)
	writeTransformFixture(t, directory, "src/service.ts", strings.Join([]string{
		`interface Contract {`,
		`  /** @metadata reflect */`,
		`  run(): void`,
		`}`,
		`class Implementation implements Contract {`,
		`  run(): void {}`,
		`}`,
	}, "\n"))
	input := newClassAnalysisInput(t, directory, "src/service.ts")
	state, err := newTransformState(input, nil)
	if err != nil {
		t.Fatalf("newTransformState() error = %v", err)
	}
	member := input.Files[0].Statements.Nodes[1].Members()[0]

	// When
	metadata := collectNodeMetadata(state, member)

	// Then
	if !metadata.Requested("reflect") {
		t.Fatal("interface member metadata did not request reflect")
	}
}

func TestAnalyzeFlameworkClasses_discoversMetadataAndRejectsConstraint(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"experimentalDecorators":true,"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}`)
	writeTransformFixture(t, directory, "src/service.ts", strings.Join([]string{
		`interface Required { required(): void }`,
		`type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };`,
		`/** @metadata reflect {@link Required constraint} */`,
		`declare const Service: () => FlameworkDecorator;`,
		`@Service() export class Invalid {}`,
	}, "\n"))
	input := newClassAnalysisInput(t, directory, "src/service.ts")

	// When
	analysis, err := AnalyzeFlameworkClasses(input)
	// Then
	if err != nil {
		t.Fatalf("AnalyzeFlameworkClasses() error = %v", err)
	}
	classes, diagnostics := analysis.Classes(), analysis.Diagnostics()
	if len(classes) != 1 || len(classes[0].DecoratorIDs) != 1 || len(diagnostics) != 1 {
		t.Fatalf("classes = %#v diagnostics = %#v", classes, diagnostics)
	}
	related := diagnostics[0].RelatedInformation()
	if len(related) != 1 || diagnostics[0].String() != "Type 'Invalid' does not satisfy the constraint 'Required'." {
		t.Fatalf("diagnostic = %q related = %#v", diagnostics[0].String(), related)
	}
	t.Logf("discovered class internal ID: %s", classes[0].InternalID)
	t.Logf("discovered decorator internal ID: %s", classes[0].DecoratorIDs[0])
	t.Logf("ordered diagnostic: %s", diagnostics[0].String())
	t.Logf("related constraint trace: %s at %d", related[0].String(), related[0].Pos())
	t.Logf("unexpected diagnostics: %d", len(diagnostics)-1)
}
