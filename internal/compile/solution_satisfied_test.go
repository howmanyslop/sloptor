package compile

import (
	"bytes"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"testing"
	"time"
)

type failingSatisfiedValidationDrainer struct {
	*recordingSolutionDrainer
	invalid string
}

func (d *failingSatisfiedValidationDrainer) validateSatisfiedProject(project SolutionProject, _ *solutionCompileCache) *solutionSelectionError {
	if project.ConfigPath != d.invalid {
		return nil
	}
	return &solutionSelectionError{
		configPath: project.ConfigPath,
		code:       "SOLUTION_SATISFIED_OUTPUT_INVALID",
		message:    "restored output is invalid",
	}
}

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

func TestSatisfiedValidationFailureBlocksOnlyTransitiveDependants(t *testing.T) {
	root := t.TempDir()
	restoredDir := filepath.Join(root, "restored")
	dependentDir := filepath.Join(root, "dependent")
	successDir := filepath.Join(root, "success")
	failureDir := filepath.Join(root, "failure")
	writeSolutionConfig(t, root, "tsconfig.json", []string{"./dependent", "./success", "./failure"}, true)
	writeSolutionConfig(t, restoredDir, "tsconfig.json", nil, false)
	writeSolutionConfig(t, dependentDir, "tsconfig.json", []string{"../restored"}, false)
	writeSolutionConfig(t, successDir, "tsconfig.json", nil, false)
	writeSolutionConfig(t, failureDir, "tsconfig.json", nil, false)

	restoredConfig := filepath.Join(restoredDir, "tsconfig.json")
	dependentConfig := filepath.Join(dependentDir, "tsconfig.json")
	successConfig := filepath.Join(successDir, "tsconfig.json")
	failureConfig := filepath.Join(failureDir, "tsconfig.json")
	recorder := &recordingSolutionDrainer{fail: failureConfig}
	drainer := &failingSatisfiedValidationDrainer{recordingSolutionDrainer: recorder, invalid: restoredConfig}
	timings := NewBuildTimings()
	coordinator, err := NewSolutionCoordinatorWithDrainer(
		filepath.Join(root, "tsconfig.json"),
		ProjectOptions{Timings: timings},
		drainer,
	)
	if err != nil {
		t.Fatalf("NewSolutionCoordinatorWithDrainer: %v", err)
	}
	_, projects, _, err := coordinator.DrainWithProjectResultsForSelection(&SolutionProjectSelection{
		Selected:  []string{dependentConfig, successConfig, failureConfig},
		Satisfied: []string{restoredConfig},
	})
	if err == nil {
		t.Fatal("DrainWithProjectResultsForSelection unexpectedly succeeded")
	}

	results := solutionProjectResultsByConfig(projects)
	if len(results) != 4 {
		t.Fatalf("project results = %+v, want one terminal result per owned config", projects)
	}
	if got := results[restoredConfig]; got.Status != SolutionProjectFailed || len(got.Diagnostics) != 1 || got.Diagnostics[0].Code != "SOLUTION_SATISFIED_OUTPUT_INVALID" {
		t.Errorf("restored result = %+v, want owned validation failure", got)
	}
	if got := results[dependentConfig]; got.Status != SolutionProjectBlocked || !reflect.DeepEqual(got.Blockers, []string{restoredConfig}) || len(got.Diagnostics) != 0 {
		t.Errorf("dependent result = %+v, want blocked only by restored project", got)
	}
	if got := results[successConfig]; got.Status != SolutionProjectSuccess {
		t.Errorf("unrelated success result = %+v, want success", got)
	}
	if got := results[failureConfig]; got.Status != SolutionProjectFailed || len(got.Diagnostics) != 1 {
		t.Errorf("unrelated failure result = %+v, want normal compiler failure", got)
	}
	drained := append([]string(nil), recorder.drained...)
	slices.Sort(drained)
	if want := []string{"failure", "success"}; !reflect.DeepEqual(drained, want) {
		t.Errorf("drained projects = %v, want unrelated projects exactly once as %v", drained, want)
	}
	if got, want := []int{timings.Counts.SelectedProjects, timings.Counts.SatisfiedProjects, timings.Counts.ScheduledProjects}, []int{3, 1, 3}; !reflect.DeepEqual(got, want) {
		t.Errorf("selection telemetry = %v, want %v", got, want)
	}
}

