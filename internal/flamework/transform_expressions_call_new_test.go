package flamework

import (
	"context"
	"errors"
	"path/filepath"
	"regexp"
	"strings"
	"testing"

	"rotor/internal/config"
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
	"rotor/tsgo/core"
	"rotor/tsgo/parser"
	"rotor/tsgo/printer"
	"rotor/tsgo/tspath"
)
func TestTransformFlameworkExpressionsInSourceFile_preservesCallChildRecursion(t *testing.T) {
	// Given
	state, sourceFile := newExpressionTransformFixture(t, `
import { BaseComponent } from "./base";
interface Attributes { count: number }
class Counter extends BaseComponent<Attributes> {}
declare function next(): number;
declare function consume(value: number): void;
const counter = new Counter();
consume(counter.attributes.count = next());
`)

	// When
	transformed, err := transformFlameworkExpressionsInSourceFile(state, sourceFile)
	// Then
	if err != nil {
		t.Fatalf("transformFlameworkExpressionsInSourceFile() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(transformed)
	if !strings.Contains(printed, `consume(counter[SYMBOL_ATTRIBUTE_SETTER]("count", next()));`) {
		t.Fatalf("transformed source did not preserve call child recursion:\n%s", printed)
	}
	t.Logf("transformed TypeScript:\n%s", printed)
}

func TestTransformSourceFile_preservesCheckerParents_whenSourceFileIsReused(t *testing.T) {
	// Given
	state, sourceFile := newExpressionTransformFixture(t, `
import { BaseComponent } from "./base";
interface Attributes { count: number }
class Counter extends BaseComponent<Attributes> {}
declare function next(): number;
declare function consume<T>(value: T): void;
const counter = new Counter();
class Host<T> {
	accept(value: T): void {}
	method(value: T): void {
		consume(counter.attributes.count = next());
		this.accept(value);
	}
}
`)
	var acceptCall *ast.Node
	for _, call := range collectCallExpressions(sourceFile) {
		if ast.IsPropertyAccessExpression(call.Expression()) && call.Expression().Name().Text() == "accept" {
			acceptCall = call
			break
		}
	}
	if acceptCall == nil {
		t.Fatal("accept call not found")
	}
	state.EmitContext().AddSyntheticLeadingComment(acceptCall.Parent, ast.KindSingleLineCommentTrivia, " (Flamework) Host metadata", true)
	firstTransformed, firstErr := transformSourceFile(state, sourceFile)
	if firstErr != nil {
		t.Fatalf("first transform error = %v", firstErr)
	}
	if firstTransformed == sourceFile {
		t.Fatal("first transform reused source file after a structural change")
	}
	if firstTransformed.AsNode().Flags&ast.NodeFlagsReparsed == 0 {
		t.Fatal("first transform did not DeepCloneReparse the changed source file")
	}
	var verifyParents func(*ast.Node)
	verifyParents = func(node *ast.Node) {
		node.ForEachChild(func(child *ast.Node) bool {
			if child.Parent != node {
				t.Fatalf("child %v parent = %p, want %p", child.Kind, child.Parent, node)
			}
			verifyParents(child)
			return false
		})
	}
	verifyParents(firstTransformed.AsNode())
	firstPrinted := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, state.EmitContext()).EmitSourceFile(firstTransformed)
	if !strings.Contains(firstPrinted, "// (Flamework) Host metadata") {
		t.Fatalf("first transform lost generated Flamework comment:\n%s", firstPrinted)
	}
	containingClass := ast.FindAncestor(acceptCall, ast.IsClassLike)
	if containingClass == nil {
		t.Fatal("accept call has no containing class")
	}
	if ast.NodeIsSynthesized(containingClass) || containingClass.Symbol() == nil {
		t.Fatalf("containing class = synthesized:%v symbol:%p, want bound parse-tree declaration", ast.NodeIsSynthesized(containingClass), containingClass.Symbol())
	}
	state.checker, _ = checker.NewChecker(state.program, nil)

	// When
	transformed, err := transformFlameworkExpressionsInSourceFile(state, sourceFile)
	// Then
	if err != nil {
		t.Fatalf("second transform error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(transformed)
	if !strings.Contains(printed, `consume(counter[SYMBOL_ATTRIBUTE_SETTER]("count", next()));`) {
		t.Fatalf("second transform did not preserve Flamework attribute transform:\n%s", printed)
	}
}

func TestTransformSourceFile_returnsOriginalSourceFile_whenNoStructuralChangeOccurs(t *testing.T) {
	// Given
	state, sourceFile := newExpressionTransformFixture(t, "export const stable = true;\n")
	if sourceNeedsFlameworkExpressionTransform(state, sourceFile) {
		t.Fatal("no-call fixture unexpectedly entered the expression walk")
	}

	// When
	transformed, err := transformSourceFile(state, sourceFile)
	// Then
	if err != nil {
		t.Fatalf("transformSourceFile() error = %v", err)
	}
	if transformed != sourceFile {
		t.Fatalf("transformSourceFile() = %p, want original source file %p", transformed, sourceFile)
	}
}

func TestCallMayBeFlameworkMacro_admitsTypeArgsKnownNamesAndSurface(t *testing.T) {
	// Given: ordinary calls, type-argument macros, and same-file Flamework surface.
	ordinary, ordinaryFile := newExpressionTransformFixture(t, `
export function add(left: number, right: number) { return left + right; }
export const value = add(1, 2);
`)
	typed, typedFile := newExpressionTransformFixture(t, `
declare function inspect<T>(value?: T): T;
inspect<string>();
`)
	surface, surfaceFile := newExpressionTransformFixture(t, `
type DeclarationUid = { readonly _flamework_intrinsic: ["declaration-uid"] };
/** @metadata macro */
declare function declarationUid(value?: DeclarationUid): unknown;
declarationUid();
`)
	middleware, middlewareFile := newExpressionTransformFixture(t, `
interface Configuration { readonly middleware: number; }
/** @metadata macro {@link config intrinsic-middleware} */
declare function createServer(config: Configuration): void;
createServer({ middleware: 1 });
`)

	// When / Then: only Flamework-capable calls pay GetResolvedSignature.
	for _, call := range collectCallExpressions(ordinaryFile) {
		if callMayBeFlameworkMacro(ordinary, call) {
			t.Fatal("ordinary add() was treated as a Flamework macro")
		}
	}
	if !callMayBeFlameworkMacro(typed, collectCallExpressions(typedFile)[0]) {
		t.Fatal("inspect<string>() should admit type-argument macros")
	}
	if !callMayBeFlameworkMacro(surface, collectCallExpressions(surfaceFile)[0]) {
		t.Fatal("declarationUid() should admit same-file Flamework surface")
	}
	if !callMayBeFlameworkMacro(middleware, collectCallExpressions(middlewareFile)[0]) {
		t.Fatal("createServer() should admit known Flamework callee names")
	}
}

func TestTransformSourceFile_skipsExpressionWalk_whenFileHasNoFlameworkSurface(t *testing.T) {
	// Given: call-heavy source with no Flamework imports, APIs, or planned classes.
	state, sourceFile := newExpressionTransformFixture(t, `
export function add(left: number, right: number) { return left + right; }
export function run() {
	return add(add(1, 2), add(3, 4));
}
`)
	if sourceNeedsFlameworkExpressionTransform(state, sourceFile) {
		t.Fatal("fixture unexpectedly matched Flamework expression surface")
	}

	// When: transformSourceFile runs.
	transformed, err := transformSourceFile(state, sourceFile)
	// Then: original pointer is reused without expression rewrite.
	if err != nil {
		t.Fatal(err)
	}
	if transformed != sourceFile {
		t.Fatalf("transformSourceFile() = %p, want original %p", transformed, sourceFile)
	}
	t.Log("observable expression_walk_skipped=true source_pointer_reused=true")
}

func TestTransformSourceFile_walksExpressions_whenMacroIsDeclaredInAnImportedModule(t *testing.T) {
	// Given: a call site whose only Flamework evidence lives in the imported module.
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"strict":true,"rootDir":"src","outDir":"out"},"include":["src/**/*.ts"]}`)
	writeTransformFixture(t, directory, "src/macros.ts", strings.Join([]string{
		`type Caller<M extends string> = { _flamework_macro_caller: M };`,
		`export declare function callerText(value?: Caller<"text">): string;`,
	}, "\n"))
	writeTransformFixture(t, directory, "src/main.ts", strings.Join([]string{
		`import { callerText } from "./macros";`,
		`export const label = callerText();`,
	}, "\n"))
	program := newTransformProgram(t, directory)
	typeChecker, release := program.GetTypeChecker(context.Background())
	t.Cleanup(release)
	sourceFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/main.ts")))
	if sourceFile == nil {
		t.Fatal("call site source file was not loaded")
	}
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}
	state, err := newTransformState(TransformInput{Program: program, Checker: typeChecker, Files: []*ast.SourceFile{sourceFile}, Project: project}, nil)
	if err != nil {
		t.Fatalf("newTransformState() error = %v", err)
	}
	if !sourceNeedsFlameworkExpressionTransform(state, sourceFile) {
		t.Fatal("call site importing a macro declaration was rejected by the expression prefilter")
	}

	// When: transformSourceFile runs.
	transformed, err := transformSourceFile(state, sourceFile)
	// Then: the caller macro argument is substituted instead of silently skipped.
	if err != nil {
		t.Fatalf("transformSourceFile() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(transformed)
	if !strings.Contains(printed, `callerText("callerText()" as never)`) {
		t.Fatalf("imported caller macro was not expanded:\n%s", printed)
	}
	t.Logf("cross-module macro TypeScript:\n%s", printed)
}

func TestTransform_preservesUnchangedSourceMetadata_whenNoStructuralChangeOccurs(t *testing.T) {
	// Given
	base, sourceFile := newExpressionTransformFixture(t, "export const stable = true;\n")

	// When
	result, err := Transform(TransformInput{
		Program: base.program,
		Checker: base.checker,
		Files:   []*ast.SourceFile{sourceFile},
		Project: base.project,
	})
	// Then
	if err != nil {
		t.Fatalf("Transform() error = %v", err)
	}
	if len(result.Files) != 1 || result.Files[0] != sourceFile {
		t.Fatalf("Transform() files = %#v, want original source file %p", result.Files, sourceFile)
	}
	if len(result.Sources) != 1 || result.Sources[0].Original() != sourceFile || result.Sources[0].Transformed() != sourceFile || result.Sources[0].Changed() {
		t.Fatalf("Transform() source metadata = %#v, want unchanged original source", result.Sources)
	}
}

func TestTransformFlameworkExpressionsInSourceFile_expandsCallAndNewMacroArguments(t *testing.T) {
	// Given
	state, sourceFile := newExpressionTransformFixture(t, `
type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };
declare function callMacro(value?: Generic<"call", "text">): void;
declare class NewMacro {
	constructor(value?: Generic<"new", "text">);
}
callMacro();
new NewMacro();
`)

	// When
	transformed, err := transformFlameworkExpressionsInSourceFile(state, sourceFile)
	// Then
	if err != nil {
		t.Fatalf("transformFlameworkExpressionsInSourceFile() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(transformed)
	for _, want := range []string{
		`callMacro("\"call\"" as never);`,
		`new NewMacro("\"new\"" as never);`,
	} {
		if !strings.Contains(printed, want) {
			t.Fatalf("transformed source missing %q:\n%s", want, printed)
		}
	}
	reparsed := parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: "/reparsed.ts", Path: tspath.Path("/reparsed.ts")}, printed, core.ScriptKindTS)
	if len(reparsed.Diagnostics()) != 0 {
		t.Fatalf("reparse diagnostics = %v", reparsed.Diagnostics())
	}
	t.Logf("transformed TypeScript:\n%s", printed)
}

func TestTransformFlameworkExpressionsInSourceFileWithRuntime_insertsMacroPrerequisitesBeforeOwningStatement(t *testing.T) {
	// Given
	state, sourceFile := newExpressionTransformFixture(t, `
type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };
declare function prepareGuard(): void;
declare function guardMacro(value?: Generic<string, "guard">): void;
guardMacro();
`)
	runtime := MacroRuntime{BuildGuard: func(state *TransformState, _ *ast.Node, _ *checker.Type) (GuardBuildResult, error) {
		prepare := state.factory.NewCallExpression(
			state.factory.NewIdentifier("prepareGuard"),
			nil,
			nil,
			state.factory.NewNodeList(nil),
			ast.NodeFlagsNone,
		)
		return GuardBuildResult{
			Expression: state.factory.NewIdentifier("generatedGuard"),
			Statements: []*ast.Node{state.factory.NewExpressionStatement(prepare)},
		}, nil
	}}

	// When
	transformed, err := transformFlameworkExpressionsInSourceFileWithRuntime(state, sourceFile, runtime)
	// Then
	if err != nil {
		t.Fatalf("transformFlameworkExpressionsInSourceFileWithRuntime() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(transformed)
	prerequisite := strings.Index(printed, "prepareGuard();")
	owner := strings.Index(printed, "guardMacro(generatedGuard as never);")
	if prerequisite < 0 || owner < 0 || prerequisite > owner {
		t.Fatalf("macro prerequisite was not emitted before its owner:\n%s", printed)
	}
	reparsed := parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: "/reparsed.ts", Path: tspath.Path("/reparsed.ts")}, printed, core.ScriptKindTS)
	if len(reparsed.Diagnostics()) != 0 {
		t.Fatalf("reparse diagnostics = %v", reparsed.Diagnostics())
	}
	t.Logf("transformed TypeScript:\n%s", printed)
}

func TestTransformFlameworkExpressionsInSourceFileWithRuntime_returnsTypedMalformedMacroError(t *testing.T) {
	// Given
	state, sourceFile := newExpressionTransformFixture(t, `
