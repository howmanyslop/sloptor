//! `solution_persistence_test.go`. The build drainer's persistence queue is the coordinator's
//! in the port; a test drainer opts into it, as Go's embeds the build drainer.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use rotor_support::gopath;

use super::Temp;
use super::solution::{builders, coordinator_with, join, write_solution_config};
use crate::compile::Error;
use crate::output::BuildResult;
use crate::project::{PendingPersist, lock};
use crate::solution::{DrainResult, SolutionCoordinator, SolutionProject, SolutionProjectDrainer};
use crate::solution_drain::PersistQueue;

/// `retentionSolutionDrainer`
#[derive(Default)]
struct RetentionDrainer {
	generation: Mutex<HashMap<String, String>>,
	fail_build: Mutex<HashMap<String, String>>,
	fail_queued: Mutex<HashMap<String, String>>,
	fail_write: Mutex<HashSet<String>>,
	attempts: Arc<Mutex<Vec<String>>>,
}

impl SolutionProjectDrainer for RetentionDrainer {
	fn drain(&self, project: SolutionProject) -> DrainResult {
		let name = gopath::base(&gopath::dir(&project.config_path));
		let failed = |error: &str| DrainResult {
			result: None,
			messages: Vec::new(),
			error: Some(Error::message(error)),
		};
		if let Some(error) = lock(&self.fail_build).get(&name) {
			return failed(error);
		}
		let generation = lock(&self.generation).get(&name).cloned().unwrap_or_default();
		if !generation.is_empty()
			&& let Some(pending) = &project.options.pending_solution_persists
		{
			for stage in ["cache", "manifest"] {
				let id = format!("{name}:{generation}:{stage}");
				let attempts = Arc::clone(&self.attempts);
				let fail = lock(&self.fail_write).contains(&id);
				let persist: PendingPersist = Box::new(move || {
					lock(&attempts).push(id.clone());
					if fail {
						return Err(Error::message(format!("write {id}")));
					}
					Ok(())
				});
				lock(pending).push(persist);
			}
		}
		if let Some(error) = lock(&self.fail_queued).get(&name) {
			return failed(error);
		}
		DrainResult {
			result: Some(BuildResult {
				outputs: [(name, generation.into_bytes())].into(),
				..BuildResult::default()
			}),
			..DrainResult::default()
		}
	}

	fn queues_persistence(&self) -> bool {
		true
	}
}

/// `retentionCoordinator`
fn retention_coordinator(drainer: &Arc<RetentionDrainer>) -> (Temp, SolutionCoordinator, HashMap<String, String>) {
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_config(&root, "tsconfig.json", &["./first", "./second", "./third"], true);
	let mut configs = HashMap::new();
	for name in ["first", "second", "third"] {
		write_solution_config(&join(&root, &[name]), "tsconfig.json", &[], false);
		configs.insert(name.to_owned(), join(&root, &[name, "tsconfig.json"]));
	}
	let coordinator = coordinator_with(&join(&root, &["tsconfig.json"]), &builders(1), drainer.clone());
	(temp, coordinator, configs)
}

fn set(map: &Mutex<HashMap<String, String>>, entries: &[(&str, &str)]) {
	let mut map = lock(map);
	map.clear();
	for (key, value) in entries {
		map.insert((*key).to_owned(), (*value).to_owned());
	}
}

