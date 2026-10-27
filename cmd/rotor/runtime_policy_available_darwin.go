package main

import "golang.org/x/sys/unix"

// availableMemory is the memory macOS reports free for use, the figure behind
// memory pressure; -1 when unknown.
func availableMemory() int64 {
	level, err := unix.SysctlUint32("kern.memorystatus_level")
	if err != nil {
		return -1
	}
	total, err := unix.SysctlUint64("hw.memsize")
	if err != nil {
		return -1
	}
	return int64(total / 100 * uint64(level))
}
