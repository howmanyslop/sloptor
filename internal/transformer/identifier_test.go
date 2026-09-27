// External test package: these tests build real tsgo programs (the fixture
// project needs compile.SanitizeFS, which lives in a package that imports
// transformer), so they exercise the transformer through its exported API.
package transformer_test

import (
	"context"
	"path/filepath"
	"testing"

	"rotor/internal/compile"
	"rotor/internal/luau"
	"rotor/internal/luau/render"
	"rotor/internal/transformer"
	"rotor/tsgo/ast"
	"rotor/tsgo/bundled"
	"rotor/tsgo/compiler"
	"rotor/tsgo/tsoptions"
	"rotor/tsgo/vfs/osvfs"
)

// buildState creates a program over projDir (through compile.SanitizeFS —
// tsconfig sanitizing plus the compiler-types Iterable-arity overlay, both
// harmless for already-clean projects) and returns a fresh transform state
// for relPath. Semantic diagnostics fail the test to keep fixtures honest —
// strictly: the historical escape hatch tolerating the TS5->TS7
// Iterable-arity divergence ("can only be iterated through ...") was deleted
// when the SanitizeFS overlay fixed the divergence at the source, so any
// regression resurfaces here.
func buildState(t *testing.T, projDir, relPath string) *transformer.State {
	t.Helper()

	dir, err := filepath.Abs(projDir)
	if err != nil {
		t.Fatal(err)
	}
	dir = filepath.ToSlash(dir)

	fs := compile.SanitizeFS(bundled.WrapFS(osvfs.FS()))
	host := compiler.NewCompilerHost(dir, fs, bundled.LibPath(), nil, nil)

	parsed, configDiags := tsoptions.GetParsedCommandLineOfConfigFile(dir+"/tsconfig.json", nil, nil, host, nil)
	if len(configDiags) > 0 {
		t.Fatalf("config diagnostics: %v", configDiags)
	}
	compile.ApplyAutomaticTypes(parsed.CompilerOptions())

	program := compiler.NewProgram(compiler.ProgramOptions{Host: host, Config: parsed})
	ctx := context.Background()

	sourceFile := program.GetSourceFile(dir + "/" + filepath.ToSlash(relPath))
	if sourceFile == nil {
		t.Fatalf("source file not in program: %s", relPath)
	}
	for _, d := range program.GetSemanticDiagnostics(ctx, sourceFile) {
		t.Errorf("unexpected semantic diagnostic: %s", d.String())
	}
	if t.Failed() {
		t.FailNow()
	}

	chk, release := program.GetTypeChecker(ctx)
	t.Cleanup(release)

	return transformer.NewState(program, chk, sourceFile, transformer.NewDiagService(), transformer.NewMultiState())
}

// collectIdentifiers returns every Identifier named text under root, in
// source order.
func collectIdentifiers(root *ast.Node, text string) []*ast.Node {
	var out []*ast.Node
	var visit func(node *ast.Node) bool
	visit = func(node *ast.Node) bool {
		if node.Kind == ast.KindIdentifier && node.Text() == text {
			out = append(out, node)
		}
		node.ForEachChild(visit)
		return false
	}
	root.ForEachChild(visit)
	return out
}

func mustOneIdentifier(t *testing.T, root *ast.Node, text string) *ast.Node {
	t.Helper()
	ids := collectIdentifiers(root, text)
	if len(ids) != 1 {
		t.Fatalf("found %d identifiers named %q, want 1", len(ids), text)
	}
	return ids[0]
}

