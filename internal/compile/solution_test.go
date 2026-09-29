package compile

import (
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"sync"
	"testing"
)

type recordingSolutionDrainer struct {
	mu      sync.Mutex
	drained []string
	fail    string
}

func (d *recordingSolutionDrainer) Drain(project SolutionProject) (*BuildResult, []string, error) {
	d.mu.Lock()
	d.drained = append(d.drained, filepath.Base(filepath.Dir(project.ConfigPath)))
	fail := project.ConfigPath == d.fail
	d.mu.Unlock()
	if fail {
		return &BuildResult{Diagnostics: []DiagnosticInfo{{Message: "failed project"}}}, []string{"failed project"}, errors.New("failed project")
	}
	return &BuildResult{Outputs: map[string]string{}}, nil, nil
}

func TestSolutionGraph(t *testing.T) {
	root := t.TempDir()
	writeSolutionConfig(t, root, "tsconfig.json", []string{"./app"}, true)
	writeSolutionConfig(t, filepath.Join(root, "app"), "tsconfig.json", []string{"../left", "../right"}, false)
	writeSolutionConfig(t, filepath.Join(root, "left"), "tsconfig.json", []string{"../shared"}, false)
	writeSolutionConfig(t, filepath.Join(root, "right"), "tsconfig.json", []string{"../shared"}, false)
	writeSolutionConfig(t, filepath.Join(root, "shared"), "tsconfig.json", nil, false)

	graph, err := BuildSolutionGraph(filepath.Join(root, "tsconfig.json"), ProjectOptions{})
	if err != nil {
		t.Fatalf("BuildSolutionGraph: %v", err)
	}

	got := solutionProjectNames(graph.Projects)
	want := []string{"shared", "left", "right", "app"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("project order = %v, want %v", got, want)
	}
}

func TestSolutionGraphForRootsBuildsCanonicalUnion(t *testing.T) {
	root := t.TempDir()
	productionDir := filepath.Join(root, "production")
	testsDir := filepath.Join(root, "tests")
	sharedDir := filepath.Join(root, "shared")
	writeSolutionConfig(t, productionDir, "tsconfig.json", []string{"../shared"}, false)
	writeSolutionConfig(t, testsDir, "tsconfig.json", []string{"../shared"}, false)
	writeSolutionConfig(t, sharedDir, "tsconfig.json", nil, false)

	productionConfig := filepath.Join(productionDir, "tsconfig.json")
	testsConfig := filepath.Join(testsDir, "tsconfig.json")
	graph, err := BuildSolutionGraphForRoots([]string{
		productionConfig,
		filepath.Join(productionDir, "..", "production", "tsconfig.json"),
		testsConfig,
	}, ProjectOptions{})
	if err != nil {
		t.Fatalf("BuildSolutionGraphForRoots: %v", err)
	}

	if got, want := solutionProjectNames(graph.Projects), []string{"shared", "production", "tests"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("project order = %v, want %v", got, want)
	}
	for _, project := range graph.Projects {
		if !filepath.IsAbs(project.ConfigPath) || project.ConfigPath != filepath.Clean(project.ConfigPath) {
			t.Fatalf("config path %q is not canonical", project.ConfigPath)
		}
	}
	sharedConfig, _, err := canonicalSolutionConfigPath(filepath.Join(sharedDir, "tsconfig.json"))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := graph.Projects[1].References, []string{sharedConfig}; !reflect.DeepEqual(got, want) {
		t.Fatalf("production references = %v, want %v", got, want)
	}
	if got, want := graph.Projects[2].References, []string{sharedConfig}; !reflect.DeepEqual(got, want) {
		t.Fatalf("tests references = %v, want %v", got, want)
	}
}

