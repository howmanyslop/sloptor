//go:build darwin

package compile

import (
	"fmt"

	"golang.org/x/sys/unix"
)

func sidecarProcessGeneration(pid int) string {
	if pid <= 0 {
		return ""
	}
	process, err := unix.SysctlKinfoProc("kern.proc.pid", pid)
	if err != nil || process == nil {
		return ""
	}
	started := process.Proc.P_starttime
	return fmt.Sprintf("%d:%d", started.Sec, started.Usec)
}
