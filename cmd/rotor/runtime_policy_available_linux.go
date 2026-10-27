package main

import "github.com/mackerelio/go-osstat/memory"

// availableMemory is MemAvailable; -1 when unknown.
func availableMemory() int64 {
	stats, err := memory.Get()
	if err != nil || !stats.MemAvailableEnabled {
		return -1
	}
	return int64(stats.Available)
}
