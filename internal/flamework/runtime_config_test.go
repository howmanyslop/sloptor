package flamework

import (
	"errors"
	"os"
	"path/filepath"
	"testing"
)

func TestLoadRuntimeConfig_distinguishes_absent_and_empty(t *testing.T) {
	// Given
	root := t.TempDir()

	// When
	_, present, err := LoadRuntimeConfig(root)
	// Then
	if err != nil {
		t.Fatalf("LoadRuntimeConfig absent: %v", err)
	}
	if present {
		t.Fatal("LoadRuntimeConfig absent present = true, want false")
	}

	// Given
	if err := os.WriteFile(filepath.Join(root, RuntimeConfigFileName), []byte("{}"), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	// When
	config, present, err := LoadRuntimeConfig(root)
	// Then
	if err != nil {
		t.Fatalf("LoadRuntimeConfig empty: %v", err)
	}
	if !present {
		t.Fatal("LoadRuntimeConfig empty present = false, want true")
	}
	got, err := config.MarshalJSON()
	if err != nil {
		t.Fatalf("MarshalJSON: %v", err)
	}
	if string(got) != "{}" {
		t.Fatalf("empty config = %s, want {}", got)
	}
}

func TestLoadRuntimeConfig_preserves_explicit_zero_and_schema_additions(t *testing.T) {
	// Given
	root := t.TempDir()
	input := `{"logLevel":"none","profiling":false,"disableDependencyWarnings":false,"salt":"fixed","future":{"value":1}}`
	if err := os.WriteFile(filepath.Join(root, RuntimeConfigFileName), []byte(input), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	// When
	config, present, err := LoadRuntimeConfig(root)
	// Then
	if err != nil {
		t.Fatalf("LoadRuntimeConfig: %v", err)
	}
	if !present || config.Profiling == nil || *config.Profiling || config.DisableDependencyWarnings == nil || *config.DisableDependencyWarnings {
		t.Fatalf("LoadRuntimeConfig did not preserve explicit false: %+v", config)
	}
	got, err := config.MarshalJSON()
	if err != nil {
		t.Fatalf("MarshalJSON: %v", err)
	}
	want := `{"disableDependencyWarnings":false,"future":{"value":1},"logLevel":"none","profiling":false,"salt":"fixed"}`
	if string(got) != want {
		t.Fatalf("runtime config = %s, want %s", got, want)
	}
}

func TestLoadRuntimeConfig_rejects_invalid_known_option(t *testing.T) {
	// Given
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, RuntimeConfigFileName), []byte(`{"logLevel":"debug"}`), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	// When
	_, _, err := LoadRuntimeConfig(root)

	// Then
	if !errors.Is(err, ErrInvalidRuntimeConfig) {
		t.Fatalf("LoadRuntimeConfig error = %v, want ErrInvalidRuntimeConfig", err)
	}
}

func TestLoadRuntimeConfig_rejects_null_boolean(t *testing.T) {
	// Given
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, RuntimeConfigFileName), []byte(`{"profiling":null}`), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	// When
	_, _, err := LoadRuntimeConfig(root)

	// Then
	if !errors.Is(err, ErrInvalidRuntimeConfig) {
		t.Fatalf("LoadRuntimeConfig error = %v, want ErrInvalidRuntimeConfig", err)
	}
}

func TestLoadRuntimeConfig_rejects_trailing_JSON_value(t *testing.T) {
	// Given
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, RuntimeConfigFileName), []byte(`{} {}`), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	// When
	_, _, err := LoadRuntimeConfig(root)

	// Then
	if !errors.Is(err, ErrInvalidRuntimeConfig) {
		t.Fatalf("LoadRuntimeConfig error = %v, want ErrInvalidRuntimeConfig", err)
	}
}

func TestMarshalRuntimeConfigArtifact_is_deterministic_and_omits_absent_game(t *testing.T) {
	// Given
	profiling := true
	packages := map[string]RuntimeConfig{
		"pkg-b": {Profiling: &profiling},
		"pkg-a": {},
	}

	// When
	got, emit, err := MarshalRuntimeConfigArtifact(nil, packages)
	// Then
	if err != nil {
		t.Fatalf("MarshalRuntimeConfigArtifact: %v", err)
	}
	if !emit {
		t.Fatal("MarshalRuntimeConfigArtifact emit = false, want true")
	}
	want := `{"packages":{"pkg-a":{},"pkg-b":{"profiling":true}}}`
	if string(got) != want {
		t.Fatalf("MarshalRuntimeConfigArtifact = %s, want %s", got, want)
	}
}
