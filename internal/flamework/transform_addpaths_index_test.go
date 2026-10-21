package flamework

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"sort"
	"testing"

	"rotor/internal/config"
	"rotor/tsgo/ast"
	"rotor/tsgo/bundled"
	"rotor/tsgo/compiler"
	"rotor/tsgo/tsoptions"
	"rotor/tsgo/tspath"
	"rotor/tsgo/vfs/osvfs"
)

// referenceAddPathsModules is the c5428f9 addPathsModules loop. It computes
// every source path again on each call and is the oracle for the index.
func referenceAddPathsModules(state *TransformState, file *ast.SourceFile, pattern string) ([]string, map[string]bool, error) {
	root := state.project.RootDirectory()
	resolver := state.project.RojoResolver()
	translator := state.project.PathTranslator()
	compare := tspath.ComparePathsOptions{UseCaseSensitiveFileNames: state.program.UseCaseSensitiveFileNames()}
	fromDirectory := tspath.GetDirectoryPath(file.FileName())
	type match struct{ relative, module string }
	matches := make([]match, 0)
	sources := make(map[string]bool)
	for _, source := range state.program.GetSourceFiles() {
		if source.IsDeclarationFile || state.program.IsSourceFileFromExternalLibrary(source) {
			continue
		}
		relative, err := projectRelativePath(root, source.FileName())
		if err != nil {
			continue
		}
		sources[relative] = true
		if source == file {
			continue
		}
		matched, err := matchGlobOrAncestor(pattern, relative)
		if err != nil {
			return nil, nil, err
		}
		if !matched {
			continue
		}
		output := translator.GetOutputPath(source.FileName())
		if _, ok := resolver.GetRbxPathFromFilePath(output); !ok {
			continue
		}
		if isScriptRbxType(resolver.GetRbxTypeFromFilePath(output)) {
			continue
		}
		matches = append(matches, match{relative: relative, module: moduleSpecifier(fromDirectory, source.FileName(), compare)})
	}
	sort.Slice(matches, func(i, j int) bool { return matches[i].relative < matches[j].relative })
	modules := make([]string, len(matches))
	for index, match := range matches {
		modules[index] = match.module
	}
	return modules, sources, nil
}

func newAddPathsIndexState(t testing.TB, directory string) *TransformState {
	t.Helper()
	slashed := filepath.ToSlash(directory)
	host := compiler.NewCompilerHost(slashed, bundled.WrapFS(osvfs.FS()), bundled.LibPath(), nil, nil)
	parsed, diagnostics := tsoptions.GetParsedCommandLineOfConfigFile(slashed+"/tsconfig.json", nil, nil, host, nil)
	if len(diagnostics) != 0 {
		t.Fatalf("config diagnostics = %v", diagnostics)
	}
	program := compiler.NewProgram(compiler.ProgramOptions{Host: host, Config: parsed})
	checker, release := program.GetTypeChecker(context.Background())
	t.Cleanup(release)
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}
	state, err := newTransformState(TransformInput{Program: program, Checker: checker, Project: project}, nil)
	if err != nil {
		t.Fatalf("newTransformState() error = %v", err)
	}
	return state
}

func addPathsIndexSource(t testing.TB, state *TransformState, directory, name string) *ast.SourceFile {
	t.Helper()
	file := state.program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, name)))
	if file == nil {
		t.Fatalf("source %s not found", name)
	}
	return file
}

