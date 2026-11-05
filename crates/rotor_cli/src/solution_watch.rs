//! `build --build --watch` (`cmd/rotor/solution_watch.go`): builds the solution, then rebuilds
//! the projects each batch of file-system events touches.
//!
//! Watch callbacks only record events and trigger the compile gate; one worker thread (on the
//! checker's large stack) owns the coordinator and runs the cycles, at most one at a time, with
//! events that arrive during a cycle coalesced into exactly one more.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::Write as _;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Instant;

use rotor_compile::SolutionCoordinator;
use rotor_compile::compile::DiagnosticInfo;
use rotor_compile::output::BuildResult;
use rotor_compile::solution_watch::{SolutionWatchSet, WatchAssetEvent};
use rotor_support::gopath;

use crate::build::Outcome;
use crate::fswatch::{Event, EventKind, WatchCallback, WatchError, Watcher};
use crate::options::{MergedOptions, project_compile_options};
use crate::solution_watch_coverage::{
	SolutionWatches, artifact_directories, canonical_path, directory_is_artifact, exclusions,
};
use crate::watch::{BuildWatchReporter, Stop, TreeWatcher};

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `compileGate`: at most one run at a time; triggers during a run cost exactly one more.
#[derive(Default)]
pub struct CompileGate {
	state: Mutex<GateState>,
	signal: Condvar,
}

#[derive(Default)]
struct GateState {
	running: bool,
	pending: bool,
	closed: bool,
}

impl CompileGate {
	/// `Trigger`
	pub fn trigger(&self) {
		lock(&self.state).pending = true;
		self.signal.notify_all();
	}

	/// Ends [`CompileGate::serve`] once the running and pending runs are done.
	pub fn close(&self) {
		lock(&self.state).closed = true;
		self.signal.notify_all();
	}

	/// `Drain`: waits until no run is in flight or pending.
	#[cfg(test)]
	#[expect(
		clippy::significant_drop_tightening,
		reason = "the condition variable needs the guard"
	)]
	pub fn drain(&self) {
		let mut state = lock(&self.state);
		while state.running || state.pending {
			state = self.signal.wait(state).unwrap_or_else(PoisonError::into_inner);
		}
	}

	/// Runs `run` once per batch of triggers on this thread, until closed and idle.
	pub fn serve(&self, run: &mut dyn FnMut()) {
		loop {
			let mut state = lock(&self.state);
			while !state.pending && !state.closed {
				state = self.signal.wait(state).unwrap_or_else(PoisonError::into_inner);
			}
			if !state.pending {
				return;
			}
			state.pending = false;
			state.running = true;
			drop(state);
			run();
			lock(&self.state).running = false;
			self.signal.notify_all();
		}
	}
}

/// `solutionWatchKind`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WatchKind {
	/// A project's source root.
	Root,
	/// A directory a Rojo config maps.
	Rojo,
	/// A tsconfig or Rojo config.
	Config,
	/// A file the package layout read.
	PackageFile,
	/// A directory the package layout lists.
	PackageDirectory,
	/// A file `$env` or `$file` reads.
	Macro,
}

/// `solutionWatchKey`: one logical watch.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WatchKey {
	/// What is watched.
	pub kind: WatchKind,
	/// The owning project's config.
	pub project: String,
	/// The watched path.
	pub path: String,
}

impl WatchKey {
	/// A key.
	#[must_use]
	pub fn new(kind: WatchKind, project: &str, path: &str) -> Self {
		Self {
			kind,
			project: project.to_owned(),
			path: path.to_owned(),
		}
	}

	/// `directory`: the directory the OS watches for this key, and whether recursively. Watches of
	/// one directory share one OS watch, so they die together.
	#[must_use]
	pub fn directory(&self) -> (String, bool) {
		if self.file_watch() {
			return (gopath::dir(&gopath::clean(&self.path)), false);
		}
		(gopath::clean(&self.path), self.kind != WatchKind::PackageDirectory)
	}

	/// `fileWatch`
	#[must_use]
	pub const fn file_watch(&self) -> bool {
		matches!(self.kind, WatchKind::Config | WatchKind::PackageFile | WatchKind::Macro)
	}
}