func TestSolutionGraphForRootsDeduplicatesSymlinkAlias(t *testing.T) {
	root := t.TempDir()
	projectDir := filepath.Join(root, "project")
	aliasDir := filepath.Join(root, "project-alias")
	writeSolutionConfig(t, projectDir, "tsconfig.json", nil, false)
	if err := os.Symlink(projectDir, aliasDir); err != nil {
		t.Skipf("symlinks unavailable: %v", err)
	}

	graph, err := BuildSolutionGraphForRoots([]string{
		filepath.Join(projectDir, "tsconfig.json"),
		filepath.Join(aliasDir, "tsconfig.json"),
	}, ProjectOptions{})
	if err != nil {
		t.Fatalf("BuildSolutionGraphForRoots: %v", err)
	}
	if got, want := len(graph.Projects), 1; got != want {
		t.Fatalf("project count = %d, want %d", got, want)
	}
	if got, want := graph.Projects[0].ConfigPath, filepath.Join(projectDir, "tsconfig.json"); got != want {
		t.Fatalf("canonical config = %q, want %q", got, want)
	}
}

func TestSolutionGraphForRootsRejectsConflictingEffectiveOptions(t *testing.T) {
	root := t.TempDir()
	firstDir := filepath.Join(root, "first")
	secondDir := filepath.Join(root, "second")
	sharedDir := filepath.Join(root, "shared")
	writeSolutionConfig(t, firstDir, "tsconfig.json", []string{"../shared"}, true)
	writeSolutionConfig(t, secondDir, "tsconfig.json", []string{"../shared"}, true)
	writeSolutionConfig(t, sharedDir, "tsconfig.json", nil, false)
	argv := &RbxtsOptions{}

	_, err := NewSolutionCoordinatorForRootOptions([]SolutionRoot{
		{ConfigPath: filepath.Join(firstDir, "tsconfig.json"), Options: ProjectOptions{Type: "game", SolutionArgv: argv}},
		{ConfigPath: filepath.Join(secondDir, "tsconfig.json"), Options: ProjectOptions{Type: "model", SolutionArgv: argv}},
	})
	if err == nil || !strings.Contains(err.Error(), "conflicting effective options") {
		t.Fatalf("NewSolutionCoordinatorForRootOptions error = %v, want conflicting effective options", err)
	}
}

func TestSolutionCoordinator(t *testing.T) {
	root := t.TempDir()
	writeSolutionConfig(t, root, "tsconfig.json", []string{"./left", "./right"}, true)
	writeSolutionConfig(t, filepath.Join(root, "left"), "tsconfig.json", []string{"../shared"}, false)
	writeSolutionConfig(t, filepath.Join(root, "right"), "tsconfig.json", []string{"../shared"}, false)
	writeSolutionConfig(t, filepath.Join(root, "shared"), "tsconfig.json", nil, false)
	drainer := &recordingSolutionDrainer{}

	builders := 1
	coordinator, err := NewSolutionCoordinatorWithDrainer(filepath.Join(root, "tsconfig.json"), ProjectOptions{Builders: &builders}, drainer)
	if err != nil {
		t.Fatalf("NewSolutionCoordinatorWithDrainer: %v", err)
	}
	_, _, err = coordinator.Drain()
	if err != nil {
		t.Fatalf("Drain: %v", err)
	}

	want := []string{"shared", "left", "right"}
	if !reflect.DeepEqual(drainer.drained, want) {
		t.Fatalf("drained projects = %v, want %v", drainer.drained, want)
	}
	sharedConfig := filepath.Join(root, "shared", "tsconfig.json")
	state, ok := coordinator.ProjectState(sharedConfig)
	if !ok || !state.UpToDate {
		t.Fatalf("shared state = %+v, found = %t, want up-to-date", state, ok)
	}
}

