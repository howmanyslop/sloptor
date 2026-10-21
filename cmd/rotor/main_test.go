package main

import (
	"os"
	"path/filepath"
	"testing"
)

// TestMain points TMPDIR at its real path. On macOS the default lives under
// /var, a symlink to /private/var, and solution paths through an explicit
// symlink are spelled by their real path (see
// compile.canonicalSolutionConfigPath), so paths built from t.TempDir would
// never compare equal to the ones rotor reports.
func TestMain(m *testing.M) {
	if resolved, err := filepath.EvalSymlinks(os.TempDir()); err == nil {
		_ = os.Setenv("TMPDIR", resolved)
	}
	os.Exit(m.Run())
}
