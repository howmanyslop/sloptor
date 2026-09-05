//go:build linux

package compile

import (
	"fmt"
	"os"
	"strings"
)

func sidecarProcessGeneration(pid int) string {
	if pid <= 0 {
		return ""
	}
	status, err := os.ReadFile(fmt.Sprintf("/proc/%d/stat", pid))
	if err != nil {
		return ""
	}
	commandEnd := strings.LastIndexByte(string(status), ')')
	if commandEnd < 0 {
		return ""
	}
	fields := strings.Fields(string(status[commandEnd+1:]))
	if len(fields) <= 19 {
		return ""
	}
	return fields[19]
}