func TestSolutionCoordinatorBlocksDependentAfterFailure(t *testing.T) {
	root := t.TempDir()
	writeSolutionConfig(t, root, "tsconfig.json", []string{"./app"}, true)
	writeSolutionConfig(t, filepath.Join(root, "app"), "tsconfig.json", []string{"../broken"}, false)
	writeSolutionConfig(t, filepath.Join(root, "broken"), "tsconfig.json", nil, false)
	brokenConfig := filepath.Join(root, "broken", "tsconfig.json")
	drainer := &recordingSolutionDrainer{fail: brokenConfig}

	builders := 1
	coordinator, err := NewSolutionCoordinatorWithDrainer(filepath.Join(root, "tsconfig.json"), ProjectOptions{Builders: &builders}, drainer)
	if err != nil {
		t.Fatalf("NewSolutionCoordinatorWithDrainer: %v", err)
	}
	_, _, err = coordinator.Drain()
	if err == nil {
		t.Fatal("Drain unexpectedly succeeded")
	}

	if want := []string{"broken"}; !reflect.DeepEqual(drainer.drained, want) {
		t.Fatalf("drained projects = %v, want %v", drainer.drained, want)
	}
	state, ok := coordinator.ProjectState(filepath.Join(root, "app", "tsconfig.json"))
	if !ok || state.BlockedBy != brokenConfig {
		t.Fatalf("app state = %+v, found = %t, want blocked by %s", state, ok, brokenConfig)
	}

	drainer.fail = ""
	_, _, err = coordinator.Drain()
	if err != nil {
		t.Fatalf("Drain after dependency recovery: %v", err)
	}
	if want := []string{"broken", "broken", "app"}; !reflect.DeepEqual(drainer.drained, want) {
		t.Fatalf("drained projects after recovery = %v, want %v", drainer.drained, want)
	}
}

func TestSolutionCoordinatorReturnsOneOwnedOutcomePerUnionProject(t *testing.T) {
	root := t.TempDir()
	productionDir := filepath.Join(root, "production")
	testsDir := filepath.Join(root, "tests")
	sharedDir := filepath.Join(root, "shared")
	writeSolutionConfig(t, productionDir, "tsconfig.json", []string{"../shared"}, false)
	writeSolutionConfig(t, testsDir, "tsconfig.json", []string{"../shared"}, false)
	writeSolutionConfig(t, sharedDir, "tsconfig.json", nil, false)
	sharedConfig, _, err := canonicalSolutionConfigPath(filepath.Join(sharedDir, "tsconfig.json"))
	if err != nil {
		t.Fatal(err)
	}
	drainer := &recordingSolutionDrainer{fail: sharedConfig}

	coordinator, err := NewSolutionCoordinatorForRootsWithDrainer(
		[]string{filepath.Join(productionDir, "tsconfig.json"), filepath.Join(testsDir, "tsconfig.json")},
		ProjectOptions{},
		drainer,
	)
	if err != nil {
		t.Fatalf("NewSolutionCoordinatorForRootsWithDrainer: %v", err)
	}
	_, projects, _, drainErr := coordinator.DrainWithProjectResults()
	if drainErr == nil {
		t.Fatal("DrainWithProjectResults unexpectedly succeeded")
	}
	if len(projects) != 3 {
		t.Fatalf("project outcomes = %d, want 3", len(projects))
	}
	if got, want := []SolutionProjectStatus{projects[0].Status, projects[1].Status, projects[2].Status}, []SolutionProjectStatus{SolutionProjectFailed, SolutionProjectBlocked, SolutionProjectBlocked}; !reflect.DeepEqual(got, want) {
		t.Fatalf("project statuses = %v, want %v", got, want)
	}
	if got := len(projects[0].Diagnostics); got != 1 {
		t.Fatalf("failed project diagnostics = %d, want 1", got)
	}
	for _, project := range projects[1:] {
		if len(project.Diagnostics) != 0 {
			t.Fatalf("blocked project diagnostics = %v, want none", project.Diagnostics)
		}
		if got, want := project.Blockers, []string{sharedConfig}; !reflect.DeepEqual(got, want) {
			t.Fatalf("project blockers = %v, want %v", got, want)
		}
	}
	if got, want := drainer.drained, []string{"shared"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("drained projects = %v, want %v", got, want)
	}
}

func TestSolutionCoordinatorReportsNoChangeForUpToDateProject(t *testing.T) {
	root := t.TempDir()
	writeSolutionConfig(t, root, "tsconfig.json", nil, false)
	drainer := &recordingSolutionDrainer{}
	coordinator, err := NewSolutionCoordinatorWithDrainer(filepath.Join(root, "tsconfig.json"), ProjectOptions{}, drainer)
	if err != nil {
		t.Fatalf("NewSolutionCoordinatorWithDrainer: %v", err)
	}
	_, first, _, err := coordinator.DrainWithProjectResults()
	if err != nil {
		t.Fatalf("first DrainWithProjectResults: %v", err)
	}
	_, second, _, err := coordinator.DrainWithProjectResults()
	if err != nil {
		t.Fatalf("second DrainWithProjectResults: %v", err)
	}
	if first[0].Status != SolutionProjectSuccess || second[0].Status != SolutionProjectNoChange {
		t.Fatalf("statuses = %q then %q, want success then no-change", first[0].Status, second[0].Status)
	}
}

