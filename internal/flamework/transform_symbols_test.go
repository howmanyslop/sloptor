package flamework

import (
	"context"
	"path/filepath"
	"testing"

	"rotor/internal/config"
	"rotor/tsgo/ast"
)

func TestNodeUID_resolves_import_alias_to_declaration(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"strict":true,"rootDir":"src","outDir":"out"},"include":["src/**/*.ts"]}`)
	writeTransformFixture(t, directory, "src/payload.ts", `export interface Payload { readonly value: string }`)
	writeTransformFixture(t, directory, "src/main.ts", "import type { Payload as Alias } from \"./payload\";\ndeclare function id<T>(): string;\nid<Alias>();")
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	defer release()
	file := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/main.ts")))
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}
	state, err := newTransformState(TransformInput{Program: program, Checker: checker, Files: []*ast.SourceFile{file}, Project: project}, nil)
	if err != nil {
		t.Fatalf("newTransformState() error = %v", err)
	}
	typeArgument := collectCallExpressions(file)[0].TypeArgumentList().Nodes[0]

	// When
	id, err := nodeUID(state, typeArgument)
	// Then
	if err != nil {
		t.Fatalf("nodeUID() error = %v", err)
	}
	if id != "payload@Payload" {
		t.Fatalf("nodeUID() = %q, want %q", id, "payload@Payload")
	}
}
