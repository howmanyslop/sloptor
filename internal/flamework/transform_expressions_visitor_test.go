package flamework

import (
	"errors"
	"fmt"
	"slices"
	"strings"
	"testing"

	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
	"rotor/tsgo/diagnostics"
	"rotor/tsgo/printer"
)

const expressionVisitorMacros = `
type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };
declare function guardMacro<T>(value?: Generic<T, "guard">): unknown;
declare namespace Guards {
    function _use(value: unknown): unknown;
    /** @metadata macro {@link _use intrinsic-flamework-rewrite} */
    function use<T>(value?: Generic<T, "guard">): unknown;
}
declare namespace Other {
    function _use(value: unknown): unknown;
    /** @metadata macro {@link _use intrinsic-flamework-rewrite} */
    function use<T>(value?: Generic<T, "guard">): unknown;
}
`

func TestFlameworkExpressionVisitor_preservesNestedResultsAcrossGrowthAndLaterCalls(t *testing.T) {
	deep := nestVisitorExpression(`Guards.use<"deep">()`, 48)
	expression := `{ left: Guards.use<"left">(), middle: ` + deep + `, right: Other.use<"right">() }`
	state, file := newExpressionTransformFixture(t, expressionVisitorMacros+
		"const first = "+expression+";\n"+
		`const later = [Other.use<"later">(), Other.use<"later">(), Other.use<"later">(), Other.use<"later">()];`)
	state.guardLibrary = "@rbxts/t"
	parents := expressionVisitorParents(file.AsNode())
	var calls []string
	runtime := MacroRuntime{BuildGuard: func(state *TransformState, _ *ast.Node, target *checker.Type) (GuardBuildResult, error) {
		label := strings.Trim(state.checker.TypeToString(target), `"`)
		calls = append(calls, label)
		return expressionVisitorGuard(state, label), nil
	}}
	result, err := transformFlameworkExpressionWithRuntime(state, expressionVisitorInitializer(t, file, "first"), runtime)
	if err != nil {
		t.Fatal(err)
	}
	wantExpression := strings.NewReplacer(
		`Guards.use<"left">()`, `Guards["_use"]<"left">(t.g_left as never)`,
		`Guards.use<"deep">()`, `Guards["_use"]<"deep">(t.g_deep as never)`,
		`Other.use<"right">()`, `Other["_use"]<"right">(t.g_right as never)`,
	).Replace(expression)
	assertExpressionVisitorText(t, state, file, result.expression, wantExpression)
	assertExpressionVisitorPrerequisites(t, state, file, result, "left", "deep", "right")
	wantImports := []MacroImport{
		{Module: flameworkCoreModule, Export: "Guards", Local: "Guards"},
		{Module: "@rbxts/t", Export: "t", Local: "t"},
		{Module: flameworkCoreModule, Export: "Guards", Local: "Guards"},
		{Module: "@rbxts/t", Export: "t", Local: "t"},
		{Module: flameworkCoreModule, Export: "Other", Local: "Other"},
		{Module: "@rbxts/t", Export: "t", Local: "t"},
	}
	if !slices.Equal(result.imports, wantImports) || !slices.Equal(calls, []string{"left", "deep", "right"}) {
		t.Fatalf("imports=%v calls=%v", result.imports, calls)
	}
	heldPrerequisites := slices.Clone(result.prerequisites)
	heldImports := slices.Clone(result.imports)
	for range 3 {
		later, err := transformFlameworkExpressionWithRuntime(state, expressionVisitorInitializer(t, file, "later"), runtime)
		if err != nil || len(later.prerequisites) != 4 || len(later.imports) != 8 {
			t.Fatalf("later result=%#v error=%v", later, err)
		}
	}
	if !slices.Equal(result.prerequisites, heldPrerequisites) || !slices.Equal(result.imports, heldImports) {
		t.Fatal("later invocation overwrote retained result slices")
	}
	assertExpressionVisitorText(t, state, file, result.expression, wantExpression)
	assertExpressionVisitorPrerequisites(t, state, file, result, "left", "deep", "right")
	assertExpressionVisitorParents(t, parents)
	assertExpressionVisitorIdle(t, state)
	if len(state.diagnostics) != 0 {
		t.Fatalf("diagnostics=%v", state.diagnostics)
	}
}