func TestSolutionCoordinatorReportsUniqueProjectScheduling(t *testing.T) {
	root := t.TempDir()
	writeSolutionConfig(t, root, "tsconfig.json", []string{"./left", "./right"}, true)
	writeSolutionConfig(t, filepath.Join(root, "left"), "tsconfig.json", []string{"../shared"}, false)
	writeSolutionConfig(t, filepath.Join(root, "right"), "tsconfig.json", []string{"../shared"}, false)
	writeSolutionConfig(t, filepath.Join(root, "shared"), "tsconfig.json", nil, false)
	timings := NewBuildTimings()
	coordinator, err := NewSolutionCoordinatorWithDrainer(
		filepath.Join(root, "tsconfig.json"),
		ProjectOptions{Timings: timings},
		&recordingSolutionDrainer{},
	)
	if err != nil {
		t.Fatalf("NewSolutionCoordinatorWithDrainer: %v", err)
	}
	_, projects, _, err := coordinator.DrainWithProjectResults()
	if err != nil {
		t.Fatalf("DrainWithProjectResults: %v", err)
	}
	if got, want := timings.Counts.ScheduledProjects, 3; got != want {
		t.Fatalf("scheduled projects = %d, want %d", got, want)
	}
	for _, project := range projects {
		if project.Timings.Counts.ScheduledProjects != 1 {
			t.Fatalf("%s scheduled count = %d, want 1", project.ConfigPath, project.Timings.Counts.ScheduledProjects)
		}
	}
}

func TestSolutionCoordinatorBlocksDependentThroughSkippedCoordinator(t *testing.T) {
	root := t.TempDir()
	appDir := filepath.Join(root, "app")
	bridgeDir := filepath.Join(root, "bridge")
	brokenDir := filepath.Join(root, "broken")
	writeSolutionConfig(t, root, "tsconfig.json", []string{"./app"}, true)
	writeSolutionConfig(t, appDir, "tsconfig.json", []string{"../bridge"}, false)
	writeSolutionFile(t, bridgeDir, "tsconfig.base.json", `{"files":[],"include":[]}`)
	writeSolutionFile(t, bridgeDir, "tsconfig.json", `{"extends":"./tsconfig.base.json","references":[{"path":"../broken"}]}`)
	writeSolutionConfig(t, brokenDir, "tsconfig.json", nil, false)
	brokenConfig := filepath.Join(brokenDir, "tsconfig.json")
	drainer := &recordingSolutionDrainer{fail: brokenConfig}

	builders := 1
	coordinator, err := NewSolutionCoordinatorWithDrainer(
		filepath.Join(root, "tsconfig.json"),
		ProjectOptions{Builders: &builders},
		drainer,
	)
	if err != nil {
		t.Fatalf("NewSolutionCoordinatorWithDrainer: %v", err)
	}
	if _, _, err := coordinator.Drain(); err == nil {
		t.Fatal("Drain unexpectedly succeeded")
	}

	if want := []string{"broken"}; !reflect.DeepEqual(drainer.drained, want) {
		t.Fatalf("drained projects = %v, want %v", drainer.drained, want)
	}
	state, ok := coordinator.ProjectState(filepath.Join(appDir, "tsconfig.json"))
	if !ok || state.BlockedBy != brokenConfig {
		t.Fatalf("app state = %+v, found = %t, want blocked by %s", state, ok, brokenConfig)
	}
}

