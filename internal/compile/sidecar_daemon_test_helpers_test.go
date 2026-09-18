package compile

import (
	"testing"
	"time"
)

func waitForSidecarDaemon(t *testing.T, runtimeDir, id string, done <-chan error) {
	t.Helper()
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		if _, err := readSidecarDaemonMetadata(runtimeDir, id); err == nil {
			return
		}
		select {
		case err := <-done:
			t.Fatalf("sidecar daemon exited during startup: %v", err)
		case <-time.After(10 * time.Millisecond):
		}
	}
	t.Fatal("sidecar daemon did not publish metadata")
}
