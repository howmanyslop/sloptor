//! `solution_scheduler_test.go`.

use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel, sync_channel};
use std::time::Duration;

use crate::solution_scheduler::{SolutionTask, run_solution_tasks};

/// `indexedSolutionTasks`
fn indexed_solution_tasks(count: usize) -> Vec<SolutionTask> {
	(0..count)
		.map(|index| SolutionTask {
			index,
			..SolutionTask::default()
		})
		.collect()
}

/// `assertTaskIndexes`
fn assert_task_indexes(signal: &Receiver<usize>, count: usize) {
	let mut seen = vec![false; count];
	for _ in 0..count {
		let index = signal.recv().expect("a task signal");
		assert!(index < seen.len() && !seen[index], "unexpected task index {index}");
		seen[index] = true;
	}
}

/// `assertNoTaskSignal`
fn assert_no_task_signal(signal: &Receiver<usize>) {
	if let Ok(index) = signal.try_recv() {
		panic!("unexpected task signal for index {index}");
	}
}

/// `assertNoSignal`
fn assert_no_signal(signal: &Receiver<()>) {
	assert!(signal.try_recv().is_err(), "unexpected signal");
}

/// `assertNoErrors`
fn assert_no_errors(outcomes: &[Option<String>]) {
	for (index, error) in outcomes.iter().enumerate() {
		assert!(error.is_none(), "task {index} error = {error:?}, want nil");
	}
}

/// Go's `<-release` on an unbuffered channel shared by every task.
fn wait(release: &Mutex<Receiver<()>>) {
	let _ = crate::project::lock(release).recv();
}

fn run_in_background(
	tasks: Vec<SolutionTask>,
	builders: i64,
	run: impl Fn(usize) -> Result<(), String> + Send + Sync + 'static,
) -> Receiver<Vec<Option<String>>> {
	let (done, outcomes) = channel();
	std::thread::spawn(move || {
		let _ = done.send(run_solution_tasks(&tasks, builders, run));
	});
	outcomes
}

#[test]
fn solution_scheduler_runs_ready_task_past_blocked_dependants() {
	for wait_only in [false, true] {
		let mut tasks = indexed_solution_tasks(5);
		for task in &mut tasks[1..=3] {
			if wait_only {
				task.wait_only = vec![0];
			} else {
				task.predecessors = vec![0];
			}
		}
		let (started, started_rx) = channel::<usize>();
		let (release, release_rx) = channel::<()>();
		let release_rx = Mutex::new(release_rx);
		let done = run_in_background(tasks, 4, move |index| {
			let _ = started.send(index);
			if index == 0 {
				wait(&release_rx);
			}
			Ok(())
		});
		// Neither runnable root may depend on releasing the other. The timeout guards deadlock.
		let mut seen = std::collections::HashSet::new();
		while seen.len() < 2 {
			let index = started_rx
				.recv_timeout(Duration::from_secs(2))
				.unwrap_or_else(|_| panic!("runnable roots did not both start while root 0 was held: {seen:?}"));
			assert!(
				(index == 0 || index == 4) && !seen.contains(&index),
				"task {index} started before root 0 completed"
			);
			seen.insert(index);
		}
		drop(release);
		assert_no_errors(&done.recv().expect("the outcomes"));
	}
}

#[test]
fn solution_scheduler_deduplicates_barriers_and_prioritizes_graph_order() {
	let tasks = vec![
		SolutionTask {
			index: 0,
			..SolutionTask::default()
		},
		SolutionTask {
			index: 1,
			predecessors: vec![0, 0],
			wait_only: vec![0, 0],
		},
		SolutionTask {
			index: 2,
			..SolutionTask::default()
		},
	];
	let order = Mutex::new(Vec::new());
	let outcomes = run_solution_tasks(&tasks, 1, |index| -> Result<(), String> {
		crate::project::lock(&order).push(index);
		Ok(())
	});
	assert_no_errors(&outcomes);
	let order = order.into_inner().expect("the order");
	assert_eq!(order, vec![0, 1, 2], "callback order = {order:?}, want [0 1 2]");
}

