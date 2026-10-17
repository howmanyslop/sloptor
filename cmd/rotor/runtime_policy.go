package main

import (
	"os"
	"runtime/debug"
)

const (
	fallbackMemory = int64(8 << 30)
	minimumLimit   = int64(512 << 20)
	maximumLimit   = int64(16 << 30)
	watchLimit     = int64(6 << 30)
)

// applyRuntimePolicy scopes CLI defaults to an invocation. Environment values
// belong to the caller, including empty values and GOGC=off, so never replace
// them. Library/API watch sessions retain their process owner's runtime policy.
func applyRuntimePolicy(watch bool) func() {
	var restoreGC, restoreLimit func()
	if _, set := os.LookupEnv("GOGC"); !set {
		percent := 400
		if watch {
			percent = 100
		}
		previous := debug.SetGCPercent(percent)
		restoreGC = func() { debug.SetGCPercent(previous) }
	}
	if _, set := os.LookupEnv("GOMEMLIMIT"); !set {
		limit := automaticMemoryLimit(effectiveMemory())
		if watch {
			limit = min(limit, watchLimit)
		}
		previous := debug.SetMemoryLimit(limit)
		restoreLimit = func() { debug.SetMemoryLimit(previous) }
	}
	return func() {
		if restoreGC != nil {
			restoreGC()
		}
		if restoreLimit != nil {
			restoreLimit()
		}
	}
}

func automaticMemoryLimit(memory int64) int64 {
	if memory <= 0 {
		memory = fallbackMemory
	}
	limit := memory - memory/4
	if limit < minimumLimit {
		return minimumLimit
	}
	if limit > maximumLimit {
		return maximumLimit
	}
	return limit
}
