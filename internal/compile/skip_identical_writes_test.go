package compile

import (
	"io/fs"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"
)

func TestBuildProjectRebuildLeavesUnchangedOutputMtimes(t *testing.T) {
	for _, incremental := range []bool{false, true} {
		t.Run(map[bool]string{false: "full", true: "incremental"}[incremental], func(t *testing.T) {
			dir := writeProject(t, "@scope/skip-writes-fixture", "")
			if incremental {
				enableIncrementalBuilds(t, dir)
			}
			assertRebuildKeepsOutputMtimes(t, dir)
		})
	}
}

func assertRebuildKeepsOutputMtimes(t *testing.T, dir string) {
	t.Helper()
	writeIncrementalFixture(t, dir)
	for range 2 {
		if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{}); err != nil || len(diags) > 0 {
			t.Fatalf("seed build: %v (diags: %v)", err, diags)
		}
	}

	old := time.Unix(100, 0)
	outputs := ageOutputs(t, dir, old)
	if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{}); err != nil || len(diags) > 0 {
		t.Fatalf("no-change build: %v (diags: %v)", err, diags)
	}
	if changed := changedOutputs(t, dir, outputs, old); len(changed) > 0 {
		t.Fatalf("no-change build rewrote %v", changed)
	}

	if err := os.WriteFile(filepath.Join(dir, "src", "side.ts"), []byte("export const side = 2;\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{}); err != nil || len(diags) > 0 {
		t.Fatalf("edit build: %v (diags: %v)", err, diags)
	}
	for _, rel := range changedOutputs(t, dir, outputs, old) {
		if !strings.HasPrefix(rel, "out/side.") && !strings.HasSuffix(rel, ".tsbuildinfo") {
			t.Errorf("edit build rewrote unrelated output %s", rel)
		}
	}
}

func ageOutputs(t *testing.T, dir string, at time.Time) []string {
	t.Helper()
	var outputs []string
	err := filepath.WalkDir(dir, func(path string, entry fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, _ := filepath.Rel(dir, path)
		rel = filepath.ToSlash(rel)
		if entry.IsDir() {
			if rel == "src" || rel == "node_modules" {
				return filepath.SkipDir
			}
			return nil
		}
		if !strings.HasPrefix(rel, "out/") && !strings.HasPrefix(rel, "include/") {
			return nil
		}
		outputs = append(outputs, rel)
		return os.Chtimes(path, at, at)
	})
	if err != nil {
		t.Fatal(err)
	}
	if len(outputs) == 0 {
		t.Fatal("no outputs found")
	}
	return outputs
}

func changedOutputs(t *testing.T, dir string, outputs []string, at time.Time) []string {
	t.Helper()
	var changed []string
	for _, rel := range outputs {
		info, err := os.Stat(filepath.Join(dir, filepath.FromSlash(rel)))
		if err != nil || !info.ModTime().Equal(at) {
			changed = append(changed, rel)
		}
	}
	slices.Sort(changed)
	return changed
}
