package compile

import (
	"context"
	"os"
	"path/filepath"
	"strings"
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

func TestSkipSemanticDiagnosticsEmitsUnresolvedIdentifierInsteadOfICE(t *testing.T) {
	// Given: noSemanticDiagnostics skipped the TS2304 that would have aborted
	// transform, so an unresolved identifier reaches the roblox-ts emitter.
	dir := writeProject(t, "@scope/no-semantic-identifier", "")
	if err := os.WriteFile(filepath.Join(dir, "src", "main.ts"), []byte("export const x = neverDeclared;\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	// When: the real compile path honors SkipSemanticDiagnostics.
	result, diags, err := BuildProjectWithOptions(dir, ProjectOptions{SkipSemanticDiagnostics: true})

	// Then: emit the identifier text instead of the upstream symbol assert.
	if err != nil || len(diags) != 0 {
		t.Fatalf("BuildProjectWithOptions = (%v, %v)", diags, err)
	}
	got := result.Outputs["out/main.luau"]
	if !strings.Contains(got, "neverDeclared") {
		t.Fatalf("output missing unresolved identifier:\n%s", got)
	}
}

func TestFlameworkNoSemanticDiagnosticsEmitsUnresolvedIdentifierInsteadOfICE(t *testing.T) {
	// Given: native Flamework is on and the profile disables semantic diagnostics.
	dir := task7FlameworkProject(t, "[flamework]\nnoSemanticDiagnostics = true\n")
	if err := os.WriteFile(filepath.Join(dir, "src", "main.ts"), []byte("export const x = neverDeclared;\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	// When: the build reads noSemanticDiagnostics from rotor.toml.
	result, diags, err := BuildProjectWithOptions(dir, ProjectOptions{})

	// Then: the Flamework flag reaches the transformer and does not ICE.
	if err != nil || len(diags) != 0 {
		t.Fatalf("BuildProjectWithOptions = (%v, %v)", diags, err)
	}
	if !strings.Contains(result.Outputs["out/main.luau"], "neverDeclared") {
		t.Fatalf("output missing unresolved identifier:\n%s", result.Outputs["out/main.luau"])
	}
}
