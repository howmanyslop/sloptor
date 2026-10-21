package main

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"
)

func TestMirrorPreservesTransientCheckerMappers(t *testing.T) {
	t.Chdir(filepath.Join("..", ".."))
	destination := mirrorDestination(t)
	source := mirrorSource(t, func(_ string, data []byte) []byte { return data })
	if _, _, err := mirror(source, "HEAD", destination); err != nil {
		t.Fatalf("mirror checker mappers: %v", err)
	}

	for _, name := range []string{"checker.go", "mapper.go", "mapper_test.go"} {
		want, err := os.ReadFile(filepath.Join(outDir, "checker", name))
		if err != nil {
			t.Fatal(err)
		}
		got, err := os.ReadFile(filepath.Join(destination, "checker", name))
		if err != nil {
			t.Fatal(err)
		}
		if name != "mapper_test.go" {
			got = bytes.ReplaceAll(got, []byte("\r\n"), []byte("\n"))
			want = bytes.ReplaceAll(want, []byte("\r\n"), []byte("\n"))
		}
		if !bytes.Equal(got, want) {
			t.Errorf("regenerated checker/%s differs from the checked-in file", name)
		}
	}
}
