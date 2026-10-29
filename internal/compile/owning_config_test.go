package compile

import (
	"errors"
	"os"
	"path/filepath"
	"testing"
)

func writeOwningConfigFile(t *testing.T, path, text string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o750); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(text), 0o600); err != nil {
		t.Fatal(err)
	}
}

// A solution coordinator at the root owns nothing; the referenced project that
// includes the file does. Opening the file must resolve to that project and
// not to the empty coordinator.
func TestOwningConfigFollowsCoordinatorReferences(t *testing.T) {
	root := t.TempDir()
	writeOwningConfigFile(t, filepath.Join(root, "tsconfig.json"),
		`{"files":[],"include":[],"references":[{"path":"./other"},{"path":"./tsconfig.lib.json"}]}`)
	writeOwningConfigFile(t, filepath.Join(root, "tsconfig.lib.json"),
		`{"compilerOptions":{"rootDir":"src"},"include":["src/**/*.ts"]}`)
	writeOwningConfigFile(t, filepath.Join(root, "other", "tsconfig.json"),
		`{"include":["**/*.ts"]}`)
	source := filepath.Join(root, "src", "a.ts")
	writeOwningConfigFile(t, source, "export {};\n")

	got, err := OwningConfig(filepath.Join(root, "tsconfig.json"), source)
	if err != nil {
		t.Fatal(err)
	}
	if want := filepath.Join(root, "tsconfig.lib.json"); got != want {
		t.Errorf("OwningConfig = %s, want %s", got, want)
	}
}

func TestOwningConfigKeepsPlainConfig(t *testing.T) {
	root := t.TempDir()
	config := filepath.Join(root, "tsconfig.json")
	writeOwningConfigFile(t, config, `{"include":["src"]}`)
	source := filepath.Join(root, "src", "a.ts")
	writeOwningConfigFile(t, source, "export {};\n")

	got, err := OwningConfig(config, source)
	if err != nil {
		t.Fatal(err)
	}
	if got != config {
		t.Errorf("OwningConfig = %s, want %s", got, config)
	}
}

func TestOwningConfigReportsFileNoProjectIncludes(t *testing.T) {
	root := t.TempDir()
	writeOwningConfigFile(t, filepath.Join(root, "tsconfig.json"),
		`{"files":[],"include":[],"references":[{"path":"./tsconfig.lib.json"}]}`)
	writeOwningConfigFile(t, filepath.Join(root, "tsconfig.lib.json"), `{"include":["src"]}`)
	source := filepath.Join(root, "scripts", "a.ts")
	writeOwningConfigFile(t, source, "export {};\n")

	_, err := OwningConfig(filepath.Join(root, "tsconfig.json"), source)
	if !errors.Is(err, ErrNoOwningConfig) {
		t.Errorf("err = %v, want ErrNoOwningConfig", err)
	}
}
