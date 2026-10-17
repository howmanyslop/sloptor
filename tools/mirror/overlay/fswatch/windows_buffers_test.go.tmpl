//go:build windows

package fswatch

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"testing"
	"time"
	"unsafe"

	"golang.org/x/sys/windows"
)

func bufferTestSubscription(t testing.TB) *windowsSubscription {
	t.Helper()
	s, err := newWindowsSubscription(newWindowsBackend(), &dirWatch{dir: t.TempDir(), recursive: true})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if s.handle != 0 {
			s.close()
		}
	})
	return s
}

func armBufferTestRead(t testing.TB, s *windowsSubscription, r *windowsRead) *windowsRead {
	t.Helper()
	r, err := s.beginRead(r)
	if err != nil || r == nil {
		t.Fatalf("beginRead: %v, %v", r, err)
	}
	return r
}

func completeBufferTestRead(t testing.TB, s *windowsSubscription, r *windowsRead) uint32 {
	t.Helper()
	// Bound even a regression that loses a notification; cancellation must also
	// finish before returning, so cleanup never races an outstanding wait.
	timer := time.AfterFunc(5*time.Second, s.stop)
	bytes, waitErr, err := r.wait(s)
	timer.Stop()
	if waitErr != nil || err != nil || bytes == 0 {
		t.Fatalf("read completion: bytes=%d wait=%v result=%v", bytes, waitErr, err)
	}
	return bytes
}

func TestWindowsReadBuffersReuse(t *testing.T) {
	s := bufferTestSubscription(t)
	current := armBufferTestRead(t, s, &s.reads[0])
	spare := &s.reads[1]
	var buffers [2]*byte
	var events [2]windows.Handle
	for i := range 40 {
		path := filepath.Join(s.dirWatch.dir, fmt.Sprintf("change-%d.txt", i))
		if err := os.WriteFile(path, []byte("changed"), 0o600); err != nil {
			t.Fatal(err)
		}
		bytes := completeBufferTestRead(t, s, current)
		next := armBufferTestRead(t, s, spare)
		if s.processCompletion(nil, current.buf, bytes) {
			t.Fatal("successful completion stopped the watch")
		}
		for j := range s.reads {
			r := &s.reads[j]
			if buffers[j] == nil {
				buffers[j], events[j] = &r.buf[0], r.event
			} else if buffers[j] != &r.buf[0] || events[j] != r.event {
				t.Fatalf("request %d replaced its buffer or event", j)
			}
		}
		spare, current = current, next
	}
	if buffers[0] == buffers[1] {
		t.Fatal("processing and pending reads share a buffer")
	}
	// Cleanup has an armed next request, including a completed-but-unwaited
	// request if Windows reports another change from the last write.
	s.close()
	for i := range s.reads {
		if s.reads[i].pending || s.reads[i].event != 0 || s.reads[i].buf != nil {
			t.Fatalf("request %d retained resources after close", i)
		}
	}
	// close is owned by run in production; avoid calling it twice in Cleanup.
	s.handle = 0
}

func TestWindowsReadCancellation(t *testing.T) {
	for range 30 {
		s := bufferTestSubscription(t)
		current := armBufferTestRead(t, s, &s.reads[0])
		done := make(chan error, 1)
		go func() {
			_, waitErr, err := current.wait(s)
			done <- errors.Join(waitErr, err)
		}()
		s.stop()
		select {
		case err := <-done:
			if !errors.Is(err, windows.ERROR_OPERATION_ABORTED) {
				t.Fatalf("cancellation result: %v", err)
			}
		case <-time.After(5 * time.Second):
			t.Fatal("cancellation did not complete")
		}
		if next, err := s.beginRead(&s.reads[1]); next != nil || err != nil {
			t.Fatalf("stopped subscription rearmed: %v, %v", next, err)
		}
	}
}

func TestWindowsReadNetworkFallback(t *testing.T) {
	s := bufferTestSubscription(t)
	current := armBufferTestRead(t, s, &s.reads[0])
	path := filepath.Join(s.dirWatch.dir, "change.txt")
	if err := os.WriteFile(path, []byte("changed"), 0o600); err != nil {
		t.Fatal(err)
	}
	completeBufferTestRead(t, s, current)
	// Simulate the completion status supplied by a network redirector, then
	// exercise real I/O with each slot resized only after its old I/O is done.
	if s.processCompletion(windows.ERROR_INVALID_PARAMETER, current.buf, 0) {
		t.Fatal("network buffer fallback terminated watch")
	}
	for _, r := range []*windowsRead{&s.reads[1], &s.reads[0]} {
		armBufferTestRead(t, s, r)
		if len(r.buf) != networkBufSize {
			t.Fatalf("network buffer has %d bytes", len(r.buf))
		}
		if err := os.WriteFile(path, []byte("again"), 0o600); err != nil {
			t.Fatal(err)
		}
		completeBufferTestRead(t, s, r)
	}
}

