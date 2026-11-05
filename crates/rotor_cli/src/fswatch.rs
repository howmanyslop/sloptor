//! The `tsgo/fswatch` surface the solution watcher uses (`solution_watch.rs`), on the `notify`
//! crate.
//!
//! tsrs mirrors no file watcher, so every platform uses `notify`'s recommended backend (`FSEvents` on
//! macOS, inotify on Linux, `ReadDirectoryChangesW` on Windows), or its polling backend under
//! `--use-polling`. On top of it this module keeps fswatch's contract:
//!
//! - **Coalescing** (`event.go`): per watched directory, events are keyed by path; a create after
//!   a delete is an update, and a create then delete in one batch cancels out.
//! - **Debounce** (`debounce.go`): one debouncer per watcher. The first event after 500 ms of quiet
//!   fires at once; later ones wait for 50 ms without a new event.
//! - **Errors**: an overflow (`notify`'s rescan flag) keeps the watch; a removed root or a backend
//!   error terminates it, delivering the error once and dropping its callbacks.
//! - **File watches** share their parent directory's watch and see only their own path.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, Instant};

use rotor_support::gopath;

/// `defaultMinWaitTime`: how long a burst must go quiet before its batch fires.
const MIN_WAIT: Duration = Duration::from_millis(50);
/// `defaultMaxWaitTime`: an event after this much quiet fires at once.
const MAX_WAIT: Duration = Duration::from_millis(500);
/// The polling backend's interval (`--use-polling`), the single-project watcher's floor.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// `EventKind`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EventKind {
	/// A file or directory was created or changed.
	Update,
	/// A file or directory was removed.
	Delete,
}

/// `Event`: one coalesced change.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Event {
	/// What happened.
	pub kind: EventKind,
	/// The changed path, with OS separators.
	pub path: String,
}

impl Event {
	/// An update of `path`.
	#[must_use]
	#[cfg(test)]
	pub fn update(path: impl Into<String>) -> Self {
		Self {
			kind: EventKind::Update,
			path: path.into(),
		}
	}

	/// A deletion of `path`.
	#[must_use]
	#[cfg(test)]
	pub fn delete(path: impl Into<String>) -> Self {
		Self {
			kind: EventKind::Delete,
			path: path.into(),
		}
	}
}

/// A watch's error: `ErrOverflow`, `ErrWatchTerminated`, or another backend failure (also
/// terminal).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WatchError {
	/// Changes were missed; the watch stays live.
	Overflow,
	/// The watch ended, with the reason.
	Terminated(String),
	/// Another failure; the watch ended.
	Failed(String),
}

impl WatchError {
	/// `errors.Is(err, fswatch.ErrOverflow)`
	#[must_use]
	pub const fn is_overflow(&self) -> bool {
		matches!(self, Self::Overflow)
	}
}

impl fmt::Display for WatchError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Overflow => formatter.write_str("fswatch: event overflow; some changes were missed"),
			Self::Terminated(reason) => write!(formatter, "fswatch: watch terminated: {reason}"),
			Self::Failed(reason) => formatter.write_str(reason),
		}
	}
}

/// `WatchCallback`: a batch of events, an error, or both. Never runs concurrently with itself for
/// one watch.
pub type WatchCallback = Arc<dyn Fn(&[Event], Option<&WatchError>) + Send + Sync>;

/// `Watch`: a live watch. Closing it is idempotent.
pub trait Watch: Send {
	/// Stops the watch.
	fn close(&mut self);
}

/// `Watcher`: a file-system watching implementation.
pub trait Watcher: Send + Sync {
	/// `WatchDirectory`: watches `dir`, an absolute path to an existing directory; its direct
	/// children only unless `recursive`.
	///
	/// # Errors
	///
	/// The backend could not watch `dir`.
	fn watch_directory(&self, dir: &str, callback: WatchCallback, recursive: bool) -> Result<Box<dyn Watch>, String>;

