package compile

import (
	"slices"
	"sync"
)

type solutionTask struct {
	index        int
	predecessors []int
	waitOnly     []int
}

// RunSolutionTasks runs each task once after all of its dependency barriers
// close. A task error is recorded at that task's index and does not affect any
// other task.
func RunSolutionTasks(tasks []solutionTask, builders int, run func(index int) error) []error {
	if builders <= 0 {
		builders = 4
	}

	successors := make(map[int][]int, len(tasks))
	remainingBarriers := make([]int, len(tasks))
	ready := make([]int, 0, len(tasks))
	outcomeLength := 0
	for position, task := range tasks {
		seen := make(map[int]struct{}, len(task.predecessors)+len(task.waitOnly))
		for _, barriers := range [][]int{task.predecessors, task.waitOnly} {
			for _, predecessor := range barriers {
				if _, ok := seen[predecessor]; ok {
					continue
				}
				seen[predecessor] = struct{}{}
				successors[predecessor] = append(successors[predecessor], position)
				remainingBarriers[position]++
			}
		}
		if remainingBarriers[position] == 0 {
			ready = append(ready, position)
		}
		if task.index >= outcomeLength {
			outcomeLength = task.index + 1
		}
	}
	outcomes := make([]error, outcomeLength)
	var mu sync.Mutex
	changed := sync.NewCond(&mu)
	remaining := len(tasks)
	var workers sync.WaitGroup
	workers.Add(min(builders, len(tasks)))
	for range min(builders, len(tasks)) {
		go func() {
			defer workers.Done()
			for {
				mu.Lock()
				for len(ready) == 0 && remaining > 0 {
					changed.Wait()
				}
				if remaining == 0 {
					mu.Unlock()
					return
				}
				position := ready[0]
				ready = ready[1:]
				mu.Unlock()

				task := tasks[position]
				err := run(task.index)

				mu.Lock()
				outcomes[task.index] = err
				for _, successor := range successors[task.index] {
					remainingBarriers[successor]--
					if remainingBarriers[successor] == 0 {
						// Prefer graph order among runnable projects, including
						// projects released by a just-completed predecessor.
						at, _ := slices.BinarySearch(ready, successor)
						ready = slices.Insert(ready, at, successor)
					}
				}
				remaining--
				changed.Broadcast()
				mu.Unlock()
			}
		}()
	}
	workers.Wait()

	return outcomes
}