/// `solutionWatchEvents`: what a batch of callbacks recorded for the next cycle.
#[derive(Clone, Debug, Default)]
pub struct SolutionWatchEvents {
	/// Projects to invalidate.
	pub projects: HashSet<String>,
	/// Configs that changed.
	pub configs: HashSet<String>,
	/// Per project, the changed asset paths and whether each was deleted.
	pub assets: HashMap<String, HashMap<String, bool>>,
	/// Every changed path, for the change list.
	pub paths: HashSet<String>,
	/// Projects whose package layout inputs changed.
	pub packages: HashSet<String>,
	/// The watches that stopped delivering: their path was deleted or the watch terminated.
	pub dead: HashSet<WatchKey>,
}

impl SolutionWatchEvents {
	/// `addWatchError`: records the paths and errors, and whether the route needs full
	/// invalidation. Deleting a logical root needs the same recovery as a terminated watch, even
	/// under live shared coverage. An overflow invalidates without ending the watch.
	pub fn add_watch_error(&mut self, key: &WatchKey, events: &[Event], error: Option<&WatchError>) -> bool {
		let mut root_deleted = false;
		for event in events {
			self.paths.insert(event.path.clone());
			if event.kind == EventKind::Delete && gopath::clean(&event.path) == gopath::clean(&key.path) {
				self.dead.insert(key.clone());
				root_deleted = true;
			}
		}
		if error.is_some_and(|error| !error.is_overflow()) {
			self.dead.insert(key.clone());
		}
		error.is_some() || root_deleted
	}

	/// `add`: a source root's events.
	pub fn add(&mut self, key: &WatchKey, events: &[Event], error: Option<&WatchError>) {
		if self.add_watch_error(key, events, error) {
			self.projects.insert(key.project.clone());
		}
		for event in events {
			if watch_compilable(&event.path) {
				self.projects.insert(key.project.clone());
				continue;
			}
			if watch_project_input(event) {
				self.projects.insert(key.project.clone());
			}
			self.add_asset(&key.project, event);
		}
	}

	/// `addRojo`: a Rojo directory's events.
	pub fn add_rojo(&mut self, key: &WatchKey, events: &[Event], error: Option<&WatchError>) {
		if self.add_watch_error(key, events, error) {
			self.configs.insert(key.project.clone());
		}
		for event in events {
			if gopath::base(&event.path).ends_with(".project.json") {
				self.configs.insert(event.path.clone());
				continue;
			}
			if !watch_compilable(&event.path) {
				self.add_asset(&key.project, event);
			}
		}
	}

	/// `addConfig`
	pub fn add_config(&mut self, key: &WatchKey, events: &[Event], error: Option<&WatchError>) {
		self.add_watch_error(key, events, error);
		self.configs.insert(key.path.clone());
	}

	/// `addPackageInputs`
	pub fn add_package_inputs(&mut self, key: &WatchKey, events: &[Event], error: Option<&WatchError>) {
		self.add_watch_error(key, events, error);
		if error.is_some() {
			self.projects.insert(key.project.clone());
		}
		for event in events {
			if key.file_watch() || event.kind == EventKind::Delete {
				self.packages.insert(key.project.clone());
				continue;
			}
			if std::fs::symlink_metadata(&event.path).is_ok_and(|metadata| {
				let file_type = metadata.file_type();
				file_type.is_dir() || file_type.is_symlink() || !file_type.is_file()
			}) {
				self.packages.insert(key.project.clone());
			}
		}
	}

	fn add_asset(&mut self, project: &str, event: &Event) {
		self.assets
			.entry(project.to_owned())
			.or_default()
			.insert(event.path.clone(), event.kind == EventKind::Delete);
	}
}

/// `solutionWatchDirectories`: the source roots, then the Rojo directories outside every
/// artifact directory.
#[must_use]
#[cfg(test)]
pub fn solution_watch_directories(set: &SolutionWatchSet) -> Vec<String> {
	let mut directories = set.root_dirs.clone();
	directories.extend(
		set.rojo_directories
			.iter()
			.filter(|directory| !directory_is_artifact(directory, &set.artifact_directories))
			.cloned(),
	);
	directories
}