#[test]
fn solution_scheduler_preserves_sparse_outcome_indexes() {
	let tasks = vec![
		SolutionTask {
			index: 2,
			..SolutionTask::default()
		},
		SolutionTask {
			index: 7,
			predecessors: vec![2],
			..SolutionTask::default()
		},
		SolutionTask {
			index: 9,
			wait_only: vec![2],
			..SolutionTask::default()
		},
	];
	let (started, started_rx) = channel();
	let outcomes = run_solution_tasks(&tasks, 3, |index| {
		let _ = started.send(index);
		if index == 2 {
			return Err("dependency failed".to_owned());
		}
		Ok(())
	});
	assert!(
		outcomes.len() == 10 && outcomes[2].as_deref() == Some("dependency failed"),
		"sparse outcomes = {outcomes:?}, want failure at index 2 of 10"
	);
	let mut seen = std::collections::HashSet::new();
	for _ in &tasks {
		let index = started_rx.recv().expect("a start");
		assert!(seen.insert(index), "callback {index} ran twice");
	}
	for index in [7, 9] {
		assert!(
			seen.contains(&index) && outcomes[index].is_none(),
			"callback {index} did not complete successfully: seen={seen:?}, outcomes={outcomes:?}"
		);
	}
}

#[test]
fn solution_scheduler_serializes_builders_one() {
	let tasks = indexed_solution_tasks(3);
	let count = tasks.len();
	let (started, started_rx) = channel();
	let (finished, finished_rx) = channel();
	let (release, release_rx) = sync_channel::<()>(0);
	let release_rx = Mutex::new(release_rx);
	let done = run_in_background(tasks, 1, move |index| {
		let _ = started.send(index);
		wait(&release_rx);
		let _ = finished.send(index);
		Ok(())
	});
	for expected in 0..count {
		let got = started_rx.recv().expect("a start");
		assert_eq!(got, expected, "started task {got}, want {expected}");
		assert_no_task_signal(&started_rx);
		release.send(()).expect("the release");
		let got = finished_rx.recv().expect("a finish");
		assert_eq!(got, expected, "finished task {got}, want {expected}");
	}
	assert_no_errors(&done.recv().expect("the outcomes"));
}

#[test]
fn solution_scheduler_runs_independent_tasks_concurrently() {
	let tasks = indexed_solution_tasks(2);
	let (started, started_rx) = channel();
	let (release, release_rx) = channel::<()>();
	let release_rx = Mutex::new(release_rx);
	let done = run_in_background(tasks, 2, move |index| {
		let _ = started.send(index);
		wait(&release_rx);
		Ok(())
	});
	assert_task_indexes(&started_rx, 2);
	drop(release);
	assert_no_errors(&done.recv().expect("the outcomes"));
}

#[test]
fn solution_scheduler_waits_for_all_predecessors() {
	let tasks = vec![
		SolutionTask {
			index: 0,
			..SolutionTask::default()
		},
		SolutionTask {
			index: 1,
			..SolutionTask::default()
		},
		SolutionTask {
			index: 2,
			predecessors: vec![0, 1],
			..SolutionTask::default()
		},
	];
	let (started, started_rx) = channel();
	let (finished, finished_rx) = channel();
	let (first_release, first_gate) = sync_channel::<()>(0);
	let (second_release, second_gate) = sync_channel::<()>(0);
	let releases = [Mutex::new(first_gate), Mutex::new(second_gate)];
	let (dependent_started, dependent_started_rx) = channel::<()>();
	let done = run_in_background(tasks, 3, move |index| {
		if index < 2 {
			let _ = started.send(index);
			wait(&releases[index]);
			let _ = finished.send(index);
		} else {
			let _ = dependent_started.send(());
		}
		Ok(())
	});
	assert_task_indexes(&started_rx, 2);
	assert_no_signal(&dependent_started_rx);
	first_release.send(()).expect("the release");
	let got: usize = finished_rx.recv().expect("a finish");
	assert_eq!(got, 0, "finished predecessor {got}, want 0");
	assert_no_signal(&dependent_started_rx);
	second_release.send(()).expect("the release");
	let got = finished_rx.recv().expect("a finish");
	assert_eq!(got, 1, "finished predecessor {got}, want 1");
	dependent_started_rx.recv().expect("the dependant starts");
	assert_no_errors(&done.recv().expect("the outcomes"));
}

#[test]
fn solution_scheduler_wait_only_does_not_propagate_failure() {
	let tasks = vec![
		SolutionTask {
			index: 0,
			..SolutionTask::default()
		},
		SolutionTask {
			index: 1,
			wait_only: vec![0],
			..SolutionTask::default()
		},
	];
	let (started, started_rx) = channel();
	let (finished, finished_rx) = channel();
	let (dependent_started, dependent_rx) = channel::<()>();
	let (release, release_rx) = sync_channel::<()>(0);
	let release_rx = Mutex::new(release_rx);
	let done = run_in_background(tasks, 2, move |index| {
		if index == 0 {
			let _ = started.send(index);
			wait(&release_rx);
			let _ = finished.send(index);
			return Err("wait-only task failed".to_owned());
		}
		let _ = dependent_started.send(());
		Ok(())
	});
	let got: usize = started_rx.recv().expect("a start");
	assert_eq!(got, 0, "started task {got}, want 0");
	assert_no_signal(&dependent_rx);
	release.send(()).expect("the release");
	let got: usize = finished_rx.recv().expect("a finish");
	assert_eq!(got, 0, "finished task {got}, want 0");
	dependent_rx.recv().expect("the dependant starts");
	let outcomes = done.recv().expect("the outcomes");
	assert_eq!(
		outcomes[0].as_deref(),
		Some("wait-only task failed"),
		"task 0 error = {:?}, want wait-only task failed",
		outcomes[0]
	);
	assert!(outcomes[1].is_none(), "task 1 error = {:?}, want nil", outcomes[1]);
}

