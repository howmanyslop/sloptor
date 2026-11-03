package compile

import (
	"context"
	"os"
	"path/filepath"
	"testing"

	"rotor/internal/transformer"
	"rotor/tsgo/core"
)

// Mirrors the native transform-render stage of compileProjectSourceFiles.
// reused_checker repeats the stage on warm checkers, so it measures rotor-owned
// work only. fresh_program rebuilds and prechecks a Program with the timer
// stopped, so it also pays the checker work that diagnostics left uncached, as
// one real build does.
func BenchmarkTransformRender(b *testing.B) {
	dir := filepath.Join("..", "..", "testdata", "conformance", "project")
	if _, err := os.Stat(filepath.Join(dir, "node_modules", "@rbxts", "compiler-types")); err != nil {
		b.Skipf("fixture dependencies missing, run `bun install --no-save` in %s: %v", dir, err)
	}
	b.Run("reused_checker", func(b *testing.B) {
		run := prepareTransformRender(b, dir)
		run(b)
		b.ReportAllocs()
		for b.Loop() {
			run(b)
		}
	})
	b.Run("fresh_program", func(b *testing.B) {
		b.ReportAllocs()
		for b.Loop() {
			b.StopTimer()
			run := prepareTransformRender(b, dir)
			b.StartTimer()
			run(b)
		}
	})
}

func prepareTransformRender(b *testing.B, projectDir string) func(*testing.B) {
	b.Helper()
	opts := ProjectOptions{AllowCommentDirectives: true}
	dir, program, diags, err := newProjectProgram(projectDir, "")
	if err != nil || len(diags) != 0 {
		b.Fatalf("newProjectProgram: %v (diagnostics: %v)", err, diags)
	}
	pctx, diags, err := newProjectContext(dir, program, opts)
	if err != nil {
		b.Fatalf("newProjectContext: %v (diagnostics: %v)", err, diags)
	}
	ctx := context.Background()
	sourceFiles := projectSourceFiles(program)
	for _, sourceFile := range sourceFiles {
		precheck := precheckProjectSourceFile(ctx, program, sourceFile, opts)
		if len(precheck.tsDiags) != 0 || len(precheck.commentDiags) != 0 {
			b.Fatalf("%s: precheck diagnostics: %v %v", sourceFile.FileName(), precheck.tsDiags, precheck.commentDiags)
		}
	}
	groups := groupSourceFilesByChecker(ctx, program, sourceFiles)
	labels := compileProjectProgressLabels(sourceFiles)
	return func(b *testing.B) {
		results := make([]compiledProjectSourceFile, len(sourceFiles))
		wg := core.NewWorkGroup(program.SingleThreaded() || len(groups) <= 1)
		for _, group := range groups {
			wg.Queue(func() {
				multi := transformer.NewMultiState()
				for i, sourceFile := range group.files {
					index := group.indices[i]
					results[index] = compileProjectSourceFile(ctx, dir, program, pctx, sourceFile, opts, multi, labels[index])
				}
			})
		}
		wg.RunAndWait()
		for i, result := range results {
			if result.err != nil {
				b.Fatalf("%s: %v (diagnostics: %v)", sourceFiles[i].FileName(), result.err, result.diags)
			}
		}
	}
}