func TestWindowsReadFatalCleanup(t *testing.T) {
	s := bufferTestSubscription(t)
	armBufferTestRead(t, s, &s.reads[0])
	// A broken startup invariant is fatal even with an already armed read.
	// run must drain that read before releasing the event and closing doneCh.
	go s.run()
	select {
	case <-s.doneCh:
	case <-time.After(5 * time.Second):
		t.Fatal("fatal cleanup did not drain the pending read")
	}
	for i := range s.reads {
		if s.reads[i].pending || s.reads[i].event != 0 || s.reads[i].buf != nil {
			t.Fatalf("fatal cleanup retained request %d", i)
		}
	}
	s.handle = 0
}

func TestWindowsReadStartupFailureCleanup(t *testing.T) {
	s := bufferTestSubscription(t)
	if err := windows.CloseHandle(s.handle); err != nil {
		t.Fatal(err)
	}
	s.handle = windows.InvalidHandle
	if r, err := s.beginRead(&s.reads[0]); r != nil || err == nil {
		t.Fatalf("invalid handle unexpectedly armed: %v, %v", r, err)
	}
	s.close()
	if s.reads[0].event != 0 || s.reads[0].buf != nil || s.reads[0].pending {
		t.Fatal("failed initial read retained resources")
	}
	s.handle = 0
}

func TestWindowsZeroByteCompletionIsOverflow(t *testing.T) {
	s := bufferTestSubscription(t)
	buf := make([]byte, 128)
	fni := (*windows.FileNotifyInformation)(unsafe.Pointer(&buf[0]))
	fni.Action = windows.FILE_ACTION_ADDED
	fni.FileNameLength = 2
	fni.FileName = 'x'
	if s.processCompletion(nil, buf, 0) {
		t.Fatal("overflow terminated watch")
	}
	if !errors.Is(s.dirWatch.events.getError(), ErrOverflow) || s.dirWatch.events.size() != 0 {
		t.Fatal("zero-byte completion parsed stale records instead of reporting overflow")
	}
}

func TestWindowsWatchCloseFromCallback(t *testing.T) {
	w := &watcher{name: "windows-buffer-test", factory: func() watcherImpl { return newWindowsBackend() }}
	root := t.TempDir()
	changed := make(chan error, 1)
	var handle Watch
	var err error
	handle, err = w.WatchDirectory(root, func(events []Event, err error) {
		if err == nil && len(events) == 0 {
			return
		}
		if closeErr := handle.Close(); err == nil {
			err = closeErr
		}
		changed <- err
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = handle.Close() })
	if err := os.WriteFile(filepath.Join(root, "first.txt"), []byte("first"), 0o600); err != nil {
		t.Fatal(err)
	}
	select {
	case err := <-changed:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("initial notification or Close from callback deadlocked")
	}
}

func TestWindowsWatchRepeatedNotifications(t *testing.T) {
	w := &watcher{name: "windows-buffer-repeat-test", factory: func() watcherImpl { return newWindowsBackend() }}
	root := t.TempDir()
	changed := make(chan struct {
		events []Event
		err    error
	}, 128)
	handle, err := w.WatchDirectory(root, func(events []Event, err error) {
		changed <- struct {
			events []Event
			err    error
		}{events, err}
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = handle.Close() })
	for i := range 20 {
		path := filepath.Join(root, fmt.Sprintf("change-%d.txt", i))
		if err := os.WriteFile(path, []byte("changed"), 0o600); err != nil {
			t.Fatal(err)
		}
		timer := time.NewTimer(5 * time.Second)
		found := false
		for !found {
			select {
			case result := <-changed:
				if result.err != nil {
					t.Fatal(result.err)
				}
				for _, event := range result.events {
					if event.Path == path && event.Kind == EventUpdate {
						found = true
					}
				}
			case <-timer.C:
				t.Fatalf("notification %d was lost", i)
			}
		}
		timer.Stop()
	}
}

// BenchmarkWindowsReadNotifications includes ReadDirectoryChangesW submission,
// a real write, completion, and event processing. Two warmup iterations exclude
// the bounded initial buffers from the steady-state allocation measurement.
func BenchmarkWindowsReadNotifications(b *testing.B) {
	s := bufferTestSubscription(b)
	path := filepath.Join(s.dirWatch.dir, "change.txt")
	current := armBufferTestRead(b, s, &s.reads[0])
	spare := &s.reads[1]
	notify := func() {
		if err := os.WriteFile(path, []byte("changed"), 0o600); err != nil {
			b.Fatal(err)
		}
		bytes := completeBufferTestRead(b, s, current)
		next := armBufferTestRead(b, s, spare)
		s.processCompletion(nil, current.buf, bytes)
		s.dirWatch.events.drain()
		spare, current = current, next
	}
	notify()
	notify()
	b.ReportAllocs()
	b.ResetTimer()
	for range b.N {
		notify()
	}
	b.StopTimer()
}
