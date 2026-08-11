package flamework

import (
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"rotor/internal/rojo"
)

func TestExpandGlobs_matches_case_insensitively_and_translates(t *testing.T) {
	// Given
	root := t.TempDir()
	for _, rel := range []string{"src/Alpha.TS", "src/nested/index.ts", "src/.hidden.ts", "src/data.json"} {
		path := filepath.Join(root, filepath.FromSlash(rel))
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatalf("MkdirAll: %v", err)
		}
		if err := os.WriteFile(path, nil, 0o644); err != nil {
			t.Fatalf("WriteFile: %v", err)
		}
	}
	translator := rojo.NewPathTranslator(filepath.Join(root, "src"), filepath.Join(root, "out"), "", false, false)

	// When
	got, err := ExpandGlobs(root, translator, []string{"src/**/*.ts", "src/*.json"})
	// Then
	if err != nil {
		t.Fatalf("ExpandGlobs: %v", err)
	}
	want := map[string][]string{
		"src/**/*.ts": {"out/Alpha.TS", "out/nested/init.lua"},
		"src/*.json":  {"out/data.json"},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("ExpandGlobs = %#v, want %#v", got, want)
	}
}

func TestExpandGlobs_rejects_pattern_escape(t *testing.T) {
	// Given
	root := t.TempDir()
	translator := rojo.NewPathTranslator(filepath.Join(root, "src"), filepath.Join(root, "out"), "", false, false)

	// When
	_, err := ExpandGlobs(root, translator, []string{"../**/*.ts"})

	// Then
	if !errors.Is(err, ErrPathEscape) {
		t.Fatalf("ExpandGlobs error = %v, want ErrPathEscape", err)
	}
}

func TestExpandGlobs_rejects_invalid_pattern_without_files(t *testing.T) {
	// Given
	root := t.TempDir()
	translator := rojo.NewPathTranslator(filepath.Join(root, "src"), filepath.Join(root, "out"), "", false, false)

	// When
	_, err := ExpandGlobs(root, translator, []string{"src/[.ts"})

	// Then
	if !errors.Is(err, ErrInvalidGlob) {
		t.Fatalf("ExpandGlobs error = %v, want ErrInvalidGlob", err)
	}
}

func TestResolveGlobPaths_preserves_empty_patterns_and_skips_unmapped_files(t *testing.T) {
	// Given
	root := t.TempDir()
	mapped := filepath.Join(root, "out", "mapped.luau")
	resolver, err := rojo.FromState(rojo.ResolverState{
		FilePaths: []rojo.ResolverFilePathState{{Path: mapped, RbxPath: rojo.RbxPath{"ReplicatedStorage", "mapped"}}},
	})
	if err != nil {
		t.Fatalf("FromState: %v", err)
	}
	expanded := map[string][]string{
		"src/**/*.ts": {"out/mapped.luau", "out/unmapped.luau"},
		"src/*.json":  {},
	}

	// When
	got, err := ResolveGlobPaths(root, resolver, expanded)
	// Then
	if err != nil {
		t.Fatalf("ResolveGlobPaths: %v", err)
	}
	want := GlobPaths{
		"src/**/*.ts": {rojo.RbxPath{"ReplicatedStorage", "mapped"}},
		"src/*.json":  {},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("ResolveGlobPaths = %#v, want %#v", got, want)
	}
}

func TestResolveGlobPaths_returns_empty_paths_without_rojo_config(t *testing.T) {
	// Given
	root := t.TempDir()
	expanded := map[string][]string{"src/**/*.ts": {"out/file.luau"}}

	// When
	got, err := ResolveGlobPaths(root, nil, expanded)
	// Then
	if err != nil {
		t.Fatalf("ResolveGlobPaths: %v", err)
	}
	want := GlobPaths{"src/**/*.ts": {}}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("ResolveGlobPaths = %#v, want %#v", got, want)
	}
}

func TestMarshalGlobsArtifact_is_deterministic(t *testing.T) {
	// Given
	game := GlobPaths{
		"z": {rojo.RbxPath{"ServerStorage", "Z"}},
		"a": {},
	}
	packages := map[string]GlobPaths{
		"pkg-b": {"q": {rojo.RbxPath{"ReplicatedStorage", "Q"}}},
		"pkg-a": {},
	}

	// When
	got, emit, err := MarshalGlobsArtifact(&game, packages)
	// Then
	if err != nil {
		t.Fatalf("MarshalGlobsArtifact: %v", err)
	}
	if !emit {
		t.Fatal("MarshalGlobsArtifact emit = false, want true")
	}
	want := `{"game":{"a":[],"z":[["ServerStorage","Z"]]},"packages":{"pkg-a":{},"pkg-b":{"q":[["ReplicatedStorage","Q"]]}}}`
	if string(got) != want {
		t.Fatalf("MarshalGlobsArtifact = %s, want %s", got, want)
	}
}
