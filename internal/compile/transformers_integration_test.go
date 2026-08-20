package compile

import (
	"errors"
	"io/fs"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

func transformersFixtureDir(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("runtime.Caller failed")
	}
	dir := filepath.Clean(filepath.Join(filepath.Dir(file), "..", "..", "testdata", "transformers", "project"))
	if _, err := os.Stat(filepath.Join(dir, "node_modules", "rbxts-transformer-flamework", "package.json")); err != nil {
		skipOrFailFixture(t, "transformers fixture dependencies not installed (run `bun install --no-save` in testdata/transformers/project): %v", err)
	}
	if _, err := exec.LookPath("node"); err != nil {
		skipOrFailFixture(t, "node not on PATH: %v", err)
	}
	return dir
}

// skipOrFailFixture skips locally but fails in CI: a silently skipped
// real-package test would let a green run claim coverage it didn't have.
// CI sets ROTOR_REQUIRE_TRANSFORMERS_FIXTURE after installing the fixture.
func skipOrFailFixture(t *testing.T, format string, args ...any) {
	t.Helper()
	if os.Getenv("ROTOR_REQUIRE_TRANSFORMERS_FIXTURE") != "" {
		t.Fatalf(format, args...)
	}
	t.Skipf(format, args...)
}

func resetFlameworkFixtureGeneratedFiles(t *testing.T, dir string) {
	t.Helper()
	removeGeneratedFiles := func() error {
		for _, path := range []string{"out", "flamework.build", ".rotor"} {
			if err := os.RemoveAll(filepath.Join(dir, path)); err != nil {
				return err
			}
		}
		return nil
	}
	if err := removeGeneratedFiles(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := removeGeneratedFiles(); err != nil {
			t.Error(err)
		}
	})
}

func TestBuildProjectLegacyFlameworkUsesSidecar(t *testing.T) {
	dir := legacyFlameworkFixture(t)

	result, diags, err := BuildProjectWithOptions(dir, ProjectOptions{})
	if err != nil {
		t.Fatalf("BuildProjectWithOptions: %v (diags: %v)", err, diags)
	}
	if len(diags) > 0 {
		t.Fatalf("diagnostics: %v", diags)
	}

	envOut := result.Outputs["out/shared/env.luau"]
	if !strings.Contains(envOut, "https://env.example") {
		t.Fatalf("rbxts-transform-env did not inline ROTOR_FIXTURE_API_URL:\n%s", envOut)
	}
	if strings.Contains(envOut, "$env") {
		t.Fatalf("rbxts-transform-env left $env macros in output:\n%s", envOut)
	}

	serviceOut := result.Outputs["out/server/services/test.service.luau"]
	if !strings.Contains(serviceOut, "identifier") || !strings.Contains(serviceOut, "defineMetadata") {
		t.Fatalf("rbxts-transformer-flamework did not inject identifier metadata:\n%s", serviceOut)
	}

	mainOut := result.Outputs["out/server/main.server.luau"]
	if strings.Contains(mainOut, `"src/server/services"`) {
		t.Fatalf("Flamework.addPaths was not rewritten:\n%s", mainOut)
	}
}

func TestBuildProjectLegacyFlameworkUsesSidecarWithoutActiveNativeProfile(t *testing.T) {
	dir := legacyFlameworkFixture(t)
	if err := os.WriteFile(filepath.Join(dir, "rotor.toml"), []byte("[flamework.profiles.\"tsconfig.other.json\"]\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	_, diags, err := BuildProjectWithOptions(dir, ProjectOptions{})
	if err != nil {
		t.Fatalf("BuildProjectWithOptions: %v (diags: %v)", err, diags)
	}
}

func legacyFlameworkFixture(t *testing.T) string {
	t.Helper()
	fixture := transformersFixtureDir(t)
	dir := t.TempDir()
	copyDir(t, filepath.Join(fixture, "src"), filepath.Join(dir, "src"))
	for _, name := range []string{"default.project.json", "package.json", "tsconfig.json"} {
		data, err := os.ReadFile(filepath.Join(fixture, name))
		if err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(dir, name), data, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.Symlink(filepath.Join(fixture, "node_modules"), filepath.Join(dir, "node_modules")); err != nil {
		t.Fatal(err)
	}
	resetFlameworkFixtureGeneratedFiles(t, dir)
	closeSidecarSessions()
	t.Setenv("ROTOR_SIDECAR_PATH", "")
	redirectUserCacheDir(t)
	// Registered after redirectUserCacheDir's t.TempDir so it runs before the
	// cache dir is removed: Windows refuses to delete the extracted sidecar
	// script while the worker still has it open.
	t.Cleanup(closeSidecarSessions)
	t.Setenv("ROTOR_FIXTURE_API_URL", "https://env.example")
	return dir
}

func nativeFlameworkFixtureDir(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("runtime.Caller failed")
	}
	fixtureRoot := filepath.Clean(filepath.Join(filepath.Dir(file), "..", "..", "testdata", "transformers"))
	legacyNodeModules := filepath.Join(fixtureRoot, "project", "node_modules")
	if _, err := os.Stat(filepath.Join(legacyNodeModules, "@flamework", "core", "package.json")); err != nil {
		skipOrFailFixture(t, "Flamework fixture dependencies not installed (run `bun install --no-save` in testdata/transformers/project): %v", err)
	}
	dir := filepath.Join(fixtureRoot, "native", "project")
	nativeNodeModules := filepath.Join(dir, "node_modules")
	if err := os.Remove(nativeNodeModules); err != nil && !errors.Is(err, fs.ErrNotExist) {
		t.Fatal(err)
	}
	if err := os.Symlink(legacyNodeModules, nativeNodeModules); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := os.Remove(nativeNodeModules); err != nil && !errors.Is(err, fs.ErrNotExist) {
			t.Error(err)
		}
	})
	return dir
}

func TestFlameworkNativeFixtureEmitsServiceMetadata(t *testing.T) {
	// Given: a native-mode fixture with no rbxts-transformer-flamework plugin.
	dir := nativeFlameworkFixtureDir(t)
	resetFlameworkFixtureGeneratedFiles(t, dir)

	// When: the real build pipeline compiles the [flamework]-enabled project.
	result, diags, err := BuildProjectWithOptions(dir, ProjectOptions{})
	if err != nil {
		t.Fatalf("BuildProjectWithOptions: %v (diags: %v)", err, diags)
	}
	if len(diags) > 0 {
		t.Fatalf("diagnostics: %v", diags)
	}

	// Then: native output contains the observable Flamework metadata emitted by
	// the legacy transformer.
	serviceOut := result.Outputs["out/server/services/test.service.luau"]
	if !strings.Contains(serviceOut, "identifier") || !strings.Contains(serviceOut, "defineMetadata") {
		t.Fatalf("native [flamework] mode did not inject identifier metadata:\n%s", serviceOut)
	}
}
