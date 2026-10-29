// Package admission keeps concurrent one-shot builds on one machine from each
// sizing themselves as if they owned it. A build holds one of a fixed number
// of slot files under an OS file lock; the OS drops the lock when the process
// exits, so a crashed build never strands its slot.
package admission

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"time"
)

// MaxBuildsEnv overrides the slot count; 0 disables admission entirely.
const MaxBuildsEnv = "ROTOR_MAX_BUILDS"

const (
	slotMemory   = int64(6 << 30)
	pollInterval = 100 * time.Millisecond
)

// DefaultSlots allows one build per 6 GiB of memory, at most one per two CPUs.
func DefaultSlots(memory int64, cpus int) int {
	return max(1, min(int(memory/slotMemory), cpus/2))
}

// Slots resolves the slot count, honoring MaxBuildsEnv. Zero means disabled.
func Slots(memory int64, cpus int) int {
	if raw, set := os.LookupEnv(MaxBuildsEnv); set {
		if n, err := strconv.Atoi(raw); err == nil && n >= 0 {
			return n
		}
	}
	return DefaultSlots(memory, cpus)
}

// Dir is the per-user directory that holds the slot files.
func Dir() (string, error) {
	cache, err := os.UserCacheDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(cache, "rotor", "slots"), nil
}

// Lease is one held slot.
type Lease struct {
	dir   string
	slots int
	index int
	file  *os.File
}

// Acquire takes a free slot in dir, waiting until one frees up. onWait runs
// once if no slot is free on the first pass.
func Acquire(ctx context.Context, dir string, slots int, onWait func()) (*Lease, error) {
	if slots < 1 {
		return nil, fmt.Errorf("admission: %d slots", slots)
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return nil, err
	}
	ticker := time.NewTicker(pollInterval)
	defer ticker.Stop()
	for waited := false; ; waited = true {
		for index := range slots {
			file, err := tryLock(slotPath(dir, index))
			if err != nil {
				return nil, err
			}
			if file != nil {
				return &Lease{dir: dir, slots: slots, index: index, file: file}, nil
			}
		}
		if !waited && onWait != nil {
			onWait()
		}
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		case <-ticker.C:
		}
	}
}

// Index is this lease's slot number.
func (l *Lease) Index() int { return l.index }

// Slots is the total slot count the lease was taken against.
func (l *Lease) Slots() int { return l.slots }

// Active counts held slots, this one included, by probing the others.
func (l *Lease) Active() int {
	active := 1
	for index := range l.slots {
		if index == l.index {
			continue
		}
		file, err := tryLock(slotPath(l.dir, index))
		if err != nil {
			continue
		}
		if file == nil {
			active++
			continue
		}
		unlock(file)
	}
	return active
}

// Release frees the slot. A nil lease is a no-op so unchecked Acquire
// results cannot panic their releaser.
func (l *Lease) Release() {
	if l == nil {
		return
	}
	if l.file != nil {
		unlock(l.file)
		l.file = nil
	}
}

func slotPath(dir string, index int) string {
	return filepath.Join(dir, fmt.Sprintf("slot-%d.lock", index))
}
