//go:build windows

package compile

import (
	"context"
	"errors"
	"os"
	"os/exec"
	"testing"
	"time"

	"golang.org/x/sys/windows"
)

const sidecarDaemonTestHelperEnv = "ROTOR_SIDECAR_DAEMON_TEST_HELPER"

func TestStopSidecarDaemonsWaitsThroughWindowsDeletionPendingMetadata(t *testing.T) {
	// Catches daemon stop failing after Windows accepts metadata deletion but
	// keeps the path deletion-pending until another shared handle is closed.
	runtimeDir := t.TempDir()
	t.Setenv(sidecarDaemonRuntimeEnv, runtimeDir)
	id, err := sidecarDaemonID(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}

	daemon := exec.Command(os.Args[0], "-test.run=^TestSidecarDaemonProcessHelper$")
	daemon.Env = append(os.Environ(), sidecarDaemonTestHelperEnv+"=1", sidecarDaemonRuntimeEnv+"="+runtimeDir, "ROTOR_SIDECAR_DAEMON_TEST_ID="+id)
	daemon.Stdout = os.Stdout
	daemon.Stderr = os.Stderr
	if err := daemon.Start(); err != nil {
		t.Fatal(err)
	}
	daemonWaited := false
	t.Cleanup(func() {
		if !daemonWaited {
			_ = daemon.Process.Kill()
			_ = daemon.Wait()
		}
	})

	metadataPath := sidecarDaemonMetadataPath(runtimeDir, id)
	deadline := time.Now().Add(5 * time.Second)
	for {
		if _, err := readSidecarDaemonMetadata(runtimeDir, id); err == nil {
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("sidecar daemon did not publish metadata")
		}
		time.Sleep(10 * time.Millisecond)
	}

	path, err := windows.UTF16PtrFromString(metadataPath)
	if err != nil {
		t.Fatal(err)
	}
	metadataHandle, err := windows.CreateFile(
		path,
		windows.GENERIC_READ,
		windows.FILE_SHARE_READ|windows.FILE_SHARE_WRITE|windows.FILE_SHARE_DELETE,
		nil,
		windows.OPEN_EXISTING,
		windows.FILE_ATTRIBUTE_NORMAL,
		0,
	)
	if err != nil {
		t.Fatal(err)
	}
	defer windows.CloseHandle(metadataHandle)

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	stopped, err := StopSidecarDaemons(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if stopped != 1 {
		t.Fatalf("stopped daemons = %d, want 1", stopped)
	}
	if _, err := os.Stat(metadataPath); !errors.Is(err, os.ErrPermission) {
		t.Fatalf("deletion-pending metadata stat error = %v, want permission denied", err)
	}
	waitErr := daemon.Wait()
	daemonWaited = true
	if waitErr != nil {
		t.Fatalf("sidecar daemon exit failed: %v", waitErr)
	}
}

func TestSidecarDaemonProcessHelper(t *testing.T) {
	if os.Getenv(sidecarDaemonTestHelperEnv) != "1" {
		t.Skip("helper process")
	}
	if err := RunSidecarDaemon(os.Getenv(sidecarDaemonRuntimeEnv), os.Getenv("ROTOR_SIDECAR_DAEMON_TEST_ID")); err != nil {
		t.Fatal(err)
	}
}
