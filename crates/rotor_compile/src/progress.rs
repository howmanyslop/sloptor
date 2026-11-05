//! Live build progress for a human-facing UI (rotor's own; Go has no equivalent).
//!
//! The build only bumps atomics here: a phase, file counters and project counters. Per-project
//! events take a lock once per project, never per file. A render thread samples the counters at
//! its own pace, so the build never waits on the terminal.

use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

/// What the build is doing now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
	/// Nothing has started.
	Starting,
	/// Loading the config and the program.
	Loading,
	/// Type-checking the project's files.
	Checking,
	/// Transforming files to Luau.
	Transforming,
	/// Emitting declarations.
	Declarations,
	/// Writing outputs.
	Writing,
}

impl Phase {
	const ALL: [Self; 6] = [
		Self::Starting,
		Self::Loading,
		Self::Checking,
		Self::Transforming,
		Self::Declarations,
		Self::Writing,
	];

	/// The verb the UI shows.
	#[must_use]
	pub const fn verb(self) -> &'static str {
		match self {
			Self::Starting => "Starting",
			Self::Loading => "Loading",
			Self::Checking => "Checking",
			Self::Transforming => "Transforming",
			Self::Declarations => "Declaring",
			Self::Writing => "Writing",
		}
	}
}

/// How a project of a solution build ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectStatus {
	/// Compiled.
	Built,
	/// Up to date: nothing to compile or write.
	Fresh,
	/// Failed.
	Failed,
	/// Not built because a dependency failed.
	Blocked,
}

/// One finished project of a solution build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectEvent {
	/// The project's tsconfig.
	pub config_path: String,
	/// How it ended.
	pub status: ProjectStatus,
	/// Compiled outputs it owns.
	pub files: usize,
	/// Outputs written this build.
	pub written: usize,
	/// Error diagnostics.
	pub errors: usize,
	/// Warning diagnostics.
	pub warnings: usize,
	/// The project's wall time.
	pub elapsed: Duration,
	/// The dependency that blocked it.
	pub blocked_by: String,
}

/// One file's transform time, kept only when [`BuildProgress::collect_file_times`] is on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileTime {
	/// The file's absolute, slash-separated path.
	pub file_name: String,
	/// Its transform and render time.
	pub elapsed: Duration,
}

/// Shared progress of one build, read by a UI thread.
#[derive(Debug)]
pub struct BuildProgress {
	phase: AtomicU8,
	files_total: AtomicUsize,
	files_checked: AtomicUsize,
	files_transformed: AtomicUsize,
	projects_total: AtomicUsize,
	projects_done: AtomicUsize,
	running: Mutex<Vec<String>>,
	finished: Mutex<Vec<ProjectEvent>>,
	collect_file_times: bool,
	file_times: Mutex<Vec<FileTime>>,
}

impl BuildProgress {
	/// A progress record; `collect_file_times` keeps every file's transform time (for `--verbose`).
	#[must_use]
	pub const fn new(collect_file_times: bool) -> Self {
		Self {
			phase: AtomicU8::new(0),
			files_total: AtomicUsize::new(0),
			files_checked: AtomicUsize::new(0),
			files_transformed: AtomicUsize::new(0),
			projects_total: AtomicUsize::new(0),
			projects_done: AtomicUsize::new(0),
			running: Mutex::new(Vec::new()),
			finished: Mutex::new(Vec::new()),
			collect_file_times,
			file_times: Mutex::new(Vec::new()),
		}
	}

	/// The current phase.
	#[must_use]
	pub fn phase(&self) -> Phase {
		Phase::ALL
			.get(usize::from(self.phase.load(Ordering::Relaxed)))
			.copied()
			.unwrap_or(Phase::Starting)
	}

	/// Moves to `phase`.
	pub fn set_phase(&self, phase: Phase) {
		let index = Phase::ALL.iter().position(|candidate| *candidate == phase).unwrap_or(0);
		self.phase.store(u8::try_from(index).unwrap_or(0), Ordering::Relaxed);
	}

	/// `(checked, transformed, total)` source files so far.
	#[must_use]
	pub fn files(&self) -> (usize, usize, usize) {
		(
			self.files_checked.load(Ordering::Relaxed),
			self.files_transformed.load(Ordering::Relaxed),
			self.files_total.load(Ordering::Relaxed),
		)
	}

	/// `(done, total)` projects of a solution build.
	#[must_use]
	pub fn projects(&self) -> (usize, usize) {
		(
			self.projects_done.load(Ordering::Relaxed),
			self.projects_total.load(Ordering::Relaxed),
		)
	}

	/// The projects building now, by tsconfig.
	#[must_use]
	pub fn running(&self) -> Vec<String> {
		self.running.lock().unwrap_or_else(PoisonError::into_inner).clone()
	}

	/// The project events after the first `seen`.
	#[must_use]
	pub fn finished_since(&self, seen: usize) -> Vec<ProjectEvent> {
		let finished = self.finished.lock().unwrap_or_else(PoisonError::into_inner);
		finished.get(seen..).map(<[ProjectEvent]>::to_vec).unwrap_or_default()
	}

	/// The kept file times, slowest first.
	#[must_use]
	pub fn slowest_files(&self, count: usize) -> Vec<FileTime> {
		let mut times = self.file_times.lock().unwrap_or_else(PoisonError::into_inner).clone();
		times.sort_by_key(|time| std::cmp::Reverse(time.elapsed));
		times.truncate(count);
		times
	}

	/// Adds `count` source files to the total.
	pub fn add_files(&self, count: usize) {
		self.files_total.fetch_add(count, Ordering::Relaxed);
	}

	/// Counts one checked file.
	pub fn file_checked(&self) {
		self.files_checked.fetch_add(1, Ordering::Relaxed);
	}

	/// Counts one transformed file.
	pub fn file_transformed(&self) {
		self.files_transformed.fetch_add(1, Ordering::Relaxed);
	}

	pub(crate) const fn collects_file_times(&self) -> bool {
		self.collect_file_times
	}

	pub(crate) fn add_file_times(&self, times: impl IntoIterator<Item = FileTime>) {
		self.file_times
			.lock()
			.unwrap_or_else(PoisonError::into_inner)
			.extend(times);
	}

	pub(crate) fn set_projects(&self, total: usize) {
		self.projects_total.store(total, Ordering::Relaxed);
	}

	pub(crate) fn project_started(&self, config_path: &str) {
		self.running
			.lock()
			.unwrap_or_else(PoisonError::into_inner)
			.push(config_path.to_owned());
	}

	pub(crate) fn project_finished(&self, event: ProjectEvent) {
		self.running
			.lock()
			.unwrap_or_else(PoisonError::into_inner)
			.retain(|path| *path != event.config_path);
		self.finished.lock().unwrap_or_else(PoisonError::into_inner).push(event);
		self.projects_done.fetch_add(1, Ordering::Relaxed);
	}
}
