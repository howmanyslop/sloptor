package main

import (
	"context"
	"errors"
	"fmt"
	"maps"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"sort"
	"strings"
	"sync"
	"time"

	"rotor/internal/compile"
	"rotor/tsgo/fswatch"
)

type compileGate struct {
	mu               sync.Mutex
	running, pending bool
	done             chan struct{}
	run              func()
}

func newCompileGate(run func()) *compileGate { return &compileGate{run: run} }

func (g *compileGate) Trigger() {
	g.mu.Lock()
	if g.running {
		g.pending = true
		g.mu.Unlock()
		return
	}
	g.running, g.done = true, make(chan struct{})
	g.mu.Unlock()
	go func() {
		for {
			g.run()
			g.mu.Lock()
			if !g.pending {
				g.running = false
				close(g.done)
				g.mu.Unlock()
				return
			}
			g.pending = false
			g.mu.Unlock()
		}
	}()
}

func (g *compileGate) Drain() {
	for {
		g.mu.Lock()
		if !g.running {
			g.mu.Unlock()
			return
		}
		done := g.done
		g.mu.Unlock()
		<-done
	}
}

type solutionWatchEvents struct {
	projects map[string]struct{}
	configs  map[string]struct{}
	assets   map[string]map[string]bool
	paths    map[string]struct{}
	// dead holds the watches that stopped delivering events: their watched
	// path was deleted or the watch terminated.
	dead map[solutionWatchKey]struct{}
}

type solutionWatchKind uint8

const (
	solutionWatchRoot solutionWatchKind = iota
	solutionWatchRojo
	solutionWatchConfig
)

// solutionWatchKey names one watch: a project's root or Rojo directory, or
// one of its config files.
type solutionWatchKey struct {
	kind    solutionWatchKind
	project string
	path    string
}

// directory is the directory fswatch watches for key. Watches of one
// directory share one OS watch, so they die together.
func (key solutionWatchKey) directory() (string, bool) {
	if key.kind == solutionWatchConfig {
		return filepath.Dir(filepath.Clean(key.path)), false
	}
	return filepath.Clean(key.path), true
}

// addWatchError records the paths of events and a watch error, and reports
// whether events were missed. An overflow leaves the watch running; any
// other error, or a delete of the watched path, ends it.
func (s *solutionWatchEvents) addWatchError(key solutionWatchKey, events []fswatch.Event, watchErr error) bool {
	for _, event := range events {
		s.paths[event.Path] = struct{}{}
		if event.Kind == fswatch.EventDelete && filepath.Clean(event.Path) == filepath.Clean(key.path) {
			s.dead[key] = struct{}{}
		}
	}
	if watchErr != nil && !errors.Is(watchErr, fswatch.ErrOverflow) {
		s.dead[key] = struct{}{}
	}
	return watchErr != nil
}

func (s *solutionWatchEvents) add(key solutionWatchKey, events []fswatch.Event, watchErr error) {
	if s.addWatchError(key, events, watchErr) {
		s.projects[key.project] = struct{}{}
	}
	for _, event := range events {
		if watchCompilable(event.Path) {
			s.projects[key.project] = struct{}{}
			continue
		}
		if watchProjectInput(event) {
			s.projects[key.project] = struct{}{}
		}
		s.addAsset(key.project, event)
	}
}

func (s *solutionWatchEvents) addRojo(key solutionWatchKey, events []fswatch.Event, watchErr error) {
	if s.addWatchError(key, events, watchErr) {
		s.configs[key.project] = struct{}{}
	}
	for _, event := range events {
		if strings.HasSuffix(filepath.Base(event.Path), ".project.json") {
			s.configs[event.Path] = struct{}{}
			continue
		}
		if !watchCompilable(event.Path) {
			s.addAsset(key.project, event)
		}
	}
}

func (s *solutionWatchEvents) addConfig(key solutionWatchKey, events []fswatch.Event, watchErr error) {
	s.addWatchError(key, events, watchErr)
	s.configs[key.path] = struct{}{}
}

