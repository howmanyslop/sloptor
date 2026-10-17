package compile

import (
	"context"
	"errors"
	"path/filepath"
	"reflect"
	"testing"
)

type retentionSolutionDrainer struct {
	solutionBuildDrainer
	generation map[string]string
	failBuild  map[string]error
	failQueued map[string]error
	failWrite  map[string]bool
	attempts   []string
}

func (d *retentionSolutionDrainer) Drain(project SolutionProject) (*BuildResult, []string, error) {
	name := filepath.Base(filepath.Dir(project.ConfigPath))
	if err := d.failBuild[name]; err != nil {
		return nil, nil, err
	}
	if generation := d.generation[name]; generation != "" {
		for _, stage := range []string{"cache", "manifest"} {
			id := name + ":" + generation + ":" + stage
			*project.Options.pendingSolutionPersists = append(*project.Options.pendingSolutionPersists, func() error {
				d.attempts = append(d.attempts, id)
				if d.failWrite[id] {
					return errors.New("write " + id)
				}
				return nil
			})
		}
	}
	if err := d.failQueued[name]; err != nil {
		return nil, nil, err
	}
	return &BuildResult{Outputs: map[string]string{name: d.generation[name]}}, nil, nil
}

func retentionCoordinator(t *testing.T, drainer *retentionSolutionDrainer) (*SolutionCoordinator, map[string]string) {
	t.Helper()
	root := t.TempDir()
	writeSolutionConfig(t, root, "tsconfig.json", []string{"./first", "./second", "./third"}, true)
	configs := map[string]string{}
	for _, name := range []string{"first", "second", "third"} {
		writeSolutionConfig(t, filepath.Join(root, name), "tsconfig.json", nil, false)
		configs[name] = filepath.Join(root, name, "tsconfig.json")
	}
	builders := 1
	coordinator, err := NewSolutionCoordinatorWithDrainer(filepath.Join(root, "tsconfig.json"), ProjectOptions{Builders: &builders}, drainer)
	if err != nil {
		t.Fatal(err)
	}
	return coordinator, configs
}

func TestSolutionPersistenceBoundsFailedDrainGenerations(t *testing.T) {
	drainer := &retentionSolutionDrainer{
		generation: map[string]string{"first": "old", "second": "stable", "third": "recovered"},
		failBuild:  map[string]error{"third": errors.New("broken sibling")},
	}
	coordinator, configs := retentionCoordinator(t, drainer)
	for _, generation := range []string{"old", "middle", "latest"} {
		drainer.generation["first"] = generation
		coordinator.Invalidate(configs["first"])
		if _, _, err := coordinator.Drain(); err == nil {
			t.Fatal("drain succeeded with broken sibling")
		}
		if len(drainer.attempts) != 0 {
			t.Fatalf("published during failed solution: %v", drainer.attempts)
		}
		if got := len(drainer.persists); got > 2 {
			t.Fatalf("pending persistence generations = %d, want at most two successful projects", got)
		}
	}
	delete(drainer.failBuild, "third")
	if _, _, err := coordinator.Drain(); err != nil {
		t.Fatal(err)
	}
	want := []string{"first:latest:cache", "first:latest:manifest", "second:stable:cache", "second:stable:manifest", "third:recovered:cache", "third:recovered:manifest"}
	if !reflect.DeepEqual(drainer.attempts, want) {
		t.Fatalf("published generations = %v, want %v", drainer.attempts, want)
	}
	if len(drainer.persists) != 0 {
		t.Fatal("successful recovery retained callbacks")
	}
}

