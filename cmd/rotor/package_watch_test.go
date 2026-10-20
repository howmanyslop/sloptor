package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"rotor/internal/fsutil"
	"rotor/tsgo/fswatch"
)

func writePackageWatchFixture(t *testing.T) (string, string) {
	t.Helper()
	root := writeBuildableProject(t, `export { answer } from "@rbxts/lib";`)
	root, err := fsutil.EvalSymlinks(root)
	if err != nil {
		t.Fatal(err)
	}
	mustWrite(t, filepath.Join(root, "package.json"), `{"name":"@scope/app"}`)
	packageDir := filepath.Join(root, "node_modules/@rbxts/lib")
	mustWrite(t, filepath.Join(packageDir, "package.json"), `{"name":"@rbxts/lib","main":"out/init.luau","types":"out/index.d.ts"}`)
	mustWrite(t, filepath.Join(packageDir, "out/index.d.ts"), `export declare const answer: number;`)
	mustWrite(t, filepath.Join(packageDir, "out/init.luau"), `return { answer = 42 }`)
	return root, packageDir
}

func TestSolutionWatchPackageLayout(t *testing.T) {
	root, packageDir := writePackageWatchFixture(t)
	named := filepath.Join(packageDir, ".cache/place/clean.project.json")
	mustWrite(t, named, `{"name":"testing","tree":{"$path":"../../out"}}`)
	watcher, events := startArtifactSolutionWatch(t, root)
	config := filepath.Join(packageDir, "default.project.json")
	if opens, _ := watcher.counts(config); opens == 0 {
		t.Fatal("package default-project candidate is not watched")
	}
	// Ignored test projects must not enter the reload or asset-copy route.
	watcher.deliver(t, filepath.Dir(named), []fswatch.Event{{Kind: fswatch.EventUpdate, Path: named}}, nil)
	select {
	case event := <-events:
		t.Fatalf("ignored project triggered a build: %v", event)
	case <-time.After(100 * time.Millisecond):
	}
	if err := os.Remove(named); err != nil {
		t.Fatal(err)
	}
	if err := os.Remove(filepath.Dir(named)); err != nil {
		t.Fatal(err)
	}
	watcher.deliver(t, filepath.Dir(named), []fswatch.Event{{Kind: fswatch.EventDelete, Path: filepath.Dir(named)}}, nil)
	select {
	case event := <-events:
		t.Fatalf("removing an irrelevant directory triggered a build: %v", event)
	case <-time.After(100 * time.Millisecond):
	}
	mustWrite(t, config, `{"name":"runtime-lib","tree":{"$path":"out"}}`)
	watcher.deliver(t, config, []fswatch.Event{{Kind: fswatch.EventUpdate, Path: config}}, nil)
	nextWatchEvent(t, events, "buildStart")
	if end := nextWatchEvent(t, events, "buildEnd"); end["ok"] != true {
		t.Fatalf("package layout edit failed: %v", end)
	}
	output, err := os.ReadFile(filepath.Join(root, "out/main.luau"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(output), `TS.getModule(script, "@rbxts", "runtime-lib")`) {
		t.Fatalf("watch left a stale import:\n%s", output)
	}
	if err := os.Remove(config); err != nil {
		t.Fatal(err)
	}
	watcher.deliver(t, config, []fswatch.Event{{Kind: fswatch.EventDelete, Path: config}}, nil)
	nextWatchEvent(t, events, "buildStart")
	if end := nextWatchEvent(t, events, "buildEnd"); end["ok"] != true {
		t.Fatalf("removing package layout failed: %v", end)
	}
	output, err = os.ReadFile(filepath.Join(root, "out/main.luau"))
	if err != nil || !strings.Contains(string(output), `TS.getModule(script, "@rbxts", "lib").out`) {
		t.Fatalf("removing layout left a stale import: %v\n%s", err, output)
	}
	// A newly installed subtree can contain a default before any exact-file
	// watch exists. Its parent listing event must discover the new layout.
	mustWrite(t, filepath.Join(packageDir, "examples/place/default.project.json"), `{"name":"example","tree":{"$path":"../../out"}}`)
	watcher.deliver(t, packageDir, []fswatch.Event{{Kind: fswatch.EventUpdate, Path: filepath.Join(packageDir, "examples")}}, nil)
	nextWatchEvent(t, events, "buildStart")
	if end := nextWatchEvent(t, events, "buildEnd"); end["ok"] != true {
		t.Fatalf("new nested default failed: %v", end)
	}
	output, err = os.ReadFile(filepath.Join(root, "out/main.luau"))
	if err != nil || !strings.Contains(string(output), `.examples.example`) {
		t.Fatalf("new nested layout was not discovered: %v\n%s", err, output)
	}
	if _, err := os.Stat(filepath.Join(root, "out/node_modules")); !os.IsNotExist(err) {
		t.Fatalf("dependency assets copied into output: %v", err)
	}
}

func TestSolutionWatchPackageConfigSymlinkRetarget(t *testing.T) {
	root, packageDir := writePackageWatchFixture(t)
	mustWrite(t, filepath.Join(packageDir, "default.project.json"), `{"name":"lib","tree":{"Mounted":{"$path":"layout.project.json"}}}`)
	for _, name := range []string{"One", "Two"} {
		mustWrite(t, filepath.Join(packageDir, name+".project.json"), `{"name":"ignored","tree":{"`+name+`":{"$path":"out"}}}`)
	}
	alias := filepath.Join(packageDir, "layout.project.json")
	if err := os.Symlink(filepath.Join(packageDir, "One.project.json"), alias); err != nil {
		t.Skipf("file symlinks unavailable: %v", err)
	}
	watcher, events := startArtifactSolutionWatch(t, root)
	if err := os.Remove(alias); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(filepath.Join(packageDir, "Two.project.json"), alias); err != nil {
		t.Fatal(err)
	}
	// fswatch coalesces rapid delete/recreate into one update. The old target
	// still exists, so only the logical parent reports this change.
	watcher.deliver(t, packageDir, []fswatch.Event{{Kind: fswatch.EventUpdate, Path: alias}}, nil)
	nextWatchEvent(t, events, "buildStart")
	if end := nextWatchEvent(t, events, "buildEnd"); end["ok"] != true {
		t.Fatalf("symlink retarget failed: %v", end)
	}
	output, err := os.ReadFile(filepath.Join(root, "out/main.luau"))
	if err != nil || !strings.Contains(string(output), ".Mounted.Two") {
		t.Fatalf("retargeted config is stale: %v\n%s", err, output)
	}
}
