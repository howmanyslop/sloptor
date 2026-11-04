//! `scheduler.go`: when a census runs. A census is a full cold compile, so the goal is the fewest
//! runs that still leave the editor showing the latest text:
//!
//! - at most one run per tsconfig at a time;
//! - edits during a run mark the config dirty for exactly one more run, however many edits
//!   there were (coalesced, never queued);
//! - typing is debounced, while opening and saving a file run immediately.

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// Runs one census of a config.
pub type Run = Arc<dyn Fn(&str) + Send + Sync>;

/// `configState`
#[derive(Default)]
struct ConfigState {
	/// The config's inputs changed since its last run started.
	dirty: bool,
	/// A thread is looping over this config's runs.
	running: bool,
	/// The pending debounce's deadline.
	deadline: Option<Instant>,
}

#[derive(Default)]
struct State {
	closed: bool,
	configs: HashMap<String, ConfigState>,
	/// Run loops in flight (Go's wait group).
	loops: usize,
}

struct Shared {
	debounce: Duration,
	run: Run,
	state: Mutex<State>,
	/// Wakes the timer thread and `close`.
	signal: Condvar,
}

/// `scheduler`
pub struct Scheduler {
	shared: Arc<Shared>,
	timer: Option<std::thread::JoinHandle<()>>,
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
	state.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Scheduler {
	/// `newScheduler`
	pub fn new(debounce: Duration, run: Run) -> Self {
		let shared = Arc::new(Shared {
			debounce,
			run,
			state: Mutex::new(State::default()),
			signal: Condvar::new(),
		});
		let timer = {
			let shared = Arc::clone(&shared);
			std::thread::Builder::new()
				.name("rotor-lsp-debounce".to_owned())
				.spawn(move || shared.timers())
				.ok()
		};
		Self { shared, timer }
	}

	/// `schedule`: marks `config` dirty and arranges a run, now when `immediate`, after the
	/// debounce otherwise. A run in flight notices the dirty flag when it finishes.
	pub fn schedule(&self, config: &str, immediate: bool) {
		let mut state = lock(&self.shared.state);
		if state.closed {
			return;
		}
		let entry = state.configs.entry(config.to_owned()).or_default();
		entry.dirty = true;
		if entry.running {
			return;
		}
		if immediate {
			entry.deadline = None;
			Shared::start(&self.shared, &mut state, config);
			return;
		}
		// `time.AfterFunc` or `Timer.Reset`: the debounce restarts.
		entry.deadline = Some(Instant::now() + self.shared.debounce);
		drop(state);
		self.shared.signal.notify_all();
	}

	/// `close`: stops scheduling and waits for the runs in flight. The caller cancels what the
	/// runs observe first, or this waits out a full census.
	pub fn close(&mut self) {
		let mut state = lock(&self.shared.state);
		state.closed = true;
		for entry in state.configs.values_mut() {
			entry.deadline = None;
		}
		self.shared.signal.notify_all();
		while state.loops > 0 {
			state = self.shared.signal.wait(state).unwrap_or_else(PoisonError::into_inner);
		}
		drop(state);
		if let Some(timer) = self.timer.take() {
			let _ = timer.join();
		}
	}
}

impl Drop for Scheduler {
	fn drop(&mut self) {
		self.close();
	}
}

impl Shared {
	/// The debounce timers: `fire` each config whose deadline passed.
	fn timers(self: &Arc<Self>) {
		let mut state = lock(&self.state);
		loop {
			if state.closed {
				return;
			}
			let now = Instant::now();
			let due: Vec<String> = state
				.configs
				.iter()
				.filter(|(_, entry)| entry.deadline.is_some_and(|deadline| deadline <= now))
				.map(|(config, _)| config.clone())
				.collect();
			for config in due {
				// `fire`: an immediate request may have overtaken the timer.
				let Some(entry) = state.configs.get_mut(&config) else {
					continue;
				};
				entry.deadline = None;
				if entry.running || !entry.dirty {
					continue;
				}
				Self::start(self, &mut state, &config);
			}
			let next = state.configs.values().filter_map(|entry| entry.deadline).min();
			state = match next {
				Some(deadline) => {
					let wait = deadline.saturating_duration_since(Instant::now());
					self.signal
						.wait_timeout(state, wait)
						.unwrap_or_else(PoisonError::into_inner)
						.0
				}
				None => self.signal.wait(state).unwrap_or_else(PoisonError::into_inner),
			};
		}
	}

	/// `start`: launches the run loop of `config`. The caller holds the lock.
	fn start(shared: &Arc<Self>, state: &mut State, config: &str) {
		if let Some(entry) = state.configs.get_mut(config) {
			entry.running = true;
		}
		state.loops += 1;
		let shared = Arc::clone(shared);
		let owned = config.to_owned();
		let spawned = std::thread::Builder::new()
			.name("rotor-lsp-census".to_owned())
			.spawn(move || {
				let config = owned;
				loop {
					{
						let mut state = lock(&shared.state);
						let closed = state.closed;
						let entry = state.configs.entry(config.clone()).or_default();
						if closed || !entry.dirty {
							entry.running = false;
							state.loops -= 1;
							drop(state);
							shared.signal.notify_all();
							return;
						}
						entry.dirty = false;
						drop(state);
					}
					// A run that panics must not leave the config running for good, or `close`
					// would wait on it forever.
					let run = std::panic::AssertUnwindSafe(|| (shared.run)(&config));
					let _ = std::panic::catch_unwind(run);
				}
			});
		if spawned.is_err() {
			if let Some(entry) = state.configs.get_mut(config) {
				entry.running = false;
			}
			state.loops -= 1;
		}
	}
}