func (s *solutionWatchEvents) addAsset(project string, event fswatch.Event) {
	if s.assets[project] == nil {
		s.assets[project] = map[string]bool{}
	}
	s.assets[project][event.Path] = event.Kind == fswatch.EventDelete
}

func solutionWatchDirectories(set compile.SolutionWatchSet) []string {
	directories := make([]string, 0, len(set.RootDirs)+len(set.RojoDirectories))
	directories = append(directories, set.RootDirs...)
	for _, directory := range set.RojoDirectories {
		if solutionWatchDirectoryIsArtifact(directory, set.ArtifactDirectories) {
			continue
		}
		directories = append(directories, directory)
	}
	return directories
}

func solutionWatchDirectoryIsArtifact(directory string, artifactDirectories []string) bool {
	directory = filepath.Clean(directory)
	for _, artifactDirectory := range artifactDirectories {
		rel, err := filepath.Rel(filepath.Clean(artifactDirectory), directory)
		if err == nil && (rel == "." || (rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator)))) {
			return true
		}
	}
	return false
}

// styledSolutionWatchReporter renders `build --build --watch` passes in the
// terminal: only the build result, with no change list or idle stats (the
// solution watcher's output before reporters existed).
type styledSolutionWatchReporter struct{ maxErrors int }

func (r *styledSolutionWatchReporter) buildStart([]string) {}

func (r *styledSolutionWatchReporter) buildEnd(result *compile.BuildResult, diags []compile.DiagnosticInfo, elapsed time.Duration, err error) {
	reportBuildPass(newUI(fmtWriter{}), result, diags, elapsed, err, &watchStats{maxErrors: r.maxErrors})
}

func (r *styledSolutionWatchReporter) watching(func() int) {}

func runBuildSolutionWatch(tsConfigPath string, opts projectOptions, reload func() (projectOptions, error), wopts watchOptions) int {
	return runBuildSolutionWatchLoop(context.Background(), tsConfigPath, opts, reload, &styledSolutionWatchReporter{maxErrors: wopts.maxErrors})
}

func newSolutionWatchEvents() *solutionWatchEvents {
	return &solutionWatchEvents{projects: map[string]struct{}{}, configs: map[string]struct{}{}, assets: map[string]map[string]bool{}, paths: map[string]struct{}{}, dead: map[solutionWatchKey]struct{}{}}
}

// newSolutionWatcher is the file watcher; tests replace it.
var newSolutionWatcher = fswatch.Default

// solutionWatches keeps one watch per key of the solution's watch sets.
type solutionWatches struct {
	watcher fswatch.Watcher
	// callback returns the event handler of key.
	callback func(key solutionWatchKey) fswatch.WatchCallback
	watches  map[solutionWatchKey]fswatch.Watch
	sets     []compile.SolutionWatchSet
	// armed is false while a wanted watch failed to open.
	armed bool
}

// refresh re-arms the watches. It keeps them when the watch sets are
// unchanged, every watch is open, and no watch died. Otherwise it closes the
// watches that are no longer wanted or share a directory with a dead watch,
// and opens the missing ones; the others stay open.
func (w *solutionWatches) refresh(sets []compile.SolutionWatchSet, dead map[solutionWatchKey]struct{}) {
	if len(dead) == 0 && w.armed && reflect.DeepEqual(sets, w.sets) {
		return
	}
	keys := solutionWatchKeys(sets)
	wanted := make(map[solutionWatchKey]struct{}, len(keys))
	for _, key := range keys {
		wanted[key] = struct{}{}
	}
	type directory struct {
		path      string
		recursive bool
	}
	deadDirectories := map[directory]struct{}{}
	for key := range dead {
		path, recursive := key.directory()
		deadDirectories[directory{path, recursive}] = struct{}{}
	}
	for key, watch := range w.watches {
		path, recursive := key.directory()
		_, isWanted := wanted[key]
		if _, isDead := deadDirectories[directory{path, recursive}]; isWanted && !isDead {
			continue
		}
		_ = watch.Close()
		delete(w.watches, key)
	}
	w.sets = sets
	w.armed = true
	for _, key := range keys {
		if _, ok := w.watches[key]; ok {
			continue
		}
		var watch fswatch.Watch
		var err error
		if key.kind == solutionWatchConfig {
			watch, err = w.watcher.WatchFile(key.path, w.callback(key))
		} else {
			watch, err = w.watcher.WatchDirectory(key.path, w.callback(key), fswatch.WithRecursive())
		}
		if err != nil {
			w.armed = false
			continue
		}
		w.watches[key] = watch
	}
}