func TestFlameworkExpressionVisitor_isolatesReentrantRuntimesAndErrors(t *testing.T) {
	innerExpression := nestVisitorExpression(`[guardMacro<"captured">(), guardMacro<"inner">()]`, 64)
	source := expressionVisitorMacros + `
const outer = [guardMacro<"before">(), guardMacro<"reenter">(), guardMacro<"after">()];
const inner = ` + innerExpression + `;
const failing = [guardMacro<"fatal_before">(), guardMacro<"fatal">(), guardMacro<"skipped">()];
const healthy = [guardMacro<"healthy">()];
`
	state, file := newExpressionTransformFixture(t, source)
	state.guardLibrary = "@rbxts/t"
	state.macroRuntime = MacroRuntime{BuildGuard: func(*TransformState, *ast.Node, *checker.Type) (GuardBuildResult, error) {
		t.Fatal("replaced the supplied runtime with the state's default")
		return GuardBuildResult{}, nil
	}}
	parents := expressionVisitorParents(file.AsNode())
	var calls []string
	var inner, healthy expressionTransformResult
	fatalErr := errors.New("visitor fatal")
	fatalRuntime := MacroRuntime{BuildGuard: func(state *TransformState, _ *ast.Node, target *checker.Type) (GuardBuildResult, error) {
		label := strings.Trim(state.checker.TypeToString(target), `"`)
		calls = append(calls, "fatal:"+label)
		if label == "fatal" {
			return GuardBuildResult{}, fatalErr
		}
		return expressionVisitorGuard(state, label), nil
	}}
	var innerRuntime MacroRuntime
	innerRuntime.BuildGuard = func(state *TransformState, trace *ast.Node, target *checker.Type) (GuardBuildResult, error) {
		label := strings.Trim(state.checker.TypeToString(target), `"`)
		calls = append(calls, "inner:"+label)
		if label == "captured" {
			return GuardBuildResult{}, &MacroError{
				Node: trace, Message: "captured visitor diagnostic",
				RelatedInformation: []MacroRelatedInformation{{Node: trace, Message: "related visitor diagnostic"}},
			}
		}
		if label == "inner" {
			failed, err := transformFlameworkExpressionWithRuntime(state, expressionVisitorInitializer(t, file, "failing"), fatalRuntime)
			if !errors.Is(err, fatalErr) || failed.expression != nil || failed.prerequisites != nil || failed.imports != nil {
				t.Fatalf("fatal result=%#v error=%v", failed, err)
			}
			healthy, err = transformFlameworkExpressionWithRuntime(state, expressionVisitorInitializer(t, file, "healthy"), innerRuntime)
			if err != nil {
				t.Fatal(err)
			}
		}
		return expressionVisitorGuard(state, "inner_"+label), nil
	}
	outerRuntime := MacroRuntime{BuildGuard: func(state *TransformState, _ *ast.Node, target *checker.Type) (GuardBuildResult, error) {
		label := strings.Trim(state.checker.TypeToString(target), `"`)
		calls = append(calls, "outer:"+label)
		if label == "reenter" {
			var err error
			inner, err = transformFlameworkExpressionWithRuntime(state, expressionVisitorInitializer(t, file, "inner"), innerRuntime)
			if err != nil {
				t.Fatal(err)
			}
		}
		return expressionVisitorGuard(state, "outer_"+label), nil
	}}
	outer, err := transformFlameworkExpressionWithRuntime(state, expressionVisitorInitializer(t, file, "outer"), outerRuntime)
	if err != nil {
		t.Fatal(err)
	}
	wantCalls := []string{"outer:before", "outer:reenter", "inner:captured", "inner:inner", "fatal:fatal_before", "fatal:fatal", "inner:healthy", "outer:after"}
	if !slices.Equal(calls, wantCalls) {
		t.Fatalf("callbacks=%v, want %v", calls, wantCalls)
	}
	assertExpressionVisitorText(t, state, file, outer.expression, `[guardMacro<"before">(t.g_outer_before as never), guardMacro<"reenter">(t.g_outer_reenter as never), guardMacro<"after">(t.g_outer_after as never)]`)
	assertExpressionVisitorText(t, state, file, inner.expression, strings.Replace(innerExpression, `guardMacro<"inner">()`, `guardMacro<"inner">(t.g_inner_inner as never)`, 1))
	assertExpressionVisitorText(t, state, file, healthy.expression, `[guardMacro<"healthy">(t.g_inner_healthy as never)]`)
	assertExpressionVisitorPrerequisites(t, state, file, outer, "outer_before", "outer_reenter", "outer_after")
	assertExpressionVisitorPrerequisites(t, state, file, inner, "inner_inner")
	assertExpressionVisitorPrerequisites(t, state, file, healthy, "inner_healthy")
	for _, result := range []expressionTransformResult{outer, inner, healthy} {
		if len(result.imports) != len(result.prerequisites) {
			t.Fatalf("imports leaked across invocations: %#v", result)
		}
		for _, imported := range result.imports {
			if imported != (MacroImport{Module: "@rbxts/t", Export: "t", Local: "t"}) {
				t.Fatalf("unexpected import: %v", imported)
			}
		}
	}
	ordered := state.orderedDiagnostics()
	if len(ordered) != 1 || len(ordered[0].RelatedInformation()) != 1 {
		t.Fatalf("diagnostics=%v", ordered)
	}
	call := `guardMacro<"captured">()`
	start := strings.Index(source, call)
	assertExpressionVisitorDiagnostic(t, ordered[0], file, start, start+len(call), diagnostics.CategoryError, "captured visitor diagnostic")
	assertExpressionVisitorDiagnostic(t, ordered[0].RelatedInformation()[0], file, start, start+len(call), diagnostics.CategoryMessage, "related visitor diagnostic")
	assertExpressionVisitorParents(t, parents)
	assertExpressionVisitorIdle(t, state)
}

