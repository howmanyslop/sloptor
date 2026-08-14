package transformer

import (
	"context"
	"os"
	"path/filepath"
	"testing"

	"rotor/tsgo/ast"
	"rotor/tsgo/bundled"
	"rotor/tsgo/compiler"
	"rotor/tsgo/tsoptions"
	"rotor/tsgo/vfs/osvfs"
)

func TestIsReferencedAliasValue_keepsImportWhenCheckerCannotResolveReference(t *testing.T) {
	// Given: a file with `import { t } from "@rbxts/t";` where the only usage
	// of `t` is a property access `t.interface(...)`. This mirrors the
	// Flamework macro-generated guard under noSemanticDiagnostics.
	directory := t.TempDir()
	writeFixture(t, directory, "package.json", `{"name":"fixture","version":"1.0.0"}`)
	writeFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"rootDir":"src","outDir":"out","moduleResolution":"node","strict":true},"include":["src/**/*.ts"]}`)
	writeFixture(t, directory, "node_modules/@rbxts/t/index.d.ts", `export declare const t: { interface(x: unknown): (v: unknown) => boolean; string: unknown; };`)
	writeFixture(t, directory, "src/main.ts", `import { t } from "@rbxts/t";
declare const value: unknown;
const check = t.interface({ animationId: t.string })(value);
`)

	s := buildElisionState(t, directory, "src/main.ts")
	importClause, importName := findNamedImport(s, "t")
	kept := isReferencedAliasValue(s, importClause, importName)
	t.Logf("isReferencedAliasValue (direct) = %v", kept)
	if !kept {
		t.Fatal("t import elided even though it is referenced as a value")
	}
}

func TestIsReferencedAliasValue_keepsReexportedTAlias(t *testing.T) {
	// Given: `import { t } from "@flamework/core/out/prelude"` where prelude
	// re-exports `t` from "@rbxts/t". The guard code uses `t` as a value.
	// This mirrors Flamework macro-generated guards where the guard's `t`
	// may bind to the ORIGINAL @rbxts/t symbol, not the prelude alias.
	directory := t.TempDir()
	writeFixture(t, directory, "package.json", `{"name":"fixture","version":"1.0.0"}`)
	writeFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"rootDir":"src","outDir":"out","moduleResolution":"node","strict":true},"include":["src/**/*.ts"]}`)
	writeFixture(t, directory, "node_modules/@rbxts/t/index.d.ts", `export declare const t: { interface(x: unknown): (v: unknown) => boolean; string: unknown; };`)
	writeFixture(t, directory, "node_modules/@flamework/core/out/prelude.d.ts", `import { t } from "@rbxts/t";\nexport { t };\n`)
	writeFixture(t, directory, "src/main.ts", `import { t } from "@flamework/core/out/prelude";
declare const value: unknown;
const check = t.interface({ animationId: t.string })(value);
`)

	s := buildElisionState(t, directory, "src/main.ts")
	importClause, importName := findNamedImport(s, "t")
	kept := isReferencedAliasValue(s, importClause, importName)
	t.Logf("isReferencedAliasValue (re-export) = %v", kept)
	if !kept {
		t.Fatal("re-exported t import elided even though t is used as a value")
	}
}

func buildElisionState(t *testing.T, directory, relPath string) *State {
	t.Helper()
	host := compiler.NewCompilerHost(filepath.ToSlash(directory), bundled.WrapFS(osvfs.FS()), bundled.LibPath(), nil, nil)
	parsed, configDiags := tsoptions.GetParsedCommandLineOfConfigFile(directory+"/tsconfig.json", nil, nil, host, nil)
	if len(configDiags) > 0 {
		t.Fatalf("config diagnostics: %v", configDiags)
	}
	program := compiler.NewProgram(compiler.ProgramOptions{Host: host, Config: parsed})
	ctx := context.Background()
	sourceFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, relPath)))
	if sourceFile == nil {
		t.Fatal("source file not in program")
	}
	chk, release := program.GetTypeChecker(ctx)
	t.Cleanup(release)
	return NewState(program, chk, sourceFile, NewDiagService(), NewMultiState())
}

func findNamedImport(s *State, name string) (*ast.Node, *ast.Node) {
	sourceFile := s.SourceFile
	for _, stmt := range sourceFile.AsNode().Statements() {
		if !ast.IsImportDeclaration(stmt) {
			continue
		}
		importClause := stmt.AsImportDeclaration().ImportClause
		if importClause == nil {
			continue
		}
		namedBindings := importClause.AsImportClause().NamedBindings
		if namedBindings == nil || !ast.IsNamedImports(namedBindings) {
			continue
		}
		for _, element := range namedBindings.AsNamedImports().Elements.Nodes {
			if element.Name().Text() == name {
				return importClause, element.Name()
			}
		}
	}
	return nil, nil
}

func writeFixture(t *testing.T, directory, name, contents string) {
	t.Helper()
	path := filepath.Join(directory, filepath.FromSlash(name))
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(contents), 0o644); err != nil {
		t.Fatal(err)
	}
}
