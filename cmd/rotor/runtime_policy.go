package main

import (
	"context"
	"fmt"
	"os"
	"runtime"
	"runtime/debug"
	"runtime/metrics"
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

	// A one-shot build's heap may grow to liveHeapHeadroom times the live
	// heap, but never below liveHeapFloor.
	liveHeapHeadroom = 2
	liveHeapFloor    = int64(8 << 30)

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

// admitBuild sizes a one-shot build's runtime to the machine it shares. It
// holds a machine-wide build slot, so parallel invocations (an nx run, a CI
// matrix, API sessions) split the budget instead of each claiming all of it,
// and it keeps the memory limit within what the machine has free right now,
// so the build collects harder instead of pushing other work into swap.
// Admission fails open: without a usable slot directory the build runs
// unshared.
func admitBuild(ctx context.Context, streams cliStreams) func() {
	if !streams.admit {
		return func() {}
	}
	total := effectiveMemory()
	lease := acquireBuildSlot(ctx, streams, total)
	share := 1
	if lease != nil {
		share = lease.Active()
		logservice.WriteLineIfVerbose(fmt.Sprintf("admission: slot %d/%d, sharing with %d", lease.Index()+1, lease.Slots(), share-1))
	}
	restore := applySharedRuntimePolicy(total, share)
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
				if lease != nil {
					if next := lease.Active(); next != share {
						share = next
						applySharedRuntimePolicy(total, share)
						continue
					}
				}
				if _, set := os.LookupEnv("GOMEMLIMIT"); !set {
					debug.SetMemoryLimit(sharedMemoryLimit(total, share))
				}
			}
		}
	}()
	return func() {
		close(done)
		<-stopped
		restore()
		if lease != nil {
			lease.Release()
		}
	}
}

func acquireBuildSlot(ctx context.Context, streams cliStreams, total int64) *admission.Lease {
	slots := admission.Slots(total, runtime.NumCPU())
	if slots == 0 {
		return nil
	}
	dir, err := admission.Dir()
	if err != nil {
		logservice.WriteLineIfVerbose(fmt.Sprintf("admission: disabled: %v", err))
		return nil
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
		return nil
	}
	return lease
}

// applySharedRuntimePolicy sizes the one-shot policy for share concurrent
// builds on a machine with total memory. The heap headroom of GOGC=400 only
// pays off with the machine to itself. Caller-set environment values win.
func applySharedRuntimePolicy(total int64, share int) func() {
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
		previousLimit = debug.SetMemoryLimit(sharedMemoryLimit(total, share))
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

// sharedMemoryLimit is this build's share of the automatic limit, capped at
// twice the live heap and at what the process holds plus three quarters of
// the memory free right now. GOGC=400 alone lets the heap reach five times
// live, and every fresh page costs a page fault and a madvise on first use:
// on a large solution that costs more than the extra collections.
func sharedMemoryLimit(total int64, share int) int64 {
	limit := automaticMemoryLimit(total) / int64(max(1, share))
	limit = min(limit, max(liveHeapFloor, liveHeapHeadroom*liveHeap()))
	if available := availableMemory(); available >= 0 {
		limit = min(limit, processMemory()+available-available/4)
	}
	return max(minimumLimit, limit)
}

// liveHeap is the heap the previous collection marked live.
func liveHeap() int64 {
	samples := []metrics.Sample{{Name: "/gc/heap/live:bytes"}}
	metrics.Read(samples)
	return int64(samples[0].Value.Uint64())
}

// processMemory is the memory the Go runtime holds from the OS.
func processMemory() int64 {
	samples := []metrics.Sample{{Name: "/memory/classes/total:bytes"}, {Name: "/memory/classes/heap/released:bytes"}}
	metrics.Read(samples)
	return int64(samples[0].Value.Uint64() - samples[1].Value.Uint64())
}