func TestSolutionCoordinatorRejectsSelectedWriteRootContainingSatisfiedOutputsWithoutMutation(t *testing.T) {
	root := t.TempDir()
	appDir := filepath.Join(root, "app")
	sharedDir := filepath.Join(root, "shared")
	selectedOut := filepath.Join(appDir, "out")
	satisfiedOut := filepath.Join(selectedOut, "restored")
	writeSolutionConfig(t, root, "tsconfig.json", []string{"./app"}, true)
	writeMetadataSolutionProject(t, sharedDir, satisfiedOut)
	writeMetadataSolutionProject(t, appDir, selectedOut)
	if err := os.WriteFile(filepath.Join(appDir, "package.json"), []byte(`{"name":"@scope/app"}`), 0o644); err != nil {
		t.Fatal(err)
	}
	appConfigBytes, err := os.ReadFile(filepath.Join(appDir, "tsconfig.json"))
	if err != nil {
		t.Fatal(err)
	}
	appConfigText := strings.Replace(string(appConfigBytes), "{", `{"references":[{"path":"../shared"}],`, 1)
	if err := os.WriteFile(filepath.Join(appDir, "tsconfig.json"), []byte(appConfigText), 0o644); err != nil {
		t.Fatal(err)
	}

	rootConfig := filepath.Join(root, "tsconfig.json")
	appConfig := filepath.Join(appDir, "tsconfig.json")
	sharedConfig := filepath.Join(sharedDir, "tsconfig.json")
	graph, err := BuildSolutionGraph(rootConfig, ProjectOptions{})
	if err != nil {
		t.Fatalf("BuildSolutionGraph: %v", err)
	}
	_, metadata := populateCrossProjectMetadata(graph)
	protected := append([]string(nil), metadata.restoredDeclarations[sharedConfig]...)
	protected = append(protected, filepath.Join(satisfiedOut, "nested", "orphan.luau"))
	for index, path := range protected {
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatal(err)
		}
		content := []byte("export declare const restored: number;\n")
		if index == len(protected)-1 {
			content = []byte("-- restored orphan\n")
		}
		if err := os.WriteFile(path, content, 0o644); err != nil {
			t.Fatal(err)
		}
		if err := os.Chtimes(path, time.Unix(1_700_000_000, 0), time.Unix(1_700_000_000, 0)); err != nil {
			t.Fatal(err)
		}
	}
	type fileState struct {
		content []byte
		modTime time.Time
	}
	before := make(map[string]fileState, len(protected))
	for _, path := range protected {
		content, err := os.ReadFile(path)
		if err != nil {
			t.Fatal(err)
		}
		info, err := os.Stat(path)
		if err != nil {
			t.Fatal(err)
		}
		before[path] = fileState{content: content, modTime: info.ModTime()}
	}

	coordinator, err := NewSolutionCoordinator(rootConfig, ProjectOptions{})
	if err != nil {
		t.Fatalf("NewSolutionCoordinator: %v", err)
	}
	_, projects, _, err := coordinator.DrainWithProjectResultsForSelection(&SolutionProjectSelection{
		Selected:  []string{appConfig},
		Satisfied: []string{sharedConfig},
	})
	if err == nil || !strings.Contains(err.Error(), "overlapping write roots") {
		t.Errorf("selection error = %v, want overlapping write roots", err)
	}

	results := solutionProjectResultsByConfig(projects)
	if got := results[sharedConfig].Status; got != SolutionProjectSatisfied {
		t.Fatalf("satisfied project status = %q, want %q", got, SolutionProjectSatisfied)
	}
	selected := results[appConfig]
	if selected.Status != SolutionProjectFailed {
		t.Errorf("selected project status = %q, want %q", selected.Status, SolutionProjectFailed)
	}
	if len(selected.Diagnostics) != 1 || selected.Diagnostics[0].Code != "SOLUTION_SELECTION_WRITE_ROOT_OVERLAP" {
		t.Errorf("selected diagnostics = %+v, want write-root ownership diagnostic", selected.Diagnostics)
	}
	if selected.Timings.Counts.TransformedProjects != 0 || selected.Timings.Counts.EmittedProjects != 0 {
		t.Errorf("selected project performed work: %+v", selected.Timings.Counts)
	}
	for _, path := range protected {
		content, readErr := os.ReadFile(path)
		if readErr != nil {
			t.Fatalf("protected output %s: %v", path, readErr)
		}
		info, statErr := os.Stat(path)
		if statErr != nil {
			t.Fatalf("stat protected output %s: %v", path, statErr)
		}
		if !bytes.Equal(content, before[path].content) || !info.ModTime().Equal(before[path].modTime) {
			t.Errorf("protected output %s changed: bytes equal=%t mtime %v -> %v", path, bytes.Equal(content, before[path].content), before[path].modTime, info.ModTime())
		}
	}
}