	/// `WatchFile`: watches one file, which need not exist, through its parent directory.
	///
	/// # Errors
	///
	/// `path` is not absolute or is a root, or its parent cannot be watched.
	fn watch_file(&self, path: &str, callback: WatchCallback) -> Result<Box<dyn Watch>, String>;
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `canonicalizePath`: NFC on macOS, where `FSEvents` reports whatever form is on disk.
#[must_use]
pub fn canonicalize_path(path: &str) -> String {
	if cfg!(target_os = "macos") {
		return icu_normalizer::ComposingNormalizerBorrowed::new_nfc()
			.normalize(path)
			.into_owned();
	}
	path.to_owned()
}

/// `isDirectChild`: whether `path` is an immediate child of `dir`.
fn is_direct_child(dir: &str, path: &str) -> bool {
	let Some(rest) = path.strip_prefix(dir) else {
		return false;
	};
	let Some(rest) = rest.strip_prefix(['/', std::path::MAIN_SEPARATOR]) else {
		return false;
	};
	!rest.is_empty() && !rest.contains(['/', std::path::MAIN_SEPARATOR])
}

fn is_within(dir: &str, path: &str) -> bool {
	path == dir
		|| path
			.strip_prefix(dir)
			.is_some_and(|rest| rest.starts_with(['/', std::path::MAIN_SEPARATOR]))
		|| dir.ends_with(['/', std::path::MAIN_SEPARATOR]) && path.starts_with(dir)
}

/// `eventEntry`
#[derive(Clone, Copy, Default)]
struct Entry {
	created: bool,
	deleted: bool,
}

/// `eventList`: coalesces a batch by path.
#[derive(Default)]
struct EventList {
	entries: BTreeMap<String, Entry>,
	error: Option<WatchError>,
}

impl EventList {
	fn create(&mut self, path: &str) {
		let entry = self.entries.entry(path.to_owned()).or_default();
		if entry.deleted {
			// A rapid delete and recreate is an update.
			*entry = Entry::default();
		} else {
			entry.created = true;
		}
	}

	fn update(&mut self, path: &str) {
		self.entries.entry(path.to_owned()).or_default();
	}

	fn remove(&mut self, path: &str) {
		self.entries.entry(path.to_owned()).or_default().deleted = true;
	}

	fn set_error(&mut self, error: WatchError) {
		if self.error.is_none() {
			self.error = Some(error);
		}
	}

	fn is_empty(&self) -> bool {
		self.entries.is_empty() && self.error.is_none()
	}

	/// `drain`: the batch, with created-then-deleted entries dropped, and the error.
	fn drain(&mut self) -> (Vec<Event>, Option<WatchError>) {
		let events = std::mem::take(&mut self.entries)
			.into_iter()
			.filter(|(_, entry)| !(entry.created && entry.deleted))
			.map(|(path, entry)| Event {
				kind: if entry.deleted {
					EventKind::Delete
				} else {
					EventKind::Update
				},
				path,
			})
			.collect();
		(events, self.error.take())
	}
}

/// `dirWatch`: one watched directory, its callbacks and its pending batch.
struct DirWatch {
	dir: String,
	recursive: bool,
	state: Mutex<DirWatchState>,
}

#[derive(Default)]
struct DirWatchState {
	callbacks: Vec<(u64, WatchCallback)>,
	events: EventList,
	/// A terminal error was recorded; no more events are taken.
	dead: bool,
}

impl DirWatch {
	/// `triggerCallbacks`: delivers the pending batch to every callback.
	fn trigger_callbacks(&self) {
		let (events, error, callbacks) = {
			let mut state = lock(&self.state);
			if state.callbacks.is_empty() || state.events.is_empty() {
				return;
			}
			let (events, error) = state.events.drain();
			let terminal = error.as_ref().is_some_and(|error| !error.is_overflow());
			let callbacks = if terminal {
				// `notifyError`: a terminal error ends the watch's deliveries.
				std::mem::take(&mut state.callbacks)
			} else {
				state.callbacks.clone()
			};
			(events, error, callbacks)
		};
		for (_, callback) in callbacks {
			let events: Vec<Event> = if self.recursive {
				events.clone()
			} else {
				events
					.iter()
					.filter(|event| is_direct_child(&self.dir, &event.path))
					.cloned()
					.collect()
			};
			if !events.is_empty() || error.is_some() {
				callback(&events, error.as_ref());
			}
		}
	}
}

/// `debounce`: batches the events of one watcher.
#[derive(Default)]
struct Debounce {
	state: Mutex<DebounceState>,
	signal: Condvar,
}

#[derive(Default)]
struct DebounceState {
	notified: bool,
	generation: u64,
	last_fire: Option<Instant>,
	closed: bool,
}

impl Debounce {
	fn trigger(&self) {
		let mut state = lock(&self.state);
		state.notified = true;
		state.generation += 1;
		drop(state);
		self.signal.notify_all();
	}

	fn close(&self) {
		lock(&self.state).closed = true;
		self.signal.notify_all();
	}

