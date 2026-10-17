package flamework

import (
	"context"
	"errors"
	"fmt"
	"path/filepath"
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

func TestFlameworkSurfaceCaches_distinguishSourceFilesAtTheSamePath(t *testing.T) {
	parse := func(text string) *ast.SourceFile {
		return parser.ParseSourceFile(ast.SourceFileParseOptions{
			FileName: "/same.ts", Path: tspath.Path("/same.ts"),
		}, text, core.ScriptKindTS)
	}
	state := &TransformState{}
	for _, text := range []string{
		"export const ordinary = 1;",
		"export interface Marker { _flamework_intrinsic: string; }",
		"export const ordinary = 2;",
		"export const api = Reflect.get;",
	} {
		file := parse(text)
		want := strings.Contains(text, "_flamework") || strings.Contains(text, "Reflect.")
		for range 2 {
			if got := sourceDeclaresFlameworkExpressionSurface(state, file); got != want {
				t.Fatalf("surface for %q = %v, want %v", text, got, want)
			}
		}
		if cached, found := state.flameworkExpressionSurface(file); !found || cached != want {
			t.Fatalf("direct expression predicate was not cached for %q", text)
		}
	}
}

func TestFlameworkSurfaceCaches_followRebuiltProgramsWithTheSameProject(t *testing.T) {
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"strict":true,"rootDir":"src","outDir":"out"},"include":["src/**/*.ts"]}`)
	writeTransformFixture(t, directory, "src/main.ts", "import { enqueue } from './api';\nenqueue();\n")
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{}})
	if err != nil {
		t.Fatal(err)
	}
	var previous *ast.SourceFile
	for _, macro := range []bool{false, true, false} {
		declaration := "export declare function enqueue(): void;\n"
		if macro {
			declaration = "type Caller = string & { _flamework_macro_caller: 'text' };\nexport declare function enqueue(value?: Caller): void;\n"
		}
		writeTransformFixture(t, directory, "src/api.ts", declaration)
		program := newTransformProgram(t, directory)
		typeChecker, release := program.GetTypeChecker(context.Background())
		t.Cleanup(release)
		file := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/main.ts")))
		if file == previous {
			t.Fatal("rebuild fixture reused the previous source AST")
		}
		previous = file
		// Bare states are used by static addPaths analysis and intentionally do
		// not initialize transitive expression admission on a predicate query.
		bare := &TransformState{program: program}
		if callMayBeFlameworkMacro(bare, collectCallExpressions(file)[0]) || bare.flameworkExpressionSurfaces != nil {
			t.Fatal("bare-state predicate unexpectedly initialized reachability")
		}
		result, err := Transform(TransformInput{Program: program, Checker: typeChecker, Project: project, Files: []*ast.SourceFile{file}})
		if err != nil || len(result.Diagnostics) != 0 {
			t.Fatalf("transform (macro=%v) error=%v diagnostics=%v", macro, err, result.Diagnostics)
		}
		printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, result.Sources[0].EmitContext()).EmitSourceFile(result.Files[0])
		wantCall := "enqueue();"
		if macro {
			wantCall = `enqueue("enqueue()" as never);`
		}
		if !strings.Contains(printed, wantCall) || result.Sources[0].Changed() != macro {
			t.Fatalf("transform (macro=%v) changed=%v output:\n%s", macro, result.Sources[0].Changed(), printed)
		}
	}
}

func TestFlameworkExpressionLeaves_preserveIdentityWithoutAllocations(t *testing.T) {
	state, file := newExpressionTransformFixture(t, "export const value = 1;\n")
	parsed := findExpressionKind(t, file.AsNode(), ast.KindNumericLiteral)
	factory := state.factory
	leaves := []*ast.Node{
		parsed,
		factory.NewIdentifier("value"),
		factory.NewPrivateIdentifier("#value"),
		factory.NewStringLiteral("text", ast.TokenFlagsNone),
		factory.NewNumericLiteral("2", ast.TokenFlagsNone),
		factory.NewBigIntLiteral("2n", ast.TokenFlagsNone),
		factory.NewRegularExpressionLiteral("/value/", ast.TokenFlagsNone),
		factory.NewNoSubstitutionTemplateLiteral("text", ast.TokenFlagsNone),
	}
	for _, kind := range []ast.Kind{
		ast.KindNullKeyword, ast.KindTrueKeyword, ast.KindFalseKeyword,
		ast.KindThisKeyword, ast.KindSuperKeyword, ast.KindImportKeyword,
	} {
		leaves = append(leaves, factory.NewKeywordExpression(kind))
	}
	for _, leaf := range leaves {
		t.Run(leaf.Kind.String(), func(t *testing.T) {
			parent := leaf.Parent
			allocations := testing.AllocsPerRun(10, func() {
				result, err := transformFlameworkExpressionWithRuntime(state, leaf, MacroRuntime{})
				if err != nil || result.expression != leaf || len(result.prerequisites) != 0 || len(result.imports) != 0 {
					t.Fatalf("leaf transform = %#v, %v; want unchanged leaf", result, err)
				}
			})
			if allocations != 0 {
				t.Fatalf("leaf transform allocations = %v, want 0", allocations)
			}
			if leaf.Parent != parent || len(state.diagnostics) != 0 {
				t.Fatal("leaf transform changed parent identity or diagnostics")
			}
		})
	}
	if _, err := transformFlameworkExpressionWithRuntime(nil, leaves[0], MacroRuntime{}); !errors.Is(err, ErrInvalidTransformInput) {
		t.Fatalf("nil-state leaf error = %v, want ErrInvalidTransformInput", err)
	}
	if _, err := transformFlameworkExpressionWithRuntime(state, nil, MacroRuntime{}); !errors.Is(err, ErrInvalidTransformInput) {
		t.Fatalf("nil-node error = %v, want ErrInvalidTransformInput", err)
	}
}

func TestAnalyzeFlameworkClasses_allocationsIgnoreUnrelatedImportGraph(t *testing.T) {
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"noLib":true,"rootDir":"src","outDir":"out"},"files":["src/main.ts"]}`)
	writeTransformFixture(t, directory, "src/main.ts", "export const value = 1;\n")
	small := newTransformProgram(t, directory)
	for index := range 128 {
		source := "export const value = 1;\n"
		if index > 0 {
			source = fmt.Sprintf("import './file%d';\n", index-1) + source
		}
		writeTransformFixture(t, directory, fmt.Sprintf("src/file%d.ts", index), source)
	}
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"noLib":true,"rootDir":"src","outDir":"out"},"files":["src/main.ts","src/file127.ts"]}`)
	large := newTransformProgram(t, directory)
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{}})
	if err != nil {
		t.Fatal(err)
	}
	allocations := func(input TransformInput) float64 {
		return testing.AllocsPerRun(3, func() {
			analysis, err := AnalyzeFlameworkClasses(input)
			if err != nil || len(analysis.Files()) != 0 || len(analysis.Diagnostics()) != 0 {
				t.Fatalf("class analysis = %#v, %v; want empty analysis", analysis, err)
			}
		})
	}
	smallChecker, releaseSmall := small.GetTypeChecker(context.Background())
	defer releaseSmall()
	largeChecker, releaseLarge := large.GetTypeChecker(context.Background())
	defer releaseLarge()
	name := filepath.ToSlash(filepath.Join(directory, "src/main.ts"))
	smallAllocs := allocations(TransformInput{Program: small, Checker: smallChecker, Project: project, Files: []*ast.SourceFile{small.GetSourceFile(name)}})
	largeAllocs := allocations(TransformInput{Program: large, Checker: largeChecker, Project: project, Files: []*ast.SourceFile{large.GetSourceFile(name)}})
	if largeAllocs > smallAllocs+4 {
		t.Fatalf("class-only allocations grew with unrelated imports: small=%v, large=%v", smallAllocs, largeAllocs)
	}
	t.Logf("class-only allocations: small=%v, unrelated-import-graph=%v", smallAllocs, largeAllocs)
}

func TestFlameworkExpressionLeaves_preserveNestedPrerequisiteOrder(t *testing.T) {
	state, file := newExpressionTransformFixture(t, `
