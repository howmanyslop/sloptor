package flamework

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestPersistArtifactsKeepsUnchangedMtimes(t *testing.T) {
	root := t.TempDir()
	artifacts := []Artifact{
		{Path: filepath.Join("include", "flamework", "config.json"), Data: []byte(`{"a":1}`)},
		{Path: filepath.Join("include", "flamework", "globs.json"), Data: []byte(`{}`)},
	}
	if err := PersistArtifacts(root, artifacts); err != nil {
		t.Fatal(err)
	}
	old := time.Unix(100, 0)
	for _, artifact := range artifacts {
		if err := os.Chtimes(filepath.Join(root, artifact.Path), old, old); err != nil {
			t.Fatal(err)
		}
	}
	artifacts[1].Data = []byte(`{"b":2}`)
	if err := PersistArtifacts(root, artifacts); err != nil {
		t.Fatal(err)
	}
	for index, wantOld := range []bool{true, false} {
		path := filepath.Join(root, artifacts[index].Path)
		info, err := os.Stat(path)
		if err != nil {
			t.Fatal(err)
		}
		if info.ModTime().Equal(old) != wantOld {
			t.Errorf("%s: mtime preserved = %v, want %v", artifacts[index].Path, !wantOld, wantOld)
		}
		if got, _ := os.ReadFile(path); string(got) != string(artifacts[index].Data) {
			t.Errorf("%s = %q", artifacts[index].Path, got)
		}
	}
}

func TestPersistArtifactsRejectsUnchangedDuplicatePaths(t *testing.T) {
	root := t.TempDir()
	artifact := Artifact{Path: filepath.Join("include", "flamework", "config.json"), Data: []byte(`{"a":1}`)}
	if err := PersistArtifacts(root, []Artifact{artifact}); err != nil {
		t.Fatal(err)
	}
	err := PersistArtifacts(root, []Artifact{artifact, artifact})
	if err == nil || !strings.Contains(err.Error(), "duplicate artifact path") {
		t.Fatalf("err = %v, want duplicate artifact path", err)
	}
}