func TestSolutionPersistenceRetriesOnlyUnfinishedWork(t *testing.T) {
	drainer := &retentionSolutionDrainer{
		generation: map[string]string{"first": "one", "second": "one", "third": "one"},
		failWrite:  map[string]bool{"second:one:manifest": true},
	}
	coordinator, configs := retentionCoordinator(t, drainer)
	if _, _, err := coordinator.Drain(); err == nil {
		t.Fatal("drain succeeded with persistence error")
	}
	// The failed generation is superseded while the third project's untouched
	// generation must survive. A cancelled attempt cannot replace valid work.
	drainer.failBuild = map[string]error{"second": context.Canceled}
	coordinator.Invalidate(configs["second"])
	if _, _, err := coordinator.Drain(); !errors.Is(err, context.Canceled) {
		t.Fatalf("cancelled drain = %v", err)
	}
	delete(drainer.failBuild, "second")
	drainer.generation["second"] = "two"
	if _, _, err := coordinator.Drain(); err != nil {
		t.Fatal(err)
	}
	want := []string{"first:one:cache", "first:one:manifest", "second:one:cache", "second:one:manifest", "second:two:cache", "second:two:manifest", "third:one:cache", "third:one:manifest"}
	if !reflect.DeepEqual(drainer.attempts, want) {
		t.Fatalf("persistence attempts = %v, want %v", drainer.attempts, want)
	}
}

func TestSolutionPersistenceSuccessfulEmptyGenerationSupersedesPending(t *testing.T) {
	drainer := &retentionSolutionDrainer{
		generation: map[string]string{"first": "old"},
		failBuild:  map[string]error{"third": errors.New("broken sibling")},
	}
	coordinator, configs := retentionCoordinator(t, drainer)
	if _, _, err := coordinator.Drain(); err == nil {
		t.Fatal("drain succeeded with broken sibling")
	}
	drainer.generation["first"] = ""
	coordinator.Invalidate(configs["first"])
	delete(drainer.failBuild, "third")
	if _, _, err := coordinator.Drain(); err != nil {
		t.Fatal(err)
	}
	if len(drainer.attempts) != 0 {
		t.Fatalf("published superseded work: %v", drainer.attempts)
	}
}

func TestSolutionPersistenceFailedAttemptDoesNotQueueIncompleteGeneration(t *testing.T) {
	drainer := &retentionSolutionDrainer{
		generation: map[string]string{"first": "incomplete", "second": "valid"},
		failQueued: map[string]error{"first": context.Canceled},
	}
	coordinator, configs := retentionCoordinator(t, drainer)
	if _, _, err := coordinator.Drain(); !errors.Is(err, context.Canceled) {
		t.Fatalf("cancelled drain = %v", err)
	}
	if _, queued := drainer.persists[configs["first"]]; queued {
		t.Fatal("cancelled project queued an incomplete persistence generation")
	}
	if len(drainer.attempts) != 0 {
		t.Fatalf("failed solution published callbacks: %v", drainer.attempts)
	}
}

func TestSolutionPersistenceRepeatedWriteFailureReleasesConsumedCallbacks(t *testing.T) {
	var drainer solutionBuildDrainer
	var attempts []string
	fail := true
	callbacks := []func() error{
		func() error { attempts = append(attempts, "completed"); return nil },
		func() error {
			attempts = append(attempts, "retry")
			if fail {
				return errors.New("write failed")
			}
			return nil
		},
		func() error { attempts = append(attempts, "later"); return nil },
	}
	drainer.appendPersists("project", callbacks)
	for range 3 {
		if err := drainer.persist(); err == nil {
			t.Fatal("persistence succeeded while its write was failing")
		}
		// The pending suffix shares this array. Its completed slot must no
		// longer own captured build state while a later write keeps failing.
		if callbacks[0] != nil {
			t.Fatal("pending retry still retains the consumed closure")
		}
	}
	fail = false
	if err := drainer.persist(); err != nil {
		t.Fatal(err)
	}
	want := []string{"completed", "retry", "retry", "retry", "retry", "later"}
	if !reflect.DeepEqual(attempts, want) {
		t.Fatalf("persistence attempts = %v, want %v", attempts, want)
	}
	if len(drainer.persists) != 0 || len(drainer.persistOrder) != 0 {
		t.Fatal("successful persistence retained pending work")
	}
}
