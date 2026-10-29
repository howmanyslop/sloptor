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
	if schema.AdditionalProperties || len(schema.Properties) != 4 || schema.Properties["flamework"] == nil || schema.Properties["parallelTransformers"] == nil || schema.Properties["nativeTypes"] == nil || schema.Properties["luauTypes"] == nil {
		t.Fatalf("unexpected compiler schema: %s", Schema)
	}
}

// nativeTypes is optional: unset must stay distinguishable from false so the
// tsconfig declaration can win over rotor.toml.
func TestLoadNativeTypes(t *testing.T) {
	dir := t.TempDir()
	cfg, err := Load(dir)
	if err != ErrNotFound {
		t.Fatalf("load without rotor.toml = (%+v, %v)", cfg, err)
	}

	writeConfigFile(t, dir, ConfigFileName, "")
	cfg, err = Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	if cfg.NativeTypes != nil {
		t.Fatalf("unset nativeTypes = %v, want nil", *cfg.NativeTypes)
	}

	writeConfigFile(t, dir, ConfigFileName, "nativeTypes = false\n")
	cfg, err = Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	if cfg.NativeTypes == nil || *cfg.NativeTypes {
		t.Fatalf("nativeTypes = %v, want an explicit false", cfg.NativeTypes)
	}

	writeConfigFile(t, dir, ConfigFileName, "nativeTypes = true\n")
	cfg, err = Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	if cfg.NativeTypes == nil || !*cfg.NativeTypes {
		t.Fatalf("nativeTypes = %v, want true", cfg.NativeTypes)
	}
	if len(cfg.Warnings) != 0 {
		t.Fatalf("nativeTypes must not warn: %v", cfg.Warnings)
	}
}

// luauTypes is optional too: unset must stay distinguishable from false so
// the command line and the tsconfig declaration can win over rotor.toml.
func TestLoadLuauTypes(t *testing.T) {
	dir := t.TempDir()
	writeConfigFile(t, dir, ConfigFileName, "")
	cfg, err := Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	if cfg.LuauTypes != nil {
		t.Fatalf("unset luauTypes = %v, want nil", *cfg.LuauTypes)
	}

	writeConfigFile(t, dir, ConfigFileName, "luauTypes = false\n")
	cfg, err = Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	if cfg.LuauTypes == nil || *cfg.LuauTypes {
		t.Fatalf("luauTypes = %v, want an explicit false", cfg.LuauTypes)
	}

	writeConfigFile(t, dir, ConfigFileName, "luauTypes = true\nnativeTypes = true\n")
	cfg, err = Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	if cfg.LuauTypes == nil || !*cfg.LuauTypes {
		t.Fatalf("luauTypes = %v, want true", cfg.LuauTypes)
	}
	if cfg.NativeTypes == nil || !*cfg.NativeTypes {
		t.Fatalf("nativeTypes = %v, want true beside luauTypes", cfg.NativeTypes)
	}
	if len(cfg.Warnings) != 0 {
		t.Fatalf("luauTypes must not warn: %v", cfg.Warnings)
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
