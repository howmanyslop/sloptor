package compile

import (
	"context"
	"errors"
	"math"
	"os"
	"path/filepath"
	"sync"
	"sync/atomic"
	"testing"

	"rotor/tsgo/checker"
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

func TestGlobalDiagnosticsUnderCancelledContextLeavesCheckersUsable(t *testing.T) {
	// Given: a program whose checkers ran the precheck under a live context.
	dir := writeProject(t, "@scope/cancel-gate-three", "")
	for _, file := range []string{"first.ts", "second.ts"} {
		if err := os.WriteFile(filepath.Join(dir, "src", file), []byte("export const value = 1;\n"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	_, program, diags, err := newProjectProgram(dir, "")
	if err != nil {
		t.Fatalf("newProjectProgram: %v (%v)", err, diags)
	}
	ctx, cancel := context.WithCancel(context.Background())
	for _, sourceFile := range program.GetSourceFiles() {
		preEmitProjectFileDiagnosticsWithOptions(ctx, program, sourceFile, ProjectOptions{})
	}

	// When: the context is cancelled for the whole of gate 3.
	cancel()
	program.GetGlobalDiagnostics(ctx)

	// Then: no checker is marked cancelled, so the transform stage can
	// reuse every checker without the "previously cancelled" panic.
	program.ForEachCheckerParallel(func(_ int, c *checker.Checker) {
		if c.WasCanceled() {
			t.Error("gate 3 under a cancelled context marked a checker cancelled")
		}
	})
}

func TestMultiConfigBuildCancelledAtAnyPointEndsCleanly(t *testing.T) {
	// Given: the number of context checks a full two-config build makes.
	total := drainTwoConfigs(t, newCountdownContext(math.MaxInt64))
	if total == 0 {
		t.Fatal("the build never checked its context")
	}
	t.Logf("a full build checks its context %d times", total)

	// When: the context turns cancelled after each possible number of
	// checks, which lands the cancel inside the precheck loop, between
	// precheck and gate 3, and in every later stage.
	for after := int64(0); after <= total; after++ {
		ctx := newCountdownContext(after)
		drainTwoConfigs(t, ctx)
		// Then: drainTwoConfigs failed the test on a panic or on an error
		// that is not context.Canceled.
	}
}

// drainTwoConfigs builds two fresh configs under ctx and returns how many
// times the build checked ctx. Only success or context.Canceled may result.
func drainTwoConfigs(t *testing.T, ctx *countdownContext) int64 {
	t.Helper()
	roots := make([]SolutionRoot, 0, 2)
	for _, name := range []string{"@scope/countdown-production", "@scope/countdown-tests"} {
		dir := writeProject(t, name, "")
		for _, file := range []string{"first.ts", "second.ts", "third.ts"} {
			if err := os.WriteFile(filepath.Join(dir, "src", file), []byte("export function value(n: number) { return n + 1; }\n"), 0o644); err != nil {
				t.Fatal(err)
			}
		}
		roots = append(roots, SolutionRoot{ConfigPath: filepath.Join(dir, "tsconfig.json"), Options: ProjectOptions{Context: ctx}})
	}
	coordinator, err := NewSolutionCoordinatorForRootOptions(roots)
	if err != nil {
		t.Fatal(err)
	}
	_, _, _, err = coordinator.DrainWithProjectResultsForSelection(nil)
	if err != nil && !errors.Is(err, context.Canceled) {
		t.Fatalf("cancel after %d checks: drain error = %v, want nil or context.Canceled", ctx.after, err)
	}
	return ctx.calls.Load()
}

// countdownContext reports cancellation from its (after+1)th Err call on,
// so a test can place a cancel between any two context checks.
type countdownContext struct {
	context.Context
	after int64
	calls atomic.Int64
	once  sync.Once
	done  chan struct{}
}

func newCountdownContext(after int64) *countdownContext {
	return &countdownContext{Context: context.Background(), after: after, done: make(chan struct{})}
}

func (c *countdownContext) Done() <-chan struct{} { return c.done }

func (c *countdownContext) Err() error {
	if c.calls.Add(1) <= c.after {
		return nil
	}
	c.once.Do(func() { close(c.done) })
	return context.Canceled
}
