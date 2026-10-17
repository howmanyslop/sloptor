package main

import (
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"runtime"
	"testing"

	"golang.org/x/text/unicode/norm"

	"rotor/internal/compile"
	"rotor/tsgo/fswatch"
)

func recordingSolutionWatches(watcher *fakeSolutionWatcher, events **solutionWatchEvents) *solutionWatches {
	return &solutionWatches{
		watcher: watcher,
		watches: map[solutionWatchKey]fswatch.Watch{},
		callback: func(key solutionWatchKey) fswatch.WatchCallback {
			return func(batch []fswatch.Event, err error) {
				switch key.kind {
				case solutionWatchRoot:
					(*events).add(key, batch, err)
				case solutionWatchRojo:
					(*events).addRojo(key, batch, err)
				default:
					(*events).addConfig(key, batch, err)
				}
			}
		},
	}
}

func TestSolutionWatchSharedCoverageRoutesEveryOwner(t *testing.T) {
	root := t.TempDir()
	source := filepath.Join(root, "src")
	nested := filepath.Join(source, "nested")
	first, second, rojo := "first", "second", "assets"
	sets := []compile.SolutionWatchSet{
		{ProjectPath: first, RootDirs: []string{source}, TsConfigPaths: []string{filepath.Join(root, "first.json")}},
		{ProjectPath: second, RootDirs: []string{nested}},
		{ProjectPath: rojo, RojoDirectories: []string{root, source, nested}},
	}
	watcher := newFakeSolutionWatcher()
	events := newSolutionWatchEvents()
	watches := recordingSolutionWatches(watcher, &events)
	watches.refresh(sets, nil)
	t.Cleanup(watches.close)
	if len(watches.watches) != 2 {
		t.Fatalf("subscriptions = %d, want one recursive root and one config", len(watches.watches))
	}
	if opens, _ := watcher.counts(source); opens != 0 {
		t.Fatalf("nested source opened %d subscriptions", opens)
	}
	path := filepath.Join(nested, "file.ts")
	watcher.deliver(t, root, []fswatch.Event{{Path: path, Kind: fswatch.EventUpdate}}, nil)
	if !reflect.DeepEqual(events.projects, map[string]struct{}{first: {}, second: {}}) {
		t.Fatalf("source owners = %v", events.projects)
	}
	if len(events.assets) != 0 || len(events.configs) != 0 {
		t.Fatalf("source event leaked into Rojo/config routes: %+v", events)
	}
	for _, name := range []string{"data.json", "folder", "icon.png"} {
		events = newSolutionWatchEvents()
		path = filepath.Join(nested, name)
		watcher.deliver(t, root, []fswatch.Event{{Path: path, Kind: fswatch.EventDelete}}, nil)
		for _, owner := range []string{first, second, rojo} {
			if !events.assets[owner][path] {
				t.Errorf("%s: owner %s did not receive deletion: %v", name, owner, events.assets)
			}
		}
		if want := name != "icon.png"; (len(events.projects) == 2) != want {
			t.Errorf("%s: invalidated projects = %v", name, events.projects)
		}
	}
	events = newSolutionWatchEvents()
	watcher.deliver(t, root, []fswatch.Event{{Path: filepath.Join(root, "elsewhere.ts"), Kind: fswatch.EventUpdate}}, nil)
	if len(events.projects) != 0 {
		t.Fatalf("unrelated source invalidated projects: %v", events.projects)
	}
	events = newSolutionWatchEvents()
	config := filepath.Join(root, "first.json")
	watcher.deliver(t, config, []fswatch.Event{{Path: config, Kind: fswatch.EventUpdate}}, nil)
	if _, found := events.configs[config]; !found {
		t.Fatalf("config event = %v", events.configs)
	}

	// Reconfigure ownership without changing the shared physical root.
	watches.refresh(sets[1:], nil)
	events = newSolutionWatchEvents()
	watcher.deliver(t, root, []fswatch.Event{{Path: filepath.Join(nested, "file.ts"), Kind: fswatch.EventUpdate}}, nil)
	if !reflect.DeepEqual(events.projects, map[string]struct{}{second: {}}) {
		t.Fatalf("owners after removal = %v", events.projects)
	}
	if opens, closes := watcher.counts(root); opens != 1 || closes != 0 {
		t.Fatalf("retained coverage opened %d closed %d", opens, closes)
	}
}