func TestTransformIdentifierUndefinedToNil(t *testing.T) {
	// fixture 02: `let z: number | undefined = undefined;` — the initializer
	// `undefined` is the only Identifier with that text (the type-position
	// `undefined` is an UndefinedKeyword type node).
	s := buildState(t, filepath.Join("..", "..", "testdata", "diff", "project"), "src/02_variables.ts")

	node := mustOneIdentifier(t, s.SourceFile.AsNode(), "undefined")
	got := transformer.TransformIdentifier(s, node)
	if got.Kind() != luau.KindNilLiteral {
		t.Errorf("TransformIdentifier(undefined) = %v, want NilLiteral", got.Kind())
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}

func TestTransformIdentifierExportLetRouting(t *testing.T) {
	// fixture 08: `export let mutable = "start";` — reads of `mutable` must
	// route through the exports table even though 08 isn't golden-enabled yet.
	s := buildState(t, filepath.Join("..", "..", "testdata", "diff", "project"), "src/08_exports.ts")

	// TransformSourceFile registers the file's module symbol as `exports`;
	// replicate that wiring here.
	fileSymbol := s.Checker.GetSymbolAtLocation(s.SourceFile.AsNode())
	if fileSymbol == nil {
		t.Fatal("source file has no module symbol")
	}
	s.SetModuleIDBySymbol(fileSymbol, luau.GlobalID("exports"))

	// occurrences: declaration name, write lvalue, read in print(...) — the
	// last one is the read.
	ids := collectIdentifiers(s.SourceFile.AsNode(), "mutable")
	if len(ids) != 3 {
		t.Fatalf("found %d identifiers named mutable, want 3", len(ids))
	}
	read := ids[len(ids)-1]

	got := transformer.TransformIdentifier(s, read)
	access, ok := got.(*luau.PropertyAccessExpression)
	if !ok {
		t.Fatalf("TransformIdentifier(mutable read) = %T, want *luau.PropertyAccessExpression", got)
	}
	if access.Name != "mutable" {
		t.Errorf("property name = %q, want %q", access.Name, "mutable")
	}
	if id, ok := access.Expression.(*luau.Identifier); !ok || id.Name != "exports" {
		t.Errorf("property base = %#v, want identifier `exports`", access.Expression)
	}

	// reads of the immutable export stay plain identifiers
	internalRead := collectIdentifiers(s.SourceFile.AsNode(), "internal")[1]
	if got := transformer.TransformIdentifier(s, internalRead); got.Kind() != luau.KindIdentifier {
		t.Errorf("TransformIdentifier(internal read) = %v, want Identifier", got.Kind())
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}

func TestCheckIdentifierHoistUseBeforeDeclaration(t *testing.T) {
	// testdata/hoist/src/hoist.ts:
	//   function useBeforeDeclare(): number { return a + b; }
	//   let a = 1;
	//   let b = 2;
	// References to `a`/`b` inside the function precede their declarations, so
	// both symbols hoist onto the function statement. The references are
	// transformed directly first — the same call the function transform makes
	// while walking the body.
	s := buildState(t, filepath.Join("testdata", "hoist"), "src/hoist.ts")

	statements := s.SourceFile.Statements.Nodes
	funcStmt := statements[0]
	refA := mustOneIdentifier(t, funcStmt, "a")
	refB := mustOneIdentifier(t, funcStmt, "b")

	for _, ref := range []*ast.Node{refA, refB} {
		if got := transformer.TransformIdentifier(s, ref); got.Kind() != luau.KindIdentifier {
			t.Errorf("TransformIdentifier(%s) = %v, want Identifier", ref.Text(), got.Kind())
		}
		symbol := s.Checker.GetSymbolAtLocation(ref)
		if !s.IsHoisted[symbol] {
			t.Errorf("IsHoisted[%s] = false, want true", ref.Text())
		}
	}
	if hoists := s.HoistsByStatement[funcStmt]; len(hoists) != 2 {
		t.Fatalf("HoistsByStatement[funcStmt] has %d entries, want 2", len(hoists))
	}

	// The statement-list driver merges both hoists into one `local a, b` line
	// before the premature-reference statement, and the declarations become
	// assignments (isHoisted -> assignment-instead-of-local).
	list := transformer.TransformStatementList(s, s.SourceFile.AsNode(), statements, nil)
	want := "local a, b\nlocal function useBeforeDeclare()\n\treturn a + b\nend\na = 1\nb = 2\n"
	if got := render.RenderAST(list); got != want {
		t.Errorf("rendered statement list = %q, want %q", got, want)
	}

	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}

func TestCheckIdentifierHoistSelfReferenceExemptions(t *testing.T) {
	s := buildState(t, filepath.Join("testdata", "hoist"), "src/selfref.ts")
	root := s.SourceFile.AsNode()
	statements := s.SourceFile.Statements.Nodes

	assertNotHoisted := func(t *testing.T, ref *ast.Node) {
		t.Helper()
		transformer.TransformIdentifier(s, ref)
		if _, decided := s.IsHoisted[s.Checker.GetSymbolAtLocation(ref)]; decided {
			t.Errorf("IsHoisted[%s] recorded, want no hoist decision", ref.Text())
		}
	}

	t.Run("non-async function declaration self-reference", func(t *testing.T) {
		assertNotHoisted(t, collectIdentifiers(root, "fact")[1]) // fact() in own body
	})

	t.Run("class declaration self-reference", func(t *testing.T) {
		// new Singleton() inside the class; [0] is the declaration name,
		// [1] the return-type position, [2] the construct reference.
		assertNotHoisted(t, collectIdentifiers(root, "Singleton")[2])
	})

	t.Run("variable statement direct self-reference", func(t *testing.T) {
		// `let first = 1, second = first;` — the reference's nearest
		// statement ancestor IS the declaring variable statement.
		assertNotHoisted(t, collectIdentifiers(root, "first")[1])
	})

	t.Run("function-wrapped self-reference still hoists", func(t *testing.T) {
		// `const arrow: () => number = () => arrow();` — the reference is
		// nested inside a function-like, so the exemption does not apply.
		ref := collectIdentifiers(root, "arrow")[1]
		transformer.TransformIdentifier(s, ref)
		symbol := s.Checker.GetSymbolAtLocation(ref)
		if !s.IsHoisted[symbol] {
			t.Error("IsHoisted[arrow] = false, want true")
		}
		arrowStmt := statements[len(statements)-1]
		if hoists := s.HoistsByStatement[arrowStmt]; len(hoists) != 1 {
			t.Errorf("HoistsByStatement[arrowStmt] has %d entries, want 1", len(hoists))
		}
	})

	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}