func (w *solutionWatches) close() {
	for key, watch := range w.watches {
		_ = watch.Close()
		delete(w.watches, key)
	}
}

func solutionWatchKeys(sets []compile.SolutionWatchSet) []solutionWatchKey {
	keys := []solutionWatchKey{}
	for _, set := range sets {
		for index, directory := range solutionWatchDirectories(set) {
			kind := solutionWatchRoot
			if index >= len(set.RootDirs) {
				kind = solutionWatchRojo
			}
			keys = append(keys, solutionWatchKey{kind: kind, project: set.ProjectPath, path: directory})
		}
		for _, config := range append(slices.Clone(set.TsConfigPaths), set.RojoConfigs...) {
			keys = append(keys, solutionWatchKey{kind: solutionWatchConfig, project: set.ProjectPath, path: config})
		}
	}
	return keys
}

// runBuildSolutionWatchLoop builds the solution once, then rebuilds the
// affected projects on every file-system change until ctx is done. It returns
// 1 when the solution graph cannot be loaded.
func runBuildSolutionWatchLoop(ctx context.Context, tsConfigPath string, opts projectOptions, reload func() (projectOptions, error), rep buildWatchReporter) int {
	s := newSolutionWatchEvents()
	coordinator, err := compile.NewSolutionCoordinator(tsConfigPath, projectCompileOptions(tsConfigPath, opts))
	if err != nil {
		rep.buildStart(nil)
		rep.buildEnd(nil, nil, 0, err)
		return 1
	}
	var mu sync.Mutex
	stopped := false // guarded by mu; set once ctx is done
	watches := &solutionWatches{watcher: newSolutionWatcher(), watches: map[solutionWatchKey]fswatch.Watch{}}
	refresh := func(dead map[solutionWatchKey]struct{}) {
		watches.refresh(coordinator.WatchSets(), dead)
	}
	// rebuild invalidates and drains the changed projects, first reloading
	// options and the graph when a config changed. reloaded is false when
	// that reload failed; the dead watches are reopened even then.
	rebuild := func(events solutionWatchEvents) (*compile.BuildResult, []compile.DiagnosticInfo, bool, error) {
		var cleanStaleOutputs func() error
		if len(events.configs) > 0 {
			for _, set := range coordinator.WatchSets() {
				for _, config := range set.TsConfigPaths {
					if _, changed := events.configs[config]; changed {
						events.projects[set.ProjectPath] = struct{}{}
					}
				}
			}
			next, err := reload()
			if err != nil {
				refresh(events.dead)
				return nil, nil, false, err
			}
			opts = next
			cleanStaleOutputs, err = coordinator.ReloadForWatch(tsConfigPath, projectCompileOptions(tsConfigPath, opts))
			if err != nil {
				refresh(events.dead)
				return nil, nil, false, err
			}
			if len(events.projects) == 0 {
				for _, set := range coordinator.WatchSets() {
					events.projects[set.ProjectPath] = struct{}{}
				}
			}
		}
		for project := range events.projects {
			coordinator.Invalidate(project)
		}
		result, messages, err := coordinator.Drain()
		if err == nil && cleanStaleOutputs != nil {
			err = cleanStaleOutputs()
		}
		refresh(events.dead)
		return result, buildDiagnostics(result, messages), true, err
	}
	initial := true
	cycle := func() {
		mu.Lock()
		events := *s
		s = newSolutionWatchEvents()
		mu.Unlock()
		if len(events.projects) > 0 || len(events.configs) > 0 {
			var changed []string
			if !initial {
				changed = slices.Sorted(maps.Keys(events.paths))
			}
			start := time.Now()
			rep.buildStart(changed)
			result, diags, reloaded, err := rebuild(events)
			rep.buildEnd(result, diags, time.Since(start), err)
			if initial {
				initial = false
				rep.watching(func() int { return solutionWatchedFileCount(coordinator.WatchSets()) })
			}
			if !reloaded {
				return
			}
		}
		for project, paths := range events.assets {
			changes := make([]compile.WatchAssetEvent, 0, len(paths))
			for path, deleted := range paths {
				changes = append(changes, compile.WatchAssetEvent{Path: path, Deleted: deleted})
			}
			sort.Slice(changes, func(i, j int) bool { return changes[i].Path < changes[j].Path })
			if err := coordinator.DrainAssets(project, changes); err != nil {
				fmt.Fprintln(stderrWriter{}, err)
			}
		}
	}
	gate := newCompileGate(cycle)
	watches.callback = func(key solutionWatchKey) fswatch.WatchCallback {
		return func(events []fswatch.Event, watchErr error) {
			mu.Lock()
			defer mu.Unlock()
			if stopped {
				return
			}
			switch key.kind {
			case solutionWatchRoot:
				s.add(key, events, watchErr)
			case solutionWatchRojo:
				s.addRojo(key, events, watchErr)
			default:
				s.addConfig(key, events, watchErr)
			}
			gate.Trigger()
		}
	}
	refresh(nil)
	for _, set := range coordinator.WatchSets() {
		s.projects[set.ProjectPath] = struct{}{}
	}
	gate.Trigger()
	<-ctx.Done()
	// Stop callbacks from queueing cycles, let the running one finish (it may
	// refresh the watches), then close the watches it left behind.
	mu.Lock()
	stopped = true
	mu.Unlock()
	gate.Drain()
	watches.close()
	return 0
}