func TestSolutionWatchSharedCoverageExcludesSolutionArtifacts(t *testing.T) {
	root := t.TempDir()
	out := filepath.Join(root, "game", "out")
	include := filepath.Join(root, "game", "include")
	cache := filepath.Join(root, "game", ".rotor")
	external := filepath.Join(root, "external-assets")
	sets := []compile.SolutionWatchSet{
		{ProjectPath: "game", RootDirs: []string{filepath.Join(root, "game", "src")}, ArtifactDirectories: []string{out, include, cache}},
		{ProjectPath: "logger", RojoDirectories: []string{root, out, filepath.Join(out, "nested"), include, cache, external}, RojoConfigs: []string{filepath.Join(out, "generated.project.json")}},
		// Explicit source roots remain inputs, including generated dependencies.
		{ProjectPath: "generated-consumer", RootDirs: []string{out}},
		{ProjectPath: "workspace", RootDirs: []string{root}},
	}
	watcher := newFakeSolutionWatcher()
	events := newSolutionWatchEvents()
	watches := recordingSolutionWatches(watcher, &events)
	watches.refresh(sets, nil)
	t.Cleanup(watches.close)
	if len(watches.watches) != 1 {
		t.Fatalf("subscriptions = %d, want one recursive root", len(watches.watches))
	}
	for _, artifact := range []string{out, include, cache} {
		events = newSolutionWatchEvents()
		watcher.deliver(t, root, []fswatch.Event{{Path: filepath.Join(artifact, "generated.json"), Kind: fswatch.EventUpdate}}, nil)
		if len(events.assets["logger"]) != 0 || len(events.configs) != 0 {
			t.Errorf("artifact %s reached inherited Rojo: %+v", artifact, events)
		}
	}
	events = newSolutionWatchEvents()
	watcher.deliver(t, root, []fswatch.Event{{Path: filepath.Join(out, "input.d.ts"), Kind: fswatch.EventUpdate}}, nil)
	if !reflect.DeepEqual(events.projects, map[string]struct{}{"generated-consumer": {}}) {
		t.Fatalf("generated input invalidated %v, want only its explicit owner", events.projects)
	}
	events = newSolutionWatchEvents()
	asset := filepath.Join(external, "icon.png")
	watcher.deliver(t, root, []fswatch.Event{{Path: asset, Kind: fswatch.EventUpdate}}, nil)
	if _, ok := events.assets["logger"][asset]; !ok {
		t.Fatalf("external asset was filtered: %v", events.assets)
	}
}

