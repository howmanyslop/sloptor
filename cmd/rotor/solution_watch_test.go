package main

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"sync"
	"testing"
	"time"

	"rotor/internal/compile"
	"rotor/tsgo/fswatch"
)

func TestCompileGate(t *testing.T) {
	started := make(chan struct{}, 2)
	release := make(chan struct{}, 2)
	transcript := []string{}
	gate := newCompileGate(func() {
		transcript = append(transcript, "start")
		started <- struct{}{}
		<-release
		transcript = append(transcript, "end")
	})

	gate.Trigger()
	<-started
	gate.Trigger()
	gate.Trigger()
	release <- struct{}{}
	<-started
	release <- struct{}{}
	gate.Drain()

	if want := []string{"start", "end", "start", "end"}; !reflect.DeepEqual(transcript, want) {
		t.Fatalf("gate transcript = %v, want %v", transcript, want)
	}
}

func TestSolutionWatchRojoDirectoryAssetEvent(t *testing.T) {
	project := "/project/tsconfig.json"
	assetPath := "/project/assets/nested/icon.png"
	configPath := "/project/assets/nested/subtree.project.json"
	events := newSolutionWatchEvents()
	set := compile.SolutionWatchSet{
		RootDirs:            []string{"/project/src"},
		RojoDirectories:     []string{"/project/assets"},
		ArtifactDirectories: []string{"/project/out", "/project/include", "/project/.rotor"},
	}

	events.addRojo(solutionWatchKey{kind: solutionWatchRojo, project: project, path: "/project/assets"}, []fswatch.Event{
		{Kind: fswatch.EventDelete, Path: assetPath},
		{Kind: fswatch.EventUpdate, Path: configPath},
		{Kind: fswatch.EventUpdate, Path: "/project/assets/nested/source.ts"},
	}, nil)

	if got := solutionWatchDirectories(set); !reflect.DeepEqual(got, []string{"/project/src", "/project/assets"}) {
		t.Fatalf("solutionWatchDirectories() = %v, want root and Rojo directories", got)
	}
	if deleted, ok := events.assets[project][assetPath]; !ok || !deleted {
		t.Fatalf("asset event = %v, want deleted Rojo-directory asset", events.assets)
	}
	if _, ok := events.configs[configPath]; !ok {
		t.Fatalf("configs = %v, want Rojo project config %q", events.configs, configPath)
	}
	if len(events.projects) != 0 {
		t.Fatalf("projects = %v, want no project invalidation for Rojo-directory source event", events.projects)
	}
}

func TestWatchExcludesOutDir(t *testing.T) {
	set := compile.SolutionWatchSet{
		RootDirs: []string{"/project/src"},
		RojoDirectories: []string{
			"/project/assets",
			"/project/out",
			"/project/out/nested",
			"/project/include",
			"/project/.rotor/cache",
		},
		ArtifactDirectories: []string{
			"/project/out",
			"/project/include",
			"/project/.rotor",
		},
	}

	if got := solutionWatchDirectories(set); !reflect.DeepEqual(got, []string{"/project/src", "/project/assets"}) {
		t.Fatalf("solutionWatchDirectories() = %v, want source and asset directories only", got)
	}
}

func TestStderrWriter(t *testing.T) {
	stdoutReader, stdoutWriter, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	stderrReader, stderrWriterPipe, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	previousStdout, previousStderr := os.Stdout, os.Stderr
	os.Stdout, os.Stderr = stdoutWriter, stderrWriterPipe
	t.Cleanup(func() {
		os.Stdout, os.Stderr = previousStdout, previousStderr
		_ = stdoutReader.Close()
		_ = stdoutWriter.Close()
		_ = stderrReader.Close()
		_ = stderrWriterPipe.Close()
	})

	if _, err := fmt.Fprint(stderrWriter{}, "watch error\n"); err != nil {
		t.Fatal(err)
	}
	if err := stdoutWriter.Close(); err != nil {
		t.Fatal(err)
	}
	if err := stderrWriterPipe.Close(); err != nil {
		t.Fatal(err)
	}
	stdout, err := io.ReadAll(stdoutReader)
	if err != nil {
		t.Fatal(err)
	}
	stderr, err := io.ReadAll(stderrReader)
	if err != nil {
		t.Fatal(err)
	}
	if string(stdout) != "" {
		t.Fatalf("stdout = %q, want empty", stdout)
	}
	if string(stderr) != "watch error\n" {
		t.Fatalf("stderr = %q, want watch error", stderr)
	}
}

