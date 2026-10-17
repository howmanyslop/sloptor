package compile

import (
	"os"
	"path/filepath"
	"runtime"
	"slices"
	"testing"

	"golang.org/x/text/unicode/norm"
)

func TestWatchSetsExcludeArtifactsOwnedByOtherProjects(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("logger", nil, nil)
	f.project("game", []string{"logger"}, nil)
	f.write("game/out/generated/nested.luau", "return {}\n")
	f.write("game/include/runtime.luau", "return {}\n")
	f.write("game/.rotor/cache/state.json", "{}\n")
	f.write("game/next/nested/asset.luau", "return {}\n")
	f.write("external-assets/icon.png", "external asset\n")
	f.rojo("default.project.json", "", map[string]any{
		"gameInclude": map[string]string{"$path": "game/include"},
		"gameCache":   map[string]string{"$path": "game/.rotor"},
		"next":        map[string]string{"$path": "game/next"},
		"external":    map[string]string{"$path": "external-assets"},
	})
	coordinator, err := NewSolutionCoordinator(filepath.Join(f.dir, "game/tsconfig.json"), f.options())
	if err != nil {
		t.Fatal(err)
	}
	assertArtifacts := func(sets []SolutionWatchSet) {
		t.Helper()
		var artifacts []string
		for _, set := range sets {
			artifacts = append(artifacts, canonicalWatchPaths(set.ArtifactDirectories)...)
		}
		for _, set := range sets {
			for _, directory := range set.RojoDirectories {
				if pathWithinAnyRoot(canonicalWatchDirectory(directory), artifacts) {
					t.Errorf("%s watches solution artifact %s", set.ProjectPath, directory)
				}
			}
			if !slices.Contains(set.RojoDirectories, filepath.Join(f.dir, "external-assets")) {
				t.Errorf("%s lost intentional external assets: %v", set.ProjectPath, set.RojoDirectories)
			}
		}
	}
	assertArtifacts(coordinator.WatchSets())
	// Updating one project's output ownership must also filter the cached
	// Rojo set of an otherwise unchanged project that inherits the same tree.
	f.setCompilerOption("game", "outDir", "next")
	coordinator.Invalidate(filepath.Join(f.dir, "game/tsconfig.json"))
	assertArtifacts(coordinator.WatchSets())
}

func TestCanonicalWatchDirectoryPreservesMissingParents(t *testing.T) {
	root := t.TempDir()
	canonical, err := filepath.EvalSymlinks(root)
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(root, "missing", "nested", "out")
	if got, want := canonicalWatchDirectory(path), filepath.Join(canonical, "missing", "nested", "out"); got != want {
		t.Fatalf("canonical missing directory = %s, want %s", got, want)
	}
}

func TestCanonicalWatchDirectoryUnicodeNormalization(t *testing.T) {
	root := t.TempDir()
	existing := filepath.Join(root, "cafe\u0301")
	if err := os.MkdirAll(existing, 0o755); err != nil {
		t.Fatal(err)
	}
	for _, path := range []string{existing, filepath.Join(existing, "missing\u0301", "nested")} {
		canonical := canonicalWatchDirectory(path)
		if wantNFC := runtime.GOOS == "darwin"; norm.NFC.IsNormalString(canonical) != wantNFC {
			t.Errorf("canonical path %q, want NFC=%v", canonical, wantNFC)
		}
	}
}

func TestWatchSetsExcludeArtifactSymlinkAliases(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("logger", nil, nil)
	f.project("game", []string{"logger"}, nil)
	f.write("game/out/asset.luau", "return {}\n")
	alias := filepath.Join(f.dir, "generated-alias")
	if err := os.Symlink(filepath.Join(f.dir, "game/out"), alias); err != nil {
		t.Skipf("cannot create symlink: %v", err)
	}
	f.rojo("default.project.json", "", map[string]any{
		"generatedAlias": map[string]string{"$path": "generated-alias"},
	})
	coordinator, err := NewSolutionCoordinator(filepath.Join(f.dir, "game/tsconfig.json"), f.options())
	if err != nil {
		t.Fatal(err)
	}
	for _, set := range coordinator.WatchSets() {
		for _, directory := range set.RojoDirectories {
			if canonicalWatchDirectory(directory) == canonicalWatchDirectory(alias) {
				t.Errorf("%s watches artifact through symlink %s", set.ProjectPath, directory)
			}
		}
	}
}
