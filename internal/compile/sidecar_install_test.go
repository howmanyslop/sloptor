package compile

import (
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

// redirectUserCacheDir points os.UserCacheDir at a temp dir for the test.
func redirectUserCacheDir(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	switch runtime.GOOS {
	case "windows":
		t.Setenv("LocalAppData", dir)
	case "darwin":
		t.Setenv("HOME", dir)
	default:
		t.Setenv("XDG_CACHE_HOME", dir)
	}
	return dir
}

func TestResolveSidecarDirExtractsEmbeddedWorker(t *testing.T) {
	t.Setenv("ROTOR_SIDECAR_PATH", "")
	redirectUserCacheDir(t)

	dir, err := resolveSidecarDir()
	if err != nil {
		t.Fatalf("resolveSidecarDir: %v", err)
	}
	for _, name := range []string{"main.js", "index.js", filepath.Join("lib", "session.js"), filepath.Join("lib", "plugins.js"), filepath.Join("lib", "diagnostics.js"), "package.json"} {
		if _, err := os.Stat(filepath.Join(dir, name)); err != nil {
			t.Fatalf("extracted sidecar missing %s: %v", name, err)
		}
	}

	// Idempotent: a second call returns the same completed dir.
	again, err := resolveSidecarDir()
	if err != nil {
		t.Fatalf("second resolveSidecarDir: %v", err)
	}
	if again != dir {
		t.Fatalf("resolveSidecarDir not stable: %q != %q", again, dir)
	}
}

func TestResolveSidecarDirHonorsOverride(t *testing.T) {
	override := repoSidecarDir(t)
	t.Setenv("ROTOR_SIDECAR_PATH", override)
	dir, err := resolveSidecarDir()
	if err != nil {
		t.Fatalf("resolveSidecarDir: %v", err)
	}
	if dir != override {
		t.Fatalf("dir = %q, want override %q", dir, override)
	}
}
