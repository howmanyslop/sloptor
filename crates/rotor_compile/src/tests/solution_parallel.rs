//! `solution_parallel_test.go`. Go's `synctest` bubble becomes real threads: a project that must
//! not start yet gets a short grace period to prove it does not.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rotor_support::gopath;

use super::Temp;
use super::solution::{builders, coordinator_with, join, write_solution_config};
use super::solution_metadata::write_metadata_solution_project;
use crate::compile::{DiagnosticInfo, Error};
use crate::output::BuildResult;
use crate::project::{PendingPersist, lock};
use crate::solution::{DrainResult, SolutionProject, SolutionProjectDrainer};

const GRACE: Duration = Duration::from_millis(50);
const PATIENCE: Duration = Duration::from_secs(10);

/// `parallelSolutionDrainer`; persistence goes through the coordinator's queue.
#[derive(Default)]
struct ParallelDrainer {
	started: Mutex<Option<Sender<String>>>,
	release: Mutex<Option<Arc<Mutex<Receiver<()>>>>>,
	failures: Mutex<HashMap<String, String>>,
	persist_failure: Arc<Mutex<HashSet<String>>>,
	persisted: Arc<Mutex<Vec<String>>>,
	drained: Mutex<Vec<String>>,
}

impl ParallelDrainer {
	fn gated() -> (Arc<Self>, Receiver<String>, Sender<()>) {
		let drainer = Arc::new(Self::default());
		let (started, release) = drainer.gate();
		(drainer, started, release)
	}

	/// Installs the start channel and the release gate; dropping the sender closes it.
	fn gate(&self) -> (Receiver<String>, Sender<()>) {
		let (started, started_rx) = channel();
		let (release, release_rx) = channel();
		*lock(&self.started) = Some(started);
		*lock(&self.release) = Some(Arc::new(Mutex::new(release_rx)));
		(started_rx, release)
	}

	fn failing(failures: &[&str]) -> Arc<Self> {
		let drainer = Self::default();
		*lock(&drainer.failures) = failures
			.iter()
			.map(|name| ((*name).to_owned(), (*name).to_owned()))
			.collect();
		Arc::new(drainer)
	}

	fn persist_order(&self) -> Vec<String> {
		lock(&self.persisted).clone()
	}

	fn drain_order(&self) -> Vec<String> {
		lock(&self.drained).clone()
	}
}

impl SolutionProjectDrainer for ParallelDrainer {
	fn drain(&self, project: SolutionProject) -> DrainResult {
		let name = gopath::base(&gopath::dir(&project.config_path));
		if let Some(started) = &*lock(&self.started) {
			let _ = started.send(name.clone());
		}
		let release = lock(&self.release).clone();
		if let Some(release) = release {
			let _ = lock(&release).recv();
		}
		lock(&self.drained).push(name.clone());
		let diagnostic = DiagnosticInfo {
			message: format!("diagnostic:{name}"),
			..DiagnosticInfo::default()
		};
		if let Some(failure) = lock(&self.failures).get(&name) {
			return DrainResult {
				result: Some(BuildResult {
					diagnostics: vec![diagnostic],
					..BuildResult::default()
				}),
				messages: vec![format!("message:{name}")],
				error: Some(Error::message(failure.clone())),
			};
		}
		if let Some(pending) = &project.options.pending_solution_persists {
			let persisted = Arc::clone(&self.persisted);
			let persist_failure = Arc::clone(&self.persist_failure);
			let name = name.clone();
			let persist: PendingPersist = Box::new(move || {
				lock(&persisted).push(name.clone());
				if lock(&persist_failure).contains(&name) {
					return Err(Error::message(format!("persist {name}")));
				}
				Ok(())
			});
			lock(pending).push(persist);
		}
		DrainResult {
			result: Some(BuildResult {
				outputs: [(name.clone(), format!("output:{name}").into_bytes())].into(),
				emitted_files: vec![format!("emitted:{name}")],
				diagnostics: vec![diagnostic],
				..BuildResult::default()
			}),
			messages: vec![format!("message:{name}")],
			error: None,
		}
	}

	fn queues_persistence(&self) -> bool {
		true
	}
}

fn write_two_metadata_projects(root: &str, first_out: &str, second_out: &str) {
	write_solution_config(root, "tsconfig.json", &["./first", "./second"], true);
	write_metadata_solution_project(&join(root, &["first"]), first_out);
	write_metadata_solution_project(&join(root, &["second"]), second_out);
}

fn recv(started: &Receiver<String>) -> String {
	started.recv_timeout(PATIENCE).expect("a project starts")
}