func TestSolutionCoordinatorSkipsUpToDateProjects(t *testing.T) {
	root := t.TempDir()
	child := filepath.Join(root, "child")
	writeSolutionConfig(t, root, "tsconfig.json", []string{"./child"}, true)
	writeBuildableSolutionProject(t, child)

	coordinator, err := NewSolutionCoordinator(filepath.Join(root, "tsconfig.json"), ProjectOptions{})
	if err != nil {
		t.Fatalf("NewSolutionCoordinator: %v", err)
	}
	first, _, err := coordinator.Drain()
	if err != nil {
		t.Fatalf("first Drain: %v", err)
	}
	if len(first.EmittedFiles) == 0 {
		t.Fatal("first Drain emitted no files")
	}
	second, _, err := coordinator.Drain()
	if err != nil {
		t.Fatalf("second Drain: %v", err)
	}
	if len(second.EmittedFiles) != 0 {
		t.Fatalf("second Drain emitted files = %v, want none", second.EmittedFiles)
	}
}

func TestReferenceOnlySolutionCoordinatorAllowsEmptyInclude(t *testing.T) {
	root := t.TempDir()
	child := filepath.Join(root, "child")
	rootConfig := filepath.Join(root, "tsconfig.json")
	if err := os.WriteFile(rootConfig, []byte(`{"files":[],"include":[],"references":[{"path":"./child"}]}`), 0o644); err != nil {
		t.Fatal(err)
	}
	writeBuildableSolutionProject(t, child)

	_, messages, err := BuildSolutionWithOptions(rootConfig, ProjectOptions{})
	if err != nil {
		t.Fatalf("BuildSolutionWithOptions: %v (%v)", err, messages)
	}
	if _, err := os.Stat(filepath.Join(child, "out", "main.luau")); err != nil {
		t.Fatalf("child output: %v", err)
	}
	if _, err := os.Stat(filepath.Join(root, "out")); !os.IsNotExist(err) {
		t.Fatalf("coordinator output stat error = %v, want not exists", err)
	}
}

func TestSolutionGraphSkipsCoordinatorWithInheritedEmptyFiles(t *testing.T) {
	root := t.TempDir()
	packageDir := filepath.Join(root, "package")
	writeSolutionFile(t, root, "tsconfig.json", `{"files":[],"references":[{"path":"./package"},{"path":"./package/tsconfig.lib.json"}]}`)
	writeSolutionFile(t, packageDir, "tsconfig.base.json", `{
		"files": [],
		"include": [], // inherited reference-only coordinator
	}`)
	writeSolutionFile(t, packageDir, "tsconfig.json", `{"extends":"./tsconfig.base.json","references":[{"path":"./tsconfig.lib.json"},{"path":"./tsconfig.spec.json"}]}`)
	writeSolutionFile(t, packageDir, "tsconfig.lib.json", `{}`)
	writeSolutionFile(t, packageDir, "tsconfig.spec.json", `{"extends":"./tsconfig.base.json","references":[{"path":"./tsconfig.lib.json"}],"include":["test"]}`)

	graph, err := BuildSolutionGraph(filepath.Join(root, "tsconfig.json"), ProjectOptions{})
	if err != nil {
		t.Fatalf("BuildSolutionGraph: %v", err)
	}
	want := []string{filepath.Join(packageDir, "tsconfig.lib.json"), filepath.Join(packageDir, "tsconfig.spec.json")}
	if len(graph.Projects) != len(want) || graph.Projects[0].ConfigPath != want[0] || graph.Projects[1].ConfigPath != want[1] {
		t.Fatalf("projects = %+v, want emitting projects %v", graph.Projects, want)
	}
}

func TestSolutionGraphResolvesCoordinatorExtendsWithPackageTSConfig(t *testing.T) {
	root := t.TempDir()
	basePackage := filepath.Join(root, "node_modules", "@scope", "base")
	writeSolutionFile(t, root, "tsconfig.json", `{"extends":"@scope/base","references":[{"path":"./child"}]}`)
	writeSolutionFile(t, basePackage, "package.json", `{"tsconfig":"config.json"}`)
	writeSolutionFile(t, basePackage, "config.json", `{"files":[],"include":[]}`)
	writeSolutionConfig(t, filepath.Join(root, "child"), "tsconfig.json", nil, false)

	graph, err := BuildSolutionGraph(filepath.Join(root, "tsconfig.json"), ProjectOptions{SolutionArgv: &RbxtsOptions{}})
	if err != nil {
		t.Fatalf("BuildSolutionGraph: %v", err)
	}
	if got, want := solutionProjectNames(graph.Projects), []string{"child"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("project order = %v, want %v", got, want)
	}
}

