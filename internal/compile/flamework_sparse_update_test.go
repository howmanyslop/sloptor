package compile

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"rotor/internal/transformer"
	"rotor/tsgo/ast"
	"rotor/tsgo/compiler"
)

func TestNativeFlameworkSparseUpdate_reusesIncomingProgramAndFiles_whenEverySourceIsUnchanged(t *testing.T) {
	// Given: a native Flamework project whose ordinary sources need no structural transform.
	dir := task7FlameworkProject(t, "[flamework]\n")
	_, program, diagnostics, err := newProjectProgram(dir, "")
	if err != nil {
		t.Fatalf("newProjectProgram: %v (%v)", err, diagnostics)
	}
	pipeline, diagnostics, err := prepareFlameworkPipeline(filepath.ToSlash(dir), program, ProjectOptions{})
	if err != nil {
		t.Fatalf("prepareFlameworkPipeline: %v (%v)", err, diagnostics)
	}
	sourceFiles := projectSourceFiles(program)
	traces := diagnosticTraces{"unchanged-prefix": {fileName: "unchanged.ts", text: "prefix"}}

	// When: the native stage runs.
	prepared, infos, err := applyNativeFlameworkTransform(filepath.ToSlash(dir), program, sourceFiles, nil, traces, pipeline.project)
	// Then: no print/overlay/reparse path replaces the incoming compiler state.
	if err != nil {
		t.Fatalf("applyNativeFlameworkTransform: %v (%v)", err, infos)
	}
	if prepared.program != program {
		t.Fatalf("native no-op Program = %p, want incoming %p", prepared.program, program)
	}
	if len(prepared.sourceFiles) != len(sourceFiles) {
		t.Fatalf("native no-op source count = %d, want %d", len(prepared.sourceFiles), len(sourceFiles))
	}
	for index := range sourceFiles {
		if prepared.sourceFiles[index] != sourceFiles[index] {
			t.Fatalf("native no-op source[%d] = %p, want incoming %p", index, prepared.sourceFiles[index], sourceFiles[index])
		}
	}
	if prepared.sourceTraces["unchanged-prefix"] != traces["unchanged-prefix"] {
		t.Fatal("native no-op replaced the incoming prefix trace")
	}
	t.Logf("observable program_reused=true source_files_reused=%d trace_reused=true", len(sourceFiles))
}

func TestNativeFlameworkSparseUpdate_updatesOnlyChangedSourcesInStableSourceOrder(t *testing.T) {
	// Given: two changed overlays arrive in reverse order around one unchanged source.
	dir, program, sourceFiles := sparseNativeProgram(t, map[string]string{
		"a.ts": "export const a = 1;\n",
		"b.ts": "export const b = 1;\n",
		"c.ts": "export const c = 1;\n",
	})
	a := program.GetSourceFile(filepath.Join(dir, "src", "a.ts"))
	b := program.GetSourceFile(filepath.Join(dir, "src", "b.ts"))
	c := program.GetSourceFile(filepath.Join(dir, "src", "c.ts"))
	authored := &sourceTraceMap{fileName: a.FileName(), text: "authored-a", mappings: []traceMapping{{generatedLine: 0, sourceLine: 4}}}
	native := &sourceTraceMap{fileName: a.FileName(), text: a.Text(), mappings: []traceMapping{{generatedLine: 0, sourceLine: 0}}}
	traces := diagnosticTraces{normalizeSourceFilePath(a.FileName()): authored, normalizeSourceFilePath(b.FileName()): {fileName: b.FileName(), text: "unchanged-prefix"}}

	// When: the sparse updater applies reverse-ordered overlays.
	updated, remapped, composed, err := updateNativeFlameworkProgram(nativeProgramUpdate{
		program: program, sourceFiles: sourceFiles, traces: traces,
		overlays: []nativeSourceOverlay{
			{fileName: c.FileName(), text: "export const c = 2;\n"},
			{fileName: a.FileName(), text: "export const a = 2;\n", trace: native},
		},
	})
	// Then: changed text is reparsed, while the middle file and its prefix trace retain identity.
	if err != nil {
		t.Fatal(err)
	}
	if updated == program {
		t.Fatal("sparse changed update reused the incoming Program")
	}
	if updated.GetSourceFile(a.FileName()) == a || !strings.Contains(updated.GetSourceFile(a.FileName()).Text(), "a = 2") {
		t.Fatal("a.ts did not receive its native overlay")
	}
	if updated.GetSourceFile(c.FileName()) == c || !strings.Contains(updated.GetSourceFile(c.FileName()).Text(), "c = 2") {
		t.Fatal("c.ts did not receive its native overlay")
	}
	if updated.GetSourceFile(b.FileName()) != b {
		t.Fatal("unchanged b.ts parse tree was not reused")
	}
	if remappedSource(remapped, b.FileName()) != b {
		t.Fatal("unchanged b.ts was not retained in the compile source list")
	}
	if composed[normalizeSourceFilePath(b.FileName())] != traces[normalizeSourceFilePath(b.FileName())] {
		t.Fatal("unchanged b.ts prefix trace was replaced")
	}
	position := composed[normalizeSourceFilePath(a.FileName())].OriginalPosition(transformer.SourcePosition{Line: 0, Column: 0})
	if position == nil || position.Line != 4 {
		t.Fatalf("changed a.ts trace position = %+v, want authored line 4", position)
	}
	t.Log("observable changed_overlays=2 unchanged_parse_tree_reused=true unchanged_trace_reused=true composed_authored_line=4")
}