	/// The debounce loop: returns once closed.
	fn run(&self, fire: &dyn Fn()) {
		loop {
			let mut state = lock(&self.state);
			while !state.notified && !state.closed {
				state = self.signal.wait(state).unwrap_or_else(PoisonError::into_inner);
			}
			if state.closed {
				return;
			}
			// `notifyIfReady`
			let quiet = state.last_fire.is_none_or(|last| last.elapsed() > MAX_WAIT);
			if !quiet {
				// `coalesceWait`: a new trigger restarts the wait.
				let generation = state.generation;
				let (next, timeout) = self
					.signal
					.wait_timeout_while(state, MIN_WAIT, |state| state.generation == generation && !state.closed)
					.unwrap_or_else(PoisonError::into_inner);
				state = next;
				if state.closed {
					return;
				}
				if !timeout.timed_out() {
					continue;
				}
			}
			state.last_fire = Some(Instant::now());
			state.notified = false;
			drop(state);
			fire();
		}
	}
}

/// What the backend has been asked to watch: a path and whether recursively.
type Registrations = HashMap<String, bool>;

struct Shared {
	/// Watched directories by (directory, recursive).
	watches: Mutex<HashMap<(String, bool), Arc<DirWatch>>>,
	next_id: Mutex<u64>,
	debounce: Debounce,
	backend: Mutex<Option<(Box<dyn notify::Watcher + Send>, Registrations)>>,
	polling: bool,
}

/// The `notify`-backed [`Watcher`] (`fswatch.Default()`).
pub struct NotifyWatcher {
	shared: Arc<Shared>,
}

impl NotifyWatcher {
	/// The platform's recommended backend, or polling.
	#[must_use]
	pub fn new(polling: bool) -> Self {
		let shared = Arc::new(Shared {
			watches: Mutex::new(HashMap::new()),
			next_id: Mutex::new(0),
			debounce: Debounce::default(),
			backend: Mutex::new(None),
			polling,
		});
		let weak = Arc::downgrade(&shared);
		let spawned = std::thread::Builder::new()
			.name("rotor-fswatch-debounce".to_owned())
			.spawn(move || {
				let Some(shared) = weak.upgrade() else {
					return;
				};
				let fire = {
					let weak = Arc::downgrade(&shared);
					move || {
						if let Some(shared) = weak.upgrade() {
							shared.fire();
						}
					}
				};
				// The loop holds the shared state until the watcher's drop closes it.
				shared.debounce.run(&fire);
			});
		drop(spawned);
		Self { shared }
	}
}

impl Drop for NotifyWatcher {
	fn drop(&mut self) {
		self.shared.debounce.close();
		*lock(&self.shared.backend) = None;
	}
}

impl Shared {
	fn fire(&self) {
		let watches: Vec<Arc<DirWatch>> = lock(&self.watches).values().cloned().collect();
		for watch in watches {
			watch.trigger_callbacks();
		}
	}

	/// The registrations the watched directories need.
	fn wanted(&self) -> Registrations {
		let mut wanted = Registrations::new();
		for (dir, recursive) in lock(&self.watches).keys() {
			let entry = wanted.entry(dir.clone()).or_insert(false);
			*entry |= *recursive;
		}
		wanted
	}