// solutionWatchedFileCount counts the distinct files the solution watcher
// observes: every file under the watched directories plus the config files.
func solutionWatchedFileCount(sets []compile.SolutionWatchSet) int {
	seen := map[string]struct{}{}
	for _, set := range sets {
		for _, directory := range solutionWatchDirectories(set) {
			for path := range newTreeWatcher(directory, set.ArtifactDirectories...).snapshot() {
				seen[path] = struct{}{}
			}
		}
		for _, config := range append(set.TsConfigPaths, set.RojoConfigs...) {
			seen[config] = struct{}{}
		}
	}
	return len(seen)
}

func watchCompilable(path string) bool {
	return filepath.Ext(path) == ".ts" || filepath.Ext(path) == ".tsx" || filepath.Ext(path) == ".d.ts"
}

// watchProjectInput reports whether a non-source event under a project root
// can change the project's file set: a JSON file (a source with
// resolveJsonModule) or a directory (a folder rename or move reports only
// the folder). A path with no extension counts as a directory, since a
// removed one cannot be checked; another path is checked only while it
// exists.
func watchProjectInput(event fswatch.Event) bool {
	switch filepath.Ext(event.Path) {
	case ".json", "":
		return true
	}
	if event.Kind == fswatch.EventDelete {
		return false
	}
	info, err := os.Stat(event.Path)
	return err == nil && info.IsDir()
}

type fmtWriter struct{}

func (fmtWriter) Write(p []byte) (int, error) { return fmt.Print(string(p)) }

type stderrWriter struct{}

func (stderrWriter) Write(p []byte) (int, error) { return os.Stderr.Write(p) }
