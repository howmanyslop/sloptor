package main

import (
	"path/filepath"
	"reflect"
	"runtime"
	"slices"
	"sort"
	"strings"
	"sync/atomic"

	"golang.org/x/text/unicode/norm"

	"rotor/internal/compile"
	"rotor/internal/fsutil"
	"rotor/tsgo/fswatch"
)

// A route keeps ownership separate from the directory used for OS coverage.
// Paths are canonical for containment, but events are rebased to the logical
// directory before delivery (notably for symlinked project roots).
type solutionWatchRoute struct {
	key      solutionWatchKey
	path     string
	excluded []string
	callback fswatch.WatchCallback
}

type solutionWatchDispatch struct {
	routes atomic.Pointer[[]solutionWatchRoute]
}

func (d *solutionWatchDispatch) set(routes []solutionWatchRoute) { d.routes.Store(&routes) }

func (d *solutionWatchDispatch) deliver(events []fswatch.Event, err error) {
	routes := d.routes.Load()
	if routes == nil {
		return
	}
	for _, route := range *routes {
		var matched []fswatch.Event
		for _, event := range events {
			path := solutionWatchNormalizePath(event.Path)
			if route.key.kind == solutionWatchConfig {
				if !solutionWatchPathWithin(path, route.path) || !solutionWatchPathWithin(route.path, path) {
					continue
				}
				event.Path = route.key.path
			} else {
				if solutionWatchDirectoryIsArtifact(path, route.excluded) {
					continue
				}
				if solutionWatchPathWithin(path, route.path) {
					relative, _ := filepath.Rel(route.path, path)
					event.Path = filepath.Join(filepath.FromSlash(route.key.path), relative)
				} else if solutionWatchPathWithin(route.path, path) {
					// A containing directory moved or disappeared. The route's
					// whole input tree changed, even if only its ancestor is reported.
					event.Path = route.key.path
				} else {
					continue
				}
			}
			matched = append(matched, event)
		}
		if len(matched) != 0 || err != nil {
			route.callback(matched, err)
		}
	}
}

func solutionWatchPathWithin(path, root string) bool {
	relative, err := filepath.Rel(solutionWatchNormalizePath(root), solutionWatchNormalizePath(path))
	return err == nil && relative != ".." && !strings.HasPrefix(relative, ".."+string(filepath.Separator))
}

// Darwin's fswatch backend reports NFC paths, including for watches opened
// with NFD names. Other platforms can have distinct files for the two forms.
func solutionWatchNormalizePath(path string) string {
	path = filepath.Clean(filepath.FromSlash(path))
	if runtime.GOOS == "darwin" {
		return norm.NFC.String(path)
	}
	return path
}

func solutionWatchCanonicalPath(path string) string {
	path = filepath.Clean(filepath.FromSlash(path))
	if resolved, err := fsutil.EvalSymlinks(path); err == nil {
		return solutionWatchNormalizePath(resolved)
	}
	if parent := filepath.Dir(path); parent != path {
		return solutionWatchNormalizePath(filepath.Join(solutionWatchCanonicalPath(parent), filepath.Base(path)))
	}
	return solutionWatchNormalizePath(path)
}

func solutionArtifactDirectories(sets []compile.SolutionWatchSet) []string {
	var directories []string
	for _, set := range sets {
		for _, directory := range set.ArtifactDirectories {
			directory = solutionWatchCanonicalPath(directory)
			if !slices.Contains(directories, directory) {
				directories = append(directories, directory)
			}
		}
	}
	return directories
}

func solutionWatchExclusions(key solutionWatchKey, artifacts []string) []string {
	if key.kind != solutionWatchRoot {
		return artifacts
	}
	root := solutionWatchCanonicalPath(key.path)
	// A broad source root must ignore generated children, but an explicit
	// source root inside a generated tree remains a declared project input.
	return slices.DeleteFunc(slices.Clone(artifacts), func(artifact string) bool {
		return solutionWatchPathWithin(root, artifact)
	})
}

