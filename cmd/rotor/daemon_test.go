package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// Catches scripts being unable to distinguish an empty daemon registry from a
// command failure. The literal output is the CLI contract for the empty state.
func TestDaemonStatusAndStopWhenNothingIsRunning(t *testing.T) {
	t.Setenv("ROTOR_DAEMON_RUNTIME_DIR", t.TempDir())
	code, out, errOut := runCLI(t, "daemon", "status")
	if code != 0 || out != "no sidecar daemons running\n" || errOut != "" {
		t.Fatalf("daemon status = (%d, %q, %q)", code, out, errOut)
	}
	code, out, errOut = runCLI(t, "daemon", "stop")
	if code != 0 || out != "no sidecar daemons running\n" || errOut != "" {
		t.Fatalf("daemon stop = (%d, %q, %q)", code, out, errOut)
	}
}

// Catches corrupt runtime metadata being reported as a healthy empty registry.
// Runtime failures use the existing error-only stderr contract without a help hint.
func TestDaemonStatusReportsCorruptMetadataAsRuntimeFailure(t *testing.T) {
	runtimeDir := t.TempDir()
	t.Setenv("ROTOR_DAEMON_RUNTIME_DIR", runtimeDir)
	if err := os.WriteFile(filepath.Join(runtimeDir, "broken.json"), []byte("not json\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	code, out, errOut := runCLI(t, "daemon", "status")
	if code != 1 || out != "" || !strings.Contains(errOut, "read sidecar daemon metadata") {
		t.Fatalf("daemon status = (%d, %q, %q)", code, out, errOut)
	}
	if strings.Contains(errOut, "for usage") {
		t.Fatalf("runtime failure contains usage hint: %q", errOut)
	}
}