func TestFlameworkExpressionVisitor_restoresSuspendedAndTopLevelFramesAfterPanic(t *testing.T) {
	state, file := newExpressionTransformFixture(t, expressionVisitorMacros+`
const outer = [guardMacro<"reenter">(), guardMacro<"after">()];
const panicking = `+nestVisitorExpression(`guardMacro<"panic">()`, 48)+";\n")
	state.guardLibrary = "@rbxts/t"
	parents := expressionVisitorParents(file.AsNode())
	panicValue := errors.New("visitor panic")
	panicking := expressionVisitorInitializer(t, file, "panicking")
	panicRuntime := MacroRuntime{BuildGuard: func(*TransformState, *ast.Node, *checker.Type) (GuardBuildResult, error) {
		panic(panicValue)
	}}
	invokePanic := func() {
		t.Helper()
		defer func() {
			if value := recover(); value != panicValue {
				t.Fatalf("panic=%v, want original panic %v", value, panicValue)
			}
		}()
		_, _ = transformFlameworkExpressionWithRuntime(state, panicking, panicRuntime)
		t.Fatal("transform swallowed the runtime panic")
	}
	var calls []string
	runtime := MacroRuntime{BuildGuard: func(state *TransformState, _ *ast.Node, target *checker.Type) (GuardBuildResult, error) {
		label := strings.Trim(state.checker.TypeToString(target), `"`)
		calls = append(calls, label)
		if label == "reenter" {
			invokePanic()
		}
		return expressionVisitorGuard(state, label), nil
	}}
	for range 2 {
		invokePanic()
		assertExpressionVisitorIdle(t, state)
		result, err := transformFlameworkExpressionWithRuntime(state, expressionVisitorInitializer(t, file, "outer"), runtime)
		if err != nil {
			t.Fatal(err)
		}
		assertExpressionVisitorText(t, state, file, result.expression, `[guardMacro<"reenter">(t.g_reenter as never), guardMacro<"after">(t.g_after as never)]`)
		assertExpressionVisitorPrerequisites(t, state, file, result, "reenter", "after")
		if len(result.imports) != 2 || len(state.diagnostics) != 0 {
			t.Fatalf("panic contaminated result=%#v diagnostics=%v", result, state.diagnostics)
		}
		assertExpressionVisitorIdle(t, state)
	}
	if !slices.Equal(calls, []string{"reenter", "after", "reenter", "after"}) {
		t.Fatalf("callbacks=%v", calls)
	}
	assertExpressionVisitorParents(t, parents)
}

func TestFlameworkExpressionVisitor_warmedUnchangedTraversalAllocations(t *testing.T) {
	shapes := []struct{ name, expression string }{
		{"small", `[1]`},
		{"wide", `[` + strings.TrimSuffix(strings.Repeat(`{ value: [1, 2] },`, 64), ",") + `]`},
		{"deep", nestVisitorExpression(`1`, 64)},
	}
	var source strings.Builder
	for _, shape := range shapes {
		fmt.Fprintf(&source, "const %s = %s;\n", shape.name, shape.expression)
	}
	state, file := newExpressionTransformFixture(t, source.String())
	leaf := findExpressionKind(t, file.AsNode(), ast.KindNumericLiteral)
	if result, err := transformFlameworkExpressionWithRuntime(state, leaf, MacroRuntime{}); err != nil || result.expression != leaf || state.expressionChildren != nil {
		t.Fatalf("leaf initialized the visitor: result=%#v error=%v", result, err)
	}
	for _, shape := range shapes {
		t.Run(shape.name, func(t *testing.T) {
			expression := expressionVisitorInitializer(t, file, shape.name)
			parents := expressionVisitorParents(expression)
			// AllocsPerRun warms the traversal before measuring. This counts real
			// unchanged AST walks, excluding parsing, checking and stack growth.
			allocations := testing.AllocsPerRun(20, func() {
				result, err := transformFlameworkExpressionWithRuntime(state, expression, MacroRuntime{})
				if err != nil || result.expression != expression || result.prerequisites == nil || result.imports == nil || len(result.prerequisites) != 0 || len(result.imports) != 0 {
					t.Fatalf("unchanged traversal=%#v error=%v", result, err)
				}
			})
			if allocations != 0 {
				t.Fatalf("warmed %s traversal allocations=%v, want 0", shape.name, allocations)
			}
			t.Logf("warmed %s expression allocations=%v", shape.name, allocations)
			assertExpressionVisitorText(t, state, file, expression, shape.expression)
			assertExpressionVisitorParents(t, parents)
			assertExpressionVisitorIdle(t, state)
		})
	}
	if len(state.diagnostics) != 0 {
		t.Fatalf("unchanged traversal diagnostics=%v", state.diagnostics)
	}
}