func TestSolutionWatchSharedCoverageRecovery(t *testing.T) {
	root := t.TempDir()
	child := filepath.Join(root, "child")
	sets := []compile.SolutionWatchSet{
		{ProjectPath: "parent", RootDirs: []string{root}},
		{ProjectPath: "child", RootDirs: []string{child}},
	}
	watcher := newFakeSolutionWatcher()
	watcher.fail[filepath.ToSlash(root)] = true
	events := newSolutionWatchEvents()
	watches := recordingSolutionWatches(watcher, &events)
	watches.refresh(sets, nil)
	t.Cleanup(watches.close)
	if watches.armed || len(watches.watches) != 1 {
		t.Fatalf("failed parent: armed %v subscriptions %d", watches.armed, len(watches.watches))
	}
	watcher.deliver(t, child, []fswatch.Event{{Path: filepath.Join(child, "file.ts"), Kind: fswatch.EventUpdate}}, nil)
	if len(events.projects) != 2 {
		t.Fatalf("fallback coverage lost owners: %v", events.projects)
	}
	delete(watcher.fail, filepath.ToSlash(root))
	watches.refresh(sets, nil)
	if !watches.armed || len(watches.watches) != 1 {
		t.Fatalf("retry: armed %v subscriptions %d", watches.armed, len(watches.watches))
	}
	if _, closes := watcher.counts(child); closes != 1 {
		t.Fatalf("fallback child closed %d times", closes)
	}
	events = newSolutionWatchEvents()
	watcher.deliver(t, root, nil, fswatch.ErrOverflow)
	watches.refresh(sets, events.dead)
	if len(events.projects) != 2 || len(events.dead) != 0 {
		t.Fatalf("overflow routing: %+v", events)
	}
	if opens, closes := watcher.counts(root); opens != 2 || closes != 0 {
		t.Fatalf("overflow reopened coverage: opens %d closes %d", opens, closes)
	}
	for _, err := range []error{fswatch.ErrWatchTerminated, errors.New("backend error")} {
		events = newSolutionWatchEvents()
		watcher.deliver(t, root, nil, err)
		if len(events.projects) != 2 || len(events.dead) != 2 {
			t.Fatalf("terminal error routing: %+v", events)
		}
		watches.refresh(sets, events.dead)
	}
	if opens, closes := watcher.counts(root); opens != 4 || closes != 2 {
		t.Fatalf("terminal recovery: opens %d closes %d", opens, closes)
	}
	// Deleting and recreating a child root under live ancestor coverage does
	// not drop that child's subscription or the parent's ownership.
	for _, kind := range []fswatch.EventKind{fswatch.EventDelete, fswatch.EventUpdate} {
		events = newSolutionWatchEvents()
		watcher.deliver(t, root, []fswatch.Event{{Path: child, Kind: kind}}, nil)
		if len(events.projects) != 2 {
			t.Fatalf("root event %v lost owners: %+v", kind, events)
		}
		watches.refresh(sets, events.dead)
	}
	watches.close()
	if len(watches.watches) != 0 || len(watches.dispatch) != 0 {
		t.Fatal("close retained subscriptions or routes")
	}
	events = newSolutionWatchEvents()
	watcher.deliver(t, root, nil, fswatch.ErrWatchTerminated)
	if len(events.projects) != 0 {
		t.Fatal("closed subscription retained callbacks")
	}
}

func TestSolutionWatchCanonicalMissingPath(t *testing.T) {
	root := t.TempDir()
	path := filepath.Join(root, "missing", "nested", "src")
	canonical, err := filepath.EvalSymlinks(root)
	if err != nil {
		t.Fatal(err)
	}
	if got, want := solutionWatchCanonicalPath(path), filepath.Join(canonical, "missing", "nested", "src"); got != want {
		t.Fatalf("canonical missing path = %s, want %s", got, want)
	}
}

