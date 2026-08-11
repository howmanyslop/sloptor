package flamework

import (
	"errors"
	"path/filepath"
	"testing"

	"rotor/internal/rojo"
)

func TestTranslateOutputPath_translates_project_path(t *testing.T) {
	// Given
	root := t.TempDir()
	translator := rojo.NewPathTranslator(
		filepath.Join(root, "src"),
		filepath.Join(root, "out"),
		"",
		false,
		false,
	)

	// When
	got, err := TranslateOutputPath(root, translator, "src/nested/index.ts")
	// Then
	if err != nil {
		t.Fatalf("TranslateOutputPath: %v", err)
	}
	if got != "out/nested/init.lua" {
		t.Fatalf("TranslateOutputPath = %q, want %q", got, "out/nested/init.lua")
	}
}

func TestTranslateOutputPath_rejects_input_escape(t *testing.T) {
	// Given
	root := t.TempDir()
	translator := rojo.NewPathTranslator(filepath.Join(root, "src"), filepath.Join(root, "out"), "", false, false)

	// When
	_, err := TranslateOutputPath(root, translator, "../escape.ts")

	// Then
	if !errors.Is(err, ErrPathEscape) {
		t.Fatalf("TranslateOutputPath error = %v, want ErrPathEscape", err)
	}
}

func TestTranslateOutputPath_rejects_output_escape(t *testing.T) {
	// Given
	root := t.TempDir()
	translator := rojo.NewPathTranslator(filepath.Join(root, "src"), filepath.Join(root, "..", "out"), "", false, false)

	// When
	_, err := TranslateOutputPath(root, translator, "src/file.ts")

	// Then
	if !errors.Is(err, ErrPathEscape) {
		t.Fatalf("TranslateOutputPath error = %v, want ErrPathEscape", err)
	}
}
