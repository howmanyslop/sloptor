//! `solution_scheduler.go`: runs the projects of a solution as their dependencies finish.

use std::sync::{Condvar, Mutex};

use crate::project::{CHECKER_STACK_BYTES, lock};

/// `solutionTask`: one project, by graph index, and the graph indexes it waits for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SolutionTask {
	/// The project's graph index.
	pub index: usize,
	/// Projects whose outputs this one reads.
	pub predecessors: Vec<usize>,
	/// Projects this one only waits for, because they share write roots.
	pub wait_only: Vec<usize>,
}

struct SchedulerState {
	ready: Vec<usize>,
	remaining_barriers: Vec<usize>,
	remaining: usize,
	/// A task panicked: no further task starts.
	aborted: bool,
}

/// `RunSolutionTasks`: runs each task once after all of its barriers close.
///
/// At most `builders` threads run (4 when not positive). A task's error is recorded at its index and affects
/// no other task. Among runnable tasks, graph order goes first.
pub fn run_solution_tasks<E: Send>(
	tasks: &[SolutionTask],
	builders: i64,
	run: impl Fn(usize) -> Result<(), E> + Sync,
) -> Vec<Option<E>> {
	let builders = usize::try_from(builders)
		.ok()
		.filter(|&builders| builders > 0)
		.unwrap_or(4);
	let mut successors: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
	let mut remaining_barriers = vec![0; tasks.len()];
	let mut ready = Vec::with_capacity(tasks.len());
	let mut outcome_length = 0;
	for (position, task) in tasks.iter().enumerate() {
		let mut seen = std::collections::HashSet::new();
		for &predecessor in task.predecessors.iter().chain(&task.wait_only) {
			if !seen.insert(predecessor) {
				continue;
			}
			successors.entry(predecessor).or_default().push(position);
			remaining_barriers[position] += 1;
		}
		if remaining_barriers[position] == 0 {
			ready.push(position);
		}
		outcome_length = outcome_length.max(task.index + 1);
	}
	let outcomes: Vec<Mutex<Option<E>>> = (0..outcome_length).map(|_| Mutex::new(None)).collect();
	let state = Mutex::new(SchedulerState {
		ready,
		remaining_barriers,
		remaining: tasks.len(),
		aborted: false,
	});
	// Go's unrecovered panic ends the process; here the first panic stops the scheduling, wakes
	// every worker, and is re-raised once the workers are joined.
	let panicked: Mutex<Option<Box<dyn std::any::Any + Send>>> = Mutex::new(None);
	let changed = Condvar::new();
	let workers = builders.min(tasks.len());
	let worker = || {
		loop {
			let position = {
				let mut guard = lock(&state);
				while guard.ready.is_empty() && guard.remaining > 0 && !guard.aborted {
					guard = changed.wait(guard).unwrap_or_else(std::sync::PoisonError::into_inner);
				}
				if guard.remaining == 0 || guard.aborted {
					return;
				}
				guard.ready.remove(0)
			};
			let task = &tasks[position];
			let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(task.index))) {
				Ok(result) => result,
				Err(payload) => {
					lock(&panicked).get_or_insert(payload);
					lock(&state).aborted = true;
					changed.notify_all();
					return;
				}
			};
			let mut guard = lock(&state);
			*lock(&outcomes[task.index]) = result.err();
			if let Some(next) = successors.get(&task.index) {
				for &successor in next {
					guard.remaining_barriers[successor] -= 1;
					if guard.remaining_barriers[successor] == 0 {
						// Prefer graph order among runnable projects, including projects a
						// just-completed predecessor released.
						let at = guard.ready.binary_search(&successor).unwrap_or_else(|at| at);
						guard.ready.insert(at, successor);
					}
				}
			}
			guard.remaining -= 1;
			drop(guard);
			changed.notify_all();
		}
	};
	std::thread::scope(|scope| {
		let handles: Vec<_> = (0..workers)
			.filter_map(|_| {
				std::thread::Builder::new()
					.stack_size(CHECKER_STACK_BYTES)
					.spawn_scoped(scope, worker)
					.ok()
			})
			.collect();
		if handles.is_empty() && workers > 0 {
			worker();
		}
		for handle in handles {
			if let Err(payload) = handle.join() {
				lock(&panicked).get_or_insert(payload);
			}
		}
	});
	if let Some(payload) = panicked.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner) {
		std::panic::resume_unwind(payload);
	}
	outcomes
		.into_iter()
		.map(|outcome| outcome.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner))
		.collect()
}
