package flamework

import (
	"errors"
	"os"
	"path/filepath"
	"testing"

	"rotor/internal/config"
)

func TestOpenProject_whenGamePackageHasDefaults(t *testing.T) {
	// Given: an unscoped game project with the default Flamework options.
	root := t.TempDir()
	writeProjectFixture(t, root, `{"name":"fixture-game","version":"1.0.0"}`)

	// When: the concrete native project state is opened.
	project, err := OpenProject(ProjectOptions{
		ProjectDir: root,
		RootDir:    filepath.Join(root, "src"),
		OutDir:     filepath.Join(root, "out"),
	})
	if err != nil {
		t.Fatalf("OpenProject: %v", err)
	}

	// Then: identity, runtime locations, and upstream defaults are resolved.
	if got, want := project.PackageName(), "fixture-game"; got != want {
		t.Fatalf("PackageName = %q, want %q", got, want)
	}
	if !project.IsGame() {
		t.Fatal("IsGame = false, want true")
	}
	if got, want := project.IncludeDirectory(), filepath.Join(root, "include"); got != want {
		t.Fatalf("IncludeDirectory = %q, want %q", got, want)
	}
	if got, want := project.IDMode(), IDModeFull; got != want {
		t.Fatalf("IDMode = %q, want %q", got, want)
	}
	if got := project.HashPrefix(); got != "" {
		t.Fatalf("HashPrefix = %q, want empty", got)
	}
}

func TestOpenProject_whenScopedPackageUsesIdentityPrefix(t *testing.T) {
	// Given: a scoped package and explicit obfuscation without an ID mode.
	root := t.TempDir()
	writeProjectFixture(t, root, `{"name":"@scope/library","version":"2.0.0"}`)

	// When: native state applies upstream package defaults.
	project, err := OpenProject(ProjectOptions{
		ProjectDir: root,
		RootDir:    filepath.Join(root, "src"),
		OutDir:     filepath.Join(root, "out"),
		Config:     config.FlameworkConfig{Obfuscation: true},
	})
	if err != nil {
		t.Fatalf("OpenProject: %v", err)
	}

	// Then: package mode and prefix are inferred without a public pack-mode option.
	if project.IsGame() {
		t.Fatal("IsGame = true, want false")
	}
	if got, want := project.HashPrefix(), "@scope/library"; got != want {
		t.Fatalf("HashPrefix = %q, want %q", got, want)
	}
	if got, want := project.IDMode(), IDModeObfuscated; got != want {
		t.Fatalf("IDMode = %q, want %q", got, want)
	}
}

func TestOpenProject_whenPackageJSONIsMalformed(t *testing.T) {
	// Given: malformed package identity input.
	root := t.TempDir()
	writeProjectFixture(t, root, `{"name":`)

	// When: project state crosses the package boundary.
	_, err := OpenProject(ProjectOptions{ProjectDir: root, RootDir: filepath.Join(root, "src"), OutDir: filepath.Join(root, "out")})

	// Then: malformed input is rejected with a typed boundary error.
	if !errors.Is(err, ErrInvalidPackage) {
		t.Fatalf("OpenProject error = %v, want ErrInvalidPackage", err)
	}
}

func writeProjectFixture(t *testing.T, root, packageJSON string) {
	t.Helper()
	for _, dir := range []string{"src", "out"} {
		if err := os.Mkdir(filepath.Join(root, dir), 0o755); err != nil {
			t.Fatalf("Mkdir %s: %v", dir, err)
		}
	}
	if err := os.WriteFile(filepath.Join(root, "package.json"), []byte(packageJSON), 0o644); err != nil {
		t.Fatalf("WriteFile package.json: %v", err)
	}
}