func TestSolutionWatchSharedCoverageDottedRootRemoval(t *testing.T) {
	for _, containingDirectory := range []bool{false, true} {
		name := "source root"
		if containingDirectory {
			name = "containing directory"
		}
		t.Run(name, func(t *testing.T) {
			root := t.TempDir()
			group := filepath.Join(root, "group.v1")
			source := filepath.Join(group, "src.v1")
			if err := os.MkdirAll(source, 0o755); err != nil {
				t.Fatal(err)
			}
			sets := []compile.SolutionWatchSet{
				{ProjectPath: "assets", RojoDirectories: []string{root}},
				{ProjectPath: "source", RootDirs: []string{source}},
				{ProjectPath: "sibling", RootDirs: []string{filepath.Join(root, "sibling")}},
			}
			watcher := newFakeSolutionWatcher()
			events := newSolutionWatchEvents()
			watches := recordingSolutionWatches(watcher, &events)
			watches.refresh(sets, nil)
			t.Cleanup(watches.close)
			if len(watches.watches) != 1 {
				t.Fatalf("subscriptions = %d, want shared ancestor coverage", len(watches.watches))
			}
			removed := source
			if containingDirectory {
				removed = group
			}
			if err := os.Rename(removed, filepath.Join(root, "moved")); err != nil {
				t.Fatal(err)
			}
			// The ancestor watch remains live: the backend reports a directory
			// deletion with no terminal error for the coalesced source route.
			watcher.deliver(t, root, []fswatch.Event{{Path: removed, Kind: fswatch.EventDelete}}, nil)
			if !reflect.DeepEqual(events.projects, map[string]struct{}{"source": {}}) {
				t.Fatalf("removed dotted root invalidated %v, want only its source owner", events.projects)
			}
			key := solutionWatchKey{kind: solutionWatchRoot, project: "source", path: source}
			if _, dead := events.dead[key]; !dead {
				t.Fatalf("removed source route was not rearmed: %v", events.dead)
			}
			watches.refresh(sets, events.dead)
			if opens, closes := watcher.counts(root); opens != 2 || closes != 1 {
				t.Fatalf("rearmed coverage opened %d closed %d, want 2 and 1", opens, closes)
			}
			if err := os.MkdirAll(source, 0o755); err != nil {
				t.Fatal(err)
			}
			for _, path := range []string{source, filepath.Join(source, "file.ts")} {
				events = newSolutionWatchEvents()
				watcher.deliver(t, root, []fswatch.Event{{Path: path, Kind: fswatch.EventUpdate}}, nil)
				if _, invalidated := events.projects["source"]; !invalidated {
					t.Fatalf("recreated dotted root lost event %s: %v", path, events.projects)
				}
			}
		})
	}
}

func TestSolutionWatchSharedCoverageSymlink(t *testing.T) {
	root := t.TempDir()
	real := filepath.Join(root, "real")
	if err := os.MkdirAll(filepath.Join(real, "src"), 0o755); err != nil {
		t.Fatal(err)
	}
	alias := filepath.Join(root, "alias")
	if err := os.Symlink(real, alias); err != nil {
		t.Skipf("cannot create symlink: %v", err)
	}
	sets := []compile.SolutionWatchSet{
		{ProjectPath: "real", RootDirs: []string{real}, ArtifactDirectories: []string{filepath.Join(real, "out")}},
		{ProjectPath: "alias", RootDirs: []string{filepath.Join(alias, "src")}, RojoConfigs: []string{filepath.Join(alias, "out", "generated.project.json")}},
	}
	watcher := newFakeSolutionWatcher()
	events := newSolutionWatchEvents()
	watches := recordingSolutionWatches(watcher, &events)
	watches.refresh(sets, nil)
	t.Cleanup(watches.close)
	if len(watches.watches) != 1 {
		t.Fatalf("symlink subscriptions = %d, want one canonical root", len(watches.watches))
	}
	watcher.deliver(t, real, []fswatch.Event{{Path: filepath.Join(real, "src", "data.json"), Kind: fswatch.EventUpdate}}, nil)
	if len(events.projects) != 2 {
		t.Fatalf("symlink owners = %v", events.projects)
	}
	if _, ok := events.assets["alias"][filepath.Join(alias, "src", "data.json")]; !ok {
		t.Fatalf("symlink event was not rebased: %v", events.assets)
	}
}