func nestVisitorExpression(expression string, depth int) string {
	for index := range depth {
		if index%2 == 0 {
			expression = "[" + expression + "]"
		} else {
			expression = "{ nested: " + expression + " }"
		}
	}
	return expression
}

func expressionVisitorInitializer(t *testing.T, file *ast.SourceFile, name string) *ast.Node {
	t.Helper()
	var found *ast.Node
	var visit func(*ast.Node) bool
	visit = func(node *ast.Node) bool {
		if node.Kind == ast.KindVariableDeclaration && node.Name().Text() == name {
			found = node.Initializer()
			return true
		}
		return node.ForEachChild(visit)
	}
	visit(file.AsNode())
	if found == nil {
		t.Fatalf("initializer %q not found", name)
	}
	return found
}

func expressionVisitorGuard(state *TransformState, label string) GuardBuildResult {
	return GuardBuildResult{
		Expression: state.factory.NewPropertyAccessExpression(state.factory.NewIdentifier("t"), nil, state.factory.NewIdentifier("g_"+label), ast.NodeFlagsNone),
		Statements: []*ast.Node{state.factory.NewExpressionStatement(state.factory.NewIdentifier("prepare_" + label))},
	}
}

func assertExpressionVisitorText(t *testing.T, state *TransformState, file *ast.SourceFile, node *ast.Node, want string) {
	t.Helper()
	got := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, state.EmitContext()).Emit(node, file)
	if strings.Join(strings.Fields(got), "") != strings.Join(strings.Fields(want), "") {
		t.Fatalf("printed expression:\n%s\nwant:\n%s", got, want)
	}
}

func assertExpressionVisitorPrerequisites(t *testing.T, state *TransformState, file *ast.SourceFile, result expressionTransformResult, labels ...string) {
	t.Helper()
	if len(result.prerequisites) != len(labels) {
		t.Fatalf("prerequisites=%v, want labels %v", result.prerequisites, labels)
	}
	for index, label := range labels {
		assertExpressionVisitorText(t, state, file, result.prerequisites[index], "prepare_"+label+";")
	}
}

func assertExpressionVisitorDiagnostic(t *testing.T, diagnostic *ast.Diagnostic, file *ast.SourceFile, start, end int, category diagnostics.Category, message string) {
	t.Helper()
	code, hasCode := diagnostic.StringCode()
	got := fmt.Sprintf("%s|%d|%d|%s|%s|%s", diagnostic.File().FileName(), diagnostic.Pos(), diagnostic.End(), diagnostic.Category(), code, diagnostic.String())
	want := fmt.Sprintf("%s|%d|%d|%s|%s|%s", file.FileName(), start, end, category, flameworkDiagnosticCode, message)
	if !hasCode || got != want {
		t.Fatalf("diagnostic tuple=%q, want %q", got, want)
	}
}

func expressionVisitorParents(root *ast.Node) map[*ast.Node]*ast.Node {
	parents := make(map[*ast.Node]*ast.Node)
	var visit func(*ast.Node) bool
	visit = func(node *ast.Node) bool {
		parents[node] = node.Parent
		node.ForEachChild(visit)
		return false
	}
	visit(root)
	return parents
}

func assertExpressionVisitorParents(t *testing.T, parents map[*ast.Node]*ast.Node) {
	t.Helper()
	for node, parent := range parents {
		if node.Parent != parent {
			t.Fatalf("original %v parent=%p, want %p", node.Kind, node.Parent, parent)
		}
	}
}

func assertExpressionVisitorIdle(t *testing.T, state *TransformState) {
	t.Helper()
	visitor := state.expressionChildren
	if visitor == nil || len(visitor.frames) != 0 {
		t.Fatal("visitor did not restore its invocation depth")
	}
	// Observable results above cover semantics. Inspect inactive capacity only
	// to verify that high-water frame storage retains no invocation references.
	for index, frame := range visitor.frames[:cap(visitor.frames)] {
		if frame.runtime.UUID != nil || frame.runtime.RandomIndex != nil || frame.runtime.BuildGuard != nil || frame.err != nil || frame.prerequisites != nil || frame.imports != nil {
			t.Fatalf("inactive frame %d retained invocation references", index)
		}
	}
}
