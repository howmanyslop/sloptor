package compile

import (
	"context"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
)

func TestTask7RuntimeFixtureMatchesPinnedFlameworkUpstreamOracle(t *testing.T) {
	// Given: one real lifecycle/DI project and the installed upstream v1.3.2 transformer.
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("runtime.Caller failed")
	}
	repo := filepath.Clean(filepath.Join(filepath.Dir(file), "..", ".."))
	fixture := filepath.Join(repo, "testdata", "flamework", "task7-runtime")
	modules := filepath.Join(repo, "testdata", "transformers", "project", "node_modules")
	versionData, err := os.ReadFile(filepath.Join(modules, "rbxts-transformer-flamework", "package.json"))
	if err != nil {
		skipOrFailFixture(t, "pinned Flamework transformer is unavailable: %v", err)
	}
	var manifest struct {
		Version string `json:"version"`
	}
	if err := json.Unmarshal(versionData, &manifest); err != nil {
		t.Fatal(err)
	}
	if manifest.Version != "1.3.2" {
		t.Fatalf("rbxts-transformer-flamework version = %q, want 1.3.2", manifest.Version)
	}

	oracleDir := t.TempDir()
	copyDir(t, fixture, oracleDir)
	if err := os.Symlink(modules, filepath.Join(oracleDir, "node_modules")); err != nil {
		t.Fatal(err)
	}

	// When: the upstream rbxtsc CLI and native phase compile independent copies.
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	oracle := exec.CommandContext(ctx, filepath.Join(oracleDir, "node_modules", ".bin", "rbxtsc"), "-p", "tsconfig.json")
	oracle.Dir = oracleDir
	if output, err := oracle.CombinedOutput(); err != nil {
		t.Fatalf("upstream rbxtsc: %v\n%s", err, output)
	}
	oracleOutputs := readTask7LuauTree(t, filepath.Join(oracleDir, "out"))
	if got := len(oracleOutputs); got != 6 {
		t.Fatalf("upstream transformer-owned Luau output count = %d, want 6", got)
	}
	oracleBuildInfo, err := os.ReadFile(filepath.Join(oracleDir, "flamework.build"))
	if err != nil {
		t.Fatal(err)
	}
	if got := len(oracleBuildInfo); got != 579 {
		t.Fatalf("upstream flamework.build size = %d bytes, want 579", got)
	}

	nativeDir := t.TempDir()
	copyDir(t, fixture, nativeDir)
	if err := os.Symlink(modules, filepath.Join(nativeDir, "node_modules")); err != nil {
		t.Fatal(err)
	}
	tsconfigPath := filepath.Join(nativeDir, "tsconfig.json")
	tsconfig, err := os.ReadFile(tsconfigPath)
	if err != nil {
		t.Fatal(err)
	}
	pluginStart := strings.Index(string(tsconfig), `"plugins": [`)
	if pluginStart < 0 {
		t.Fatal("legacy transformer entry not found in runtime fixture")
	}
	pluginEnd := strings.Index(string(tsconfig)[pluginStart:], "]\n")
	if pluginEnd < 0 {
		t.Fatal("legacy transformer entry not found in runtime fixture")
	}
	pluginEnd += pluginStart + 1
	nativeConfig := string(tsconfig[:pluginStart]) + `"plugins": []` + string(tsconfig[pluginEnd:])
	if nativeConfig == string(tsconfig) {
		t.Fatal("legacy transformer entry not found in runtime fixture")
	}
	if err := os.WriteFile(tsconfigPath, []byte(nativeConfig), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(nativeDir, "rotor.toml"), []byte("[flamework]\nidGenerationMode = \"short\"\nsalt = \"task7-runtime-fixed-salt\"\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	nativeResult, nativeDiags, err := BuildProjectWithOptions(nativeDir, ProjectOptions{})
	if err != nil || len(nativeDiags) != 0 {
		t.Fatalf("native build: %v (diags: %v)", err, nativeDiags)
	}
	nativeBuildInfo, err := os.ReadFile(filepath.Join(nativeDir, "flamework.build"))
	if err != nil {
		t.Fatal(err)
	}
	if got := len(nativeResult.Outputs); got != 6 {
		t.Fatalf("native transformer-owned Luau output count = %d, want 6", got)
	}
	if got := len(nativeBuildInfo); got != 579 {
		t.Fatalf("native flamework.build size = %d bytes, want 579", got)
	}

	// Then: every transformer-owned final Luau file and deterministic build artifact is byte-identical.
	if diff := cmp.Diff(stripTask7CompilerBanners(t, oracleOutputs), stripTask7CompilerBanners(t, nativeResult.Outputs)); diff != "" {
		t.Fatalf("final Luau tree mismatch (-oracle +native):\n%s", diff)
	}
	if diff := cmp.Diff(string(oracleBuildInfo), string(nativeBuildInfo)); diff != "" {
		t.Fatalf("flamework.build mismatch (-oracle +native):\n%s", diff)
	}
	t.Logf("oracle=%s; Luau files=%d; flamework.build bytes=%d", manifest.Version, len(oracleOutputs), len(oracleBuildInfo))
}

func readTask7LuauTree(t *testing.T, root string) map[string]string {
	t.Helper()
	outputs := map[string]string{}
	err := filepath.WalkDir(root, func(path string, entry os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if entry.IsDir() || filepath.Ext(path) != ".luau" {
			return nil
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		relative, err := filepath.Rel(filepath.Dir(root), path)
		if err != nil {
			return err
		}
		outputs[filepath.ToSlash(relative)] = string(data)
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	return outputs
}

func stripTask7CompilerBanners(t *testing.T, outputs map[string]string) map[string]string {
	t.Helper()
	stripped := make(map[string]string, len(outputs))
	for path, output := range outputs {
		banner, body, found := strings.Cut(output, "\n")
		if !found || !strings.HasPrefix(banner, "-- Compiled with ") {
			t.Fatalf("%s is missing the compiler banner", path)
		}
		stripped[path] = body
	}
	return stripped
}
