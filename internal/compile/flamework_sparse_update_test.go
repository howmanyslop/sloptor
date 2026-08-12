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

func TestNativeFlameworkSparseUpdate_fallsBackOnceWithEveryOverlay_whenModuleShapeChanges(t *testing.T) {
	// Given: the first source changes its import boundary and a later source also has native output.
	dir, program, sourceFiles := sparseNativeProgram(t, map[string]string{
		"a.ts":   "import { value } from \"./dep\"; export const a = value;\n",
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

	// When: reverse-ordered input is normalized to source order and the incompatible update rebuilds.
	updated, _, _, err := updateNativeFlameworkProgram(nativeProgramUpdate{
		program: program, sourceFiles: sourceFiles,
		overlays: []nativeSourceOverlay{
			{fileName: b.FileName(), text: "export const b = 2;\n"},
			{fileName: a.FileName(), text: "export const a = 2;\n"},
		},
	})
	// Then: the one full rebuild contains all overlays and retains no old parse tree.
	if err != nil {
		t.Fatal(err)
	}
	if programCreations != 1 {
		t.Fatalf("module-boundary update created %d Programs, want one full fallback", programCreations)
	}
	if updated.GetSourceFile(a.FileName()) == a || updated.GetSourceFile(b.FileName()) == b || updated.GetSourceFile(dep.FileName()) == dep {
		t.Fatal("module-boundary fallback retained an incoming parse tree")
	}
	if !strings.Contains(updated.GetSourceFile(a.FileName()).Text(), "a = 2") || !strings.Contains(updated.GetSourceFile(b.FileName()).Text(), "b = 2") {
		t.Fatal("module-boundary fallback did not expose every native overlay")
	}
	t.Logf("observable full_rebuilds=%d native_overlays_present=2 incoming_parse_trees_retained=0", programCreations)
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
