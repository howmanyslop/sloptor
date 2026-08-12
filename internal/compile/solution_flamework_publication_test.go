package compile

import (
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"rotor/internal/flamework"
)

type flameworkSolutionPublicationDrainer struct {
	artifactPath string
	next         []byte
	failPublish  bool
	drained      []string
	observed     string
}

func (d *flameworkSolutionPublicationDrainer) Drain(project SolutionProject) (*BuildResult, []string, error) {
	name := filepath.Base(filepath.Dir(project.ConfigPath))
	d.drained = append(d.drained, name)
	if name == "package" {
		persist := func() error {
			if d.failPublish {
				return errors.New("publish failed")
			}
			packageRoot := filepath.Dir(project.ConfigPath)
			return flamework.PersistArtifacts(packageRoot, []flamework.Artifact{{Path: filepath.Base(d.artifactPath), Data: d.next}})
		}
		persists := project.Options.pendingSolutionDependencyPersists
		if persists == nil {
			persists = project.Options.pendingSolutionPersists
		}
		*persists = append(*persists, persist)
		return &BuildResult{Outputs: map[string]string{"package": "compiled"}}, nil, nil
	}
	if name == "game" {
		published, err := os.ReadFile(d.artifactPath)
		if err != nil {
			return nil, nil, err
		}
		d.observed = string(published)
		return &BuildResult{Outputs: map[string]string{"game": string(published)}}, nil, nil
	}
	return &BuildResult{Outputs: map[string]string{name: "compiled"}}, nil, nil
}

func TestSolutionFlameworkPackagePublishesBeforeDependentCompiles(t *testing.T) {
	// Given: a package and dependent game with distinct old and pending build data.
	root, packageConfig, _ := writeFlameworkPublicationSolution(t, false)
	artifactPath := filepath.Join(filepath.Dir(packageConfig), "flamework.build")
	if err := os.WriteFile(artifactPath, []byte(`{"identifiers":{"service":"old-id"}}`), 0o644); err != nil {
		t.Fatal(err)
	}
	drainer := &flameworkSolutionPublicationDrainer{
		artifactPath: artifactPath,
		next:         []byte(`{"identifiers":{"service":"new-id"}}`),
	}
	builders := 2
	coordinator, err := NewSolutionCoordinatorWithDrainer(root, ProjectOptions{Builders: &builders}, drainer)
	if err != nil {
		t.Fatalf("NewSolutionCoordinatorWithDrainer: %v", err)
	}

	// When: the real solution coordinator drains the package-to-game graph.
	_, _, err = coordinator.Drain()
	// Then: the game observes the newly published identifier in the same drain.
	if err != nil {
		t.Fatalf("Drain: %v", err)
	}
	if got, want := drainer.observed, `{"identifiers":{"service":"new-id"}}`; got != want {
		t.Fatalf("dependent build data = %s, want %s", got, want)
	}
}

func TestSolutionFlameworkPackagePublishFailurePreservesStateAndBlocksOnlyDependent(t *testing.T) {
	// Given: a prior successful artifact and a package publication that will fail.
	root, packageConfig, gameConfig := writeFlameworkPublicationSolution(t, true)
	artifactPath := filepath.Join(filepath.Dir(packageConfig), "flamework.build")
	prior := []byte(`{"identifiers":{"service":"stable-id"}}`)
	if err := os.WriteFile(artifactPath, prior, 0o644); err != nil {
		t.Fatal(err)
	}
	drainer := &flameworkSolutionPublicationDrainer{
		artifactPath: artifactPath,
		next:         []byte(`{"identifiers":{"service":"partial-id"}}`),
		failPublish:  true,
	}
	builders := 1
	coordinator, err := NewSolutionCoordinatorWithDrainer(root, ProjectOptions{Builders: &builders}, drainer)
	if err != nil {
		t.Fatalf("NewSolutionCoordinatorWithDrainer: %v", err)
	}

	// When: package publication fails before the dependent barrier closes.
	_, messages, drainErr := coordinator.Drain()

	// Then: prior state remains, the game is blocked, and its sibling still builds.
	if drainErr == nil {
		t.Fatal("Drain unexpectedly succeeded")
	}
	if len(messages) != 2 || !strings.Contains(messages[0], "publish dependency state") || !strings.Contains(messages[1], "blocked by failed dependency") {
		t.Fatalf("messages = %v, want publication and dependent-block diagnostics", messages)
	}
	if got := string(mustReadFile(t, artifactPath)); got != string(prior) {
		t.Fatalf("artifact after failure = %s, want %s", got, prior)
	}
	if got, want := drainer.drained, []string{"package", "sibling"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("drained projects = %v, want %v", got, want)
	}
	state, ok := coordinator.ProjectState(gameConfig)
	if !ok || state.BlockedBy != packageConfig {
		t.Fatalf("game state = %+v, found=%t", state, ok)
	}
}

func TestSolutionWatchFlameworkPackageInvalidatesDependentDeterministically(t *testing.T) {
	// Given: a successfully built package-to-game solution.
	root, packageConfig, gameConfig := writeFlameworkPublicationSolution(t, false)
	artifactPath := filepath.Join(filepath.Dir(packageConfig), "flamework.build")
	if err := os.WriteFile(artifactPath, []byte(`{"id":"old"}`), 0o644); err != nil {
		t.Fatal(err)
	}
	drainer := &flameworkSolutionPublicationDrainer{artifactPath: artifactPath, next: []byte(`{"id":"first"}`)}
	builders := 2
	coordinator, err := NewSolutionCoordinatorWithDrainer(root, ProjectOptions{Builders: &builders}, drainer)
	if err != nil {
		t.Fatalf("NewSolutionCoordinatorWithDrainer: %v", err)
	}
	if _, _, err := coordinator.Drain(); err != nil {
		t.Fatalf("initial Drain: %v", err)
	}
	drainer.drained = nil
	drainer.next = []byte(`{"id":"second"}`)

	// When: solution watch invalidates the package project.
	invalidated := coordinator.Invalidate(packageConfig)
	_, _, drainErr := coordinator.Drain()

	// Then: package and dependent rebuild once in graph order using new state.
	if drainErr != nil {
		t.Fatalf("watch Drain: %v", drainErr)
	}
	if want := []string{packageConfig, gameConfig}; !reflect.DeepEqual(invalidated, want) {
		t.Fatalf("Invalidate() = %v, want %v", invalidated, want)
	}
	if want := []string{"package", "game"}; !reflect.DeepEqual(drainer.drained, want) {
		t.Fatalf("drained projects = %v, want %v", drainer.drained, want)
	}
	if got, want := drainer.observed, `{"id":"second"}`; got != want {
		t.Fatalf("dependent build data = %s, want %s", got, want)
	}
}

func writeFlameworkPublicationSolution(t *testing.T, sibling bool) (string, string, string) {
	t.Helper()
	root := t.TempDir()
	references := []string{"./package"}
	if sibling {
		references = append(references, "./sibling")
	}
	references = append(references, "./game")
	writeSolutionConfig(t, root, "tsconfig.json", references, true)
	writeSolutionConfig(t, filepath.Join(root, "package"), "tsconfig.json", nil, false)
	writeSolutionConfig(t, filepath.Join(root, "game"), "tsconfig.json", []string{"../package"}, false)
	if sibling {
		writeSolutionConfig(t, filepath.Join(root, "sibling"), "tsconfig.json", nil, false)
	}
	return filepath.Join(root, "tsconfig.json"), filepath.Join(root, "package", "tsconfig.json"), filepath.Join(root, "game", "tsconfig.json")
}