type Invalid<T> = { _flamework_macro_generic: [T, number] };
declare function invalidMacro(value?: Invalid<string>): void;
invalidMacro();
`)

	// When
	transformed, err := transformFlameworkExpressionsInSourceFileWithRuntime(state, sourceFile, MacroRuntime{})
	// Then
	if err != nil {
		t.Fatalf("transformFlameworkExpressionsInSourceFileWithRuntime() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(transformed)
	if !strings.Contains(printed, "invalidMacro();") {
		t.Fatalf("malformed generic call was not preserved:\n%s", printed)
	}
}

func TestFlameworkMacroRandomIndex_returnsTypedErrorForNonpositiveUpperBound(t *testing.T) {
	// Given
	runtime := defaultFlameworkMacroRuntime()

	// When
	_, err := runtime.RandomIndex(0)

	// Then
	if !errors.Is(err, ErrInvalidMacroRandomIndex) {
		t.Fatalf("RandomIndex(0) error = %v, want ErrInvalidMacroRandomIndex", err)
	}
}

func TestTransformFlameworkExpressionsInSourceFile_doesNotReenterGeneratedGuardCalls(t *testing.T) {
	// Given
	state, sourceFile := newExpressionTransformFixture(t, `
type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };
interface Leaf { id: string; value: number }
interface Repeated { first: Leaf; second: Leaf; third: Leaf }
declare function guardMacro(value?: Generic<Repeated, "guard">): unknown;
guardMacro();
`)
	limit := 2
	state.project.config.Optimizations.GuardGenerationDedupLimit = &limit

	// When
	transformed, err := transformFlameworkExpressionsInSourceFile(state, sourceFile)
	// Then
	if err != nil {
		t.Fatalf("transformFlameworkExpressionsInSourceFile() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(transformed)
	if !strings.Contains(printed, `guardMacro(t[`) || !strings.Contains(printed, `as never)`) {
		t.Fatalf("generated guard macro was not preserved as a call argument:\n%s", printed)
	}
	reparsed := parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: "/reparsed.ts", Path: tspath.Path("/reparsed.ts")}, printed, core.ScriptKindTS)
	if len(reparsed.Diagnostics()) != 0 {
		t.Fatalf("reparse diagnostics = %v", reparsed.Diagnostics())
	}
	t.Logf("transformed TypeScript:\n%s", printed)
}

func TestTransformFlameworkExpressionsInSourceFile_importsPreludeForFlameworkCreateGuard(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"strict":true,"rootDir":"src","outDir":"out"},"files":["src/shared/guards.ts"]}`)
	writeTransformFixture(t, directory, "src/shared/guards.ts", strings.Join([]string{
		`type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };`,
		`declare namespace Flamework {`,
		`  /** @metadata macro */`,
		`  function createGuard<T>(meta?: Generic<T, "guard">): unknown;`,
		`}`,
		`interface Leaf { readonly id: string; readonly value: number }`,
		`interface Repeated { readonly first: Leaf; readonly second: Leaf; readonly third: Leaf }`,
		`export const repeatedGuard = Flamework.createGuard<Repeated>();`,
		`export const unionGuard = Flamework.createGuard<"ready" | number | undefined>();`,
	}, "\n"))
	program := newTransformProgram(t, directory)
	typeChecker, release := program.GetTypeChecker(context.Background())
	t.Cleanup(release)
	sourceFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/shared/guards.ts")))
	if sourceFile == nil {
		t.Fatal("guards source file was not loaded")
	}
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}
	state, err := newTransformState(TransformInput{Program: program, Checker: typeChecker, Files: []*ast.SourceFile{sourceFile}, Project: project}, nil)
	if err != nil {
		t.Fatalf("newTransformState() error = %v", err)
	}
	limit := 2
	state.project.config.Optimizations.GuardGenerationDedupLimit = &limit

	// When
	transformed, err := transformFlameworkExpressionsInSourceFile(state, sourceFile)
	// Then
	if err != nil {
		t.Fatalf("transformFlameworkExpressionsInSourceFile() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(transformed)
	guardImport := `import { t } from "@rbxts/t";`
	if strings.Count(printed, guardImport) != 1 {
		t.Fatalf("guard import count = %d, want 1:\n%s", strings.Count(printed, guardImport), printed)
	}
	diagnostics := state.orderedDiagnostics()
	if len(diagnostics) == 0 {
		t.Fatal("guard diagnostics are empty, want missing-core warnings")
	}
	for _, diagnostic := range diagnostics {
		if diagnostic.String() != "Flamework core was not found, guard generation may not work." {
			t.Fatalf("guard diagnostics = %v, want only exact missing-core warnings", diagnostics)
		}
	}
	prerequisite := strings.Index(printed, `const dedup =`)
	consumer := strings.Index(printed, `Flamework.createGuard<Repeated>(`)
	if prerequisite < 0 || consumer < 0 || prerequisite > consumer {
		t.Fatalf("guard prerequisite was not emitted before its createGuard consumer:\n%s", printed)
	}
	if strings.Count(printed, `Flamework.createGuard<`) != 2 {
		t.Fatalf("createGuard calls were not preserved:\n%s", printed)
	}
	reparsed := parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: "/reparsed.ts", Path: tspath.Path("/reparsed.ts")}, printed, core.ScriptKindTS)
	if len(reparsed.Diagnostics()) != 0 {
		t.Fatalf("reparse diagnostics = %v", reparsed.Diagnostics())
	}
	t.Logf("guards-only transformed TypeScript:\n%s", printed)
}

