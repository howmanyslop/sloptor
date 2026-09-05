//go:build windows

package compile

import (
	"context"
	"fmt"
	"net"
	"os/exec"
	"path/filepath"
	"strings"

	"github.com/Microsoft/go-winio"
	"golang.org/x/sys/windows"
)

func sidecarDaemonEndpoint(runtimeDir, id string) string {
	root := strings.NewReplacer(":", "-", "\\", "-", "/", "-").Replace(filepath.Clean(runtimeDir))
	return `\\.\pipe\rotor-sidecar-` + shortSidecarDaemonHash(root) + "-" + id
}

func listenSidecarDaemon(endpoint string) (net.Listener, error) {
	var token windows.Token
	if err := windows.OpenProcessToken(windows.CurrentProcess(), windows.TOKEN_QUERY, &token); err != nil {
		return nil, err
	}
	defer token.Close()
	user, err := token.GetTokenUser()
	if err != nil {
		return nil, err
	}
	sid := user.User.Sid.String()
	return winio.ListenPipe(endpoint, &winio.PipeConfig{
		SecurityDescriptor: fmt.Sprintf("D:P(A;;GA;;;%s)", sid),
	})
}

func prepareSidecarDaemonEndpoint(string) error { return nil }

func dialSidecarDaemon(ctx context.Context, endpoint string) (net.Conn, error) {
	return winio.DialPipeContext(ctx, endpoint)
}

func removeSidecarDaemonEndpoint(string) {}

func configureSidecarDaemonProcess(cmd *exec.Cmd) {
	cmd.SysProcAttr = &windows.SysProcAttr{
		CreationFlags: windows.CREATE_NEW_PROCESS_GROUP | windows.DETACHED_PROCESS,
	}
}

func sidecarProcessAlive(pid int) bool {
	handle, err := windows.OpenProcess(windows.PROCESS_QUERY_LIMITED_INFORMATION, false, uint32(pid))
	if err != nil {
		return false
	}
	_ = windows.CloseHandle(handle)
	return true
}

func sidecarProcessGeneration(pid int) string {
	if pid <= 0 {
		return ""
	}
	handle, err := windows.OpenProcess(windows.PROCESS_QUERY_LIMITED_INFORMATION, false, uint32(pid))
	if err != nil {
		return ""
	}
	defer windows.CloseHandle(handle)
	var created, exited, kernel, user windows.Filetime
	if err := windows.GetProcessTimes(handle, &created, &exited, &kernel, &user); err != nil {
		return ""
	}
	return fmt.Sprintf("%d:%d", created.HighDateTime, created.LowDateTime)
}
