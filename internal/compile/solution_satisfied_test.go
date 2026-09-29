package compile

import (
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func TestSatisfiedDeclarationValidationReusesSolutionParseCache(t *testing.T) {
	declaration := filepath.Join(t.TempDir(), "restored.d.ts")
	if err := os.WriteFile(declaration, []byte("export declare const restored: number;\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	project := SolutionProject{ConfigPath: filepath.Join(filepath.Dir(declaration), "tsconfig.json")}
	drainer := &solutionBuildDrainer{restoredDeclarations: map[string][]string{project.ConfigPath: {declaration}}}
	cache := newSolutionCompileCache()
	for attempt := 1; attempt <= 2; attempt++ {
		if selectionErr := drainer.validateSatisfiedProject(project, cache); selectionErr != nil {
			t.Fatalf("validation %d: %s", attempt, selectionErr.message)
		}
	}
	if cache.misses.Load() != 1 || cache.hits.Load() != 1 {
		t.Fatalf("parse cache hits=%d misses=%d, want one of each", cache.hits.Load(), cache.misses.Load())
	}
}

func TestSolutionCoordinatorBuildsSelectedProjectFromSatisfiedDependency(t *testing.T) {
	root := t.TempDir()
	appDir := filepath.Join(root, "app")
	sharedDir := filepath.Join(root, "shared")
	writeSolutionConfig(t, root, "tsconfig.json", []string{"./app"}, true)
	writeSolutionConfig(t, appDir, "tsconfig.json", []string{"../shared"}, false)
	writeSolutionConfig(t, sharedDir, "tsconfig.json", nil, false)
	drainer := &recordingSolutionDrainer{}

	coordinator, err := NewSolutionCoordinatorWithDrainer(
		filepath.Join(root, "tsconfig.json"),
		ProjectOptions{},
		drainer,
	)
	if err != nil {
		t.Fatalf("NewSolutionCoordinatorWithDrainer: %v", err)
	}
	_, projects, _, err := coordinator.DrainWithProjectResultsForSelection(&SolutionProjectSelection{
		Selected:  []string{filepath.Join(appDir, "..", "app", "tsconfig.json")},
		Satisfied: []string{filepath.Join(sharedDir, "tsconfig.json")},
	})
	if err != nil {
		t.Fatalf("DrainWithProjectResultsForSelection: %v", err)
	}

	if got, want := drainer.drained, []string{"app"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("drained projects = %v, want %v", got, want)
	}
	if got, want := []SolutionProjectStatus{projects[0].Status, projects[1].Status}, []SolutionProjectStatus{SolutionProjectSatisfied, SolutionProjectSuccess}; !reflect.DeepEqual(got, want) {
		t.Fatalf("project statuses = %v, want %v", got, want)
	}
	if projects[0].Timings.Counts.ScheduledProjects != 0 || projects[0].Timings.Counts.TransformedProjects != 0 || projects[0].Timings.Counts.EmittedProjects != 0 {
		t.Fatalf("satisfied project performed work: %+v", projects[0].Timings.Counts)
	}
}

func TestSolutionCoordinatorTelemetrySeparatesSelectedAndSatisfiedProjects(t *testing.T) {
	root := t.TempDir()
	appDir := filepath.Join(root, "app")
	sharedDir := filepath.Join(root, "shared")
	writeSolutionConfig(t, root, "tsconfig.json", []string{"./app"}, true)
	writeSolutionConfig(t, appDir, "tsconfig.json", []string{"../shared"}, false)
	writeSolutionConfig(t, sharedDir, "tsconfig.json", nil, false)
	timings := NewBuildTimings()
	coordinator, err := NewSolutionCoordinatorWithDrainer(
		filepath.Join(root, "tsconfig.json"),
		ProjectOptions{Timings: timings},
		&recordingSolutionDrainer{},
	)
	if err != nil {
		t.Fatalf("NewSolutionCoordinatorWithDrainer: %v", err)
	}
	_, projects, _, err := coordinator.DrainWithProjectResultsForSelection(&SolutionProjectSelection{
		Selected:  []string{filepath.Join(appDir, "tsconfig.json")},
		Satisfied: []string{filepath.Join(sharedDir, "tsconfig.json")},
	})
	if err != nil {
		t.Fatalf("DrainWithProjectResultsForSelection: %v", err)
	}

	if got, want := []int{timings.Counts.SelectedProjects, timings.Counts.SatisfiedProjects, timings.Counts.ScheduledProjects}, []int{1, 1, 1}; !reflect.DeepEqual(got, want) {
		t.Fatalf("selected/satisfied/scheduled telemetry = %v, want %v", got, want)
	}
	if got := projects[0].Timings.Counts; got.SelectedProjects != 0 || got.SatisfiedProjects != 1 || got.ScheduledProjects != 0 || got.TransformedProjects != 0 || got.EmittedProjects != 0 {
		t.Fatalf("satisfied project telemetry = %+v, want one satisfied and zero work", got)
	}
	if got := projects[1].Timings.Counts; got.SelectedProjects != 1 || got.SatisfiedProjects != 0 || got.ScheduledProjects != 1 {
		t.Fatalf("selected project telemetry = %+v, want one selected and scheduled", got)
	}
}

func TestSolutionCoordinatorKeepsSelectedDependencyBarriers(t *testing.T) {
	root := t.TempDir()
	appDir := filepath.Join(root, "app")
	sharedDir := filepath.Join(root, "shared")
	writeSolutionConfig(t, root, "tsconfig.json", []string{"./app"}, true)
	writeSolutionConfig(t, appDir, "tsconfig.json", []string{"../shared"}, false)
	writeSolutionConfig(t, sharedDir, "tsconfig.json", nil, false)
	sharedConfig := filepath.Join(sharedDir, "tsconfig.json")
	drainer := &recordingSolutionDrainer{fail: sharedConfig}
	coordinator, err := NewSolutionCoordinatorWithDrainer(
		filepath.Join(root, "tsconfig.json"),
		ProjectOptions{},
		drainer,
	)
	if err != nil {
		t.Fatalf("NewSolutionCoordinatorWithDrainer: %v", err)
	}
	_, projects, _, err := coordinator.DrainWithProjectResultsForSelection(&SolutionProjectSelection{
		Selected:  []string{sharedConfig, filepath.Join(appDir, "tsconfig.json")},
		Satisfied: []string{},
	})
	if err == nil {
		t.Fatal("DrainWithProjectResultsForSelection unexpectedly succeeded")
	}
	if got, want := drainer.drained, []string{"shared"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("drained projects = %v, want %v", got, want)
	}
	if got, want := []SolutionProjectStatus{projects[0].Status, projects[1].Status}, []SolutionProjectStatus{SolutionProjectFailed, SolutionProjectBlocked}; !reflect.DeepEqual(got, want) {
		t.Fatalf("project statuses = %v, want %v", got, want)
	}
	if got, want := projects[1].Blockers, []string{sharedConfig}; !reflect.DeepEqual(got, want) {
		t.Fatalf("app blockers = %v, want %v", got, want)
	}
}

func TestSolutionCoordinatorRejectsOverlappingOwnershipBeforeScheduling(t *testing.T) {
	root := t.TempDir()
	writeSolutionConfig(t, root, "tsconfig.json", nil, false)
	drainer := &recordingSolutionDrainer{}
	coordinator, err := NewSolutionCoordinatorWithDrainer(
		filepath.Join(root, "tsconfig.json"),
		ProjectOptions{},
		drainer,
	)
	if err != nil {
		t.Fatalf("NewSolutionCoordinatorWithDrainer: %v", err)
	}
	config := filepath.Join(root, "tsconfig.json")
	_, projects, _, err := coordinator.DrainWithProjectResultsForSelection(&SolutionProjectSelection{
		Selected:  []string{config},
		Satisfied: []string{filepath.Join(root, ".", "tsconfig.json")},
	})
	if err == nil || !strings.Contains(err.Error(), "both selected and satisfied") {
		t.Fatalf("selection error = %v, want overlapping ownership", err)
	}
	if len(drainer.drained) != 0 {
		t.Fatalf("drained projects = %v, want none", drainer.drained)
	}
	if len(projects) != 1 || projects[0].ConfigPath != config || projects[0].Status != SolutionProjectFailed {
		t.Fatalf("project result = %+v, want one failed result owned by %s", projects, config)
	}
	if len(projects[0].Diagnostics) != 1 || projects[0].Diagnostics[0].Code != "SOLUTION_SELECTION_OVERLAP" {
		t.Fatalf("project diagnostics = %+v, want structured overlap diagnostic", projects[0].Diagnostics)
	}
}

func TestSolutionCoordinatorRejectsUnknownOwnershipBeforeScheduling(t *testing.T) {
	root := t.TempDir()
	writeSolutionConfig(t, root, "tsconfig.json", nil, false)
	drainer := &recordingSolutionDrainer{}
	coordinator, err := NewSolutionCoordinatorWithDrainer(
		filepath.Join(root, "tsconfig.json"),
		ProjectOptions{},
		drainer,
	)
	if err != nil {
		t.Fatalf("NewSolutionCoordinatorWithDrainer: %v", err)
	}
	unknown := filepath.Join(root, "unknown", "tsconfig.json")
	_, projects, _, err := coordinator.DrainWithProjectResultsForSelection(&SolutionProjectSelection{
		Selected: []string{unknown},
	})
	if err == nil || !strings.Contains(err.Error(), "not part of the discovered solution graph") {
		t.Fatalf("selection error = %v, want unknown config", err)
	}
	if len(drainer.drained) != 0 {
		t.Fatalf("drained projects = %v, want none", drainer.drained)
	}
	if len(projects) != 1 || projects[0].ConfigPath != unknown || projects[0].Status != SolutionProjectFailed {
		t.Fatalf("project result = %+v, want one failed result owned by %s", projects, unknown)
	}
	if len(projects[0].Diagnostics) != 1 || projects[0].Diagnostics[0].Code != "SOLUTION_SELECTION_UNKNOWN" {
		t.Fatalf("project diagnostics = %+v, want structured unknown diagnostic", projects[0].Diagnostics)
	}
}

func TestSolutionCoordinatorRejectsCoordinatorOnlyOwnershipBeforeScheduling(t *testing.T) {
	root := t.TempDir()
	writeSolutionConfig(t, root, "tsconfig.json", []string{"./child"}, true)
	writeSolutionConfig(t, filepath.Join(root, "child"), "tsconfig.json", nil, false)
	drainer := &recordingSolutionDrainer{}
	coordinator, err := NewSolutionCoordinatorWithDrainer(
		filepath.Join(root, "tsconfig.json"),
		ProjectOptions{},
		drainer,
	)
	if err != nil {
		t.Fatalf("NewSolutionCoordinatorWithDrainer: %v", err)
	}
	config := filepath.Join(root, "tsconfig.json")
	_, projects, _, err := coordinator.DrainWithProjectResultsForSelection(&SolutionProjectSelection{
		Selected: []string{config},
	})
	if err == nil || !strings.Contains(err.Error(), "coordinator-only") {
		t.Fatalf("selection error = %v, want coordinator-only claim", err)
	}
	if len(drainer.drained) != 0 {
		t.Fatalf("drained projects = %v, want none", drainer.drained)
	}
	if len(projects) != 1 || projects[0].ConfigPath != config || projects[0].Status != SolutionProjectFailed {
		t.Fatalf("project result = %+v, want one failed result owned by %s", projects, config)
	}
	if len(projects[0].Diagnostics) != 1 || projects[0].Diagnostics[0].Code != "SOLUTION_SELECTION_COORDINATOR" {
		t.Fatalf("project diagnostics = %+v, want structured coordinator diagnostic", projects[0].Diagnostics)
	}
}

func TestSolutionCoordinatorRejectsMissingDependencyOwnershipBeforeScheduling(t *testing.T) {
	root := t.TempDir()
	appDir := filepath.Join(root, "app")
	sharedDir := filepath.Join(root, "shared")
	writeSolutionConfig(t, root, "tsconfig.json", []string{"./app"}, true)
	writeSolutionConfig(t, appDir, "tsconfig.json", []string{"../shared"}, false)
	writeSolutionConfig(t, sharedDir, "tsconfig.json", nil, false)
	drainer := &recordingSolutionDrainer{}
	coordinator, err := NewSolutionCoordinatorWithDrainer(
		filepath.Join(root, "tsconfig.json"),
		ProjectOptions{},
		drainer,
	)
	if err != nil {
		t.Fatalf("NewSolutionCoordinatorWithDrainer: %v", err)
	}
	appConfig := filepath.Join(appDir, "tsconfig.json")
	sharedConfig := filepath.Join(sharedDir, "tsconfig.json")
	_, projects, _, err := coordinator.DrainWithProjectResultsForSelection(&SolutionProjectSelection{
		Selected: []string{appConfig},
	})
	if err == nil || !strings.Contains(err.Error(), sharedConfig) || !strings.Contains(err.Error(), "has no owner") {
		t.Fatalf("selection error = %v, want missing dependency ownership for %s", err, sharedConfig)
	}
	if len(drainer.drained) != 0 {
		t.Fatalf("drained projects = %v, want none", drainer.drained)
	}
	if len(projects) != 1 || projects[0].ConfigPath != appConfig || projects[0].Status != SolutionProjectFailed {
		t.Fatalf("project result = %+v, want one failed result owned by %s", projects, appConfig)
	}
	if len(projects[0].Diagnostics) != 1 || projects[0].Diagnostics[0].Code != "SOLUTION_SELECTION_MISSING_DEPENDENCY" {
		t.Fatalf("project diagnostics = %+v, want structured missing-dependency diagnostic", projects[0].Diagnostics)
	}
}

func TestSolutionCoordinatorRejectsEmptyOwnershipClaimBeforeScheduling(t *testing.T) {
	root := t.TempDir()
	writeSolutionConfig(t, root, "tsconfig.json", nil, false)
	drainer := &recordingSolutionDrainer{}
	coordinator, err := NewSolutionCoordinatorWithDrainer(
		filepath.Join(root, "tsconfig.json"),
		ProjectOptions{},
		drainer,
	)
	if err != nil {
		t.Fatalf("NewSolutionCoordinatorWithDrainer: %v", err)
	}
	_, projects, _, err := coordinator.DrainWithProjectResultsForSelection(&SolutionProjectSelection{
		Satisfied: []string{""},
	})
	if err == nil || !strings.Contains(err.Error(), "non-empty path") {
		t.Fatalf("selection error = %v, want invalid empty claim", err)
	}
	if len(drainer.drained) != 0 {
		t.Fatalf("drained projects = %v, want none", drainer.drained)
	}
	if len(projects) != 1 || projects[0].Status != SolutionProjectFailed {
		t.Fatalf("project result = %+v, want one failed result", projects)
	}
	if len(projects[0].Diagnostics) != 1 || projects[0].Diagnostics[0].Code != "SOLUTION_SELECTION_INVALID_PATH" {
		t.Fatalf("project diagnostics = %+v, want structured invalid-path diagnostic", projects[0].Diagnostics)
	}
}