/// `solutionWatchKeys`: every logical watch of `sets`.
#[must_use]
pub fn solution_watch_keys(sets: &[SolutionWatchSet]) -> Vec<WatchKey> {
	let mut keys = Vec::new();
	let artifacts = artifact_directories(sets);
	for set in sets {
		let project = &set.project_path;
		for path in &set.macro_files {
			keys.push(WatchKey::new(WatchKind::Macro, project, path));
		}
		if let Some(inputs) = &set.package_inputs {
			for path in &inputs.files {
				keys.push(WatchKey::new(WatchKind::PackageFile, project, path));
			}
			for path in &inputs.directories {
				keys.push(WatchKey::new(WatchKind::PackageDirectory, project, path));
			}
		}
		for directory in &set.root_dirs {
			keys.push(WatchKey::new(WatchKind::Root, project, directory));
		}
		for directory in &set.rojo_directories {
			if !directory_is_artifact(&canonical_path(directory), &artifacts) {
				keys.push(WatchKey::new(WatchKind::Rojo, project, directory));
			}
		}
		for config in set.ts_config_paths.iter().chain(&set.rojo_configs) {
			if directory_is_artifact(&canonical_path(config), &artifacts) {
				continue;
			}
			keys.push(WatchKey::new(WatchKind::Config, project, config));
		}
	}
	keys
}

/// `solutionWatchedFileCount`: the distinct files the watcher observes: every file under the
/// watched directories, plus the config files.
#[must_use]
pub fn solution_watched_file_count(sets: &[SolutionWatchSet]) -> usize {
	let mut seen = HashSet::new();
	let artifacts = artifact_directories(sets);
	for key in solution_watch_keys(sets) {
		if key.file_watch() {
			seen.insert(canonical_path(&key.path));
			continue;
		}
		if key.kind == WatchKind::PackageDirectory {
			// Discovery only: the relevant files are the exact package inputs.
			continue;
		}
		let mut watcher = TreeWatcher::new(&canonical_path(&key.path), &exclusions(&key, &artifacts));
		for path in watcher.snapshot().into_keys() {
			seen.insert(canonical_path(&path));
		}
	}
	seen.len()
}

/// `watchCompilable`
#[must_use]
pub fn watch_compilable(path: &str) -> bool {
	matches!(gopath::ext(path).as_str(), ".ts" | ".tsx")
}

/// `watchProjectInput`: whether a non-source event can change a project's file set: a JSON file
/// (a source under `resolveJsonModule`) or a directory (a folder move reports only the folder). A
/// path with no extension counts as a directory, since a removed one cannot be checked.
#[must_use]
pub fn watch_project_input(event: &Event) -> bool {
	match gopath::ext(&event.path).as_str() {
		".json" | "" => return true,
		_ => {}
	}
	if event.kind == EventKind::Delete {
		return false;
	}
	std::fs::metadata(&event.path).is_ok_and(|metadata| metadata.is_dir())
}

/// `buildDiagnostics`: the structured diagnostics, or the messages.
fn build_diagnostics(result: Option<&BuildResult>, messages: Vec<String>) -> Vec<DiagnosticInfo> {
	if let Some(result) = result
		&& !result.diagnostics.is_empty()
	{
		return result.diagnostics.clone();
	}
	messages
		.into_iter()
		.map(|message| DiagnosticInfo {
			message,
			..DiagnosticInfo::default()
		})
		.collect()
}

/// What the callbacks share with the cycle.
#[derive(Default)]
struct Shared {
	events: SolutionWatchEvents,
	/// Set once the session is stopping.
	stopped: bool,
	/// Refreshed with the watch sets.
	artifact_directories: Vec<String>,
}

/// Reloads the options a changed config may have changed.
pub type Reload<'a> = &'a (dyn Fn() -> Result<MergedOptions, String> + Sync);

