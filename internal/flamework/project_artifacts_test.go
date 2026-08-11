package flamework

import (
	"os"
	"path/filepath"
	"testing"
)

func TestProjectPersist_whenRuntimeConfigAndGlobArePresent(t *testing.T) {
	// Given: a game project with runtime configuration, one source glob, and concrete Rojo layout.
	root := t.TempDir()
	writeProjectFixture(t, root, `{"name":"fixture-game","version":"1.0.0"}`)
	if err := os.WriteFile(filepath.Join(root, "flamework.json"), []byte(`{"profiling":false}`), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "src", "main.ts"), []byte("export {}"), 0o644); err != nil {
		t.Fatal(err)
	}
	rojoJSON := `{"name":"fixture","tree":{"$className":"DataModel","ReplicatedStorage":{"TS":{"$path":"out"}}}}`
	if err := os.WriteFile(filepath.Join(root, "default.project.json"), []byte(rojoJSON), 0o644); err != nil {
		t.Fatal(err)
	}
	project, err := OpenProject(ProjectOptions{
		ProjectDir: root,
		RootDir:    filepath.Join(root, "src"),
		OutDir:     filepath.Join(root, "out"),
	})
	if err != nil {
		t.Fatal(err)
	}
	project.AddGlob("src/**/*.ts", "src/main.ts")

	// When: the public project entrypoint prepares and persists the successful artifact transaction.
	if err := project.Persist(); err != nil {
		t.Fatal(err)
	}

	// Then: exact deterministic runtime artifacts and build info exist.
	assertFileText(t, filepath.Join(root, "include", "flamework", "config.json"), `{"game":{"profiling":false},"packages":{}}`)
	assertFileText(t, filepath.Join(root, "include", "flamework", "globs.json"), `{"game":{"src/**/*.ts":[["ReplicatedStorage","TS","main"]]},"packages":{}}`)
	buildData, err := os.ReadFile(filepath.Join(root, "flamework.build"))
	if err != nil {
		t.Fatal(err)
	}
	for _, fragment := range []string{`"flameworkVersion": "1.3.2"`, `"src/**/*.ts": [`, `"out/main.lua"`} {
		if !containsBytes(buildData, fragment) {
			t.Fatalf("flamework.build missing %s:\n%s", fragment, buildData)
		}
	}
}

func TestProjectPersist_whenProjectIsPackage(t *testing.T) {
	// Given: a scoped package carrying runtime metadata for later game aggregation.
	root := t.TempDir()
	writeProjectFixture(t, root, `{"name":"@scope/package","version":"1.0.0"}`)
	if err := os.WriteFile(filepath.Join(root, "flamework.json"), []byte(`{"profiling":true}`), 0o644); err != nil {
		t.Fatal(err)
	}
	project, err := OpenProject(ProjectOptions{ProjectDir: root, RootDir: filepath.Join(root, "src"), OutDir: filepath.Join(root, "out")})
	if err != nil {
		t.Fatal(err)
	}

	// When: package artifacts are persisted.
	if err := project.Persist(); err != nil {
		t.Fatal(err)
	}

	// Then: package metadata stays in flamework.build and no game runtime files are emitted.
	if _, err := os.Stat(filepath.Join(root, "flamework.build")); err != nil {
		t.Fatalf("flamework.build: %v", err)
	}
	if _, err := os.Stat(filepath.Join(root, "include")); !os.IsNotExist(err) {
		t.Fatalf("package emitted include artifacts: %v", err)
	}
}

func assertFileText(t *testing.T, path, want string) {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile %s: %v", path, err)
	}
	if string(data) != want {
		t.Fatalf("%s = %s, want %s", filepath.Base(path), data, want)
	}
}
