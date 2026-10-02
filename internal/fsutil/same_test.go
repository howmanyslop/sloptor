package fsutil

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestWriteFileIfChangedKeepsIdenticalFile(t *testing.T) {
	path := filepath.Join(t.TempDir(), "a.txt")
	if err := os.WriteFile(path, []byte("abc"), 0o644); err != nil {
		t.Fatal(err)
	}
	old := time.Unix(100, 0)
	if err := os.Chtimes(path, old, old); err != nil {
		t.Fatal(err)
	}
	wrote, err := WriteFileIfChanged(path, []byte("abc"), 0o644)
	if err != nil || wrote {
		t.Fatalf("wrote = %v, err = %v; want skip", wrote, err)
	}
	info, _ := os.Stat(path)
	if !info.ModTime().Equal(old) {
		t.Fatalf("modtime changed to %v", info.ModTime())
	}
	for _, next := range []string{"xyz", "wxyz"} {
		wrote, err = WriteFileIfChanged(path, []byte(next), 0o644)
		got, _ := os.ReadFile(path)
		if err != nil || !wrote || string(got) != next {
			t.Fatalf("write %q: wrote = %v, err = %v, got %q", next, wrote, err, got)
		}
	}
}