func solutionProjectResultsByConfig(projects []SolutionProjectResult) map[string]SolutionProjectResult {
	results := make(map[string]SolutionProjectResult, len(projects))
	for _, project := range projects {
		results[project.ConfigPath] = project
	}
	return results
}

func TestInvalidOwnershipClaimsKeepEverySelectedProjectTerminal(t *testing.T) {
	t.Run("overlap", func(t *testing.T) {
		root := t.TempDir()
		firstDir := filepath.Join(root, "first")
		secondDir := filepath.Join(root, "second")
		writeSolutionConfig(t, root, "tsconfig.json", []string{"./first", "./second"}, true)
		writeSolutionConfig(t, firstDir, "tsconfig.json", nil, false)
		writeSolutionConfig(t, secondDir, "tsconfig.json", nil, false)
		firstConfig := filepath.Join(firstDir, "tsconfig.json")
		secondConfig := filepath.Join(secondDir, "tsconfig.json")
		drainer := &recordingSolutionDrainer{}
		coordinator, err := NewSolutionCoordinatorWithDrainer(filepath.Join(root, "tsconfig.json"), ProjectOptions{}, drainer)
		if err != nil {
			t.Fatal(err)
		}
		_, projects, _, err := coordinator.DrainWithProjectResultsForSelection(&SolutionProjectSelection{
			Selected:  []string{firstConfig, secondConfig},
			Satisfied: []string{filepath.Join(firstDir, ".", "tsconfig.json")},
		})
		if err == nil {
			t.Fatal("overlapping ownership unexpectedly succeeded")
		}
		results := solutionProjectResultsByConfig(projects)
		if len(results) != 2 || results[firstConfig].Status != SolutionProjectFailed || results[secondConfig].Status != SolutionProjectSuccess {
			t.Errorf("project results = %+v, want failed overlap and successful unrelated selected project", projects)
		}
		assertDrainedProjectNames(t, drainer, []string{"second"})
	})

	t.Run("unknown", func(t *testing.T) {
		root := t.TempDir()
		childDir := filepath.Join(root, "child")
		writeSolutionConfig(t, root, "tsconfig.json", []string{"./child"}, true)
		writeSolutionConfig(t, childDir, "tsconfig.json", nil, false)
		childConfig := filepath.Join(childDir, "tsconfig.json")
		unknownConfig := filepath.Join(root, "unknown", "tsconfig.json")
		drainer := &recordingSolutionDrainer{}
		coordinator, err := NewSolutionCoordinatorWithDrainer(filepath.Join(root, "tsconfig.json"), ProjectOptions{}, drainer)
		if err != nil {
			t.Fatal(err)
		}
		_, projects, _, err := coordinator.DrainWithProjectResultsForSelection(&SolutionProjectSelection{
			Selected:  []string{unknownConfig, childConfig},
			Satisfied: []string{},
		})
		if err == nil {
			t.Fatal("unknown ownership unexpectedly succeeded")
		}
		results := solutionProjectResultsByConfig(projects)
		if len(results) != 2 || results[unknownConfig].Status != SolutionProjectFailed || results[childConfig].Status != SolutionProjectSuccess {
			t.Errorf("project results = %+v, want failed unknown and successful valid selected project", projects)
		}
		assertDrainedProjectNames(t, drainer, []string{"child"})
	})

	t.Run("coordinator", func(t *testing.T) {
		root := t.TempDir()
		childDir := filepath.Join(root, "child")
		rootConfig := filepath.Join(root, "tsconfig.json")
		writeSolutionConfig(t, root, "tsconfig.json", []string{"./child"}, true)
		writeSolutionConfig(t, childDir, "tsconfig.json", nil, false)
		childConfig := filepath.Join(childDir, "tsconfig.json")
		drainer := &recordingSolutionDrainer{}
		coordinator, err := NewSolutionCoordinatorWithDrainer(rootConfig, ProjectOptions{}, drainer)
		if err != nil {
			t.Fatal(err)
		}
		_, projects, _, err := coordinator.DrainWithProjectResultsForSelection(&SolutionProjectSelection{
			Selected:  []string{rootConfig, childConfig},
			Satisfied: []string{},
		})
		if err == nil {
			t.Fatal("coordinator ownership unexpectedly succeeded")
		}
		results := solutionProjectResultsByConfig(projects)
		if len(results) != 2 || results[rootConfig].Status != SolutionProjectFailed || results[childConfig].Status != SolutionProjectSuccess {
			t.Errorf("project results = %+v, want failed coordinator claim and successful child", projects)
		}
		assertDrainedProjectNames(t, drainer, []string{"child"})
	})

	t.Run("missing dependency", func(t *testing.T) {
		root := t.TempDir()
		sharedDir := filepath.Join(root, "shared")
		appDir := filepath.Join(root, "app")
		dependentDir := filepath.Join(root, "dependent")
		unrelatedDir := filepath.Join(root, "unrelated")
		writeSolutionConfig(t, root, "tsconfig.json", []string{"./dependent", "./unrelated"}, true)
		writeSolutionConfig(t, sharedDir, "tsconfig.json", nil, false)
		writeSolutionConfig(t, appDir, "tsconfig.json", []string{"../shared"}, false)
		writeSolutionConfig(t, dependentDir, "tsconfig.json", []string{"../app"}, false)
		writeSolutionConfig(t, unrelatedDir, "tsconfig.json", nil, false)
		appConfig := filepath.Join(appDir, "tsconfig.json")
		dependentConfig := filepath.Join(dependentDir, "tsconfig.json")
		unrelatedConfig := filepath.Join(unrelatedDir, "tsconfig.json")
		drainer := &recordingSolutionDrainer{}
		coordinator, err := NewSolutionCoordinatorWithDrainer(filepath.Join(root, "tsconfig.json"), ProjectOptions{}, drainer)
		if err != nil {
			t.Fatal(err)
		}
		_, projects, _, err := coordinator.DrainWithProjectResultsForSelection(&SolutionProjectSelection{
			Selected:  []string{appConfig, dependentConfig, unrelatedConfig},
			Satisfied: []string{},
		})
		if err == nil {
			t.Fatal("missing dependency ownership unexpectedly succeeded")
		}
		results := solutionProjectResultsByConfig(projects)
		if len(results) != 3 || results[appConfig].Status != SolutionProjectFailed || results[dependentConfig].Status != SolutionProjectBlocked || results[unrelatedConfig].Status != SolutionProjectSuccess {
			t.Errorf("project results = %+v, want failed owner, blocked dependant, and successful unrelated project", projects)
		}
		if got := results[dependentConfig].Blockers; !reflect.DeepEqual(got, []string{appConfig}) {
			t.Errorf("dependent blockers = %v, want %v", got, []string{appConfig})
		}
		assertDrainedProjectNames(t, drainer, []string{"unrelated"})
	})
}

func assertDrainedProjectNames(t *testing.T, drainer *recordingSolutionDrainer, want []string) {
	t.Helper()
	drained := append([]string(nil), drainer.drained...)
	slices.Sort(drained)
	slices.Sort(want)
	if !reflect.DeepEqual(drained, want) {
		t.Errorf("drained projects = %v, want %v", drained, want)
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
