//go:build !windows

package compile

import (
	"context"
	"errors"
	"fmt"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"syscall"
)

func sidecarDaemonEndpoint(runtimeDir, id string) string {
	name := shortSidecarDaemonHash(runtimeDir) + "-" + id + ".sock"
	root := filepath.Join(os.TempDir(), fmt.Sprintf("rotor-%d", os.Geteuid()))
	if len(filepath.Join(root, name)) > 100 {
		root = filepath.Join("/tmp", fmt.Sprintf("rotor-%d", os.Geteuid()))
	}
	return filepath.Join(root, name)
}

func listenSidecarDaemon(endpoint string) (net.Listener, error) {
	if err := prepareSidecarDaemonEndpoint(endpoint); err != nil {
		return nil, err
	}
	listener, err := net.Listen("unix", endpoint)
	if err != nil {
		return nil, err
	}
	if err := os.Chmod(endpoint, 0o600); err != nil {
		_ = listener.Close()
		_ = os.Remove(endpoint)
		return nil, err
	}
	return listener, nil
}

func prepareSidecarDaemonEndpoint(endpoint string) error {
	root := filepath.Dir(endpoint)
	if err := ensurePrivateSidecarIPCRoot(root); err != nil {
		return err
	}
	info, err := os.Lstat(endpoint)
	if errors.Is(err, os.ErrNotExist) {
		return nil
	}
	if err != nil {
		return fmt.Errorf("inspect sidecar daemon endpoint: %w", err)
	}
	if info.Mode()&os.ModeSocket == 0 {
		return fmt.Errorf("sidecar daemon endpoint is not a socket: %s", endpoint)
	}
	return nil
}

func ensurePrivateSidecarIPCRoot(root string) error {
	err := os.Mkdir(root, 0o700)
	if err != nil && !errors.Is(err, os.ErrExist) {
		return fmt.Errorf("create sidecar daemon IPC directory: %w", err)
	}
	info, err := os.Lstat(root)
	if err != nil {
		return fmt.Errorf("inspect sidecar daemon IPC directory: %w", err)
	}
	if info.Mode()&os.ModeSymlink != 0 || !info.IsDir() {
		return fmt.Errorf("sidecar daemon IPC path is not a private directory: %s", root)
	}
	stat, ok := info.Sys().(*syscall.Stat_t)
	if !ok || stat.Uid != uint32(os.Geteuid()) {
		return fmt.Errorf("sidecar daemon IPC directory is not owned by the current user: %s", root)
	}
	if err := os.Chmod(root, 0o700); err != nil {
		return fmt.Errorf("secure sidecar daemon IPC directory: %w", err)
	}
	return nil
}

func dialSidecarDaemon(ctx context.Context, endpoint string) (net.Conn, error) {
	var dialer net.Dialer
	return dialer.DialContext(ctx, "unix", endpoint)
}

func removeSidecarDaemonEndpoint(endpoint string) {
	if ensurePrivateSidecarIPCRoot(filepath.Dir(endpoint)) != nil {
		return
	}
	info, err := os.Lstat(endpoint)
	if err == nil && info.Mode()&os.ModeSocket != 0 {
		_ = os.Remove(endpoint)
	}
}

func configureSidecarDaemonProcess(cmd *exec.Cmd) {
	cmd.SysProcAttr = &syscall.SysProcAttr{Setsid: true}
}

func sidecarProcessAlive(pid int) bool {
	process, err := os.FindProcess(pid)
	if err != nil {
		return false
	}
	err = process.Signal(syscall.Signal(0))
	return err == nil || errors.Is(err, os.ErrPermission)
}