// writeAddPathsIndexFixture adds mixed case, a dot directory, an external
// library source and a source outside the project root to the addPaths fixture.
func writeAddPathsIndexFixture(t *testing.T) string {
	t.Helper()
	base := t.TempDir()
	directory := filepath.Join(base, "game")
	writeAddPathsFixture(t, directory, `import "ext-lib";`+"\nexport {};\n")
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"moduleResolution":"node","rootDir":"src","outDir":"out"},"include":["src/**/*.ts","../outside/**/*.ts"]}`)
	writeTransformFixture(t, directory, "src/server/services/Upper/Mixed.ts", "export {};\n")
	writeTransformFixture(t, directory, "src/server/services/.hidden/skip.ts", "export {};\n")
	writeTransformFixture(t, directory, "node_modules/ext-lib/package.json", `{"name":"ext-lib"}`)
	writeTransformFixture(t, directory, "node_modules/ext-lib/index.ts", "export const value = 1;\n")
	writeTransformFixture(t, base, "outside/escape.ts", "export {};\n")
	return directory
}

func assertAddPathsModulesMatchReference(t testing.TB, state *TransformState, caller string, file *ast.SourceFile, pattern string) {
	t.Helper()
	got, gotSources, gotErr := addPathsModules(state, file, pattern)
	want, wantSources, wantErr := referenceAddPathsModules(state, file, pattern)
	if fmt.Sprint(gotErr) != fmt.Sprint(wantErr) {
		t.Fatalf("addPathsModules(%s, %q) error = %v, want %v", caller, pattern, gotErr, wantErr)
	}
	if !slices.Equal(got, want) || (wantErr == nil && !reflect.DeepEqual(gotSources, wantSources)) {
		t.Fatalf("addPathsModules(%s, %q) = %v, %v; want %v, %v", caller, pattern, got, gotSources, want, wantSources)
	}
}

func TestAddPathsModules_matchesReferenceForEveryCallerAndPattern(t *testing.T) {
	// Given: one state, and callers inside and outside the matched directories.
	directory := writeAddPathsIndexFixture(t)
	state := newAddPathsIndexState(t, directory)
	external := state.program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "node_modules/ext-lib/index.ts")))
	escape := state.program.GetSourceFile(filepath.ToSlash(filepath.Join(filepath.Dir(directory), "outside/escape.ts")))
	if external == nil || !state.program.IsSourceFileFromExternalLibrary(external) || escape == nil {
		t.Fatalf("fixture lacks an external source (%v) or an escaped source (%v)", external != nil, escape != nil)
	}
	callers := []string{"src/server/main.server.ts", "src/server/services/a.ts", "src/shared/components/x.ts", "src/server/services/Upper/Mixed.ts"}
	patterns := []string{"src/server/services/**", "src/shared/components/*.ts", "src/server/services/upper/*", "src/server/services/Upper/*", "src/**", "src/missing/**", "src/["}

	// When / Then: every call gives the result of the old loop, in two rounds
	// so that calls after an error also use the built index.
	for range 2 {
		for _, caller := range callers {
			file := addPathsIndexSource(t, state, directory, caller)
			for _, pattern := range patterns {
				assertAddPathsModulesMatchReference(t, state, caller, file, pattern)
			}
		}
	}
	sources := state.addPathsSources.sources
	if !sources["src/server/main.server.ts"] || sources["src/shared/components/types.d.ts"] || sources["node_modules/ext-lib/index.ts"] || len(sources) != len(state.addPathsSources.entries) {
		t.Fatalf("source index = %v", sources)
	}
}

func TestAddPathsModules_includesCallerWhenSourceFileIdentityChanged(t *testing.T) {
	// Given: an updated caller SourceFile, as the nested visitor gets after a
	// top-level expansion.
	directory := writeAddPathsIndexFixture(t)
	state := newAddPathsIndexState(t, directory)
	file := addPathsIndexSource(t, state, directory, "src/server/services/a.ts")
	updated := state.factory.UpdateSourceFile(file, state.factory.NewNodeList(slices.Clone(file.Statements.Nodes)), file.EndOfFileToken).AsSourceFile()
	if updated == file {
		t.Fatal("UpdateSourceFile kept the SourceFile identity")
	}
	pattern := "src/server/services/*"

	// When
	original, _, err := addPathsModules(state, file, pattern)
	if err != nil {
		t.Fatal(err)
	}
	changed, _, err := addPathsModules(state, updated, pattern)
	if err != nil {
		t.Fatal(err)
	}

	// Then: the program SourceFile excludes itself; the updated one does not.
	if slices.Contains(original, "./a") || !slices.Contains(changed, "./a") {
		t.Fatalf("original = %v, updated = %v", original, changed)
	}
	assertAddPathsModulesMatchReference(t, state, "updated a.ts", updated, pattern)
}

func TestAddPathsModules_buildsSourceIndexOncePerState(t *testing.T) {
	// Given
	directory := writeAddPathsIndexFixture(t)
	state := newAddPathsIndexState(t, directory)
	if state.addPathsSources != nil {
		t.Fatal("source index exists before any addPaths call")
	}

	// When: two callers and two patterns use one state.
	_, first, err := addPathsModules(state, addPathsIndexSource(t, state, directory, "src/server/main.server.ts"), "src/server/services/**")
	if err != nil {
		t.Fatal(err)
	}
	built := state.addPathsSources
	_, second, err := addPathsModules(state, addPathsIndexSource(t, state, directory, "src/shared/components/x.ts"), "src/shared/**")
	if err != nil {
		t.Fatal(err)
	}

	// Then: the second call computes no source path and shares the set.
	if built == nil || state.addPathsSources != built || reflect.ValueOf(first).UnsafePointer() != reflect.ValueOf(second).UnsafePointer() {
		t.Fatal("second addPaths call built the program source index again")
	}
}

func TestAddPathsModules_usesFreshIndexForEachProgram(t *testing.T) {
	// Given: a state that built its index before a file is added and one is removed.
	directory := writeAddPathsIndexFixture(t)
	before := newAddPathsIndexState(t, directory)
	pattern := "src/server/services/**"
	beforeModules, _, err := addPathsModules(before, addPathsIndexSource(t, before, directory, "src/server/main.server.ts"), pattern)
	if err != nil {
		t.Fatal(err)
	}
	writeTransformFixture(t, directory, "src/server/services/added.ts", "export {};\n")
	if err := os.Remove(filepath.Join(directory, "src/server/services/b.ts")); err != nil {
		t.Fatal(err)
	}

	// When: a new Program and state answer the same call.
	after := newAddPathsIndexState(t, directory)
	afterModules, afterSources, err := addPathsModules(after, addPathsIndexSource(t, after, directory, "src/server/main.server.ts"), pattern)
	if err != nil {
		t.Fatal(err)
	}

	// Then: each state reflects only its own Program.
	if !slices.Contains(beforeModules, "./services/b") || slices.Contains(beforeModules, "./services/added") {
		t.Fatalf("before = %v", beforeModules)
	}
	if slices.Contains(afterModules, "./services/b") || !slices.Contains(afterModules, "./services/added") || afterSources["src/server/services/b.ts"] {
		t.Fatalf("after = %v, sources = %v", afterModules, afterSources)
	}
	if before.addPathsSources == after.addPathsSources || before.addPathsSources.sources["src/server/services/added.ts"] {
		t.Fatal("a later Program changed or reused the earlier source index")
	}
}

// writeAddPathsBenchFixture is a Rojo project with 503 module sources.
func writeAddPathsBenchFixture(b *testing.B) string {
	b.Helper()
	directory := b.TempDir()
	files := map[string]string{
		"package.json":               `{"name":"fixture-game","version":"1.0.0"}`,
		"tsconfig.json":              `{"compilerOptions":{"moduleResolution":"node","rootDir":"src","outDir":"out"},"include":["src/**/*.ts"]}`,
		"default.project.json":       `{"name":"fixture","tree":{"$className":"DataModel","ServerScriptService":{"TS":{"$path":"out/server"}},"ReplicatedStorage":{"TS":{"$path":"out/shared"}}}}`,
		"src/server/main.server.ts":  "export {};\n",
		"src/server/services/a.ts":   "export {};\n",
		"src/shared/components/x.ts": "export {};\n",
		"src/shared/components/y.ts": "export {};\n",
	}
	for index := range 500 {
		files[fmt.Sprintf("src/shared/generated/m%03d.ts", index)] = "export {};\n"
	}
	for name, contents := range files {
		path := filepath.Join(directory, filepath.FromSlash(name))
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			b.Fatal(err)
		}
		if err := os.WriteFile(path, []byte(contents), 0o644); err != nil {
			b.Fatal(err)
		}
	}
	return directory
}

// BenchmarkAddPathsModules compares the old loop with the index on real
// addPaths path work. Program, checker and Project setup and the result check
// stay outside the timer. Each op builds the index again, so the first call
// pays for it.
func BenchmarkAddPathsModules(b *testing.B) {
	directory := writeAddPathsBenchFixture(b)
	state := newAddPathsIndexState(b, directory)
	callers := []*ast.SourceFile{
		addPathsIndexSource(b, state, directory, "src/server/main.server.ts"),
		addPathsIndexSource(b, state, directory, "src/server/services/a.ts"),
		addPathsIndexSource(b, state, directory, "src/shared/components/x.ts"),
	}
	patterns := []string{"src/server/services/**", "src/shared/components/**", "src/shared/generated/**", "src/missing/**"}
	for _, file := range callers {
		for _, pattern := range patterns {
			assertAddPathsModulesMatchReference(b, state, file.FileName(), file, pattern)
		}
	}
	implementations := []struct {
		name string
		call func(*TransformState, *ast.SourceFile, string) ([]string, map[string]bool, error)
	}{{"reference", referenceAddPathsModules}, {"index", addPathsModules}}
	for _, calls := range []int{1, len(callers) * len(patterns)} {
		for _, implementation := range implementations {
			b.Run(fmt.Sprintf("%s/calls=%d", implementation.name, calls), func(b *testing.B) {
				b.ReportAllocs()
				for b.Loop() {
					state.addPathsSources = nil
					for index := range calls {
						if _, _, err := implementation.call(state, callers[index%len(callers)], patterns[index%len(patterns)]); err != nil {
							b.Fatal(err)
						}
					}
				}
			})
		}
	}
}