#[test]
fn solution_coordinator_runs_independent_projects_concurrently() {
	let temp = Temp::new();
	let root = temp.dir();
	write_two_metadata_projects(&root, &join(&root, &["out-first"]), &join(&root, &["out-second"]));
	let (drainer, started, release) = ParallelDrainer::gated();
	let mut coordinator = coordinator_with(&join(&root, &["tsconfig.json"]), &builders(2), drainer);
	std::thread::scope(|scope| {
		let drain = scope.spawn(|| coordinator.drain().2);
		let first = recv(&started);
		let second_started = started.recv_timeout(PATIENCE).is_ok();
		drop(release);
		let error = drain.join().expect("the drain");
		assert!(error.is_none(), "Drain: {error:?}");
		assert!(
			second_started && !first.is_empty(),
			"independent projects did not overlap: first={first:?} secondStarted={second_started}"
		);
	});
}

#[test]
fn solution_coordinator_serializes_write_conflicts() {
	let temp = Temp::new();
	let root = temp.dir();
	let shared_out = join(&root, &["shared-out"]);
	write_two_metadata_projects(&root, &shared_out, &shared_out);
	let (drainer, started, release) = ParallelDrainer::gated();
	let mut coordinator = coordinator_with(&join(&root, &["tsconfig.json"]), &builders(2), drainer);
	std::thread::scope(|scope| {
		let drain = scope.spawn(|| coordinator.drain().2);
		assert_serialized(&started, &release, "");
		drop(release);
		let error = drain.join().expect("the drain");
		assert!(error.is_none(), "Drain: {error:?}");
	});
}

fn assert_serialized(started: &Receiver<String>, release: &Sender<()>, suffix: &str) {
	let got = recv(started);
	assert_eq!(got, "first", "first started project{suffix} = {got:?}, want first");
	match started.recv_timeout(GRACE) {
		Ok(got) => panic!("conflicting project {got:?} started before first finished{suffix}"),
		Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {}
	}
	release.send(()).expect("the release");
	let got = recv(started);
	assert_eq!(got, "second", "second started project{suffix} = {got:?}, want second");
}

#[test]
fn solution_coordinator_deterministic_reduction_and_persists() {
	type Run = (BuildResult, Vec<String>, Vec<String>, Option<String>);
	let mut results: Vec<Run> = Vec::new();
	for count in [1, 2, 4] {
		let temp = Temp::new();
		let root = temp.dir();
		write_two_metadata_projects(&root, &join(&root, &["out-first"]), &join(&root, &["out-second"]));
		let drainer = Arc::new(ParallelDrainer::default());
		let mut coordinator = coordinator_with(&join(&root, &["tsconfig.json"]), &builders(count), drainer.clone());
		let (mut result, messages, error) = coordinator.drain();
		result.outputs = result
			.outputs
			.into_iter()
			.map(|(path, output)| (gopath::base(&path), output))
			.collect();
		results.push((
			result,
			messages,
			drainer.persist_order(),
			error.map(|error| error.to_string()),
		));
	}
	for (index, right) in results.iter().enumerate().skip(1) {
		let left = &results[0];
		assert!(
			left.0 == right.0 && left.1 == right.1 && left.2 == right.2,
			"builders 1 and {} differ:\nleft={left:?}\nright={right:?}",
			index + 1
		);
		assert_eq!(
			left.3.is_none(),
			right.3.is_none(),
			"builders 1 and {} differ in first error: {:?} vs {:?}",
			index + 1,
			left.3,
			right.3
		);
	}
	let mut failures: Vec<Run> = Vec::new();
	for count in [1, 2, 4] {
		let temp = Temp::new();
		let root = temp.dir();
		write_solution_config(&root, "tsconfig.json", &["./first", "./second"], true);
		write_solution_config(&join(&root, &["first"]), "tsconfig.json", &[], false);
		write_solution_config(&join(&root, &["second"]), "tsconfig.json", &[], false);
		let drainer = ParallelDrainer::failing(&["first", "second"]);
		let mut coordinator = coordinator_with(&join(&root, &["tsconfig.json"]), &builders(count), drainer);
		let (result, messages, error) = coordinator.drain();
		failures.push((result, messages, Vec::new(), error.map(|error| error.to_string())));
	}
	for (index, right) in failures.iter().enumerate().skip(1) {
		let left = &failures[0];
		assert!(
			left.0 == right.0 && left.1 == right.1 && left.3 == right.3,
			"failing builders 1 and {} differ:\nleft={left:?}\nright={right:?}",
			index + 1
		);
	}
}

