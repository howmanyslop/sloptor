package compile

import (
	"sync"
	"testing"
	"testing/synctest"
)

func TestSidecarSlotsAllowConcurrentKeys(t *testing.T) {
	closeSidecarSessions()
	t.Cleanup(closeSidecarSessions)
	synctest.Test(t, func(t *testing.T) {
		first := sidecarSlotFor("dir-a|cfg-a")
		second := sidecarSlotFor("dir-b|cfg-b")
		started := make(chan string, 2)
		release := make(chan struct{})
		var wg sync.WaitGroup
		wg.Add(2)
		lock := func(name string, slot *sidecarSlot) {
			defer wg.Done()
			slot.mu.Lock()
			started <- name
			<-release
			slot.mu.Unlock()
		}
		go lock("a", first)
		go lock("b", second)
		synctest.Wait()
		if len(started) != 2 {
			t.Fatalf("concurrent keys started = %d, want 2", len(started))
		}
		close(release)
		wg.Wait()
	})
}

func TestSidecarSlotsSerializeTheSameKey(t *testing.T) {
	closeSidecarSessions()
	t.Cleanup(closeSidecarSessions)
	slot := sidecarSlotFor("dir-a|cfg-a")
	started := make(chan struct{}, 2)
	release := make(chan struct{})
	var wg sync.WaitGroup
	wg.Add(2)
	lock := func() {
		defer wg.Done()
		slot.mu.Lock()
		started <- struct{}{}
		<-release
		slot.mu.Unlock()
	}
	go lock()
	<-started
	go lock()
	select {
	case <-started:
		t.Fatal("same key overlapped")
	default:
	}
	close(release)
	wg.Wait()
}

func TestCloseSidecarSessionsWaitsForActiveSlot(t *testing.T) {
	closeSidecarSessions()
	t.Cleanup(closeSidecarSessions)
	slot := sidecarSlotFor("dir-a|cfg-a")
	held := make(chan struct{})
	release := make(chan struct{})
	closed := make(chan struct{})
	go func() {
		slot.mu.Lock()
		close(held)
		<-release
		slot.mu.Unlock()
	}()
	<-held
	go func() {
		closeSidecarSessions()
		close(closed)
	}()
	select {
	case <-closed:
		t.Fatal("closeSidecarSessions returned while the slot was held")
	default:
	}
	close(release)
	<-closed
	if sidecarSlotCount() != 0 {
		t.Fatalf("slots after close = %d, want 0", sidecarSlotCount())
	}
}