func TestTransformSourceFile_injectsCallerLineThroughTransitiveTypeOnlyImports(t *testing.T) {
	// Given: three-module fixture with exact data flow per plan.
	// store.ts declares Modding.Caller<"line"> via _flamework_macro_caller and the overloaded enqueue.
	// context.ts uses import type for Store and re-exports interface.
	// main.ts uses only import type for Context, destructures, calls the 3-arg overload, and has ZERO Flamework surface text.
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"strict":true,"rootDir":"src","outDir":"out","moduleResolution":"node"},"include":["src/**/*.ts"]}`)
	writeTransformFixture(t, directory, "src/store.ts", strings.Join([]string{
		`type CallerMetadata = { line: number; };`,
		`declare namespace Modding {`,
		`    export type Caller<M extends keyof CallerMetadata> = CallerMetadata[M] & { _flamework_macro_caller: M };`,
		`}`,
		`export interface Store {`,
		`    enqueue(player: unknown, transform: unknown, callsiteId?: Modding.Caller<"line">): void;`,
		`    enqueue(player: unknown, key: string, transform: unknown, callsiteId?: Modding.Caller<"line">): void;`,
		`}`,
	}, "\n"))
	writeTransformFixture(t, directory, "src/context.ts", strings.Join([]string{
		`import type { Store } from "./store";`,
		`export interface Context {`,
		`    data: Store;`,
		`}`,
	}, "\n"))
	writeTransformFixture(t, directory, "src/main.ts", strings.Join([]string{
		`import type { Context } from "./context";`,
		`declare const ctx: Context;`,
		`const { data } = ctx;`,
		`declare const player: unknown;`,
		`declare const transform: unknown;`,
		`data.enqueue(player, "settings.userSettings", transform);`,
	}, "\n"))

	// Additional for diamond/cycle/disconnected (must be before program load)
	writeTransformFixture(t, directory, "src/mid1.ts", `import type { Store } from "./store"; export interface M1 { s: Store }`)
	writeTransformFixture(t, directory, "src/mid2.ts", `import type { Store } from "./store"; export interface M2 { s: Store }`)
	writeTransformFixture(t, directory, "src/diamondMain.ts", strings.Join([]string{
		`import type { M1 } from "./mid1";`,
		`import type { M2 } from "./mid2";`,
		`declare const m1: M1; declare const m2: M2;`,
		`declare const player: unknown; declare const transform: unknown;`,
		`declare const data: import("./store").Store;`,
		`data.enqueue(player, "diamond.key", transform);`,
	}, "\n"))
	writeTransformFixture(t, directory, "src/cycleB.ts", `import type { Store } from "./store"; export interface CB { s: Store }`)
	writeTransformFixture(t, directory, "src/cycleA.ts", strings.Join([]string{
		`import type { CB } from "./cycleB";`,
		`declare const cb: CB;`,
		`declare const player: unknown; declare const transform: unknown;`,
		`declare const data: import("./store").Store;`,
		`data.enqueue(player, "cycle.key", transform);`,
	}, "\n"))
	writeTransformFixture(t, directory, "src/disconnected.ts", `export function plain(left: number, right: number): number { return left + right; }`)

	program := newTransformProgram(t, directory)
	typeChecker, release := program.GetTypeChecker(context.Background())
	t.Cleanup(release)
	sourceFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/main.ts")))
	if sourceFile == nil {
		t.Fatal("call site source file was not loaded")
	}
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}
	state, err := newTransformState(TransformInput{Program: program, Checker: typeChecker, Files: []*ast.SourceFile{sourceFile}, Project: project}, nil)
	if err != nil {
		t.Fatalf("newTransformState() error = %v", err)
	}
	if !sourceNeedsFlameworkExpressionTransform(state, sourceFile) {
		t.Fatal("call site reaching macro declaration only through type-only imports was rejected by the expression prefilter")
	}

	// When: transformSourceFile runs.
	transformed, err := transformSourceFile(state, sourceFile)
	// Then: the caller macro argument is substituted (line number) and we got a replacement file.
	if err != nil {
		t.Fatalf("transformSourceFile() error = %v", err)
	}
	if transformed == sourceFile {
		t.Fatal("transformSourceFile returned original source file with no structural change; expected injection of caller line")
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(transformed)
	if !strings.Contains(printed, `data.enqueue(player, "settings.userSettings", transform`) {
		t.Fatalf("transformed call lost the original three arguments:\n%s", printed)
	}
	if !regexp.MustCompile(`enqueue\(player, "settings.userSettings", transform, \d+ as never\)`).MatchString(printed) {
		t.Fatalf("transformed call missing injected Caller<\"line\"> line number as fourth argument:\n%s", printed)
	}
	t.Logf("cross-module transitive type-only caller injection TypeScript:\n%s", printed)

	// Diamond via type-only: multiple mids reach store; diamondMain reaches surface transitively. Must admit and inject.
	writeTransformFixture(t, directory, "src/mid1.ts", `import type { Store } from "./store"; export interface M1 { s: Store }`)
	writeTransformFixture(t, directory, "src/mid2.ts", `import type { Store } from "./store"; export interface M2 { s: Store }`)
	writeTransformFixture(t, directory, "src/diamondMain.ts", strings.Join([]string{
		`import type { M1 } from "./mid1";`,
		`import type { M2 } from "./mid2";`,
		`declare const m1: M1; declare const m2: M2;`,
		`declare const player: unknown; declare const transform: unknown;`,
		`declare const data: import("./store").Store;`,
		`data.enqueue(player, "diamond.key", transform);`,
	}, "\n"))
	diamondFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/diamondMain.ts")))
	if diamondFile == nil {
		t.Fatal("diamond main not loaded")
	}
	if !sourceNeedsFlameworkExpressionTransform(state, diamondFile) {
		t.Fatal("diamond caller through type-only diamond not admitted")
	}
	dt, derr := transformSourceFile(state, diamondFile)
	if derr != nil {
		t.Fatalf("diamond transform err: %v", derr)
	}
	dp := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(dt)
	if !strings.Contains(dp, `enqueue(player, "diamond.key", transform`) || !regexp.MustCompile(`enqueue\(player, "diamond.key", transform, \d+ as never\)`).MatchString(dp) {
		t.Fatalf("diamond missing injection:\n%s", dp)
	}

	// Cycle (type-only) containing the macro declarer reach. Must terminate (BFS marked) and inject at caller.
	writeTransformFixture(t, directory, "src/cycleB.ts", `import type { Store } from "./store"; export interface CB { s: Store }`)
	writeTransformFixture(t, directory, "src/cycleA.ts", strings.Join([]string{
		`import type { CB } from "./cycleB";`,
		`declare const cb: CB;`,
		`declare const player: unknown; declare const transform: unknown;`,
		`declare const data: import("./store").Store;`,
		`data.enqueue(player, "cycle.key", transform);`,
	}, "\n"))
	cycleFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/cycleA.ts")))
	if cycleFile == nil {
		t.Fatal("cycle caller not loaded")
	}
	if !sourceNeedsFlameworkExpressionTransform(state, cycleFile) {
		t.Fatal("cycle caller not admitted")
	}
	ct, cerr := transformSourceFile(state, cycleFile)
	if cerr != nil {
		t.Fatalf("cycle transform err (did not terminate?): %v", cerr)
	}
	cp := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(ct)
	if !regexp.MustCompile(`enqueue\(player, "cycle.key", transform, \d+ as never\)`).MatchString(cp) {
		t.Fatalf("cycle missing exact injection (or did not terminate):\n%s", cp)
	}

	// Disconnected ordinary source in same program: must remain ineligible, retain original source pointer (fast path).
	writeTransformFixture(t, directory, "src/disconnected.ts", `export function plain(left: number, right: number): number { return left + right; }`)
	discFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/disconnected.ts")))
	if discFile == nil {
		t.Fatal("disconnected not loaded")
	}
	if sourceNeedsFlameworkExpressionTransform(state, discFile) {
		t.Fatal("disconnected ordinary source was incorrectly admitted")
	}
	discT, derr2 := transformSourceFile(state, discFile)
	if derr2 != nil {
		t.Fatal(derr2)
	}
	if discT != discFile {
		t.Fatal("disconnected must reuse original source pointer (no expression walk)")
	}

	t.Log("diamond/cycle/disconnected subcases: admission, injection (exactly 1 line), termination, and reuse verified")
}