func TestSolutionGraphResolvesExtensionlessCoordinatorExtends(t *testing.T) {
	root := t.TempDir()
	writeSolutionFile(t, root, "base", `{"files":[],"include":[]}`)
	writeSolutionFile(t, root, "tsconfig.json", `{"extends":"./base","references":[{"path":"./child"}]}`)
	writeSolutionConfig(t, filepath.Join(root, "child"), "tsconfig.json", nil, false)

	graph, err := BuildSolutionGraph(filepath.Join(root, "tsconfig.json"), ProjectOptions{})
	if err != nil {
		t.Fatalf("BuildSolutionGraph: %v", err)
	}
	if got, want := solutionProjectNames(graph.Projects), []string{"child"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("project order = %v, want %v", got, want)
	}
}

func TestSolutionGraphResolvesDottedCoordinatorExtends(t *testing.T) {
	root := t.TempDir()
	writeSolutionFile(t, root, "tsconfig.base.json", `{"files":[],"include":[]}`)
	writeSolutionFile(t, root, "tsconfig.json", `{"extends":"./tsconfig.base","references":[{"path":"./child"}]}`)
	writeSolutionConfig(t, filepath.Join(root, "child"), "tsconfig.json", nil, false)

	graph, err := BuildSolutionGraph(filepath.Join(root, "tsconfig.json"), ProjectOptions{})
	if err != nil {
		t.Fatalf("BuildSolutionGraph: %v", err)
	}
	if got, want := solutionProjectNames(graph.Projects), []string{"child"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("project order = %v, want %v", got, want)
	}
}

func TestSolutionBuildOrder(t *testing.T) {
	root := t.TempDir()
	writeSolutionConfig(t, root, "tsconfig.json", []string{"./app"}, true)
	writeSolutionConfig(t, filepath.Join(root, "app"), "tsconfig.json", []string{"../dependency"}, false)
	writeSolutionConfig(t, filepath.Join(root, "dependency"), "tsconfig.json", nil, false)
	drainer := &recordingSolutionDrainer{}

	builders := 1
	coordinator, err := NewSolutionCoordinatorWithDrainer(filepath.Join(root, "tsconfig.json"), ProjectOptions{Builders: &builders}, drainer)
	if err != nil {
		t.Fatalf("NewSolutionCoordinatorWithDrainer: %v", err)
	}
	_, _, err = coordinator.Drain()
	if err != nil {
		t.Fatalf("Drain: %v", err)
	}

	if want := []string{"dependency", "app"}; !reflect.DeepEqual(drainer.drained, want) {
		t.Fatalf("drained projects = %v, want %v", drainer.drained, want)
	}
}

func TestSolutionCycle(t *testing.T) {
	root := t.TempDir()
	writeSolutionConfig(t, root, "tsconfig.json", []string{"./first"}, true)
	writeSolutionConfig(t, filepath.Join(root, "first"), "tsconfig.json", []string{"../second"}, false)
	writeSolutionConfig(t, filepath.Join(root, "second"), "tsconfig.json", []string{"../first"}, false)

	_, err := BuildSolutionGraph(filepath.Join(root, "tsconfig.json"), ProjectOptions{})
	if err == nil {
		t.Fatal("BuildSolutionGraph unexpectedly succeeded")
	}
	if !strings.Contains(err.Error(), "first") || !strings.Contains(err.Error(), "second") {
		t.Fatalf("cycle error = %q, want cycle path", err)
	}
}

func TestEmitDeclarationOnly(t *testing.T) {
	root := t.TempDir()
	child := filepath.Join(root, "child")
	writeSolutionConfig(t, root, "tsconfig.json", []string{"./child"}, true)
	writeBuildableSolutionProject(t, child)

	_, messages, err := BuildSolutionWithOptions(filepath.Join(root, "tsconfig.json"), ProjectOptions{EmitDeclarationOnly: true})
	if err != nil {
		t.Fatalf("BuildSolutionWithOptions: %v (%v)", err, messages)
	}
	if _, err := os.Stat(filepath.Join(child, "out", "main.d.ts")); err != nil {
		t.Fatalf("declaration output: %v", err)
	}
	if _, err := os.Stat(filepath.Join(child, "out", "main.luau")); !os.IsNotExist(err) {
		t.Fatalf("Luau output stat error = %v, want not exists", err)
	}
}

