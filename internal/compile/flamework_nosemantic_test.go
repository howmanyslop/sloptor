package compile

import (
	"context"
	"os"
	"path/filepath"
	"testing"
)

func TestPreEmitProjectFileDiagnostics_skipsSemantic_whenNoSemanticDiagnosticsConfigured(t *testing.T) {
	// Given: a project source with a stable TypeScript semantic error.
	dir := writeProject(t, "@scope/no-semantic-preemit", "")
	if err := os.WriteFile(filepath.Join(dir, "src", "main.ts"), []byte("export const value: number = \"not-a-number\";\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	_, program, diags, err := newProjectProgram(dir, "")
	if err != nil {
		t.Fatalf("newProjectProgram: %v (%v)", err, diags)
	}
	sourceFile := program.GetSourceFile(filepath.Join(dir, "src", "main.ts"))
	if sourceFile == nil {
		t.Fatal("source missing")
	}
	ctx := context.Background()

	// When: pre-emit runs with semantic checks enabled.
	withSemantic := preEmitProjectFileDiagnosticsWithOptions(ctx, program, sourceFile, ProjectOptions{})
	// Then: the semantic assignment error is reported.
	if len(withSemantic) == 0 {
		t.Fatal("expected semantic diagnostics when SkipSemanticDiagnostics is false")
	}

	// When: pre-emit honors noSemanticDiagnostics.
	skipped := preEmitProjectFileDiagnosticsWithOptions(ctx, program, sourceFile, ProjectOptions{SkipSemanticDiagnostics: true})
	// Then: only syntactic diagnostics remain (none for this fixture).
	if len(skipped) != 0 {
		t.Fatalf("SkipSemanticDiagnostics pre-emit = %v, want no diagnostics", skipped)
	}
	t.Logf("observable semantic_diags=%d skipped_diags=%d", len(withSemantic), len(skipped))
}
