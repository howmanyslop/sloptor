package config

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func TestSchemaOnlyAdvertisesCompilerSettings(t *testing.T) {
	var schema struct {
		Properties           map[string]json.RawMessage `json:"properties"`
		AdditionalProperties bool                       `json:"additionalProperties"`
	}
	if err := json.Unmarshal([]byte(Schema), &schema); err != nil {
		t.Fatal(err)
	}
	if schema.AdditionalProperties || len(schema.Properties) != 2 || schema.Properties["flamework"] == nil || schema.Properties["parallelTransformers"] == nil {
		t.Fatalf("unexpected compiler schema: %s", Schema)
	}
}

func TestLoadCompilerSettingsAndWarnOnRemovedSections(t *testing.T) {
	dir := t.TempDir()
	writeConfigFile(t, dir, ConfigFileName, "[flamework]\nidGenerationMode = 'full'\n[parallelTransformers]\nworkers = 2\n[assets]\nmode = 'macro'\n[deploy]\nunused = true\n")
	cfg, err := Load(dir)
	if err != nil || cfg.Flamework == nil || cfg.ParallelTransformers == nil || len(cfg.Warnings) == 0 {
		t.Fatalf("load = (%+v, %v)", cfg, err)
	}
	writeConfigFile(t, dir, "rotor.config.js", "throw new Error('must never execute');")
	if err := os.Remove(filepath.Join(dir, ConfigFileName)); err != nil {
		t.Fatal(err)
	}
	if _, err := Load(dir); err != ErrNotFound {
		t.Fatalf("legacy executable config = %v, want ErrNotFound", err)
	}
}

// writeConfigFile writes content to dir/name, creating parent dirs.
func writeConfigFile(t *testing.T, dir, name, content string) {
	t.Helper()
	p := filepath.Join(dir, filepath.FromSlash(name))
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

// The schema is hosted (served from SchemaURL via raw GitHub) rather than
// written per-project, so a single rotor.schema.json is committed at the repo
// root. It must never drift from config.Schema — `rotor schema` regenerates it.
func TestCommittedSchemaMatchesConstant(t *testing.T) {
	const rel = "../../rotor.schema.json" // repo root, relative to internal/config
	data, err := os.ReadFile(rel)
	if err != nil {
		t.Fatalf("committed schema missing — run `rotor schema > rotor.schema.json` at the repo root: %v", err)
	}
	if string(data) != Schema {
		t.Errorf("%s is out of sync with config.Schema; regenerate with `rotor schema > rotor.schema.json`", rel)
	}
}
