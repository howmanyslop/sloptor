package admission

import (
	"bufio"
	"context"
	"errors"
	"os"
	"os/exec"
	"testing"
	"time"
)

const holdDirEnv = "ROTOR_ADMISSION_TEST_HOLD_DIR"

func acquireWithin(t *testing.T, dir string, slots int, wait time.Duration) (*Lease, error) {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), wait)
	defer cancel()
	return Acquire(ctx, dir, slots, nil)
}

// Catches a build starting while every slot is taken, and a waiting build
// never starting after one finishes.
func TestAcquireWaitsForAFreeSlot(t *testing.T) {
	dir := t.TempDir()
	first, err := acquireWithin(t, dir, 2, time.Second)
	if err != nil {
		t.Fatal(err)
	}
	second, err := acquireWithin(t, dir, 2, time.Second)
	if err != nil {
		t.Fatal(err)
	}
	defer second.Release()
	if _, err := acquireWithin(t, dir, 2, 300*time.Millisecond); !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("third build admitted with both slots held: err = %v", err)
	}

	waiting := make(chan error, 1)
	go func() {
		lease, err := acquireWithin(t, dir, 2, 5*time.Second)
		if err == nil {
			lease.Release()
		}
		waiting <- err
	}()
	first.Release()
	if err := <-waiting; err != nil {
		t.Fatalf("waiting build not admitted after a slot freed: %v", err)
	}
}

// Catches a build sizing itself as if alone while others run.
func TestActiveCountsConcurrentBuilds(t *testing.T) {
	dir := t.TempDir()
	var leases []*Lease
	for range 3 {
		lease, err := acquireWithin(t, dir, 4, time.Second)
		if err != nil {
			t.Fatal(err)
		}
		leases = append(leases, lease)
	}
	if got := leases[0].Active(); got != 3 {
		t.Fatalf("Active() = %d with 3 builds running, want 3", got)
	}
	leases[1].Release()
	leases[2].Release()
	if got := leases[0].Active(); got != 1 {
		t.Fatalf("Active() = %d after the others finished, want 1", got)
	}
	leases[0].Release()
}

// Catches a killed build stranding its slot so later builds wait forever.
func TestKilledHolderFreesItsSlot(t *testing.T) {
	if dir := os.Getenv(holdDirEnv); dir != "" {
		lease, err := Acquire(context.Background(), dir, 1, nil)
		if err != nil {
			os.Exit(2)
		}
		if _, err := os.Stdout.WriteString("held\n"); err != nil {
			os.Exit(2)
		}
		time.Sleep(time.Minute)
		lease.Release()
		os.Exit(0)
	}
	dir := t.TempDir()
	holder := exec.Command(os.Args[0], "-test.run=^TestKilledHolderFreesItsSlot$")
	holder.Env = append(os.Environ(), holdDirEnv+"="+dir)
	stdout, err := holder.StdoutPipe()
	if err != nil {
		t.Fatal(err)
	}
	if err := holder.Start(); err != nil {
		t.Fatal(err)
	}
	if line, err := bufio.NewReader(stdout).ReadString('\n'); err != nil || line != "held\n" {
		_ = holder.Process.Kill()
		t.Fatalf("holder did not take the slot: %q, %v", line, err)
	}
	if _, err := acquireWithin(t, dir, 1, 300*time.Millisecond); err == nil {
		t.Fatal("slot admitted while another process holds it")
	}
	if err := holder.Process.Kill(); err != nil {
		t.Fatal(err)
	}
	// Wait reports the kill as an error; only the exit matters here.
	_ = holder.Wait()
	lease, err := acquireWithin(t, dir, 1, 5*time.Second)
	if err != nil {
		t.Fatalf("slot still held after its process was killed: %v", err)
	}
	lease.Release()
}

// Catches admission being impossible to turn off, and a small machine
// getting zero slots (every build would wait forever).
func TestSlots(t *testing.T) {
	t.Setenv(MaxBuildsEnv, "0")
	if got := Slots(64<<30, 16); got != 0 {
		t.Fatalf("%s=0: Slots = %d, want 0 (disabled)", MaxBuildsEnv, got)
	}
	t.Setenv(MaxBuildsEnv, "7")
	if got := Slots(8<<30, 2); got != 7 {
		t.Fatalf("%s=7: Slots = %d, want 7", MaxBuildsEnv, got)
	}
	if err := os.Unsetenv(MaxBuildsEnv); err != nil {
		t.Fatal(err)
	}
	if got := Slots(24<<30, 14); got != 4 {
		t.Fatalf("24 GiB, 14 CPUs: Slots = %d, want 4", got)
	}
	if got := Slots(2<<30, 1); got != 1 {
		t.Fatalf("2 GiB, 1 CPU: Slots = %d, want 1", got)
	}
}