#[test]
fn solution_scheduler_runs_unrelated_tasks_after_failure() {
	let tasks = indexed_solution_tasks(2);
	let (failed, failed_rx) = channel::<()>();
	let (unrelated, unrelated_rx) = channel::<()>();
	let done = run_in_background(tasks, 2, move |index| {
		if index == 0 {
			let _ = failed.send(());
			return Err("task failed".to_owned());
		}
		let _ = unrelated.send(());
		Ok(())
	});
	failed_rx.recv().expect("the failure");
	unrelated_rx.recv().expect("the unrelated start");
	let outcomes = done.recv().expect("the outcomes");
	assert_eq!(
		outcomes[0].as_deref(),
		Some("task failed"),
		"task 0 error = {:?}",
		outcomes[0]
	);
	assert!(outcomes[1].is_none(), "task 1 error = {:?}, want nil", outcomes[1]);
}

#[test]
fn solution_scheduler_joins_blocked_tasks_before_returning() {
	let tasks = vec![SolutionTask::default()];
	let (started, started_rx) = channel::<()>();
	let (finished, finished_rx) = channel::<()>();
	let (release, release_rx) = channel::<()>();
	let release_rx = Mutex::new(release_rx);
	let done = run_in_background(tasks, 1, move |_| {
		let _ = started.send(());
		wait(&release_rx);
		let _ = finished.send(());
		Ok(())
	});
	started_rx.recv().expect("the start");
	assert!(
		done.try_recv().is_err(),
		"RunSolutionTasks returned while a task was blocked"
	);
	drop(release);
	finished_rx.recv().expect("the finish");
	assert_no_errors(&done.recv().expect("the outcomes"));
}

#[test]
fn solution_scheduler_defaults_to_four_builders() {
	for builders in [0, -2] {
		let tasks = indexed_solution_tasks(4);
		let (started, started_rx) = channel();
		let (release, release_rx) = channel::<()>();
		let release_rx = Mutex::new(release_rx);
		let done = run_in_background(tasks, builders, move |index| {
			let _ = started.send(index);
			wait(&release_rx);
			Ok(())
		});
		assert_task_indexes(&started_rx, 4);
		drop(release);
		assert_no_errors(&done.recv().expect("the outcomes"));
	}
}

#[test]
fn solution_scheduler_closes_every_done_barrier() {
	let tasks = vec![
		SolutionTask {
			index: 0,
			..SolutionTask::default()
		},
		SolutionTask {
			index: 1,
			..SolutionTask::default()
		},
		SolutionTask {
			index: 2,
			predecessors: vec![0],
			wait_only: vec![1],
		},
	];
	let (started, started_rx): (Sender<usize>, Receiver<usize>) = channel();
	let outcomes = run_solution_tasks(&tasks, 3, |index| -> Result<(), String> {
		let _ = started.send(index);
		Ok(())
	});
	assert_task_indexes(&started_rx, 3);
	assert_no_errors(&outcomes);
}

/// Catches a panicking project leaving the other workers waiting forever: the panic must reach
/// the caller, as Go's unrecovered panic ends the process.
#[test]
fn solution_tasks_panic_reaches_the_caller() {
	let tasks = vec![
		SolutionTask::default(),
		SolutionTask {
			index: 1,
			..SolutionTask::default()
		},
		SolutionTask {
			index: 2,
			predecessors: vec![0],
			..SolutionTask::default()
		},
	];
	let (done, done_rx) = channel();
	std::thread::spawn(move || {
		let outcome = std::panic::catch_unwind(|| {
			run_solution_tasks(&tasks, 2, |index| -> Result<(), String> {
				assert!(index != 0, "project 0 panicked");
				Ok(())
			})
		});
		let _ = done.send(outcome.is_err());
	});
	let panicked = done_rx
		.recv_timeout(std::time::Duration::from_secs(10))
		.expect("the scheduler hung after a panicking project");
	assert!(panicked, "the panic did not reach the caller");
}
