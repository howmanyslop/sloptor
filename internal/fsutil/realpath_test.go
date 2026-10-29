package fsutil

import (
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"testing"
)

func TestEvalSymlinksResolvesAliasedDirectories(t *testing.T) {
	for _, kind := range []string{"junction", "symlink"} {
		t.Run(kind, func(t *testing.T) {
			// Given a directory reached through an alias
			real := filepath.Join(t.TempDir(), "real")
			alias := filepath.Join(t.TempDir(), "alias")
			if err := os.Mkdir(real, 0o755); err != nil {
				t.Fatal(err)
			}
			if err := os.WriteFile(filepath.Join(real, "a.ts"), nil, 0o644); err != nil {
				t.Fatal(err)
			}
			if kind == "junction" {
				if runtime.GOOS != "windows" {
					t.Skip("directory junctions are Windows-only")
				}
				if err := exec.Command("cmd", "/c", "mklink", "/J", alias, real).Run(); err != nil {
					t.Skipf("directory junctions unavailable: %v", err)
				}
			} else if err := os.Symlink(real, alias); err != nil {
				t.Skipf("symlinks unavailable: %v", err)
			}
			want, err := EvalSymlinks(filepath.Join(real, "a.ts"))
			if err != nil {
				t.Fatal(err)
			}

			// When a file is resolved through the alias
			got, err := EvalSymlinks(filepath.Join(alias, "a.ts"))
			// Then it has the real spelling
			if err != nil {
				t.Fatal(err)
			}
			if got != want {
				t.Errorf("EvalSymlinks through %s = %q, want %q", kind, got, want)
			}
		})
	}
}

func TestEvalSymlinksFailsOnMissingPath(t *testing.T) {
	if _, err := EvalSymlinks(filepath.Join(t.TempDir(), "missing")); err == nil {
		t.Error("EvalSymlinks of a missing path succeeded")
	}
}