#[test]
fn solution_persistence_bounds_failed_drain_generations() {
	let drainer = Arc::new(RetentionDrainer::default());
	set(
		&drainer.generation,
		&[("first", "old"), ("second", "stable"), ("third", "recovered")],
	);
	set(&drainer.fail_build, &[("third", "broken sibling")]);
	let (_temp, mut coordinator, configs) = retention_coordinator(&drainer);
	for generation in ["old", "middle", "latest"] {
		lock(&drainer.generation).insert("first".to_owned(), generation.to_owned());
		coordinator.invalidate(&[configs["first"].clone()]);
		assert!(coordinator.drain().2.is_some(), "drain succeeded with broken sibling");
		assert!(
			lock(&drainer.attempts).is_empty(),
			"published during failed solution: {:?}",
			lock(&drainer.attempts)
		);
		let pending = lock(&coordinator.persist_queue).persists.len();
		assert!(
			pending <= 2,
			"pending persistence generations = {pending}, want at most two successful projects"
		);
	}
	lock(&drainer.fail_build).remove("third");
	let (_, _, error) = coordinator.drain();
	assert!(error.is_none(), "{error:?}");
	assert_eq!(
		*lock(&drainer.attempts),
		[
			"first:latest:cache",
			"first:latest:manifest",
			"second:stable:cache",
			"second:stable:manifest",
			"third:recovered:cache",
			"third:recovered:manifest"
		],
		"published generations"
	);
	assert!(
		lock(&coordinator.persist_queue).persists.is_empty(),
		"successful recovery retained callbacks"
	);
}

#[test]
fn solution_persistence_retries_only_unfinished_work() {
	let drainer = Arc::new(RetentionDrainer::default());
	set(
		&drainer.generation,
		&[("first", "one"), ("second", "one"), ("third", "one")],
	);
	lock(&drainer.fail_write).insert("second:one:manifest".to_owned());
	let (_temp, mut coordinator, configs) = retention_coordinator(&drainer);
	assert!(
		coordinator.drain().2.is_some(),
		"drain succeeded with persistence error"
	);
	// The failed generation is superseded while the third project's untouched generation must
	// survive. A cancelled attempt cannot replace valid work.
	set(&drainer.fail_build, &[("second", "context canceled")]);
	coordinator.invalidate(&[configs["second"].clone()]);
	let error = coordinator.drain().2.map(|error| error.to_string());
	assert!(
		error.as_deref().is_some_and(|error| error.contains("context canceled")),
		"cancelled drain = {error:?}"
	);
	lock(&drainer.fail_build).remove("second");
	lock(&drainer.generation).insert("second".to_owned(), "two".to_owned());
	let (_, _, error) = coordinator.drain();
	assert!(error.is_none(), "{error:?}");
	assert_eq!(
		*lock(&drainer.attempts),
		[
			"first:one:cache",
			"first:one:manifest",
			"second:one:cache",
			"second:one:manifest",
			"second:two:cache",
			"second:two:manifest",
			"third:one:cache",
			"third:one:manifest"
		],
		"persistence attempts"
	);
}

#[test]
fn solution_persistence_successful_empty_generation_supersedes_pending() {
	let drainer = Arc::new(RetentionDrainer::default());
	set(&drainer.generation, &[("first", "old")]);
	set(&drainer.fail_build, &[("third", "broken sibling")]);
	let (_temp, mut coordinator, configs) = retention_coordinator(&drainer);
	assert!(coordinator.drain().2.is_some(), "drain succeeded with broken sibling");
	lock(&drainer.generation).insert("first".to_owned(), String::new());
	coordinator.invalidate(&[configs["first"].clone()]);
	lock(&drainer.fail_build).remove("third");
	let (_, _, error) = coordinator.drain();
	assert!(error.is_none(), "{error:?}");
	assert!(
		lock(&drainer.attempts).is_empty(),
		"published superseded work: {:?}",
		lock(&drainer.attempts)
	);
}

#[test]
fn solution_persistence_failed_attempt_does_not_queue_incomplete_generation() {
	let drainer = Arc::new(RetentionDrainer::default());
	set(&drainer.generation, &[("first", "incomplete"), ("second", "valid")]);
	set(&drainer.fail_queued, &[("first", "context canceled")]);
	let (_temp, mut coordinator, configs) = retention_coordinator(&drainer);
	let error = coordinator.drain().2.map(|error| error.to_string());
	assert!(
		error.as_deref().is_some_and(|error| error.contains("context canceled")),
		"cancelled drain = {error:?}"
	);
	assert!(
		!lock(&coordinator.persist_queue)
			.persists
			.contains_key(&configs["first"]),
		"cancelled project queued an incomplete persistence generation"
	);
	assert!(
		lock(&drainer.attempts).is_empty(),
		"failed solution published callbacks: {:?}",
		lock(&drainer.attempts)
	);
}