func TestNativeFlameworkSparseUpdate_batchesCompatibleOverlaysIntoOneProgramReconstruction(t *testing.T) {
	// Given: three shape-compatible text overlays that would previously each rebuild a Program.
	dir, program, sourceFiles := sparseNativeProgram(t, map[string]string{
		"a.ts": "export const a = 1;\n",
		"b.ts": "export const b = 1;\n",
		"c.ts": "export const c = 1;\n",
	})
	programCreations := 0
	program = compiler.NewProgram(compiler.ProgramOptions{
		Host:   program.Host(),
		Config: program.CommandLine(),
		CreateCheckerPool: func(*compiler.Program) compiler.CheckerPool {
			programCreations++
			return nil
		},
	})
	programCreations = 0
	sourceFiles = projectSourceFiles(program)
	a := program.GetSourceFile(filepath.Join(dir, "src", "a.ts"))
	b := program.GetSourceFile(filepath.Join(dir, "src", "b.ts"))
	c := program.GetSourceFile(filepath.Join(dir, "src", "c.ts"))

	// When: three reverse-ordered compatible overlays apply together.
	updated, _, _, err := updateNativeFlameworkProgram(nativeProgramUpdate{
		program: program, sourceFiles: sourceFiles,
		overlays: []nativeSourceOverlay{
			{fileName: c.FileName(), text: "export const c = 2;\n"},
			{fileName: b.FileName(), text: "export const b = 2;\n"},
			{fileName: a.FileName(), text: "export const a = 2;\n"},
		},
	})
	// Then: one Program reconstruction covers every overlay.
	if err != nil {
		t.Fatal(err)
	}
	if programCreations != 1 {
		t.Fatalf("compatible multi-overlay update created %d Programs, want one batched reconstruction", programCreations)
	}
	if updated.GetSourceFile(a.FileName()) == a || updated.GetSourceFile(b.FileName()) == b || updated.GetSourceFile(c.FileName()) == c {
		t.Fatal("batched compatible update retained an incoming changed parse tree")
	}
	if !strings.Contains(updated.GetSourceFile(a.FileName()).Text(), "a = 2") ||
		!strings.Contains(updated.GetSourceFile(b.FileName()).Text(), "b = 2") ||
		!strings.Contains(updated.GetSourceFile(c.FileName()).Text(), "c = 2") {
		t.Fatal("batched compatible update did not expose every native overlay")
	}
	t.Logf("observable program_reconstructions=%d changed_overlays=3", programCreations)
}

func TestNativeFlameworkSparseUpdate_reusesUnchangedFiles_whenImportTargetsAlreadyLoaded(t *testing.T) {
	// Given: Flamework-like output that adds an import of a module already in the Program.
	dir, program, sourceFiles := sparseNativeProgram(t, map[string]string{
		"a.ts":   "export const a = 1;\n",
		"b.ts":   "export const b = 1;\n",
		"dep.ts": "export const value = 1;\n",
	})
	programCreations := 0
	program = compiler.NewProgram(compiler.ProgramOptions{
		Host:   program.Host(),
		Config: program.CommandLine(),
		CreateCheckerPool: func(*compiler.Program) compiler.CheckerPool {
			programCreations++
			return nil
		},
	})
	programCreations = 0
	sourceFiles = projectSourceFiles(program)
	a := program.GetSourceFile(filepath.Join(dir, "src", "a.ts"))
	b := program.GetSourceFile(filepath.Join(dir, "src", "b.ts"))
	dep := program.GetSourceFile(filepath.Join(dir, "src", "dep.ts"))

	// When: a.ts gains an import of already-loaded dep and b.ts only changes text.
	updated, _, _, err := updateNativeFlameworkProgram(nativeProgramUpdate{
		program: program, sourceFiles: sourceFiles,
		overlays: []nativeSourceOverlay{
			{fileName: b.FileName(), text: "export const b = 2;\n"},
			{fileName: a.FileName(), text: "import { value } from \"./dep\"; export const a = value;\n"},
		},
	})
	// Then: one sparse reconstruction reuses untouched parse trees instead of full rebuild.
	if err != nil {
		t.Fatal(err)
	}
	if programCreations != 1 {
		t.Fatalf("already-loaded import update created %d Programs, want one sparse reconstruction", programCreations)
	}
	if updated.GetSourceFile(dep.FileName()) != dep {
		t.Fatal("already-loaded import update reparsed untouched dep.ts")
	}
	if updated.GetSourceFile(a.FileName()) == a || !strings.Contains(updated.GetSourceFile(a.FileName()).Text(), "from \"./dep\"") {
		t.Fatal("a.ts did not receive its import-bearing overlay")
	}
	if updated.GetSourceFile(b.FileName()) == b || !strings.Contains(updated.GetSourceFile(b.FileName()).Text(), "b = 2") {
		t.Fatal("b.ts did not receive its text overlay")
	}
	t.Log("observable sparse_import_add=true unchanged_dep_reused=true program_reconstructions=1")
}

