package compile

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"testing"
)

func TestCancelledMultiConfigBuildEndsWithCancellation(t *testing.T) {
	// Given: two real configs built together under an already-cancelled
	// context, the state a second project sees after the first is cancelled.
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	roots := make([]SolutionRoot, 0, 2)
	for _, name := range []string{"@scope/cancel-production", "@scope/cancel-tests"} {
		dir := writeProject(t, name, "")
		for _, file := range []string{"first.ts", "second.ts"} {
			if err := os.WriteFile(filepath.Join(dir, "src", file), []byte("export const value = 1;\n"), 0o644); err != nil {
				t.Fatal(err)
			}
		}
		roots = append(roots, SolutionRoot{ConfigPath: filepath.Join(dir, "tsconfig.json"), Options: ProjectOptions{Context: ctx}})
	}
	coordinator, err := NewSolutionCoordinatorForRootOptions(roots)
	if err != nil {
		t.Fatal(err)
	}

	// When: the coordinator drains both projects.
	_, projects, _, err := coordinator.DrainWithProjectResultsForSelection(nil)

	// Then: the build reports cancellation instead of panicking on a
	// checker that saw the cancelled context.
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("drain error = %v, want context.Canceled", err)
	}
	for _, project := range projects {
		if project.Status == SolutionProjectSuccess {
			t.Fatalf("project %s succeeded under a cancelled context", project.ConfigPath)
		}
	}
}
