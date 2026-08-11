package compile

import (
	"bytes"
	"crypto/sha256"
	"os"
	"path/filepath"
	"testing"

	"rotor/internal/config"
)

func TestBuildProjectFlameworkIncrementalInputsRebuildOnce(t *testing.T) {
	// Given: a native Flamework project with incremental output and one persisted glob.
	dir := writeProject(t, "flamework-incremental-inputs", "")
	enableIncrementalBuilds(t, dir)
	writeFlameworkIncrementalFile(t, filepath.Join(dir, "default.project.json"), `{"name":"fixture","tree":{"$className":"DataModel","ReplicatedStorage":{"TS":{"$path":"out"},"rbxts_include":{"$path":"include"}}}}`)
	writeFlameworkIncrementalFile(t, filepath.Join(dir, config.ConfigFileName), "[flamework]\nidGenerationMode = \"full\"\n")
	writeFlameworkIncrementalFile(t, filepath.Join(dir, "flamework.build"), `{"version":1,"flameworkVersion":"1.3.2","identifiers":{},"metadata":{"globs":{"paths":{"assets/*.txt":[]},"origins":{"manual":["assets/*.txt"]}}}}`)
	if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{}); err != nil {
		t.Fatalf("seed build: %v (diags: %v)", err, diags)
	}
	manifestPath := filepath.Join(dir, "out", "cache.rbxtsc.tsbuildinfo")
	previousSalt := incrementalManifestSalt(t, manifestPath)

	mutations := []struct {
		name   string
		mutate func()
	}{
		{
			name: "effective config",
			mutate: func() {
				writeFlameworkIncrementalFile(t, filepath.Join(dir, config.ConfigFileName), "[flamework]\nidGenerationMode = \"full\"\nhashPrefix = \"changed\"\n")
			},
		},
		{
			name: "package state",
			mutate: func() {
				packagePath := filepath.Join(dir, "package.json")
				writeFlameworkIncrementalFile(t, packagePath, `{"name":"flamework-incremental-inputs","version":"2.0.0"}`)
			},
		},
		{
			name: "glob matches",
			mutate: func() {
				writeFlameworkIncrementalFile(t, filepath.Join(dir, "assets", "added.txt"), "asset\n")
			},
		},
	}
	for _, mutation := range mutations {
		t.Run(mutation.name, func(t *testing.T) {
			mutation.mutate()

			// When: one real Flamework input changes and the project builds twice.
			rebuildTimings := NewBuildTimings()
			if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{Timings: rebuildTimings}); err != nil {
				t.Fatalf("input rebuild: %v (diags: %v)", err, diags)
			}
			currentSalt := incrementalManifestSalt(t, manifestPath)
			warmTimings := NewBuildTimings()
			if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{Timings: warmTimings}); err != nil {
				t.Fatalf("warm build: %v (diags: %v)", err, diags)
			}

			// Then: the input changes the output salt once and the next build is a cache hit.
			if currentSalt == previousSalt {
				t.Fatalf("%s preserved incremental salt %s", mutation.name, previousSalt)
			}
			if rebuildTimings.Counts.SelectedSources == 0 {
				t.Fatalf("%s selected no sources", mutation.name)
			}
			if warmTimings.Counts.SelectedSources != 0 || warmTimings.Counts.ActualWrites != 0 {
				t.Fatalf("%s warm build selected=%d writes=%d, want cache hit", mutation.name, warmTimings.Counts.SelectedSources, warmTimings.Counts.ActualWrites)
			}
			previousSalt = currentSalt
		})
	}
}

func TestBuildProjectFlameworkFailurePreservesArtifactsAndIncrementalState(t *testing.T) {
	// Given: a successful native Flamework incremental build.
	dir := writeProject(t, "flamework-incremental-failure", "")
	enableIncrementalBuilds(t, dir)
	writeFlameworkIncrementalFile(t, filepath.Join(dir, "default.project.json"), `{"name":"fixture","tree":{"$className":"DataModel","ReplicatedStorage":{"TS":{"$path":"out"},"rbxts_include":{"$path":"include"}}}}`)
	writeFlameworkIncrementalFile(t, filepath.Join(dir, config.ConfigFileName), "[flamework]\n")
	if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{}); err != nil {
		t.Fatalf("seed build: %v (diags: %v)", err, diags)
	}
	paths := []string{
		filepath.Join(dir, "out", "main.luau"),
		filepath.Join(dir, "out", "cache.rbxtsc.tsbuildinfo"),
		filepath.Join(dir, "flamework.build"),
	}
	before := fileHashes(t, paths)
	originalSource, err := os.ReadFile(filepath.Join(dir, "src", "main.ts"))
	if err != nil {
		t.Fatal(err)
	}

	// When: a malformed source build fails, then the exact successful source resumes.
	writeFlameworkIncrementalFile(t, filepath.Join(dir, "src", "main.ts"), "export const broken: = 1;\n")
	if _, _, err := BuildProjectWithOptions(dir, ProjectOptions{}); err == nil {
		t.Fatal("malformed native Flamework build succeeded")
	}
	afterFailure := fileHashes(t, paths)
	writeFlameworkIncrementalFile(t, filepath.Join(dir, "src", "main.ts"), string(originalSource))
	resumeTimings := NewBuildTimings()
	if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{Timings: resumeTimings}); err != nil {
		t.Fatalf("resume build: %v (diags: %v)", err, diags)
	}

	// Then: failure publishes nothing and the last successful cache state remains reusable.
	if !bytes.Equal(before, afterFailure) {
		t.Fatalf("failed build changed artifact hashes: before=%x after=%x", before, afterFailure)
	}
	if resumeTimings.Counts.SelectedSources != 0 || resumeTimings.Counts.ActualWrites != 0 {
		t.Fatalf("resume selected=%d writes=%d, want last-success cache hit", resumeTimings.Counts.SelectedSources, resumeTimings.Counts.ActualWrites)
	}
}

func incrementalManifestSalt(t *testing.T, path string) string {
	t.Helper()
	manifest, err := readIncrementalManifest(path)
	if err != nil || manifest == nil {
		t.Fatalf("read incremental manifest: %v", err)
	}
	return manifest.Salt
}

func fileHashes(t *testing.T, paths []string) []byte {
	t.Helper()
	var hashes []byte
	for _, path := range paths {
		contents, err := os.ReadFile(path)
		if err != nil {
			t.Fatal(err)
		}
		sum := sha256.Sum256(contents)
		hashes = append(hashes, sum[:]...)
	}
	return hashes
}
