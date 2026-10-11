package compile

import (
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

// oracleStep changes the fixture and names the projects a watcher would
// invalidate for that change. reload runs ReloadForWatch first, as the
// watcher does for a config event.
type oracleStep struct {
	name       string
	change     func(f *referenceFixture)
	invalidate []string
	reload     bool
	// missed marks a step after a change no project was invalidated for;
	// its metadata may lag the disk, so only the drain error is checked.
	missed bool
	// fails marks a step whose drain must fail.
	fails bool
}

func failingStep(step oracleStep) oracleStep {
	step.fails = true
	return step
}

func includeStep(name, project, include string) oracleStep {
	return oracleStep{name: name, change: func(f *referenceFixture) {
		var config map[string]any
		if err := json.Unmarshal(mustReadFile(f.t, filepath.Join(f.dir, project, "tsconfig.json")), &config); err != nil {
			f.t.Fatal(err)
		}
		config["include"] = []string{include}
		f.json(project+"/tsconfig.json", config)
	}, invalidate: []string{project}}
}

func missedStep(step oracleStep) oracleStep {
	step.missed = true
	return step
}

func newOracleFixture(t *testing.T) *referenceFixture {
	f := newReferenceFixture(t)
	f.project("core", nil, nil)
	f.project("left", []string{"core"}, map[string]any{"declarationDir": "types"})
	f.project("mid", []string{"left"}, nil)
	f.project("right", []string{"core"}, nil)
	f.project("game", []string{"mid", "right"}, nil)
	return f
}

// assertMetadataMatchesFresh compares the coordinator's incremental metadata
// with metadata derived from scratch for the files on disk now.
func assertMetadataMatchesFresh(t *testing.T, f *referenceFixture, coordinator *SolutionCoordinator, step string, drainErr error) {
	t.Helper()
	graph, err := BuildSolutionGraph(filepath.Join(f.dir, "game/tsconfig.json"), f.options())
	if err != nil {
		t.Fatalf("%s: fresh graph: %v", step, err)
	}
	fresh, _ := newCrossProjectMetadata(graph)
	if fresh.validationErr != nil {
		if drainErr == nil {
			t.Fatalf("%s: drain passed, fresh metadata fails: %v", step, fresh.validationErr)
		}
		return
	}
	drainer := coordinator.drainer.(*solutionBuildDrainer)
	for _, check := range []struct {
		name      string
		got, want any
	}{
		{"writeRoots", coordinator.writeRoots, fresh.writeRoots},
		{"waitOnlyDependencies", coordinator.waitOnlyDependencies, fresh.waitOnlyDependencies},
		{"referenceOutputs", coordinator.referenceOutputs, fresh.referenceOutputs},
		{"drainer referenceOutputs", drainer.referenceOutputs, fresh.referenceOutputs},
		{"restoredMetadataErrs", drainer.restoredMetadataErrs, fresh.restoredMetadataErrs},
		// Project options feed each project's incremental salt, including
		// projectReferencePaths.
		{"projects", coordinator.graph.Projects, graph.Projects},
	} {
		if !reflect.DeepEqual(check.got, check.want) {
			t.Fatalf("%s: %s = %v, fresh %v", step, check.name, check.got, check.want)
		}
	}
	for _, project := range graph.Projects {
		if state := coordinator.states[project.ConfigPath]; !reflect.DeepEqual(state.Project, project) {
			t.Fatalf("%s: state project %s = %+v, fresh %+v", step, project.ConfigPath, state.Project, project)
		}
	}
	for configPath, declarations := range drainer.restoredDeclarations {
		state, _ := coordinator.ProjectState(configPath)
		want, err := projectRestoredDeclarations(state.Project)
		if err == nil && !reflect.DeepEqual(declarations, want) {
			t.Fatalf("%s: restored declarations of %s = %v, fresh %v", step, configPath, declarations, want)
		}
	}
}

func runOracleSteps(t *testing.T, steps []oracleStep) {
	f := newOracleFixture(t)
	coordinator := f.coordinator("game")
	rootConfig := filepath.Join(f.dir, "game/tsconfig.json")
	for _, step := range steps {
		step.change(f)
		if step.reload {
			if _, err := coordinator.ReloadForWatch(rootConfig, f.options()); err != nil {
				t.Fatalf("%s: reload: %v", step.name, err)
			}
		}
		for _, project := range step.invalidate {
			coordinator.Invalidate(filepath.Join(f.dir, project, "tsconfig.json"))
		}
		_, messages, err := coordinator.Drain()
		if step.fails != (err != nil) {
			t.Fatalf("%s: drain error = %v (%v), want failure %v", step.name, err, messages, step.fails)
		}
		if step.missed {
			continue
		}
		assertMetadataMatchesFresh(t, f, coordinator, step.name, err)
	}
}

func writeStep(name, path, text string, invalidate ...string) oracleStep {
	return oracleStep{name: name, change: func(f *referenceFixture) { f.write(path, text) }, invalidate: invalidate}
}

func removeStep(name, path string, invalidate ...string) oracleStep {
	return oracleStep{name: name, change: func(f *referenceFixture) {
		if err := os.Remove(filepath.Join(f.dir, path)); err != nil {
			f.t.Fatal(err)
		}
	}, invalidate: invalidate}
}

func renameStep(name, from, to string, invalidate ...string) oracleStep {
	return oracleStep{name: name, change: func(f *referenceFixture) {
		if err := os.Rename(filepath.Join(f.dir, from), filepath.Join(f.dir, to)); err != nil {
			f.t.Fatal(err)
		}
	}, invalidate: invalidate}
}

func optionStep(name, project, key string, value any, reload bool, invalidate ...string) oracleStep {
	return oracleStep{name: name, change: func(f *referenceFixture) { f.setCompilerOption(project, key, value) }, invalidate: invalidate, reload: reload}
}

func TestWatchMetadataMatchesFreshDerivation(t *testing.T) {
	sequences := map[string][]oracleStep{
		"sources": {
			writeStep("core text edit", "core/src/index.ts", "export const value = 2;\n", "core"),
			writeStep("left add", "left/src/added.ts", "export const added = 1;\n", "left"),
			writeStep("core add", "core/src/extra.ts", "export const extra = 1;\n", "core"),
			renameStep("left rename", "left/src/added.ts", "left/src/renamed.ts", "left"),
			removeStep("core remove", "core/src/extra.ts", "core"),
			writeStep("game edit", "game/src/index.ts", "export const value = 3;\n", "game"),
			writeStep("right nested add", "right/src/nested/deep.ts", "export const deep = 1;\n", "right"),
			renameStep("right folder rename", "right/src/nested", "right/src/moved", "right"),
		},
		"configs": {
			optionStep("left declarationDir", "left", "declarationDir", "decl", false, "left"),
			optionStep("core outDir", "core", "outDir", "build", false, "core"),
			optionStep("mid declarationDir with reload", "mid", "declarationDir", "types", true, "mid"),
			optionStep("core outDir back with reload", "core", "outDir", "out", true),
			writeStep("left add after configs", "left/src/added.ts", "export const added = 1;\n", "left"),
		},
		"upstream not invalidated": {
			// A change the watcher missed must not break a consumer-only
			// cycle; the next cycle that invalidates its owner picks it up.
			missedStep(writeStep("left add, not invalidated", "left/src/added.ts", "export const added = 1;\n")),
			missedStep(writeStep("game edit", "game/src/index.ts", "export const value = 3;\n", "game")),
			writeStep("left edit", "left/src/index.ts", "export const value = 2;\n", "left"),
		},
		"config error recovers": {
			includeStep("right no inputs", "right", "missing"),
			includeStep("right fixed", "right", "src"),
			writeStep("right edit", "right/src/index.ts", "export const value = 4;\n", "right"),
		},
		"validation error recovers": {
			failingStep(optionStep("left not composite", "left", "composite", false, false, "left")),
			failingStep(writeStep("game edit while invalid", "game/src/index.ts", "export const value = 5;\n", "game")),
			optionStep("left composite", "left", "composite", true, false, "left"),
			writeStep("left edit", "left/src/index.ts", "export const value = 2;\n", "left"),
		},
	}
	for name, steps := range sequences {
		t.Run(name, func(t *testing.T) { runOracleSteps(t, steps) })
	}
}
