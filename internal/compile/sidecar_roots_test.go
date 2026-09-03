package compile

import (
	"path/filepath"
	"slices"
	"testing"

	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/parser"
	"rotor/tsgo/tspath"
)

func fakeSourceFiles(t *testing.T, names ...string) []*ast.SourceFile {
	t.Helper()
	files := make([]*ast.SourceFile, 0, len(names))
	for _, name := range names {
		files = append(files, parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: name, Path: tspath.Path(name)}, "", core.ScriptKindTS))
	}
	return files
}

func TestNarrowedSidecarRootsKeepsEveryDeclarationFile(t *testing.T) {
	project := fakeSourceFiles(t, "/proj/a.ts", "/proj/b.ts", "/proj/globals.d.ts")
	compile := project[:1]

	roots := narrowedSidecarRoots(compile, project)
	want := []string{filepath.FromSlash("/proj/a.ts"), filepath.FromSlash("/proj/globals.d.ts")}
	if !slices.Equal(roots, want) {
		t.Fatalf("narrowedSidecarRoots = %v, want %v", roots, want)
	}
}

// A full build already compiles every file, so narrowing would name the whole
// project and only cost the request the extra bytes.
func TestNarrowedSidecarRootsSkipsFullBuilds(t *testing.T) {
	project := fakeSourceFiles(t, "/proj/a.ts", "/proj/b.ts")

	if roots := narrowedSidecarRoots(project, project); roots != nil {
		t.Fatalf("narrowedSidecarRoots(full build) = %v, want nil", roots)
	}
	if roots := narrowedSidecarRoots(nil, project); roots != nil {
		t.Fatalf("narrowedSidecarRoots(no selection) = %v, want nil", roots)
	}
}

// A compiled file that the tsconfig never named still has to become a root, or
// the worker's program would not contain the file it was asked to transform.
func TestNarrowedSidecarRootsNamesEveryCompiledFile(t *testing.T) {
	project := fakeSourceFiles(t, "/proj/a.ts", "/proj/b.ts", "/proj/c.ts")
	compile := fakeSourceFiles(t, "/proj/a.ts", "/proj/generated.ts")

	roots := narrowedSidecarRoots(compile, project)
	want := []string{filepath.FromSlash("/proj/a.ts"), filepath.FromSlash("/proj/generated.ts")}
	if !slices.Equal(roots, want) {
		t.Fatalf("narrowedSidecarRoots = %v, want %v", roots, want)
	}
}