/// `runBuildSolutionWatchLoop`: builds the solution, then rebuilds the affected projects on every
/// change until `stop`. Returns 1 when the solution graph cannot be loaded.
#[must_use]
pub fn run_build_solution_watch_loop(
	stop: &Stop,
	ts_config_path: &str,
	opts: MergedOptions,
	reload: Reload<'_>,
	reporter: &mut (dyn BuildWatchReporter + Send),
	watcher: Arc<dyn Watcher>,
) -> u8 {
	let shared = Arc::new(Mutex::new(Shared::default()));
	let gate = Arc::new(CompileGate::default());
	std::thread::scope(|scope| {
		let worker = {
			let shared = Arc::clone(&shared);
			let gate = Arc::clone(&gate);
			scope.spawn(move || {
				rotor_tsrs::with_large_stack(move || {
					worker(ts_config_path, opts, reload, reporter, &watcher, &shared, &gate)
				})
			})
		};
		while !worker.is_finished() && !stop.sleep(std::time::Duration::from_millis(50)) {}
		// Stop callbacks from queueing cycles, let the running one finish (it may refresh the
		// watches), then let the worker close the watches it left behind.
		lock(&shared).stopped = true;
		gate.close();
		worker.join().unwrap_or(1)
	})
}

#[expect(clippy::too_many_lines, reason = "one function in Go")]
fn worker(
	ts_config_path: &str,
	mut opts: MergedOptions,
	reload: Reload<'_>,
	reporter: &mut (dyn BuildWatchReporter + Send),
	backend: &Arc<dyn Watcher>,
	shared: &Arc<Mutex<Shared>>,
	gate: &Arc<CompileGate>,
) -> u8 {
	let mut coordinator =
		match SolutionCoordinator::new(ts_config_path, &project_compile_options(ts_config_path, &opts)) {
			Ok(coordinator) => coordinator,
			Err(error) => {
				reporter.build_start(None);
				reporter.build_end(&Outcome {
					result: None,
					diagnostics: Vec::new(),
					elapsed: std::time::Duration::ZERO,
					error: Some(error.to_string()),
				});
				return 1;
			}
		};
	let callback_shared = Arc::clone(shared);
	let callback_gate = Arc::clone(gate);
	let mut watches = SolutionWatches::new(
		Arc::clone(backend),
		Box::new(move |key: &WatchKey| -> WatchCallback {
			let key = key.clone();
			let shared = Arc::clone(&callback_shared);
			let gate = Arc::clone(&callback_gate);
			Arc::new(move |events: &[Event], error: Option<&WatchError>| {
				let mut state = lock(&shared);
				if state.stopped {
					return;
				}
				// A source or Rojo directory can hold another project's outputs: filter against the
				// whole solution before recording anything.
				let exclusions = exclusions(&key, &state.artifact_directories);
				let filtered: Vec<Event> = events
					.iter()
					.filter(|event| !directory_is_artifact(&canonical_path(&event.path), &exclusions))
					.cloned()
					.collect();
				if filtered.is_empty() && error.is_none() {
					return;
				}
				let recorded = &mut state.events;
				match key.kind {
					WatchKind::Root => recorded.add(&key, &filtered, error),
					WatchKind::Rojo => recorded.add_rojo(&key, &filtered, error),
					WatchKind::PackageFile | WatchKind::PackageDirectory => {
						recorded.add_package_inputs(&key, &filtered, error);
					}
					WatchKind::Macro => {
						recorded.add_watch_error(&key, &filtered, error);
						recorded.projects.insert(key.project.clone());
					}
					WatchKind::Config => recorded.add_config(&key, &filtered, error),
				}
				drop(state);
				gate.trigger();
			})
		}),
	);
	let refresh = |coordinator: &mut SolutionCoordinator, watches: &mut SolutionWatches, dead: &HashSet<WatchKey>| {
		let sets = coordinator.watch_sets();
		lock(shared).artifact_directories = artifact_directories(&sets);
		watches.refresh(&sets, dead);
	};
	refresh(&mut coordinator, &mut watches, &HashSet::new());
	{
		let mut state = lock(shared);
		for set in coordinator.watch_sets() {
			state.events.projects.insert(set.project_path);
		}
	}
	gate.trigger();

	let mut initial = true;
	let mut cycle = || {
		let mut events = std::mem::take(&mut lock(shared).events);
		for project in &events.packages {
			if coordinator.refresh_package_watch_inputs(project) {
				events.projects.insert(project.clone());
			}
		}
		if !events.packages.is_empty() {
			refresh(&mut coordinator, &mut watches, &events.dead);
		}
		if !events.projects.is_empty() || !events.configs.is_empty() {
			let changed: Option<Vec<String>> = (!initial).then(|| {
				events
					.paths
					.iter()
					.cloned()
					.collect::<BTreeSet<_>>()
					.into_iter()
					.collect()
			});
			let started = Instant::now();
			reporter.build_start(changed.as_deref());
			// `rebuild`: reloads options and the graph first when a config changed. `reloaded`
			// is false when that reload failed; the dead watches reopen even then.
			let mut rebuild = || -> (Option<BuildResult>, Vec<DiagnosticInfo>, bool, Option<String>) {
				let mut clean_stale = None;
				if !events.configs.is_empty() {
					for set in coordinator.watch_sets() {
						if set.ts_config_paths.iter().any(|config| events.configs.contains(config)) {
							events.projects.insert(set.project_path.clone());
						}
					}
					let next = match reload() {
						Ok(next) => next,
						Err(error) => {
							refresh(&mut coordinator, &mut watches, &events.dead);
							return (None, Vec::new(), false, Some(error));
						}
					};
					opts = next;
					match coordinator.reload_for_watch(ts_config_path, &project_compile_options(ts_config_path, &opts))
					{
						Ok(clean) => clean_stale = Some(clean),
						Err(error) => {
							refresh(&mut coordinator, &mut watches, &events.dead);
							return (None, Vec::new(), false, Some(error.to_string()));
						}
					}
					// A reloaded project may emit into new directories during this drain: keep the
					// old ones excluded until the stale outputs are cleaned, then the refresh
					// replaces the union with the current set.
					let next_artifacts = artifact_directories(&coordinator.watch_sets());
					lock(shared).artifact_directories.extend(next_artifacts);
					if events.projects.is_empty() {
						for set in coordinator.watch_sets() {
							events.projects.insert(set.project_path);
						}
					}
				}
				let projects: Vec<String> = events.projects.iter().cloned().collect();
				for project in projects {
					coordinator.invalidate(&[project]);
				}
				let (result, messages, mut error) = coordinator.drain();
				if error.is_none()
					&& let Some(clean) = clean_stale
				{
					error = clean().err();
				}
				refresh(&mut coordinator, &mut watches, &events.dead);
				let diagnostics = build_diagnostics(Some(&result), messages);
				(Some(result), diagnostics, true, error.map(|error| error.to_string()))
			};
			let (result, diagnostics, reloaded, error) = rebuild();
			reporter.build_end(&Outcome {
				result,
				diagnostics,
				elapsed: started.elapsed(),
				error,
			});
			crate::memory::trim();
			let sets = coordinator.watch_sets();
			let count = || solution_watched_file_count(&sets);
			reporter.idle(&count);
			if initial {
				initial = false;
				reporter.watching(&count);
			}
			if !reloaded {
				return;
			}
		}
		let mut assets: Vec<(String, HashMap<String, bool>)> = events.assets.into_iter().collect();
		assets.sort_by(|left, right| left.0.cmp(&right.0));
		for (project, paths) in assets {
			let mut changes: Vec<WatchAssetEvent> = paths
				.into_iter()
				.map(|(path, deleted)| WatchAssetEvent { path, deleted })
				.collect();
			changes.sort_by(|left, right| left.path.cmp(&right.path));
			if let Err(error) = coordinator.drain_assets(&project, &changes) {
				let _ = writeln!(std::io::stderr(), "{error}");
			}
		}
	};
	gate.serve(&mut cycle);
	watches.close();
	0
}

#[cfg(test)]
mod tests;
