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

// parseSourceFile parses one file the way a project program would, so the
// external-module indicator and the module augmentation list are populated.
func parseSourceFile(name, text string) *ast.SourceFile {
	return parser.ParseSourceFile(
		ast.SourceFileParseOptions{FileName: name, Path: tspath.Path(name)},
		text,
		core.ScriptKindTS,
	)
}

// fakeSourceFiles names files whose content does not matter, so they are
// parsed as ordinary external modules: nothing about them reaches another file
// except through an import.
func fakeSourceFiles(t *testing.T, names ...string) []*ast.SourceFile {
	t.Helper()
	files := make([]*ast.SourceFile, 0, len(names))
	for _, name := range names {
		files = append(files, parseSourceFile(name, "export {};\n"))
	}
	return files
}

func TestNarrowedSidecarRootsKeepsEveryDeclarationFile(t *testing.T) {
	project := fakeSourceFiles(t, "/proj/a.ts", "/proj/b.ts", "/proj/globals.d.ts", "/proj/node.d.mts", "/proj/legacy.d.cts")
	compile := project[:1]

	roots := narrowedSidecarRoots(compile, project)
	want := []string{
		filepath.FromSlash("/proj/a.ts"),
		filepath.FromSlash("/proj/globals.d.ts"),
		filepath.FromSlash("/proj/node.d.mts"),
		filepath.FromSlash("/proj/legacy.d.cts"),
	}
	if !slices.Equal(roots, want) {
		t.Fatalf("narrowedSidecarRoots = %v, want %v", roots, want)
	}
}

// A `.ts` that is not an external module puts every top-level name in the
// global scope, so dropping it changes what the checker answers about files
// that never import it.
func TestNarrowedSidecarRootsKeepsGlobalScripts(t *testing.T) {
	project := []*ast.SourceFile{
		parseSourceFile("/proj/a.ts", "export {};\n"),
		parseSourceFile("/proj/b.ts", "export {};\n"),
		parseSourceFile("/proj/script.ts", "declare const AMBIENT: number;\n"),
	}

	roots := narrowedSidecarRoots(project[:1], project)
	want := []string{filepath.FromSlash("/proj/a.ts"), filepath.FromSlash("/proj/script.ts")}
	if !slices.Equal(roots, want) {
		t.Fatalf("narrowedSidecarRoots = %v, want %v", roots, want)
	}
}

// `declare global` inside a module reaches every file too, and the module it
// lives in need not be imported by any of them.
func TestNarrowedSidecarRootsKeepsGlobalAugmentations(t *testing.T) {
	project := []*ast.SourceFile{
		parseSourceFile("/proj/a.ts", "export {};\n"),
		parseSourceFile("/proj/b.ts", "export {};\n"),
		parseSourceFile("/proj/augment.ts", "export {};\ndeclare global {\n\tinterface Ambient { name: string }\n}\n"),
	}

	roots := narrowedSidecarRoots(project[:1], project)
	want := []string{filepath.FromSlash("/proj/a.ts"), filepath.FromSlash("/proj/augment.ts")}
	if !slices.Equal(roots, want) {
		t.Fatalf("narrowedSidecarRoots = %v, want %v", roots, want)
	}
}

// An augmentation of a NAMED module is only visible to a file that imports that
// module, so it rides in on the import closure and does not have to be a root.
func TestNarrowedSidecarRootsDropsNamedModuleAugmentations(t *testing.T) {
	project := []*ast.SourceFile{
		parseSourceFile("/proj/a.ts", "export {};\n"),
		parseSourceFile("/proj/b.ts", "export {};\n"),
		parseSourceFile("/proj/augment.ts", "export {};\ndeclare module \"other\" {\n\tinterface Thing { name: string }\n}\n"),
	}

	roots := narrowedSidecarRoots(project[:1], project)
	want := []string{filepath.FromSlash("/proj/a.ts")}
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
