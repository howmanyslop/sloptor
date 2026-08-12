package main

import (
	"path/filepath"
	"reflect"
	"testing"
	"time"
)

func TestTreeWatcherSnapshotWatchesFlameworkInputsExactlyOnce(t *testing.T) {
	// Given: one native Flamework project baseline.
	root := t.TempDir()
	for relativePath, contents := range map[string]string{
		"rotor.toml":                    "[flamework]\nidGenerationMode = \"full\"\n",
		"flamework.json":                "{}\n",
		"package.json":                  "{\"name\":\"fixture\"}\n",
		"flamework.build.backup":        "keep watching this\n",
		"flamework.build":               "{\"version\":1}\n",
		"src/server/main.ts":            "export {}\n",
		"node_modules/pkg/package.json": "{}\n",
	} {
		writeTestFile(t, filepath.Join(root, filepath.FromSlash(relativePath)), contents)
	}
	watcher := newTreeWatcher(root)
	baseline := watcher.snapshot()
	if artifact := filepath.Join(root, "flamework.build"); baseline[artifact].exists {
		t.Fatalf("generated artifact %s entered the watch baseline", artifact)
	}

	writeTestFile(t, filepath.Join(root, "rotor.toml"), "[flamework]\nidGenerationMode = \"tiny\"\n")
	if len("[flamework]\nidGenerationMode = \"full\"\n") != len("[flamework]\nidGenerationMode = \"tiny\"\n") {
		t.Fatal("test configuration edits must have equal size")
	}
	writeTestFile(t, filepath.Join(root, "package.json"), "{\"name\":\"fixture\",\"version\":\"2\"}\n")
	writeTestFile(t, filepath.Join(root, "flamework.build.backup"), "changed input\n")
	writeTestFile(t, filepath.Join(root, "flamework.build"), "{\"version\":2}\n")
	first := watcher.snapshot()

	// When: the watcher settles the complete write burst.
	_, changed := settleChanges(baseline, first, watcher.snapshot, func(time.Duration) {})

	// Then: real inputs form one event batch and only the generated artifact is absent.
	want := []string{
		filepath.Join(root, "flamework.build.backup"),
		filepath.Join(root, "package.json"),
		filepath.Join(root, "rotor.toml"),
	}
	if !reflect.DeepEqual(changed, want) {
		t.Fatalf("settled Flamework event = %v, want exactly one batch %v", changed, want)
	}
}
