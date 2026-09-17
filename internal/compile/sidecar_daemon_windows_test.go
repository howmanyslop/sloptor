//go:build windows

package compile

import (
	"context"
	"errors"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"time"

	"golang.org/x/sys/windows"
)

const (
	sidecarDaemonStopWaitHelperEnv = "ROTOR_SIDECAR_DAEMON_STOP_WAIT_HELPER"
	sidecarDaemonStopWaitReadyEnv  = "ROTOR_SIDECAR_DAEMON_STOP_WAIT_READY"
)

func TestWaitForSidecarDaemonStopWaitsThroughWindowsDeletionPendingMetadata(t *testing.T) {
	// Catches daemon stop treating legacy Windows deletion-pending metadata as
	// a failure or returning before the real daemon process has exited.
	runtimeDir := t.TempDir()
	id := "deletion-pending"
	metadataPath := sidecarDaemonMetadataPath(runtimeDir, id)
	if err := os.WriteFile(metadataPath, []byte("{}"), 0o600); err != nil {
		t.Fatal(err)
	}

	readyPath := filepath.Join(runtimeDir, "helper-ready")
	helper := exec.Command(os.Args[0], "-test.run=^TestSidecarDaemonStopWaitProcessHelper$")
	helper.Env = append(os.Environ(), sidecarDaemonStopWaitHelperEnv+"=1", sidecarDaemonStopWaitReadyEnv+"="+readyPath)
	helper.Stdout = os.Stdout
	helper.Stderr = os.Stderr
	stdin, err := helper.StdinPipe()
	if err != nil {
		t.Fatal(err)
	}
	if err := helper.Start(); err != nil {
		t.Fatal(err)
	}
	helperWaited := false
	t.Cleanup(func() {
		_ = stdin.Close()
		if !helperWaited {
			_ = helper.Process.Kill()
			_ = helper.Wait()
		}
	})

	deadline := time.Now().Add(5 * time.Second)
	for {
		if _, err := os.Stat(readyPath); err == nil {
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("stop-wait helper did not become ready")
		}
		time.Sleep(10 * time.Millisecond)
	}

	path, err := windows.UTF16PtrFromString(metadataPath)
	if err != nil {
		t.Fatal(err)
	}
	metadataHandle, err := windows.CreateFile(
		path,
		windows.DELETE|windows.GENERIC_READ,
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
	deleteFile := byte(1)
	if err := windows.SetFileInformationByHandle(metadataHandle, windows.FileDispositionInfo, &deleteFile, 1); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(metadataPath); !errors.Is(err, os.ErrPermission) {
		t.Fatalf("deletion-pending metadata stat error = %v, want permission denied", err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	waitDone := make(chan error, 1)
	go func() {
		waitDone <- waitForSidecarDaemonStop(ctx, runtimeDir, sidecarDaemonMetadata{ID: id, PID: helper.Process.Pid})
	}()
	select {
	case err := <-waitDone:
		t.Fatalf("stop wait returned while daemon process was alive: %v", err)
	case <-time.After(100 * time.Millisecond):
	}
	if err := stdin.Close(); err != nil {
		t.Fatal(err)
	}
	if err := <-waitDone; err != nil {
		t.Fatal(err)
	}
	if sidecarProcessAlive(helper.Process.Pid) {
		t.Fatal("stop wait returned before daemon process exit")
	}
	waitErr := helper.Wait()
	helperWaited = true
	if waitErr != nil {
		t.Fatalf("stop-wait helper exit failed: %v", waitErr)
	}
}

func TestSidecarDaemonStopWaitProcessHelper(t *testing.T) {
	if os.Getenv(sidecarDaemonStopWaitHelperEnv) != "1" {
		t.Skip("helper process")
	}
	if err := os.WriteFile(os.Getenv(sidecarDaemonStopWaitReadyEnv), nil, 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := io.Copy(io.Discard, os.Stdin); err != nil {
		t.Fatal(err)
	}
}