#[test]
fn solution_coordinator_independent_failure_blocks_only_dependents() {
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_config(&root, "tsconfig.json", &["./broken", "./sibling", "./app"], true);
	write_solution_config(&join(&root, &["broken"]), "tsconfig.json", &[], false);
	write_solution_config(&join(&root, &["sibling"]), "tsconfig.json", &[], false);
	write_solution_config(&join(&root, &["app"]), "tsconfig.json", &["../broken"], false);
	let broken_config = join(&root, &["broken", "tsconfig.json"]);
	let app_config = join(&root, &["app", "tsconfig.json"]);
	let drainer = ParallelDrainer::failing(&["broken"]);
	let mut coordinator = coordinator_with(&join(&root, &["tsconfig.json"]), &builders(2), drainer.clone());
	let (_, messages, error) = coordinator.drain();
	let want = [
		"diagnostic:broken".to_owned(),
		"diagnostic:sibling".to_owned(),
		format!("compile: project {app_config} blocked by failed dependency {broken_config}"),
	];
	assert!(
		error.is_some() && messages == want,
		"first Drain = messages {messages:?}, err {error:?}"
	);
	let state = coordinator.project_state(&app_config);
	assert!(
		state.as_ref().is_some_and(|state| state.blocked_by == broken_config),
		"app state = {:?}",
		state.map(|state| state.blocked_by)
	);
	let mut got = drainer.drain_order();
	got.sort();
	assert_eq!(got, ["broken", "sibling"], "first drained projects");
	let first_drain_count = got.len();
	lock(&drainer.failures).clear();
	let (_, _, error) = coordinator.drain();
	assert!(error.is_none(), "Drain after recovery: {error:?}");
	let got = drainer.drain_order();
	assert_eq!(
		got[first_drain_count..],
		["broken", "app"],
		"drained projects after recovery = {got:?}"
	);
}

#[test]
fn solution_coordinator_retries_failing_and_later_persists() {
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_config(&root, "tsconfig.json", &["./first", "./second", "./third"], true);
	for name in ["first", "second", "third"] {
		write_solution_config(&join(&root, &[name]), "tsconfig.json", &[], false);
	}
	let drainer = Arc::new(ParallelDrainer::default());
	lock(&drainer.persist_failure).insert("second".to_owned());
	let mut coordinator = coordinator_with(&join(&root, &["tsconfig.json"]), &builders(4), drainer.clone());
	assert!(
		coordinator.drain().2.is_some(),
		"Drain unexpectedly succeeded with persist failure"
	);
	lock(&drainer.persist_failure).clear();
	let (_, _, error) = coordinator.drain();
	assert!(error.is_none(), "Drain retry: {error:?}");
	assert_eq!(
		drainer.persist_order(),
		["first", "second", "second", "third"],
		"persist order"
	);
}

#[test]
fn solution_coordinator_reload_for_watch_retains_builders_and_rebuilds_conflicts() {
	let temp = Temp::new();
	let root = temp.dir();
	let first_dir = join(&root, &["first"]);
	let second_dir = join(&root, &["second"]);
	write_two_metadata_projects(&root, &join(&root, &["out-first"]), &join(&root, &["out-second"]));
	let drainer = Arc::new(ParallelDrainer::default());
	let mut coordinator = coordinator_with(&join(&root, &["tsconfig.json"]), &builders(2), drainer.clone());
	let shared_out = join(&root, &["shared-out"]);
	let first_config = join(&first_dir, &["tsconfig.json"]);
	let second_config = join(&second_dir, &["tsconfig.json"]);
	for config_path in [&first_config, &second_config] {
		let config = std::fs::read_to_string(config_path)
			.expect("the config")
			.replacen(
				&rotor_support::gostr::quote(&join(&root, &["out-first"])),
				&rotor_support::gostr::quote(&shared_out),
				1,
			)
			.replacen(
				&rotor_support::gostr::quote(&join(&root, &["out-second"])),
				&rotor_support::gostr::quote(&shared_out),
				1,
			);
		std::fs::write(config_path, config).expect("the config writes");
	}
	if let Err(error) = coordinator.reload_for_watch(&join(&root, &["tsconfig.json"]), &builders(2)) {
		panic!("ReloadForWatch: {error}");
	}
	assert_eq!(coordinator.builders, 2, "builders after reload");
	assert_eq!(
		coordinator.wait_only_dependencies.get(&second_config),
		Some(&vec![first_config]),
		"wait-only dependencies after reload"
	);
	let (started, release) = drainer.gate();
	std::thread::scope(|scope| {
		let drain = scope.spawn(|| coordinator.drain().2);
		assert_serialized(&started, &release, " after reload");
		drop(release);
		let error = drain.join().expect("the drain");
		assert!(error.is_none(), "Drain after reload: {error:?}");
	});
}