// refresh retains live coverage, updates its project routes, and retries
// missing or terminated subscriptions. Descendant watches are only replaced
// once their ancestor has successfully opened.
func (w *solutionWatches) refresh(sets []compile.SolutionWatchSet, dead map[solutionWatchKey]struct{}) {
	if len(dead) == 0 && w.armed && reflect.DeepEqual(sets, w.sets) {
		return
	}
	if w.dispatch == nil {
		w.dispatch = map[solutionWatchKey]*solutionWatchDispatch{}
	}
	// A terminal error belongs to the shared subscription. Reopen it even
	// when the logical key that reported it was one of its descendants.
	for physical, dispatch := range w.dispatch {
		isDead := false
		for _, route := range *dispatch.routes.Load() {
			if _, ok := dead[route.key]; ok {
				isDead = true
			}
		}
		if physical.kind == solutionWatchConfig {
			for key := range dead {
				if path, recursive := key.directory(); !recursive && solutionWatchPathWithin(solutionWatchCanonicalPath(path), filepath.Dir(physical.path)) && solutionWatchPathWithin(filepath.Dir(physical.path), solutionWatchCanonicalPath(path)) {
					isDead = true
				}
			}
		}
		if isDead {
			dispatch.set(nil)
			_ = w.watches[physical].Close()
			delete(w.watches, physical)
			delete(w.dispatch, physical)
		}
	}

	artifacts := solutionArtifactDirectories(sets)
	var routes []solutionWatchRoute
	candidates := map[solutionWatchKey]struct{}{}
	for _, key := range solutionWatchKeys(sets) {
		route := solutionWatchRoute{key: key, path: solutionWatchCanonicalPath(key.path), callback: w.callback(key)}
		physical := solutionWatchKey{kind: solutionWatchRoot, path: route.path}
		switch key.kind {
		case solutionWatchConfig:
			physical.kind = solutionWatchConfig
		default:
			route.excluded = solutionWatchExclusions(key, artifacts)
		}
		routes = append(routes, route)
		candidates[physical] = struct{}{}
	}
	ordered := make([]solutionWatchKey, 0, len(candidates))
	for key := range candidates {
		ordered = append(ordered, key)
	}
	// Ancestors precede descendants. Separate volumes and sibling names
	// remain independent; containment always uses filepath.Rel.
	sort.Slice(ordered, func(i, j int) bool {
		if len(ordered[i].path) != len(ordered[j].path) {
			return len(ordered[i].path) < len(ordered[j].path)
		}
		return ordered[i].path < ordered[j].path
	})
	wanted := map[solutionWatchKey]struct{}{}
	w.armed = true
	for _, physical := range ordered {
		if physical.kind != solutionWatchConfig {
			covered := false
			for selected := range wanted {
				if selected.kind != solutionWatchConfig && solutionWatchPathWithin(physical.path, selected.path) {
					covered = true
					break
				}
			}
			if covered {
				continue
			}
		}
		matched := slices.DeleteFunc(slices.Clone(routes), func(route solutionWatchRoute) bool {
			if physical.kind == solutionWatchConfig || route.key.kind == solutionWatchConfig {
				return physical.kind != route.key.kind || route.path != physical.path
			}
			return !solutionWatchPathWithin(route.path, physical.path) && !solutionWatchPathWithin(physical.path, route.path)
		})
		if dispatch := w.dispatch[physical]; dispatch != nil {
			dispatch.set(matched)
		} else {
			dispatch := &solutionWatchDispatch{}
			dispatch.set(matched)
			var watch fswatch.Watch
			var err error
			if physical.kind == solutionWatchConfig {
				watch, err = w.watcher.WatchFile(physical.path, dispatch.deliver)
			} else {
				watch, err = w.watcher.WatchDirectory(physical.path, dispatch.deliver, fswatch.WithRecursive())
			}
			if err != nil {
				w.armed = false
				// A failed ancestor must not suppress working child watches.
				continue
			}
			w.watches[physical], w.dispatch[physical] = watch, dispatch
		}
		wanted[physical] = struct{}{}
	}
	for key, watch := range w.watches {
		if _, keep := wanted[key]; keep {
			continue
		}
		w.dispatch[key].set(nil)
		_ = watch.Close()
		delete(w.watches, key)
		delete(w.dispatch, key)
	}
	w.sets = sets
}