// fakeSolutionWatcher records the watches the solution watcher opens and
// lets a test deliver events to them.
type fakeSolutionWatcher struct {
	fswatch.Watcher
	mu        sync.Mutex
	opens     map[string]int
	closes    map[string]int
	fail      map[string]bool
	callbacks map[string]fswatch.WatchCallback
}

type fakeSolutionWatch struct {
	fswatch.Watch
	watcher *fakeSolutionWatcher
	path    string
}

func newFakeSolutionWatcher() *fakeSolutionWatcher {
	return &fakeSolutionWatcher{opens: map[string]int{}, closes: map[string]int{}, fail: map[string]bool{}, callbacks: map[string]fswatch.WatchCallback{}}
}

func (w *fakeSolutionWatcher) open(path string, fn fswatch.WatchCallback) (fswatch.Watch, error) {
	w.mu.Lock()
	defer w.mu.Unlock()
	w.opens[path]++
	if w.fail[path] {
		return nil, errors.New("watch failed")
	}
	w.callbacks[path] = fn
	return &fakeSolutionWatch{watcher: w, path: path}, nil
}

func (w *fakeSolutionWatcher) WatchDirectory(dir string, fn fswatch.WatchCallback, _ ...fswatch.WatchOption) (fswatch.Watch, error) {
	return w.open(dir, fn)
}

func (w *fakeSolutionWatcher) WatchFile(path string, fn fswatch.WatchCallback) (fswatch.Watch, error) {
	return w.open(path, fn)
}

func (w *fakeSolutionWatch) Close() error {
	w.watcher.mu.Lock()
	defer w.watcher.mu.Unlock()
	w.watcher.closes[w.path]++
	return nil
}

func (w *fakeSolutionWatcher) counts(path string) (opens, closes int) {
	w.mu.Lock()
	defer w.mu.Unlock()
	return w.opens[path], w.closes[path]
}

func (w *fakeSolutionWatcher) deliver(t *testing.T, path string, events []fswatch.Event, err error) {
	t.Helper()
	w.mu.Lock()
	callback := w.callbacks[path]
	w.mu.Unlock()
	if callback == nil {
		t.Fatalf("no watch on %s", path)
	}
	callback(events, err)
}

func TestSolutionWatchTerminatedRootIsReopened(t *testing.T) {
	events := newSolutionWatchEvents()
	key := solutionWatchKey{kind: solutionWatchRoot, project: "/project/tsconfig.json", path: "/project/src"}
	events.add(key, []fswatch.Event{{Kind: fswatch.EventDelete, Path: "/project/src"}}, fmt.Errorf("%w: watched directory removed", fswatch.ErrWatchTerminated))
	if _, ok := events.dead[key]; !ok {
		t.Fatalf("dead = %v, want the terminated root", events.dead)
	}
	if _, ok := events.projects[key.project]; !ok {
		t.Fatalf("projects = %v, want the project invalidated", events.projects)
	}
	if _, ok := events.paths["/project/src"]; !ok {
		t.Fatalf("paths = %v, want the deleted root", events.paths)
	}

	rojoKey := solutionWatchKey{kind: solutionWatchRojo, project: key.project, path: "/project/assets"}
	events.addRojo(rojoKey, nil, fmt.Errorf("%w: watched directory removed", fswatch.ErrWatchTerminated))
	if _, ok := events.dead[rojoKey]; !ok {
		t.Fatalf("dead = %v, want the terminated Rojo directory", events.dead)
	}

	overflow := newSolutionWatchEvents()
	overflow.add(key, nil, fswatch.ErrOverflow)
	if _, ok := overflow.projects[key.project]; !ok || len(overflow.dead) != 0 {
		t.Fatalf("overflow: projects %v dead %v, want the project and a live watch", overflow.projects, overflow.dead)
	}
}