type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };
declare function guardMacro(value?: Generic<string, "guard">): unknown;
declare function consume(value: unknown): void;
consume({ left: guardMacro(), right: [guardMacro(), "literal"] });
`)
	calls := 0
	runtime := MacroRuntime{BuildGuard: func(state *TransformState, _ *ast.Node, _ *checker.Type) (GuardBuildResult, error) {
		calls++
		prepare := state.factory.NewCallExpression(state.factory.NewIdentifier(fmt.Sprintf("prepare%d", calls)), nil, nil, state.factory.NewNodeList(nil), ast.NodeFlagsNone)
		return GuardBuildResult{
			Expression: state.factory.NewIdentifier(fmt.Sprintf("generatedGuard%d", calls)),
			Statements: []*ast.Node{state.factory.NewExpressionStatement(prepare)},
		}, nil
	}}
	transformed, err := transformFlameworkExpressionsInSourceFileWithRuntime(state, file, runtime)
	if err != nil {
		t.Fatal(err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, state.EmitContext()).EmitSourceFile(transformed)
	want := "prepare1();\nprepare2();\nconsume({ left: guardMacro(generatedGuard1 as never), right: [guardMacro(generatedGuard2 as never), \"literal\"] });\n"
	if calls != 2 || !strings.HasSuffix(printed, want) || len(state.diagnostics) != 0 {
		t.Fatalf("nested transform calls=%d diagnostics=%v output:\n%s\nwant suffix:\n%s", calls, state.diagnostics, printed, want)
	}
}
