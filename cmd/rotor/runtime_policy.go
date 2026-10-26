package main

import (
	"context"
	"fmt"
	"os"
	"runtime"
	"runtime/debug"
	"time"

	"rotor/internal/admission"
	"rotor/internal/compile"
	"rotor/internal/logservice"
)

const (
	fallbackMemory = int64(8 << 30)
	minimumLimit   = int64(512 << 20)
	maximumLimit   = int64(16 << 30)
	watchLimit     = int64(6 << 30)

	admissionWaitNotice = time.Second
	admissionRefresh    = 500 * time.Millisecond
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

// admitBuild holds a machine-wide build slot for a one-shot build and splits
// the runtime budget across the builds sharing the machine, so parallel
// invocations (an nx run, a CI matrix, API sessions) do not each claim all of
// it. Admission
// fails open: if the slot directory is unusable, the build runs unshared.
func admitBuild(ctx context.Context, streams cliStreams) func() {
	if !streams.admit {
		return func() {}
	}
	slots := admission.Slots(effectiveMemory(), runtime.NumCPU())
	if slots == 0 {
		return func() {}
	}
	dir, err := admission.Dir()
	if err != nil {
		logservice.WriteLineIfVerbose(fmt.Sprintf("admission: disabled: %v", err))
		return func() {}
	}
	var notice *time.Timer
	lease, err := admission.Acquire(ctx, dir, slots, func() {
		notice = time.AfterFunc(admissionWaitNotice, func() {
			fmt.Fprintf(streams.err, "waiting for a build slot: %d builds already running (set %s to change)\n", slots, admission.MaxBuildsEnv)
		})
	})
	if notice != nil {
		notice.Stop()
	}
	if err != nil {
		logservice.WriteLineIfVerbose(fmt.Sprintf("admission: disabled: %v", err))
		return func() {}
	}

	share := lease.Active()
	logservice.WriteLineIfVerbose(fmt.Sprintf("admission: slot %d/%d, sharing with %d", lease.Index()+1, lease.Slots(), share-1))
	restore := applySharedRuntimePolicy(share)
	done := make(chan struct{})
	stopped := make(chan struct{})
	go func() {
		defer close(stopped)
		ticker := time.NewTicker(admissionRefresh)
		defer ticker.Stop()
		for {
			select {
			case <-done:
				return
			case <-ticker.C:
				if next := lease.Active(); next != share {
					share = next
					applySharedRuntimePolicy(share)
				}
			}
		}
	}()
	return func() {
		close(done)
		<-stopped
		restore()
		lease.Release()
	}
}

// applySharedRuntimePolicy sizes the one-shot policy for share concurrent
// builds. The heap headroom of GOGC=400 only pays off with the machine to
// itself. Caller-set environment values still win.
func applySharedRuntimePolicy(share int) func() {
	share = max(1, share)
	previousGC, previousLimit, previousProcs := -2, int64(-1), 0
	if _, set := os.LookupEnv("GOGC"); !set {
		percent := 400
		if share > 1 {
			percent = 100
		}
		previousGC = debug.SetGCPercent(percent)
	}
	if _, set := os.LookupEnv("GOMEMLIMIT"); !set {
		previousLimit = debug.SetMemoryLimit(max(minimumLimit, automaticMemoryLimit(effectiveMemory())/int64(share)))
	}
	if _, set := os.LookupEnv("GOMAXPROCS"); !set {
		previousProcs = runtime.GOMAXPROCS(max(2, runtime.NumCPU()/share))
	}
	compile.SetBuildShare(share)
	return func() {
		if previousGC != -2 {
			debug.SetGCPercent(previousGC)
		}
		if previousLimit >= 0 {
			debug.SetMemoryLimit(previousLimit)
		}
		if previousProcs > 0 {
			runtime.GOMAXPROCS(previousProcs)
		}
		compile.SetBuildShare(1)
	}
}