func TestSolutionWatchDirectoryAndJSONEventsInvalidate(t *testing.T) {
	root := t.TempDir()
	moved := filepath.Join(root, "src", "new.name")
	if err := os.MkdirAll(moved, 0o755); err != nil {
		t.Fatal(err)
	}
	key := solutionWatchKey{kind: solutionWatchRoot, project: filepath.Join(root, "tsconfig.json"), path: filepath.Join(root, "src")}
	for _, test := range []struct {
		name  string
		event fswatch.Event
		want  bool
	}{
		{"folder renamed from", fswatch.Event{Kind: fswatch.EventDelete, Path: filepath.Join(root, "src", "old")}, true},
		{"dotted folder renamed to", fswatch.Event{Kind: fswatch.EventUpdate, Path: moved}, true},
		{"json source", fswatch.Event{Kind: fswatch.EventUpdate, Path: filepath.Join(root, "src", "data.json")}, true},
		{"asset", fswatch.Event{Kind: fswatch.EventUpdate, Path: filepath.Join(root, "src", "icon.png")}, false},
		{"deleted asset", fswatch.Event{Kind: fswatch.EventDelete, Path: filepath.Join(root, "src", "icon.png")}, false},
	} {
		events := newSolutionWatchEvents()
		events.add(key, []fswatch.Event{test.event}, nil)
		if _, got := events.projects[key.project]; got != test.want {
			t.Errorf("%s: invalidated = %v, want %v", test.name, got, test.want)
		}
		if _, ok := events.assets[key.project][test.event.Path]; !ok {
			t.Errorf("%s: assets = %v, want the path copied", test.name, events.assets)
		}
	}
}

func TestSolutionWatchesReopenOnlyFailedOrDead(t *testing.T) {
	watcher := newFakeSolutionWatcher()
	watches := &solutionWatches{
		watcher:  watcher,
		watches:  map[solutionWatchKey]fswatch.Watch{},
		callback: func(solutionWatchKey) fswatch.WatchCallback { return func([]fswatch.Event, error) {} },
	}
	sets := []compile.SolutionWatchSet{
		{ProjectPath: "/a/tsconfig.json", TsConfigPaths: []string{"/a/tsconfig.json"}, RootDirs: []string{"/a/src"}, RojoDirectories: []string{"/a/assets"}},
		{ProjectPath: "/b/tsconfig.json", TsConfigPaths: []string{"/b/tsconfig.json"}, RootDirs: []string{"/b/src"}},
	}
	watcher.fail["/a/assets"] = true
	watches.refresh(sets, nil)
	if watches.armed {
		t.Fatal("armed with a failed watch")
	}

	// The failed watch is retried; the open ones stay.
	watcher.fail["/a/assets"] = false
	watches.refresh(sets, nil)
	for path, want := range map[string][2]int{"/a/assets": {2, 0}, "/a/src": {1, 0}, "/b/src": {1, 0}, "/a/tsconfig.json": {1, 0}} {
		if opens, closes := watcher.counts(path); [2]int{opens, closes} != want {
			t.Errorf("retry: %s opened %d closed %d, want %v", path, opens, closes, want)
		}
	}
	if !watches.armed {
		t.Fatal("not armed after the retry")
	}

	// A dead watch is closed and reopened alone.
	watches.refresh(sets, map[solutionWatchKey]struct{}{{kind: solutionWatchRoot, project: "/b/tsconfig.json", path: "/b/src"}: {}})
	for path, want := range map[string][2]int{"/b/src": {2, 1}, "/a/src": {1, 0}, "/b/tsconfig.json": {1, 0}} {
		if opens, closes := watcher.counts(path); [2]int{opens, closes} != want {
			t.Errorf("dead: %s opened %d closed %d, want %v", path, opens, closes, want)
		}
	}

	// Unchanged sets with no dead watch keep everything.
	watches.refresh(sets, nil)
	if opens, _ := watcher.counts("/a/src"); opens != 1 {
		t.Errorf("unchanged sets reopened /a/src %d times", opens)
	}

	// A removed project closes its watches only.
	watches.refresh(sets[:1], nil)
	if _, closes := watcher.counts("/b/tsconfig.json"); closes != 1 {
		t.Errorf("removed project config closed %d times, want 1", closes)
	}
	if _, closes := watcher.counts("/a/src"); closes != 0 {
		t.Errorf("kept project root closed %d times, want 0", closes)
	}
}

