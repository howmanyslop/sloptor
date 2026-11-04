package spike

import (
	"context"
	"path/filepath"
	"testing"
	"time"

	"rotor/tsgo/ast"
	"rotor/tsgo/bundled"
	"rotor/tsgo/compiler"
	"rotor/tsgo/tsoptions"
	"rotor/tsgo/vfs/osvfs"
)

func TestCheckerSpike(t *testing.T) {
	start := time.Now()

	dir, err := filepath.Abs(filepath.Join("..", "..", "testdata", "spike-unit", "spike"))
	if err != nil {
		t.Fatal(err)
	}
	dir = filepath.ToSlash(dir)

	fs := bundled.WrapFS(osvfs.FS())
	host := compiler.NewCompilerHost(dir, fs, bundled.LibPath(), nil, nil)

	configPath := dir + "/tsconfig.json"
	parsed, diags := tsoptions.GetParsedCommandLineOfConfigFile(configPath, nil, nil, host, nil)
	if len(diags) > 0 {
		t.Fatalf("config diagnostics: %v", diags)
	}

	program := compiler.NewProgram(compiler.ProgramOptions{
		Host:   host,
		Config: parsed,
	})

	ctx := context.Background()
	if semDiags := program.GetSemanticDiagnostics(ctx, nil); len(semDiags) > 0 {
		for _, d := range semDiags {
			t.Errorf("unexpected diagnostic: %v", d.String())
		}
		t.Fatalf("fixture must be diagnostic-free")
	}

	checker, release := program.GetTypeChecker(ctx)
	defer release()

	sf := program.GetSourceFile(dir + "/src/main.ts")
	if sf == nil {
		t.Fatal("source file not found")
	}

	// Collect the initializer type of each top-level `const`.
	got := map[string]string{}
	for _, stmt := range sf.Statements.Nodes {
		if stmt.Kind != ast.KindVariableStatement {
			continue
		}
		declList := stmt.AsVariableStatement().DeclarationList.AsVariableDeclarationList()
		for _, decl := range declList.Declarations.Nodes {
			d := decl.AsVariableDeclaration()
			name := d.Name().Text()
			typ := checker.GetTypeAtLocation(decl)
			got[name] = checker.TypeToString(typ)
		}
	}

	want := map[string]string{
		"greeting": `"hello"`,
		"count":    "42",
		"items":    "number[]",
		"lookup":   "Map<string, number>",
		"total":    "number",
	}
	for name, w := range want {
		if got[name] != w {
			t.Errorf("type of %s = %q, want %q", name, got[name], w)
		}
	}

	t.Logf("program create + check + query: %s", time.Since(start))
}