#[test]
fn solution_persistence_repeated_write_failure_releases_consumed_callbacks() {
	let mut queue = PersistQueue::default();
	let attempts = Arc::new(Mutex::new(Vec::new()));
	let fail = Arc::new(Mutex::new(true));
	let step = |name: &'static str, failing: bool| -> PendingPersist {
		let attempts = Arc::clone(&attempts);
		let fail = Arc::clone(&fail);
		Box::new(move || {
			lock(&attempts).push(name);
			if failing && *lock(&fail) {
				return Err(Error::message("write failed"));
			}
			Ok(())
		})
	};
	queue.append(
		"project",
		vec![step("completed", false), step("retry", true), step("later", false)],
	);
	for _ in 0..3 {
		assert!(
			queue.persist().is_err(),
			"persistence succeeded while its write was failing"
		);
		// The completed step is consumed; only the failed one and the later one stay queued.
		assert_eq!(
			queue.persists["project"].len(),
			2,
			"pending retry still retains the consumed closure"
		);
	}
	*lock(&fail) = false;
	queue.persist().unwrap_or_else(|error| panic!("{error}"));
	assert_eq!(
		*lock(&attempts),
		["completed", "retry", "retry", "retry", "retry", "later"],
		"persistence attempts"
	);
	assert!(
		queue.persists.is_empty() && queue.order.is_empty(),
		"successful persistence retained pending work"
	);
}

/// Go's reload builds a fresh build drainer, which drops the persistence a failed solution
/// queued: only what the next successful drain built is written.
#[test]
fn solution_reload_drops_queued_persistence() {
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_config(&root, "tsconfig.json", &["./first", "./second"], true);
	for name in ["first", "second"] {
		let dir = join(&root, &[name]);
		super::solution::write_buildable_solution_project(&dir);
		let config_path = join(&dir, &["tsconfig.json"]);
		let config = std::fs::read_to_string(&config_path).expect("read").replacen(
			r#""outDir":"out""#,
			r#""outDir":"out","incremental":true,"tsBuildInfoFile":"out/cache.tsbuildinfo""#,
			1,
		);
		std::fs::write(&config_path, config).expect("write");
	}
	let second_main = join(&root, &["second", "src", "main.ts"]);
	std::fs::write(&second_main, "export const value: number = \"broken\";\n").expect("write");
	let root_config = join(&root, &["tsconfig.json"]);
	let build_info =
		|name: &str| std::path::Path::new(&join(&root, &[name, "out", "cache.rbxtsc.tsbuildinfo"])).exists();
	super::large_stack(|| {
		let mut coordinator = SolutionCoordinator::new(&root_config, &crate::project::ProjectOptions::default())
			.unwrap_or_else(|error| panic!("{error}"));
		assert!(coordinator.drain().2.is_some(), "drain succeeded with a broken project");
		assert!(
			!build_info("first") && !build_info("second"),
			"a failed solution published state"
		);
		coordinator
			.reload(&root_config, &crate::project::ProjectOptions::default())
			.unwrap_or_else(|error| panic!("Reload: {error}"));
		std::fs::write(&second_main, "export const value = 1;\n").expect("write");
		coordinator.invalidate(&[join(&root, &["second", "tsconfig.json"])]);
		let (_, messages, error) = coordinator.drain();
		assert!(error.is_none(), "drain after reload: {error:?} ({messages:?})");
	});
	assert!(build_info("second"), "the rebuilt project's state was not written");
	assert!(
		!build_info("first"),
		"persistence queued before the reload was published"
	);
}