type buildEndReporter struct{ ends chan error }

func (r *buildEndReporter) buildStart([]string) {}

func (r *buildEndReporter) buildEnd(_ *compile.BuildResult, _ []compile.DiagnosticInfo, _ time.Duration, err error) {
	r.ends <- err
}

func (r *buildEndReporter) watching(func() int) {}

// A cycle whose config reload fails still reopens the watches that died in
// the same batch of events.
func TestSolutionWatchReopensDeadWatchAfterReloadFailure(t *testing.T) {
	root, configs := writeConcurrencySolution(t)
	for _, config := range configs {
		data, err := os.ReadFile(config)
		if err != nil {
			t.Fatal(err)
		}
		mustWrite(t, config, strings.Replace(string(data), `"types":[]`, `"types":[],"typeRoots":["node_modules/@rbxts"]`, 1))
	}
	watcher := newFakeSolutionWatcher()
	previous := newSolutionWatcher
	newSolutionWatcher = func() fswatch.Watcher { return watcher }
	t.Cleanup(func() { newSolutionWatcher = previous })
	tsConfigPath := filepath.Join(root, "tsconfig.json")
	baseReload := newBuildOptionsReload(tsConfigPath, &buildArgs{build: true})
	reloading, release := make(chan struct{}), make(chan struct{})
	reloads := 0
	reload := func() (projectOptions, error) {
		reloads++
		if reloads == 1 {
			reloading <- struct{}{}
			<-release
			return baseReload()
		}
		return projectOptions{}, errors.New("invalid config")
	}
	reporter := &buildEndReporter{ends: make(chan error, 8)}
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() {
		runBuildSolutionWatchLoop(ctx, tsConfigPath, defaultProjectOptions, reload, reporter)
		close(done)
	}()
	t.Cleanup(func() {
		cancel()
		<-done
	})
	if err := <-reporter.ends; err != nil {
		t.Fatalf("initial build: %v", err)
	}
	var config, source string
	watcher.mu.Lock()
	for path := range watcher.callbacks {
		if !strings.EqualFold(filepath.Base(filepath.Dir(path)), "left") {
			continue
		}
		switch filepath.Base(path) {
		case "tsconfig.json":
			config = path
		case "src":
			source = path
		}
	}
	watcher.mu.Unlock()
	if config == "" || source == "" {
		t.Fatalf("watches = %v, want left config and source", watcher.callbacks)
	}

	watcher.deliver(t, config, []fswatch.Event{{Kind: fswatch.EventUpdate, Path: config}}, nil)
	<-reloading
	// These land in the next cycle, whose reload fails.
	watcher.deliver(t, config, []fswatch.Event{{Kind: fswatch.EventUpdate, Path: config}}, nil)
	watcher.deliver(t, source, nil, fmt.Errorf("%w: watched directory removed", fswatch.ErrWatchTerminated))
	close(release)
	if err := <-reporter.ends; err != nil {
		t.Fatalf("reloaded build: %v", err)
	}
	if err := <-reporter.ends; err == nil {
		t.Fatal("reload did not fail")
	}
	if opens, closes := watcher.counts(source); opens != 2 || closes != 1 {
		t.Fatalf("dead source watch opened %d closed %d, want reopened once", opens, closes)
	}
}
