package compile

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"rotor/internal/luau"
	"rotor/internal/transformer"
	"rotor/tsgo/ast"
)

// withStubDispatch swaps the real statement dispatch for a TransformStatement
// that emits nothing, isolating the pipeline around it (program creation,
// sanitizer, module symbol, export shapes, return-nil rule, header, rendering)
// so these tests exercise the compile plumbing independent of transformer
// behavior.
func withStubDispatch(t *testing.T) {
	t.Helper()
	prev := transformer.TransformStatement
	transformer.TransformStatement = func(s *transformer.State, node *ast.Node) *luau.List[luau.Statement] {
		return luau.NewList[luau.Statement]()
	}
	t.Cleanup(func() { transformer.TransformStatement = prev })
}

func fixtureProjectDir(t *testing.T) string {
	t.Helper()
	dir, err := filepath.Abs(filepath.Join("..", "..", "testdata", "diff", "project"))
	if err != nil {
		t.Fatal(err)
	}
	return dir
}

func TestCompileFilePipelineNoExports(t *testing.T) {
	withStubDispatch(t)
	got, diags, err := CompileFile(fixtureProjectDir(t), filepath.Join("src", "01_literals.ts"))
	if err != nil {
		t.Fatalf("CompileFile: %v", err)
	}
	if len(diags) > 0 {
		t.Fatalf("diagnostics: %v", diags)
	}
	// Statement transforms are stubbed out, so only the header and the
	// ModuleScript `return nil` remain.
	want := "-- Compiled with sloptor v2.3.3\nreturn nil\n"
	if got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

func TestCompileFilePipelineExportShapes(t *testing.T) {
	withStubDispatch(t)
	got, diags, err := CompileFile(fixtureProjectDir(t), filepath.Join("src", "08_exports.ts"))
	if err != nil {
		t.Fatalf("CompileFile: %v", err)
	}
	if len(diags) > 0 {
		t.Fatalf("diagnostics: %v", diags)
	}
	// 08_exports has `export const constant` (immutable) + `export let
	// mutable` (mutable): the mutable export forces the exports-table shape —
	// `local exports = {}` up top, the immutable pair assigned at the bottom,
	// then `return exports`.
	want := "-- Compiled with sloptor v2.3.3\n" +
		"local exports = {}\n" +
		"exports.constant = constant\n" +
		"return exports\n"
	if got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

// TestCompileFileReturnMapShape covers the ReturnMap export shape end-to-end
// with the REAL statement dispatch. No diff fixture reaches this branch:
// 08_exports mixes in a mutable export, which forces the exports-table shape
// for the whole file, so only an immutable-only file emits the map return.
//
// The expected bytes are oracle-verified: a scratch compile of exactly
// `export const x = 1;` through testdata/diff/project with rbxtsc 3.0.0
// (2026-06-06) produced this output verbatim — multi-line map literal with a
// trailing comma, tab-indented.
func TestCompileFileReturnMapShape(t *testing.T) {
	// Self-contained temp project so shared fixture testdata is never mutated
	// while other packages' tests may be compiling it in parallel. The fixture
	// project's @rbxts packages are reused via an absolute typeRoots path.
	dir := t.TempDir()
	typeRoots := filepath.ToSlash(filepath.Join(fixtureProjectDir(t), "node_modules", "@rbxts"))
	tsconfig := fmt.Sprintf(`{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"module": "commonjs",
		"moduleResolution": "Node",
		"noLib": true,
		"forceConsistentCasingInFileNames": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"typeRoots": ["node_modules/@rbxts", %q],
		"rootDir": "src",
		"outDir": "out"
	},
	"include": ["src"]
}`, typeRoots)
	if err := os.WriteFile(filepath.Join(dir, "tsconfig.json"), []byte(tsconfig), 0o644); err != nil {
		t.Fatal(err)
	}
	// CompileFile validates the project like rbxtsc does (compileFiles.ts:
	// 82-98): a non-package project needs a package.json, a Rojo config, and
	// Rojo data for the include folder.
	if err := os.WriteFile(filepath.Join(dir, "package.json"), []byte(`{"name":"returnmap-fixture"}`), 0o644); err != nil {
		t.Fatal(err)
	}
	rojoConfig := `{"name":"returnmap","tree":{"$path":"out","include":{"$path":"include"}}}`
	if err := os.WriteFile(filepath.Join(dir, "default.project.json"), []byte(rojoConfig), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Mkdir(filepath.Join(dir, "src"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "src", "returnmap.ts"), []byte("export const x = 1;\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	got, diags, err := CompileFile(dir, filepath.Join("src", "returnmap.ts"))
	if err != nil {
		t.Fatalf("CompileFile: %v", err)
	}
	if len(diags) > 0 {
		t.Fatalf("diagnostics: %v", diags)
	}
	want := "-- Compiled with sloptor v2.3.3\n" +
		"local x = 1\n" +
		"return {\n" +
		"\tx = x,\n" +
		"}\n"
	if got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

// TestCompileFilePanicBecomesError: a transformer panic must come back from
// CompileFile as an "internal compiler error" — the CLI must never crash on
// a user's source file. Phase 2's deliberate loop-closure-capture panic is
// gone (the copy machinery landed in Phase 2b), so the boundary — which still
// guards genuine internal errors — is exercised with a synthetic panic
// injected through the dispatch func var.
func TestCompileFilePanicBecomesError(t *testing.T) {
	prev := transformer.TransformStatement
	transformer.TransformStatement = func(s *transformer.State, node *ast.Node) *luau.List[luau.Statement] {
		panic("synthetic transformer panic (test)")
	}
	t.Cleanup(func() { transformer.TransformStatement = prev })

	_, _, err := CompileFile(fixtureProjectDir(t), filepath.Join("src", "01_literals.ts"))
	if err == nil {
		t.Fatal("expected an error from the recovered panic, got nil")
	}
	if !strings.Contains(err.Error(), "internal compiler error: ") {
		t.Errorf("error missing 'internal compiler error: ' prefix: %v", err)
	}
	if !strings.Contains(err.Error(), "synthetic transformer panic") {
		t.Errorf("error missing panic message: %v", err)
	}
}

// CompileFile mirror of TestCompileProjectSyntacticErrorFails: the
// single-file fast path runs the same pre-emit collection, so a parse error
// must surface as a diagnostic, never as silently-compiled output.
func TestCompileFileSyntacticErrorFails(t *testing.T) {
	dir := writeProject(t, "@syntax/error-fixture", "")
	if err := os.WriteFile(filepath.Join(dir, "src", "main.ts"), []byte("export const x = 5;\n}\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	text, diags, err := CompileFile(dir, filepath.Join("src", "main.ts"))
	if err == nil {
		t.Fatal("expected a hard error for the parse error")
	}
	if text != "" {
		t.Errorf("text = %q, want empty", text)
	}
	if len(diags) == 0 {
		t.Fatal("expected diagnostics for the parse error")
	}
	if !strings.Contains(diags[0], "Declaration or statement expected") {
		t.Errorf("diags[0] = %q, want the TS1128 'Declaration or statement expected.' diagnostic (all: %v)", diags[0], diags)
	}
}

func TestCompileFileMissingSource(t *testing.T) {
	withStubDispatch(t)
	_, _, err := CompileFile(fixtureProjectDir(t), filepath.Join("src", "does_not_exist.ts"))
	if err == nil {
		t.Fatal("expected error for missing source file")
	}
}

// TestTransformerDiagnosticCarriesLocation verifies that the FileName, Offset,
// and Len fields added to DiagnosticInfo are populated for transformer
// diagnostics that have an AST node. env_diag_model/src/main.ts passes a
// non-literal variable to $env, which triggers DiagRotorEnvNonLiteralArg with
// the argument node attached — a reliable case that always carries a source
// location.
func TestTransformerDiagnosticCarriesLocation(t *testing.T) {
	_, infos, _ := CompileFileDetailed("testdata/env_diag_model", "src/main.ts")
	if len(infos) == 0 {
		t.Fatal("expected at least one diagnostic")
	}
	var located bool
	for _, d := range infos {
		if d.FileName != "" && d.Len > 0 {
			located = true
		}
	}
	if !located {
		t.Errorf("no diagnostic carried a location: %+v", infos)
	}
}

func TestCompileFileUsesFileAffinedCheckerForImports(t *testing.T) {
	got, diags, err := CompileFile(filepath.Join("testdata", "imports_model"), filepath.Join("src", "_scratch_once.ts"))
	if err != nil {
		t.Fatalf("CompileFile: %v (diags: %v)", err, diags)
	}
	if len(diags) > 0 {
		t.Fatalf("diagnostics: %v", diags)
	}
	want := "-- Compiled with sloptor v2.3.3\n" +
		"local TS = require(script.Parent.include.RuntimeLib)\n" +
		"local g = TS.import(script, script.Parent, \"_scratch_util\").greet\n" +
		"print(g(\"once\"))\n" +
		"return nil\n"
	if got != want {
		t.Errorf("got:\n%s\nwant:\n%s", got, want)
	}
}
