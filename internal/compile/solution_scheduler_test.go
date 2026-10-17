package compile

import (
	"errors"
	"reflect"
	"testing"
	"time"
)

func TestSolutionSchedulerRunsReadyTaskPastBlockedDependants(t *testing.T) {
	for _, waitOnly := range []bool{false, true} {
		name := "predecessors"
		if waitOnly {
			name = "waitOnly"
		}
		t.Run(name, func(t *testing.T) {
			tasks := indexedSolutionTasks(5)
			for index := 1; index <= 3; index++ {
				if waitOnly {
					tasks[index].waitOnly = []int{0}
				} else {
					tasks[index].predecessors = []int{0}
				}
			}
			started := make(chan int, len(tasks))
			release := make(chan struct{})
			done := make(chan []error, 1)
			go func() {
				done <- RunSolutionTasks(tasks, 4, func(index int) error {
					started <- index
					if index == 0 {
						<-release
					}
					return nil
				})
			}()
			defer func() {
				close(release)
				assertNoErrors(t, <-done)
			}()
			// Neither runnable root may depend on releasing the other. The
			// timeout guards deadlock; no assertion depends on their duration.
			seen := map[int]bool{}
			timer := time.NewTimer(2 * time.Second)
			defer timer.Stop()
			for len(seen) < 2 {
				select {
				case index := <-started:
					if index != 0 && index != 4 || seen[index] {
						t.Fatalf("task %d started before root 0 completed", index)
					}
					seen[index] = true
				case <-timer.C:
					t.Fatalf("runnable roots did not both start while root 0 was held: %v", seen)
				}
			}
		})
	}
}

func TestSolutionSchedulerDeduplicatesBarriersAndPrioritizesGraphOrder(t *testing.T) {
	tasks := []solutionTask{
		{index: 0},
		{index: 1, predecessors: []int{0, 0}, waitOnly: []int{0, 0}},
		{index: 2},
	}
	var order []int
	outcomes := RunSolutionTasks(tasks, 1, func(index int) error {
		order = append(order, index)
		return nil
	})
	assertNoErrors(t, outcomes)
	if !reflect.DeepEqual(order, []int{0, 1, 2}) {
		t.Fatalf("callback order = %v, want [0 1 2]", order)
	}
}

func TestSolutionSchedulerPreservesSparseOutcomeIndexes(t *testing.T) {
	failure := errors.New("dependency failed")
	tasks := []solutionTask{{index: 2}, {index: 7, predecessors: []int{2}}, {index: 9, waitOnly: []int{2}}}
	started := make(chan int, len(tasks))
	outcomes := RunSolutionTasks(tasks, 3, func(index int) error {
		started <- index
		if index == 2 {
			return failure
		}
		return nil
	})
	if len(outcomes) != 10 || !errors.Is(outcomes[2], failure) {
		t.Fatalf("sparse outcomes = %v, want failure at index 2 of 10", outcomes)
	}
	seen := map[int]bool{}
	for range tasks {
		index := <-started
		if seen[index] {
			t.Fatalf("callback %d ran twice", index)
		}
		seen[index] = true
	}
	for _, index := range []int{7, 9} {
		if !seen[index] || outcomes[index] != nil {
			t.Fatalf("callback %d did not complete successfully: seen=%v, outcomes=%v", index, seen, outcomes)
		}
	}
}

func TestSolutionSchedulerSerializesBuildersOne(t *testing.T) {
	tasks := indexedSolutionTasks(3)
	started := make(chan int, len(tasks))
	finished := make(chan int, len(tasks))
	release := make(chan struct{})
	runDone := make(chan []error, 1)

	go func() {
		runDone <- RunSolutionTasks(tasks, 1, func(index int) error {
			started <- index
			<-release
			finished <- index
			return nil
		})
	}()

	for expected := range len(tasks) {
		if got := <-started; got != expected {
			t.Fatalf("started task %d, want %d", got, expected)
		}
		assertNoTaskSignal(t, started)
		release <- struct{}{}
		if got := <-finished; got != expected {
			t.Fatalf("finished task %d, want %d", got, expected)
		}
	}

	assertNoErrors(t, <-runDone)
}

func TestSolutionSchedulerRunsIndependentTasksConcurrently(t *testing.T) {
	tasks := indexedSolutionTasks(2)
	started := make(chan int, len(tasks))
	release := make(chan struct{})
	runDone := make(chan []error, 1)

	go func() {
		runDone <- RunSolutionTasks(tasks, 2, func(index int) error {
			started <- index
			<-release
			return nil
		})
	}()

	assertTaskIndexes(t, started, len(tasks))
	close(release)
	assertNoErrors(t, <-runDone)
}

func TestSolutionSchedulerWaitsForAllPredecessors(t *testing.T) {
	tasks := []solutionTask{
		{index: 0},
		{index: 1},
		{index: 2, predecessors: []int{0, 1}},
	}
	started := make(chan int, len(tasks))
	finished := make(chan int, len(tasks))
	release := [2]chan struct{}{make(chan struct{}), make(chan struct{})}
	dependentStarted := make(chan struct{}, 1)
	runDone := make(chan []error, 1)

	go func() {
		runDone <- RunSolutionTasks(tasks, 3, func(index int) error {
			switch index {
			case 0, 1:
				started <- index
				<-release[index]
				finished <- index
			default:
				dependentStarted <- struct{}{}
			}
			return nil
		})
	}()

	assertTaskIndexes(t, started, 2)
	assertNoSignal(t, dependentStarted)

	release[0] <- struct{}{}
	if got := <-finished; got != 0 {
		t.Fatalf("finished predecessor %d, want 0", got)
	}
	assertNoSignal(t, dependentStarted)

	release[1] <- struct{}{}
	if got := <-finished; got != 1 {
		t.Fatalf("finished predecessor %d, want 1", got)
	}
	<-dependentStarted

	assertNoErrors(t, <-runDone)
}

