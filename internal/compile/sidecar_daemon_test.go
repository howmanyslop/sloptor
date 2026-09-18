package compile

import (
	"fmt"
	"os"
	"os/exec"
	"testing"
	"time"
)

func TestSidecarDaemonKeepsResultsWhileOwnerLives(t *testing.T) {
	worker := &sidecarDaemonWorker{
		leases: map[string]sidecarDaemonLease{
			"result": {
				owner:           fmt.Sprintf("%d-1", os.Getpid()),
				ownerPID:        os.Getpid(),
				ownerGeneration: sidecarProcessGeneration(os.Getpid()),
				retainedAt:      time.Now().Add(-2 * sidecarResultExpiry),
			},
		},
	}
	server := &sidecarDaemonServer{workers: map[string]*sidecarDaemonWorker{"worker": worker}}

	server.expireIdleWorkersLocked(time.Now())

	if len(worker.leases) != 1 {
		t.Fatal("live build lost its retained transformer result")
	}
}

func TestSidecarDaemonRejectsReusedOwnerPID(t *testing.T) {
	worker := &sidecarDaemonWorker{
		leases: map[string]sidecarDaemonLease{
			"result": {
				owner:           fmt.Sprintf("%d-1", os.Getpid()),
				ownerPID:        os.Getpid(),
				ownerGeneration: "an earlier process generation",
				retainedAt:      time.Now(),
			},
		},
	}
	server := &sidecarDaemonServer{workers: map[string]*sidecarDaemonWorker{"worker": worker}}

	server.expireIdleWorkersLocked(time.Now())

	if len(worker.leases) != 0 {
		t.Fatal("result remained owned after its PID was reused")
	}
}

func TestSidecarDaemonReleasesResultsAfterOwnerExits(t *testing.T) {
	executable, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	owner := exec.Command(executable, "-test.run=^$")
	if err := owner.Start(); err != nil {
		t.Fatal(err)
	}
	ownerGeneration := sidecarProcessGeneration(owner.Process.Pid)
	if err := owner.Wait(); err != nil {
		t.Fatal(err)
	}
	worker := &sidecarDaemonWorker{
		leases: map[string]sidecarDaemonLease{
			"result": {
				owner:           fmt.Sprintf("%d-1", owner.Process.Pid),
				ownerPID:        owner.Process.Pid,
				ownerGeneration: ownerGeneration,
				retainedAt:      time.Now(),
			},
		},
	}
	server := &sidecarDaemonServer{workers: map[string]*sidecarDaemonWorker{"worker": worker}}

	server.expireIdleWorkersLocked(time.Now())

	if len(worker.leases) != 0 {
		t.Fatal("abandoned transformer result remained after its build process exited")
	}
}