func TestNativeFlameworkSparseUpdate_fallsBackOnceWithEveryOverlay_whenNewModuleIsRequired(t *testing.T) {
	// Given: a new on-disk module is not part of the current Program file set.
	dir, program, sourceFiles := sparseNativeProgram(t, map[string]string{
		"a.ts": "export const a = 1;\n",
		"b.ts": "export const b = 1;\n",
	})
	if err := os.WriteFile(filepath.Join(dir, "src", "extra.ts"), []byte("export const extra = 1;\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	programCreations := 0
	program = compiler.NewProgram(compiler.ProgramOptions{
		Host:   program.Host(),
		Config: program.CommandLine(),
		CreateCheckerPool: func(*compiler.Program) compiler.CheckerPool {
			programCreations++
			return nil
		},
	})
	programCreations = 0
	sourceFiles = projectSourceFiles(program)
	a := program.GetSourceFile(filepath.Join(dir, "src", "a.ts"))
	b := program.GetSourceFile(filepath.Join(dir, "src", "b.ts"))
	if program.GetSourceFile(filepath.Join(dir, "src", "extra.ts")) != nil {
		t.Fatal("extra.ts was unexpectedly already part of the Program")
	}

	// When: a.ts imports the unloaded module and b.ts also has overlay text.
	updated, _, _, err := updateNativeFlameworkProgram(nativeProgramUpdate{
		program: program, sourceFiles: sourceFiles,
		overlays: []nativeSourceOverlay{
			{fileName: b.FileName(), text: "export const b = 2;\n"},
			{fileName: a.FileName(), text: "import { extra } from \"./extra\"; export const a = extra;\n"},
		},
	})
	// Then: one full rebuild includes every overlay and loads the new module graph.
	if err != nil {
		t.Fatal(err)
	}
	if programCreations != 1 {
		t.Fatalf("new-module update created %d Programs, want one full fallback", programCreations)
	}
	if updated.GetSourceFile(a.FileName()) == a || updated.GetSourceFile(b.FileName()) == b {
		t.Fatal("new-module fallback retained an incoming changed parse tree")
	}
	if !strings.Contains(updated.GetSourceFile(a.FileName()).Text(), "from \"./extra\"") ||
		!strings.Contains(updated.GetSourceFile(b.FileName()).Text(), "b = 2") {
		t.Fatal("new-module fallback did not expose every native overlay")
	}
	if updated.GetSourceFile(filepath.Join(dir, "src", "extra.ts")) == nil {
		t.Fatal("new-module fallback did not load extra.ts into the Program")
	}
	t.Logf("observable full_rebuilds=%d new_module_loaded=true", programCreations)
}

func sparseNativeProgram(t *testing.T, sources map[string]string) (string, *compiler.Program, []*ast.SourceFile) {
	t.Helper()
	dir := writeProject(t, "@scope/sparse-native-update", "")
	if err := os.Remove(filepath.Join(dir, "src", "main.ts")); err != nil {
		t.Fatal(err)
	}
	for name, text := range sources {
		if err := os.WriteFile(filepath.Join(dir, "src", name), []byte(text), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	_, program, diagnostics, err := newProjectProgram(dir, "")
	if err != nil {
		t.Fatalf("newProjectProgram: %v (%v)", err, diagnostics)
	}
	return dir, program, projectSourceFiles(program)
}

func remappedSource(sourceFiles []*ast.SourceFile, fileName string) *ast.SourceFile {
	for _, sourceFile := range sourceFiles {
		if sourceFile.FileName() == fileName {
			return sourceFile
		}
	}
	return nil
}