func TestSolutionWatchSharedCoverageUnicodePaths(t *testing.T) {
	root := t.TempDir()
	decomposed := filepath.Join(root, "cafe\u0301")
	source := filepath.Join(decomposed, "src")
	artifact := filepath.Join(decomposed, "out")
	config := filepath.Join(decomposed, "tsconfig.json")
	if err := os.MkdirAll(source, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(config, []byte("{}"), 0o644); err != nil {
		t.Fatal(err)
	}
	sets := []compile.SolutionWatchSet{
		{ProjectPath: "assets", RojoDirectories: []string{root}, ArtifactDirectories: []string{artifact}},
		{ProjectPath: "source", RootDirs: []string{source}, TsConfigPaths: []string{config}},
	}
	watcher := newFakeSolutionWatcher()
	events := newSolutionWatchEvents()
	watches := recordingSolutionWatches(watcher, &events)
	watches.refresh(sets, nil)
	t.Cleanup(watches.close)
	wantEquivalent := runtime.GOOS == "darwin"
	canonicalSource := solutionWatchCanonicalPath(source)
	if norm.NFC.IsNormalString(canonicalSource) != wantEquivalent {
		t.Fatalf("canonical source %q, want NFC=%v", canonicalSource, wantEquivalent)
	}
	// FSEvents delivers NFC even when the configured input root uses NFD.
	composedSource := norm.NFC.String(canonicalSource)
	watcher.deliver(t, root, []fswatch.Event{{Path: filepath.Join(composedSource, "file.ts"), Kind: fswatch.EventUpdate}}, nil)
	if _, invalidated := events.projects["source"]; invalidated != wantEquivalent {
		t.Fatalf("NFC source invalidated=%v, want %v", invalidated, wantEquivalent)
	}
	events = newSolutionWatchEvents()
	watcher.deliver(t, root, []fswatch.Event{{Path: filepath.Join(composedSource, "icon.png"), Kind: fswatch.EventUpdate}}, nil)
	if _, rebased := events.assets["source"][filepath.Join(source, "icon.png")]; rebased != wantEquivalent {
		t.Fatalf("NFC source asset rebased=%v, want %v: %v", rebased, wantEquivalent, events.assets)
	}
	events = newSolutionWatchEvents()
	canonicalConfig := solutionWatchCanonicalPath(config)
	watcher.deliver(t, canonicalConfig, []fswatch.Event{{Path: norm.NFC.String(canonicalConfig), Kind: fswatch.EventDelete}}, nil)
	if _, reloaded := events.configs[config]; reloaded != wantEquivalent {
		t.Fatalf("NFC config reloaded=%v, want %v", reloaded, wantEquivalent)
	}
	if len(events.dead) > 0 != wantEquivalent {
		t.Fatalf("NFC config deletion lost watch recovery: %v", events.dead)
	}
	events = newSolutionWatchEvents()
	artifactPath := filepath.Join(norm.NFC.String(solutionWatchCanonicalPath(artifact)), "generated.png")
	watcher.deliver(t, root, []fswatch.Event{{Path: artifactPath, Kind: fswatch.EventUpdate}}, nil)
	if delivered := len(events.assets["assets"]) != 0; delivered == wantEquivalent {
		t.Fatalf("NFC artifact delivered=%v, want %v", delivered, !wantEquivalent)
	}
}

func TestSolutionWatchedFileCountCanonicalizesAliases(t *testing.T) {
	root := t.TempDir()
	real := filepath.Join(root, "real")
	for _, directory := range []string{"src", "out"} {
		if err := os.MkdirAll(filepath.Join(real, directory), 0o755); err != nil {
			t.Fatal(err)
		}
	}
	for _, path := range []string{"src/main.ts", "out/generated.luau", "tsconfig.json"} {
		if err := os.WriteFile(filepath.Join(real, path), []byte("fixture\n"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	alias := filepath.Join(root, "alias")
	if err := os.Symlink(real, alias); err != nil {
		t.Skipf("cannot create symlink: %v", err)
	}
	sets := []compile.SolutionWatchSet{
		{ProjectPath: "source", RootDirs: []string{alias}, TsConfigPaths: []string{filepath.Join(alias, "tsconfig.json")}, ArtifactDirectories: []string{filepath.Join(real, "out")}},
		{ProjectPath: "rojo", RojoDirectories: []string{real}, RojoConfigs: []string{filepath.Join(real, "tsconfig.json")}},
	}
	if got := solutionWatchedFileCount(sets); got != 2 {
		t.Fatalf("aliased count = %d, want one source and one config, excluding generated output", got)
	}
	sets = append(sets, compile.SolutionWatchSet{ProjectPath: "generated-consumer", RootDirs: []string{filepath.Join(alias, "out")}})
	if got := solutionWatchedFileCount(sets); got != 3 {
		t.Fatalf("count with explicit generated input = %d, want source, config and declared generated input", got)
	}
}