	/// Brings the backend's registrations in line with the watched directories.
	#[expect(
		clippy::significant_drop_tightening,
		reason = "registrations change under the backend lock"
	)]
	fn sync_backend(self: &Arc<Self>) -> Result<(), String> {
		let wanted = self.wanted();
		let mut backend = lock(&self.backend);
		if backend.is_none() {
			let weak = Arc::downgrade(self);
			let handler = move |event: notify::Result<notify::Event>| {
				if let Some(shared) = weak.upgrade() {
					shared.handle(event);
				}
			};
			let watcher: Box<dyn notify::Watcher + Send> = if self.polling {
				Box::new(
					notify::PollWatcher::new(handler, notify::Config::default().with_poll_interval(POLL_INTERVAL))
						.map_err(|error| error.to_string())?,
				)
			} else {
				Box::new(notify::recommended_watcher(handler).map_err(|error| error.to_string())?)
			};
			*backend = Some((watcher, Registrations::new()));
		}
		let Some((watcher, registered)) = backend.as_mut() else {
			return Ok(());
		};
		let stale: Vec<String> = registered
			.iter()
			.filter(|(path, recursive)| wanted.get(*path) != Some(*recursive))
			.map(|(path, _)| path.clone())
			.collect();
		for path in stale {
			let _ = watcher.unwatch(Path::new(&path));
			registered.remove(&path);
		}
		let mut failure = None;
		for (path, recursive) in &wanted {
			if registered.contains_key(path) {
				continue;
			}
			let mode = if *recursive {
				notify::RecursiveMode::Recursive
			} else {
				notify::RecursiveMode::NonRecursive
			};
			match watcher.watch(Path::new(path), mode) {
				Ok(()) => {
					registered.insert(path.clone(), *recursive);
				}
				Err(error) => failure = Some(error.to_string()),
			}
		}
		failure.map_or(Ok(()), Err)
	}

	/// The watched directories `path` falls in.
	fn watches_of(&self, path: &str) -> Vec<Arc<DirWatch>> {
		lock(&self.watches)
			.values()
			.filter(|watch| is_within(&watch.dir, path) || watch.dir.starts_with(path) && is_within(path, &watch.dir))
			.cloned()
			.collect()
	}

	fn record(&self, path: &str, kind: Change, touched: &mut Vec<Arc<DirWatch>>) {
		let path = canonicalize_path(&gopath::clean(path));
		for watch in self.watches_of(&path) {
			let mut state = lock(&watch.state);
			if state.dead {
				continue;
			}
			if is_within(&watch.dir, &path) {
				match kind {
					Change::Create => state.events.create(&path),
					Change::Update => state.events.update(&path),
					Change::Remove => state.events.remove(&path),
				}
			}
			// The watched directory itself, or one containing it, went away.
			if is_within(&path, &watch.dir) && !Path::new(&watch.dir).is_dir() {
				state.events.remove(&watch.dir);
				state
					.events
					.set_error(WatchError::Terminated("watched directory removed".to_owned()));
				state.dead = true;
			}
			drop(state);
			if !touched.iter().any(|seen| Arc::ptr_eq(seen, &watch)) {
				touched.push(watch);
			}
		}
	}

	fn fail(&self, paths: &[String], error: &WatchError, touched: &mut Vec<Arc<DirWatch>>) {
		let watches: Vec<Arc<DirWatch>> = if paths.is_empty() {
			lock(&self.watches).values().cloned().collect()
		} else {
			paths.iter().flat_map(|path| self.watches_of(path)).collect()
		};
		for watch in watches {
			let mut state = lock(&watch.state);
			if state.dead {
				continue;
			}
			state.events.set_error(error.clone());
			if !error.is_overflow() {
				state.dead = true;
			}
			drop(state);
			if !touched.iter().any(|seen| Arc::ptr_eq(seen, &watch)) {
				touched.push(watch);
			}
		}
	}

	/// The backend's event handler.
	fn handle(&self, event: notify::Result<notify::Event>) {
		let mut touched = Vec::new();
		match event {
			Ok(event) => {
				let paths: Vec<String> = event
					.paths
					.iter()
					.map(|path| path.to_string_lossy().into_owned())
					.collect();
				if event.need_rescan() {
					self.fail(&paths, &WatchError::Overflow, &mut touched);
				}
				for (index, path) in paths.iter().enumerate() {
					if let Some(change) = change_of(event.kind, index, paths.len(), path) {
						self.record(path, change, &mut touched);
					}
				}
			}
			Err(error) => {
				let paths: Vec<String> = error
					.paths
					.iter()
					.map(|path| path.to_string_lossy().into_owned())
					.collect();
				self.fail(&paths, &WatchError::Failed(error.to_string()), &mut touched);
			}
		}
		if !touched.is_empty() {
			self.debounce.trigger();
		}
	}
}

#[derive(Clone, Copy)]
enum Change {
	Create,
	Update,
	Remove,
}

/// What a `notify` event says happened to its `index`th path.
fn change_of(kind: notify::EventKind, index: usize, count: usize, path: &str) -> Option<Change> {
	use notify::EventKind as Kind;
	use notify::event::{ModifyKind, RenameMode};
	let exists = || std::fs::symlink_metadata(path).is_ok();
	match kind {
		Kind::Access(_) => None,
		Kind::Modify(ModifyKind::Name(RenameMode::Both)) if count == 2 => {
			Some(if index == 0 { Change::Remove } else { Change::Create })
		}
		// FSEvents folds a path's recent history into one event's flags (created and removed),
		// which `notify` reports as a create, then a remove: the path's existence decides, as it
		// does for Go's ambiguous flag combinations. So it does for a rename seen from one side
		// only and an event of no particular kind.
		Kind::Create(_) | Kind::Remove(_) | Kind::Modify(ModifyKind::Name(_)) | Kind::Any | Kind::Other => {
			Some(if exists() { Change::Create } else { Change::Remove })
		}
		Kind::Modify(_) => Some(if exists() { Change::Update } else { Change::Remove }),
	}
}

