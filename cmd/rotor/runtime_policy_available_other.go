//go:build !darwin && !linux

package main

import "github.com/mackerelio/go-osstat/memory"

// availableMemory is the free physical memory; -1 when unknown.
func availableMemory() int64 {
	stats, err := memory.Get()
	if err != nil || stats.Total == 0 {
		return -1
	}
	return int64(stats.Free)
}