func TestSolutionSchedulerWaitOnlyDoesNotPropagateFailure(t *testing.T) {
	failure := errors.New("wait-only task failed")
	tasks := []solutionTask{{index: 0}, {index: 1, waitOnly: []int{0}}}
	started := make(chan int, len(tasks))
	finished := make(chan int, len(tasks))
	dependentStarted := make(chan struct{}, 1)
	release := make(chan struct{})
	runDone := make(chan []error, 1)

	go func() {
		runDone <- RunSolutionTasks(tasks, 2, func(index int) error {
			if index == 0 {
				started <- index
				<-release
				finished <- index
				return failure
			}
			dependentStarted <- struct{}{}
			return nil
		})
	}()

	if got := <-started; got != 0 {
		t.Fatalf("started task %d, want 0", got)
	}
	assertNoSignal(t, dependentStarted)
	release <- struct{}{}
	if got := <-finished; got != 0 {
		t.Fatalf("finished task %d, want 0", got)
	}
	<-dependentStarted

	outcomes := <-runDone
	if !errors.Is(outcomes[0], failure) {
		t.Fatalf("task 0 error = %v, want %v", outcomes[0], failure)
	}
	if outcomes[1] != nil {
		t.Fatalf("task 1 error = %v, want nil", outcomes[1])
	}
}

func TestSolutionSchedulerRunsUnrelatedTasksAfterFailure(t *testing.T) {
	failure := errors.New("task failed")
	tasks := indexedSolutionTasks(2)
	failed := make(chan struct{}, 1)
	unrelatedStarted := make(chan struct{}, 1)
	runDone := make(chan []error, 1)

	go func() {
		runDone <- RunSolutionTasks(tasks, 2, func(index int) error {
			if index == 0 {
				failed <- struct{}{}
				return failure
			}
			unrelatedStarted <- struct{}{}
			return nil
		})
	}()

	<-failed
	<-unrelatedStarted
	outcomes := <-runDone
	if !errors.Is(outcomes[0], failure) {
		t.Fatalf("task 0 error = %v, want %v", outcomes[0], failure)
	}
	if outcomes[1] != nil {
		t.Fatalf("task 1 error = %v, want nil", outcomes[1])
	}
}

func TestSolutionSchedulerJoinsBlockedTasksBeforeReturning(t *testing.T) {
	tasks := []solutionTask{{index: 0}}
	started := make(chan struct{}, 1)
	finished := make(chan struct{}, 1)
	release := make(chan struct{})
	runDone := make(chan []error, 1)

	go func() {
		runDone <- RunSolutionTasks(tasks, 1, func(index int) error {
			started <- struct{}{}
			<-release
			finished <- struct{}{}
			return nil
		})
	}()

	<-started
	select {
	case <-runDone:
		t.Fatal("RunSolutionTasks returned while a task was blocked")
	default:
	}

	close(release)
	<-finished
	assertNoErrors(t, <-runDone)
}

func TestSolutionSchedulerDefaultsToFourBuilders(t *testing.T) {
	for _, test := range []struct {
		name     string
		builders int
	}{
		{name: "zero", builders: 0},
		{name: "negative", builders: -2},
	} {
		t.Run(test.name, func(t *testing.T) {
			tasks := indexedSolutionTasks(4)
			started := make(chan int, len(tasks))
			release := make(chan struct{})
			runDone := make(chan []error, 1)

			go func() {
				runDone <- RunSolutionTasks(tasks, test.builders, func(index int) error {
					started <- index
					<-release
					return nil
				})
			}()

			assertTaskIndexes(t, started, len(tasks))
			close(release)
			assertNoErrors(t, <-runDone)
		})
	}
}

func TestSolutionSchedulerClosesEveryDoneBarrier(t *testing.T) {
	tasks := []solutionTask{
		{index: 0},
		{index: 1},
		{index: 2, predecessors: []int{0}, waitOnly: []int{1}},
	}
	started := make(chan int, len(tasks))

	outcomes := RunSolutionTasks(tasks, 3, func(index int) error {
		started <- index
		return nil
	})

	assertTaskIndexes(t, started, len(tasks))
	assertNoErrors(t, outcomes)
}

func assertNoTaskSignal(t *testing.T, signal <-chan int) {
	select {
	case index := <-signal:
		t.Fatalf("unexpected task signal for index %d", index)
	default:
	}
}

func indexedSolutionTasks(count int) []solutionTask {
	tasks := make([]solutionTask, count)
	for index := range tasks {
		tasks[index].index = index
	}
	return tasks
}

func assertTaskIndexes(t *testing.T, signal <-chan int, count int) {
	seen := make([]bool, count)
	for range count {
		index := <-signal
		if index < 0 || index >= len(seen) || seen[index] {
			t.Fatalf("unexpected task index %d", index)
		}
		seen[index] = true
	}
}

func assertNoSignal(t *testing.T, signal <-chan struct{}) {
	select {
	case <-signal:
		t.Fatal("unexpected signal")
	default:
	}
}

func assertNoErrors(t *testing.T, outcomes []error) {
	for index, err := range outcomes {
		if err != nil {
			t.Errorf("task %d error = %v, want nil", index, err)
		}
	}
}