struct NotifyWatch {
	shared: Weak<Shared>,
	key: (String, bool),
	id: u64,
	closed: bool,
}

impl Watch for NotifyWatch {
	fn close(&mut self) {
		if std::mem::replace(&mut self.closed, true) {
			return;
		}
		let Some(shared) = self.shared.upgrade() else {
			return;
		};
		let removed = {
			let mut watches = lock(&shared.watches);
			let last = watches.get(&self.key).is_some_and(|watch| {
				let mut state = lock(&watch.state);
				state.callbacks.retain(|(id, _)| *id != self.id);
				state.callbacks.is_empty()
			});
			if last {
				watches.remove(&self.key);
			}
			last
		};
		if removed {
			let _ = shared.sync_backend();
		}
	}
}

impl Drop for NotifyWatch {
	fn drop(&mut self) {
		self.close();
	}
}

impl Watcher for NotifyWatcher {
	#[expect(
		clippy::significant_drop_tightening,
		reason = "a watch's state is set up under the map lock"
	)]
	fn watch_directory(&self, dir: &str, callback: WatchCallback, recursive: bool) -> Result<Box<dyn Watch>, String> {
		let dir = gopath::clean(dir);
		if !gopath::is_abs(&dir) {
			return Err("fswatch: path must be absolute".to_owned());
		}
		let dir = canonicalize_path(&dir);
		if !Path::new(&dir).is_dir() {
			return Err(format!("fswatch: watch {dir}: not a directory"));
		}
		let key = (dir.clone(), recursive);
		let id = {
			let mut next = lock(&self.shared.next_id);
			*next += 1;
			*next
		};
		let created = {
			let mut watches = lock(&self.shared.watches);
			let watch = watches.entry(key.clone()).or_insert_with(|| {
				Arc::new(DirWatch {
					dir,
					recursive,
					state: Mutex::new(DirWatchState::default()),
				})
			});
			let mut state = lock(&watch.state);
			let fresh = state.callbacks.is_empty();
			if state.dead {
				// A terminated watch of the same directory is replaced.
				*state = DirWatchState::default();
			}
			state.callbacks.push((id, callback));
			fresh
		};
		let mut watch = NotifyWatch {
			shared: Arc::downgrade(&self.shared),
			key,
			id,
			closed: false,
		};
		if created && let Err(error) = self.shared.sync_backend() {
			watch.close();
			return Err(error);
		}
		Ok(Box::new(watch))
	}

	fn watch_file(&self, path: &str, callback: WatchCallback) -> Result<Box<dyn Watch>, String> {
		let path = gopath::clean(path);
		if !gopath::is_abs(&path) {
			return Err("fswatch: path must be absolute".to_owned());
		}
		let path = canonicalize_path(&path);
		let dir = gopath::dir(&path);
		if dir == path {
			return Err("fswatch: cannot watch a root path".to_owned());
		}
		self.watch_directory(&dir, file_callback(path, callback), false)
	}
}

/// `fileCallback`: only events for `target`; errors always pass.
fn file_callback(target: String, callback: WatchCallback) -> WatchCallback {
	Arc::new(move |events: &[Event], error: Option<&WatchError>| {
		let filtered: Vec<Event> = events.iter().filter(|event| event.path == target).cloned().collect();
		if !filtered.is_empty() || error.is_some() {
			callback(&filtered, error);
		}
	})
}

#[cfg(test)]
mod tests {
	use super::{EventKind, EventList, is_direct_child};

	#[test]
	fn coalescing_follows_event_go() {
		let mut list = EventList::default();
		list.remove("/a");
		list.create("/a");
		list.create("/b");
		list.remove("/b");
		list.update("/c");
		list.remove("/d");
		let (events, error) = list.drain();
		assert!(error.is_none());
		let got: Vec<(EventKind, &str)> = events.iter().map(|event| (event.kind, event.path.as_str())).collect();
		assert_eq!(
			got,
			[
				(EventKind::Update, "/a"),
				(EventKind::Update, "/c"),
				(EventKind::Delete, "/d")
			]
		);
	}

	#[test]
	fn direct_children_only() {
		assert!(is_direct_child("/a", "/a/b"));
		assert!(!is_direct_child("/a", "/a"));
		assert!(!is_direct_child("/a", "/a/b/c"));
		assert!(!is_direct_child("/a", "/ab"));
	}
}