func TestSolutionInvalidateDirectProjectKeepsIncrementalSelection(t *testing.T) {
	root := t.TempDir()
	child := filepath.Join(root, "child")
	writeSolutionConfig(t, root, "tsconfig.json", []string{"./child"}, true)
	writeBuildableSolutionProject(t, child)
	childConfig := filepath.Join(child, "tsconfig.json")
	if err := os.WriteFile(filepath.Join(child, "src", "extra.ts"), []byte("export const extra = 1;\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	timings := NewBuildTimings()
	builders := 1
	coordinator, err := NewSolutionCoordinator(filepath.Join(root, "tsconfig.json"), ProjectOptions{Timings: timings, Builders: &builders})
	if err != nil {
		t.Fatalf("NewSolutionCoordinator: %v", err)
	}
	if _, messages, err := coordinator.Drain(); err != nil {
		t.Fatalf("first Drain: %v (%v)", err, messages)
	}
	if timings.Counts.SelectedSources != timings.Counts.TotalSources {
		t.Fatalf("first build selected %d of %d sources, want full build", timings.Counts.SelectedSources, timings.Counts.TotalSources)
	}

	if err := os.WriteFile(filepath.Join(child, "src", "main.ts"), []byte("export const value = 2;\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	coordinator.Invalidate(childConfig)

	if _, messages, err := coordinator.Drain(); err != nil {
		t.Fatalf("second Drain: %v (%v)", err, messages)
	}
	if timings.Counts.SelectedSources == 0 || timings.Counts.SelectedSources >= timings.Counts.TotalSources {
		t.Fatalf("directly invalidated child selected %d of %d sources, want fewer than all", timings.Counts.SelectedSources, timings.Counts.TotalSources)
	}
}

func TestSolutionInvalidateKeepsDownstreamFullBuild(t *testing.T) {
	root := t.TempDir()
	writeSolutionConfig(t, root, "tsconfig.json", []string{"./app"}, true)
	writeSolutionConfig(t, filepath.Join(root, "app"), "tsconfig.json", []string{"../shared"}, false)
	writeSolutionConfig(t, filepath.Join(root, "shared"), "tsconfig.json", nil, false)
	builders := 1
	coordinator, err := NewSolutionCoordinatorWithDrainer(filepath.Join(root, "tsconfig.json"), ProjectOptions{Builders: &builders}, &recordingSolutionDrainer{})
	if err != nil {
		t.Fatalf("NewSolutionCoordinatorWithDrainer: %v", err)
	}
	if _, _, err := coordinator.Drain(); err != nil {
		t.Fatalf("initial Drain: %v", err)
	}

	sharedConfig := filepath.Join(root, "shared", "tsconfig.json")
	coordinator.Invalidate(sharedConfig)

	sharedState, ok := coordinator.ProjectState(sharedConfig)
	if !ok || sharedState.forceFullBuild {
		t.Fatalf("directly invalidated shared forceFullBuild = %t, want false", sharedState.forceFullBuild)
	}
	appState, ok := coordinator.ProjectState(filepath.Join(root, "app", "tsconfig.json"))
	if !ok || !appState.forceFullBuild {
		t.Fatalf("downstream app forceFullBuild = %t, want true", appState.forceFullBuild)
	}
}

func writeSolutionConfig(t *testing.T, dir, name string, references []string, coordinator bool) {
	t.Helper()
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	config := `{}`
	if coordinator {
		config = `{"files":[],"references":` + solutionReferences(references) + `}`
	} else if len(references) > 0 {
		config = `{"references":` + solutionReferences(references) + `}`
	}
	if err := os.WriteFile(filepath.Join(dir, name), []byte(config), 0o644); err != nil {
		t.Fatal(err)
	}
}

func solutionReferences(paths []string) string {
	result := "["
	for index, path := range paths {
		if index > 0 {
			result += ","
		}
		result += `{"path":"` + path + `"}`
	}
	return result + "]"
}

func solutionProjectNames(projects []SolutionProject) []string {
	names := make([]string, len(projects))
	for index, project := range projects {
		names[index] = filepath.Base(filepath.Dir(project.ConfigPath))
	}
	return names
}

func TestSolutionTimingsRecordsStatusesAndGraphOrder(t *testing.T) {
	root := t.TempDir()
	writeSolutionConfig(t, root, "tsconfig.json", []string{"./left", "./right"}, true)
	writeSolutionConfig(t, filepath.Join(root, "left"), "tsconfig.json", []string{"../shared"}, false)
	writeSolutionConfig(t, filepath.Join(root, "right"), "tsconfig.json", []string{"../shared"}, false)
	writeSolutionConfig(t, filepath.Join(root, "shared"), "tsconfig.json", nil, false)
	drainer := &recordingSolutionDrainer{fail: filepath.Join(root, "shared", "tsconfig.json")}
	timings := NewBuildTimings()
	builders := 1
	coordinator, err := NewSolutionCoordinatorWithDrainer(filepath.Join(root, "tsconfig.json"), ProjectOptions{Builders: &builders, Timings: timings}, drainer)
	if err != nil {
		t.Fatalf("NewSolutionCoordinatorWithDrainer: %v", err)
	}
	if _, _, err := coordinator.Drain(); err == nil {
		t.Fatal("Drain succeeded, want shared failure")
	}
	if len(timings.Projects) != 3 {
		t.Fatalf("projects = %d, want 3", len(timings.Projects))
	}
	if got := []string{filepath.Base(filepath.Dir(timings.Projects[0].ConfigPath)), filepath.Base(filepath.Dir(timings.Projects[1].ConfigPath)), filepath.Base(filepath.Dir(timings.Projects[2].ConfigPath))}; got[0] != "shared" || got[1] != "left" || got[2] != "right" {
		t.Fatalf("project order = %v, want shared left right", got)
	}
	if timings.Projects[0].Status != ProjectTimingStatusFailed {
		t.Fatalf("shared status = %q, want %q", timings.Projects[0].Status, ProjectTimingStatusFailed)
	}
	if timings.Projects[1].Status != ProjectTimingStatusBlocked || timings.Projects[2].Status != ProjectTimingStatusBlocked {
		t.Fatalf("dependent statuses = %q %q, want blocked", timings.Projects[1].Status, timings.Projects[2].Status)
	}

	timings = NewBuildTimings()
	coordinator, err = NewSolutionCoordinatorWithDrainer(filepath.Join(root, "tsconfig.json"), ProjectOptions{Builders: &builders, Timings: timings}, &recordingSolutionDrainer{})
	if err != nil {
		t.Fatalf("second coordinator: %v", err)
	}
	if _, _, err := coordinator.Drain(); err != nil {
		t.Fatalf("second Drain: %v", err)
	}
	if _, _, err := coordinator.Drain(); err != nil {
		t.Fatalf("repeat Drain: %v", err)
	}
	for _, project := range timings.Projects {
		if project.Status != ProjectTimingStatusSkipped {
			t.Fatalf("%s status = %q, want skipped on up-to-date drain", project.ConfigPath, project.Status)
		}
	}
}

func writeBuildableSolutionProject(t *testing.T, dir string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Join(dir, "src"), 0o755); err != nil {
		t.Fatal(err)
	}
	config := `{"compilerOptions":{"allowSyntheticDefaultImports":true,"composite":true,"declaration":true,"module":"CommonJS","moduleResolution":"Node","noLib":true,"moduleDetection":"force","strict":true,"target":"ESNext","types":[],"typeRoots":["node_modules/@rbxts"],"rootDir":"src","outDir":"out"},"include":["src"]}`
	files := map[string]string{
		"tsconfig.json":    config,
		"package.json":     `{"name":"@scope/solution-child"}`,
		"src/globals.d.ts": noLibGlobalStubs,
		"src/main.ts":      "export const value = 1;\n",
	}
	for path, content := range files {
		if err := os.WriteFile(filepath.Join(dir, path), []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}
}
