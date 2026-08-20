package compile

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"rotor/internal/config"
)

func TestAbsentRotorConfigLeavesGenericTransformerScheduled(t *testing.T) {
	// Given: a project with a generic TypeScript transformer and no rotor.toml.
	dir := writeProject(t, "generic-transformer-without-rotor-config", "")
	cfg, err := config.Load(dir)
	if cfg != nil || !errors.Is(err, config.ErrNotFound) {
		t.Fatalf("Load() = (%v, %v), want (nil, ErrNotFound)", cfg, err)
	}

	tsconfigPath := filepath.Join(dir, "tsconfig.json")
	tsconfig, err := os.ReadFile(tsconfigPath)
	if err != nil {
		t.Fatal(err)
	}
	updated := strings.Replace(string(tsconfig), `"compilerOptions": {`, `"compilerOptions": {"plugins": [{"transform": "example-transformer"}],`, 1)
	if err := os.WriteFile(tsconfigPath, []byte(updated), 0o644); err != nil {
		t.Fatal(err)
	}

	_, program, diags, err := newProjectProgram(dir, "")
	if err != nil {
		t.Fatalf("newProjectProgram: %v (diags: %v)", err, diags)
	}
	t.Setenv("ROTOR_NODE_PATH", filepath.Join(dir, "missing-node"))

	// When: the transformer program is prepared.
	_, diags, err = prepareTransformerProgram(dir, program, projectSourceFiles(program), nil)

	// Then: the generic transformer still reaches the Node scheduling seam.
	if err == nil || !strings.Contains(err.Error(), "node executable not found") {
		t.Fatalf("prepareTransformerProgram() = (%v, %v), want generic transformer scheduling error", diags, err)
	}
}

func TestPrepareTransformerProgramActivatesEmptyFlameworkConfig(t *testing.T) {
	dir := writeProject(t, "flamework-empty", "")
	t.Setenv("ROTOR_NODE_PATH", filepath.Join(dir, "missing-node"))
	if err := os.WriteFile(filepath.Join(dir, "rotor.toml"), []byte("[flamework]\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	_, program, diags, err := newProjectProgram(dir, "")
	if err != nil {
		t.Fatalf("newProjectProgram: %v (diags: %v)", err, diags)
	}
	prepared, diags, err := prepareTransformerProgram(dir, program, projectSourceFiles(program), nil)
	if err != nil {
		t.Fatalf("prepareTransformerProgram: %v (diags: %v)", err, diags)
	}
	if prepared.flamework == nil {
		t.Fatal("empty [flamework] must activate native preparation")
	}
}

func TestPrepareTransformerProgramSelectsFlameworkProfileForActiveTSConfig(t *testing.T) {
	// Given: two configs whose legacy transformers required different anchors.
	dir := writeProject(t, "flamework-profiles", "")
	baseConfig := filepath.Join(dir, "tsconfig.json")
	libConfig := filepath.Join(dir, "tsconfig.lib.json")
	base, err := os.ReadFile(baseConfig)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(libConfig, base, 0o644); err != nil {
		t.Fatal(err)
	}
	rotor := `[flamework.profiles."tsconfig.json"]
after = "redacted-react-compiler"

[flamework.profiles."./tsconfig.lib.json"]
after = "jest"
skipUnchangedFiles = true
`
	if err := os.WriteFile(filepath.Join(dir, "rotor.toml"), []byte(rotor), 0o644); err != nil {
		t.Fatal(err)
	}
	_, program, diags, err := newProjectProgram(dir, libConfig)
	if err != nil {
		t.Fatalf("newProjectProgram: %v (diags: %v)", err, diags)
	}

	// When: native configuration is prepared for tsconfig.lib.json.
	selected, diags, err := prepareFlameworkConfig(dir, program.CommandLine())
	// Then: the lib profile is selected, not the root profile.
	if err != nil {
		t.Fatalf("prepareFlameworkConfig: %v (diags: %v)", err, diags)
	}
	if selected == nil || selected.After != "jest" || !selected.SkipUnchangedFiles {
		t.Fatalf("selected Flamework config = %+v, want lib profile after jest with skipUnchangedFiles", selected)
	}
}

func TestPrepareTransformerProgramRejectsNativeAndLegacyFlamework(t *testing.T) {
	dir := writeProject(t, "flamework-double", "")
	if err := os.WriteFile(filepath.Join(dir, "rotor.toml"), []byte("[flamework]\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	tsconfigPath := filepath.Join(dir, "tsconfig.json")
	tsconfig, err := os.ReadFile(tsconfigPath)
	if err != nil {
		t.Fatal(err)
	}
	updated := strings.Replace(string(tsconfig), `"compilerOptions": {`, `"compilerOptions": {"plugins": [{"transform": "rbxts-transformer-flamework"}],`, 1)
	if err := os.WriteFile(tsconfigPath, []byte(updated), 0o644); err != nil {
		t.Fatal(err)
	}
	_, program, diags, err := newProjectProgram(dir, "")
	if err != nil {
		t.Fatalf("newProjectProgram: %v (diags: %v)", err, diags)
	}
	_, diags, err = prepareTransformerProgram(dir, program, projectSourceFiles(program), nil)
	if err == nil || !strings.Contains(err.Error(), "cannot be combined") {
		t.Fatalf("prepareTransformerProgram() = (%v, %v), want double-configuration error", diags, err)
	}
}
