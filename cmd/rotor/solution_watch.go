package main

import (
	"context"
	"errors"
	"fmt"
	"maps"
	"os"
	"path/filepath"
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
}

func (s *solutionWatchEvents) add(project string, events []fswatch.Event, watchErr error) {
	if watchErr != nil && !errors.Is(watchErr, fswatch.ErrOverflow) {
		return
	}
	for _, event := range events {
		s.paths[event.Path] = struct{}{}
		if watchCompilable(event.Path) {
			s.projects[project] = struct{}{}
			continue
		}
		s.addAsset(project, event)
	}
	if watchErr != nil {
		s.projects[project] = struct{}{}
	}
}

func (s *solutionWatchEvents) addRojo(project string, events []fswatch.Event, watchErr error) {
	if watchErr != nil && !errors.Is(watchErr, fswatch.ErrOverflow) {
		return
	}
	for _, event := range events {
		s.paths[event.Path] = struct{}{}
		if strings.HasSuffix(filepath.Base(event.Path), ".project.json") {
			s.configs[event.Path] = struct{}{}
			continue
		}
		if !watchCompilable(event.Path) {
			s.addAsset(project, event)
		}
	}
	if watchErr != nil {
		s.configs[project] = struct{}{}
	}
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
	return &solutionWatchEvents{projects: map[string]struct{}{}, configs: map[string]struct{}{}, assets: map[string]map[string]bool{}, paths: map[string]struct{}{}}
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
	var watches []fswatch.Watch
	watcher := fswatch.Default()
	refresh := func() {}
	// rebuild invalidates and drains the changed projects, first reloading
	// options and the graph when a config changed. reloaded is false when
	// that reload failed.
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
				return nil, nil, false, err
			}
			opts = next
			cleanStaleOutputs, err = coordinator.ReloadForWatch(tsConfigPath, projectCompileOptions(tsConfigPath, opts))
			if err != nil {
				return nil, nil, false, err
			}
			refresh()
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
		refresh()
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
	refresh = func() {
		for _, watch := range watches {
			_ = watch.Close()
		}
		watches = nil
		for _, set := range coordinator.WatchSets() {
			project := set.ProjectPath
			rootCallback := func(events []fswatch.Event, watchErr error) {
				mu.Lock()
				defer mu.Unlock()
				if stopped {
					return
				}
				s.add(project, events, watchErr)
				gate.Trigger()
			}
			rojoCallback := func(events []fswatch.Event, watchErr error) {
				mu.Lock()
				defer mu.Unlock()
				if stopped {
					return
				}
				s.addRojo(project, events, watchErr)
				gate.Trigger()
			}
			for index, directory := range solutionWatchDirectories(set) {
				callback := rootCallback
				if index >= len(set.RootDirs) {
					callback = rojoCallback
				}
				if watch, watchErr := watcher.WatchDirectory(directory, callback, fswatch.WithRecursive()); watchErr == nil {
					watches = append(watches, watch)
				}
			}
			for _, config := range append(set.TsConfigPaths, set.RojoConfigs...) {
				if watch, watchErr := watcher.WatchFile(config, func(events []fswatch.Event, watchErr error) {
					mu.Lock()
					defer mu.Unlock()
					if stopped {
						return
					}
					s.configs[config] = struct{}{}
					for _, event := range events {
						s.paths[event.Path] = struct{}{}
					}
					gate.Trigger()
				}); watchErr == nil {
					watches = append(watches, watch)
				}
			}
		}
	}
	refresh()
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
	for _, watch := range watches {
		_ = watch.Close()
	}
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

type fmtWriter struct{}

func (fmtWriter) Write(p []byte) (int, error) { return fmt.Print(string(p)) }

type stderrWriter struct{}

func (stderrWriter) Write(p []byte) (int, error) { return os.Stderr.Write(p) }
