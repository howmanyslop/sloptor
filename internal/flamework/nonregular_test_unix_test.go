//go:build !windows

package flamework

import (
	"os"
	"syscall"
	"testing"
)

func createNonRegularTarget(t *testing.T, path string) {
	t.Helper()
	if err := syscall.Mkfifo(path, 0o600); err != nil {
		t.Fatal(err)
	}
}

func isNonRegularTarget(info os.FileInfo) bool {
	return info.Mode()&os.ModeNamedPipe != 0
}
