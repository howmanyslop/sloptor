//go:build windows

package migrate

import (
	"os"
	"testing"
)

func createNonRegularTarget(t *testing.T, path string) {
	t.Helper()
	if err := os.Mkdir(path, 0o755); err != nil {
		t.Fatal(err)
	}
}

func isNonRegularTarget(info os.FileInfo) bool {
	return info.IsDir()
}
