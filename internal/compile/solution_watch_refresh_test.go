package compile

import (
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

type metadataWork struct{ parses, refreshes int }

func coordinatorMetadataWork(coordinator *SolutionCoordinator) metadataWork {
	return metadataWork{coordinator.metadata.configParses, coordinator.metadata.refreshes}
}

func (f *referenceFixture) coordinator(root string) *SolutionCoordinator {
	f.t.Helper()
	coordinator, err := NewSolutionCoordinator(filepath.Join(f.dir, root, "tsconfig.json"), f.options())
	if err != nil {
		f.t.Fatal(err)
	}
	f.drain(coordinator)
	return coordinator
}

func (f *referenceFixture) drain(coordinator *SolutionCoordinator) {
	f.t.Helper()
	if _, messages, err := coordinator.Drain(); err != nil {
		f.t.Fatalf("drain: %v (%v)", err, messages)
	}
}

func (f *referenceFixture) invalidateAndDrain(coordinator *SolutionCoordinator, projects ...string) {
	f.t.Helper()
	for _, project := range projects {
		coordinator.Invalidate(filepath.Join(f.dir, project, "tsconfig.json"))
	}
	f.drain(coordinator)
}

func (f *referenceFixture) setCompilerOption(project, key string, value any) {
	f.t.Helper()
	var config map[string]any
	if err := json.Unmarshal(mustReadFile(f.t, filepath.Join(f.dir, project, "tsconfig.json")), &config); err != nil {
		f.t.Fatal(err)
	}
	config["compilerOptions"].(map[string]any)[key] = value
	f.json(project+"/tsconfig.json", config)
}

func assertMetadataWork(t *testing.T, coordinator *SolutionCoordinator, before metadataWork, parses, refreshes int) {
	t.Helper()
	after := coordinatorMetadataWork(coordinator)
	if got := (metadataWork{after.parses - before.parses, after.refreshes - before.refreshes}); got != (metadataWork{parses, refreshes}) {
		t.Fatalf("metadata work = %+v, want %+v", got, metadataWork{parses, refreshes})
	}
}

func assertGameImport(t *testing.T, f *referenceFixture, want string) {
	t.Helper()
	output := string(mustReadFile(t, filepath.Join(f.dir, "game/out/init.luau")))
	if !strings.Contains(output, want) {
		t.Fatalf("game output lacks %s:\n%s", want, output)
	}
}

func hasReferencePathSuffix(coordinator *SolutionCoordinator, configPath, suffix string) bool {
	state, _ := coordinator.ProjectState(configPath)
	for key := range state.Project.Options.projectReferencePaths {
		if strings.HasSuffix(strings.ToLower(filepath.ToSlash(key)), suffix) {
			return true
		}
	}
	return false
}

func TestWatchTextEditSkipsMetadataRefresh(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("shared", nil, map[string]any{"declarationDir": "types"})
	f.project("game", []string{"shared"}, nil)
	f.write("game/src/index.ts", `import { value } from "../../shared/types"; export const result = value;`)
	coordinator := f.coordinator("game")
	coordinator.WatchSets()
	before := coordinatorMetadataWork(coordinator)
	f.write("shared/src/index.ts", "export const value = 42;\n")
	f.invalidateAndDrain(coordinator, "shared")
	coordinator.WatchSets()
	// The two invalidated configs are parsed once; nothing else is.
	assertMetadataWork(t, coordinator, before, 2, 0)
	if output := string(mustReadFile(t, filepath.Join(f.dir, "shared/out/init.luau"))); !strings.Contains(output, "42") {
		t.Fatalf("edit was not emitted:\n%s", output)
	}
}

func TestWatchReferenceFileSetChanges(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("shared", nil, map[string]any{"declarationDir": "types"})
	f.project("other", nil, nil)
	f.project("game", []string{"shared", "other"}, nil)
	f.write("shared/src/old.ts", "export const old = 1;\n")
	f.write("game/src/index.ts", `import { old } from "../../shared/types/old"; export const result = old;`)
	coordinator := f.coordinator("game")
	gameConfig := filepath.Join(f.dir, "game/tsconfig.json")

	before := coordinatorMetadataWork(coordinator)
	f.write("shared/src/added.ts", "export const added = 8;\n")
	f.write("game/src/index.ts", `import { added } from "../../shared/types/added"; export const result = added;`)
	f.invalidateAndDrain(coordinator, "shared")
	assertMetadataWork(t, coordinator, before, 2, 1)
	assertGameImport(t, f, `"shared", "added")`)

	before = coordinatorMetadataWork(coordinator)
	if err := os.Rename(filepath.Join(f.dir, "shared/src/old.ts"), filepath.Join(f.dir, "shared/src/renamed.ts")); err != nil {
		t.Fatal(err)
	}
	f.write("game/src/index.ts", `import { old } from "../../shared/types/renamed"; export const result = old;`)
	f.invalidateAndDrain(coordinator, "shared")
	assertMetadataWork(t, coordinator, before, 2, 1)
	assertGameImport(t, f, `"shared", "renamed")`)
	if hasReferencePathSuffix(coordinator, gameConfig, "/src/old.ts") || !hasReferencePathSuffix(coordinator, gameConfig, "/src/renamed.ts") {
		t.Fatal("rename did not move the reference path")
	}

	before = coordinatorMetadataWork(coordinator)
	if err := os.Remove(filepath.Join(f.dir, "shared/src/added.ts")); err != nil {
		t.Fatal(err)
	}
	f.write("game/src/index.ts", `import { value } from "../../shared/types"; export const result = value;`)
	f.invalidateAndDrain(coordinator, "shared")
	assertMetadataWork(t, coordinator, before, 2, 1)
	if hasReferencePathSuffix(coordinator, gameConfig, "/src/added.ts") || hasReferencePathSuffix(coordinator, gameConfig, "/types/added.d.ts") {
		t.Fatal("removed file kept its reference paths")
	}
}

func TestWatchTransitiveAddedSource(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("common", nil, map[string]any{"declarationDir": "types"})
	f.project("left", []string{"common"}, nil)
	f.project("game", []string{"left"}, nil)
	coordinator := f.coordinator("game")
	f.write("common/src/added.ts", "export const added = 3;\n")
	f.write("game/src/index.ts", `import { added } from "../../common/types/added"; export const result = added;`)
	f.invalidateAndDrain(coordinator, "common")
	assertGameImport(t, f, `"common", "added")`)
}

func TestWatchConfigEditWithoutReload(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("shared", nil, map[string]any{"declarationDir": "types"})
	f.project("game", []string{"shared"}, nil)
	f.write("game/src/index.ts", `import { value } from "../../shared/types"; export const result = value;`)
	f.rojo("default.project.json", "", map[string]any{"moved": map[string]string{"$path": "shared/next"}})
	coordinator := f.coordinator("game")
	before := coordinatorMetadataWork(coordinator)
	f.setCompilerOption("shared", "outDir", "next")
	f.invalidateAndDrain(coordinator, "shared")
	assertMetadataWork(t, coordinator, before, 2, 1)
	assertGameImport(t, f, `game:GetService("ReplicatedStorage"), "moved")`)
}

func TestWatchConfigReloadRefreshesOnce(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("shared", nil, map[string]any{"declarationDir": "types"})
	f.project("game", []string{"shared"}, nil)
	f.write("game/src/index.ts", `import { value } from "../../shared/types"; export const result = value;`)
	f.rojo("default.project.json", "", map[string]any{"moved": map[string]string{"$path": "shared/next"}})
	rootConfig := filepath.Join(f.dir, "game/tsconfig.json")
	coordinator := f.coordinator("game")
	before := coordinatorMetadataWork(coordinator)
	f.setCompilerOption("shared", "outDir", "next")
	cleanStaleOutputs, err := coordinator.ReloadForWatch(rootConfig, f.options())
	if err != nil {
		t.Fatal(err)
	}
	f.invalidateAndDrain(coordinator, "shared")
	if err := cleanStaleOutputs(); err != nil {
		t.Fatal(err)
	}
	after := coordinatorMetadataWork(coordinator)
	if refreshes := after.refreshes - before.refreshes; refreshes != 1 {
		t.Fatalf("config change refreshed metadata %d times, want 1", refreshes)
	}
	assertGameImport(t, f, `game:GetService("ReplicatedStorage"), "moved")`)
}

func TestWatchOnDiskEnvDeclarationRefreshes(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("shared", nil, nil)
	f.project("game", []string{"shared"}, nil)
	coordinator := f.coordinator("game")
	before := coordinatorMetadataWork(coordinator)
	f.write("shared/src/"+EnvDeclFileName, envDeclText)
	f.invalidateAndDrain(coordinator, "shared")
	assertMetadataWork(t, coordinator, before, 2, 1)
}

// programWatchSet is the program-based watch set WatchSets used to build.
func programWatchSet(t *testing.T, project SolutionProject) (rootDirs, artifactDirectories []string) {
	t.Helper()
	dir, program, _, err := newProjectProgram(filepath.Dir(project.ConfigPath), project.ConfigPath)
	if err != nil {
		t.Fatal(err)
	}
	translator := createPathTranslator(program, !project.Options.LuaExtension)
	return getRootDirs(program), solutionWatchArtifactDirectories(dir, translator.OutDir, project.Options.IncludePath)
}

func TestWatchSetsMatchProgramWatchSets(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("shared", nil, map[string]any{"declarationDir": "types"})
	f.project("plain", nil, map[string]any{"rootDir": nil, "rootDirs": []string{"src", "extra"}})
	f.write("plain/extra/more.ts", "export const more = 1;\n")
	f.project("game", []string{"shared", "plain"}, nil)
	coordinator := f.coordinator("game")
	sets := coordinator.WatchSets()
	for index, project := range coordinator.graph.Projects {
		rootDirs, artifactDirectories := programWatchSet(t, project)
		if !reflect.DeepEqual(sets[index].RootDirs, rootDirs) || !reflect.DeepEqual(sets[index].ArtifactDirectories, artifactDirectories) {
			t.Fatalf("%s: watch set roots %v artifacts %v, program roots %v artifacts %v", project.ConfigPath, sets[index].RootDirs, sets[index].ArtifactDirectories, rootDirs, artifactDirectories)
		}
	}
}
