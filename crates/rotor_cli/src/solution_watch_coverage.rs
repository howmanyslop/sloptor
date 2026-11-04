//! The physical watches of `build --build --watch` (`cmd/rotor/solution_watch_coverage.go`).
//!
//! A route keeps ownership (a project's root, Rojo directory or config file) apart from the
//! directory the OS watches. One recursive watch covers every route under it, so nested roots
//! share coverage. Paths are canonical for containment, and events are rebased onto the route's
//! logical path before delivery (a symlinked project root reports under the path it was named
//! by).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};

use rotor_compile::solution_watch::SolutionWatchSet;
use rotor_support::gopath;

use crate::fswatch::{Event, Watch, WatchCallback, WatchError, Watcher};
use crate::solution_watch::{WatchKey, WatchKind, solution_watch_keys};

/// `solutionWatchRoute`
#[derive(Clone)]
pub struct Route {
	key: WatchKey,
	/// The key's path, canonical.
	path: String,
	excluded: Vec<String>,
	callback: WatchCallback,
}

/// `solutionWatchDispatch`: the routes one physical watch delivers to. Replacing them never
/// reopens the watch.
#[derive(Default)]
pub struct Dispatch {
	routes: Mutex<Arc<Vec<Route>>>,
}

impl Dispatch {
	/// `set`
	pub fn set(&self, routes: Vec<Route>) {
		*self.routes.lock().unwrap_or_else(PoisonError::into_inner) = Arc::new(routes);
	}

	fn routes(&self) -> Arc<Vec<Route>> {
		Arc::clone(&self.routes.lock().unwrap_or_else(PoisonError::into_inner))
	}

	/// `deliver`: each route gets the events inside it, rebased onto its logical path.
	pub fn deliver(&self, events: &[Event], error: Option<&WatchError>) {
		let routes = self.routes();
		for route in routes.iter() {
			let mut matched = Vec::new();
			for event in events {
				let path = normalize_path(&event.path);
				let mut event = event.clone();
				if route.key.file_watch() {
					if !path_within(&path, &route.path) || !path_within(&route.path, &path) {
						continue;
					}
					event.path.clone_from(&route.key.path);
				} else {
					if directory_is_artifact(&path, &route.excluded) {
						continue;
					}
					if path_within(&path, &route.path) {
						let relative = gopath::rel(&route.path, &path).unwrap_or_default();
						if route.key.kind == WatchKind::PackageDirectory && gopath::dir(&relative) != "." {
							continue;
						}
						event.path = gopath::join(&[&gopath::from_slash(&route.key.path), &relative]);
					} else if path_within(&route.path, &path) {
						// A containing directory moved or disappeared: the route's whole input tree
						// changed, even if only its ancestor is reported.
						event.path.clone_from(&route.key.path);
					} else {
						continue;
					}
				}
				matched.push(event);
			}
			if !matched.is_empty() || error.is_some() {
				(route.callback)(&matched, error);
			}
		}
	}
}

/// `solutionWatchPathWithin`
#[must_use]
pub fn path_within(path: &str, root: &str) -> bool {
	gopath::rel(&normalize_path(root), &normalize_path(path))
		.is_ok_and(|relative| relative != ".." && !relative.starts_with(&format!("..{}", std::path::MAIN_SEPARATOR)))
}

/// `solutionWatchNormalizePath`: clean, OS separators, and NFC on macOS, whose watcher reports NFC
/// even for watches opened with NFD names.
#[must_use]
pub fn normalize_path(path: &str) -> String {
	crate::fswatch::canonicalize_path(&gopath::clean(&gopath::from_slash(path)))
}

/// `solutionWatchCanonicalPath`: symlinks resolved as far as the path exists.
#[must_use]
pub fn canonical_path(path: &str) -> String {
	let path = gopath::clean(&gopath::from_slash(path));
	if let Ok(resolved) = rotor_support::fsutil::eval_symlinks(&path) {
		return normalize_path(&resolved);
	}
	let parent = gopath::dir(&path);
	if parent != path {
		return normalize_path(&gopath::join(&[&canonical_path(&parent), &gopath::base(&path)]));
	}
	normalize_path(&path)
}

/// `solutionWatchDirectoryIsArtifact`: `directory` is, or is inside, one of `artifacts`.
#[must_use]
pub fn directory_is_artifact(directory: &str, artifacts: &[String]) -> bool {
	let directory = normalize_path(directory);
	artifacts.iter().any(|artifact| {
		gopath::rel(&normalize_path(artifact), &directory).is_ok_and(|relative| {
			relative == "." || (relative != ".." && !relative.starts_with(&format!("..{}", std::path::MAIN_SEPARATOR)))
		})
	})
}

/// `solutionArtifactDirectories`: every project's artifact directories, canonical and distinct.
#[must_use]
pub fn artifact_directories(sets: &[SolutionWatchSet]) -> Vec<String> {
	let mut directories: Vec<String> = Vec::new();
	for set in sets {
		for directory in &set.artifact_directories {
			let directory = canonical_path(directory);
			if !directories.contains(&directory) {
				directories.push(directory);
			}
		}
	}
	directories
}

/// `solutionWatchExclusions`: a broad source root ignores generated children, but an explicit
/// source root inside a generated tree stays a declared input.
#[must_use]
pub fn exclusions(key: &WatchKey, artifacts: &[String]) -> Vec<String> {
	if key.kind != WatchKind::Root {
		return artifacts.to_vec();
	}
	let root = canonical_path(&key.path);
	artifacts
		.iter()
		.filter(|artifact| !path_within(&root, artifact))
		.cloned()
		.collect()
}

/// The event handler of a key.
pub type CallbackFactory = Box<dyn Fn(&WatchKey) -> WatchCallback + Send>;

/// `solutionWatches`: physical watches shared between the logical routes.
pub struct SolutionWatches {
	watcher: Arc<dyn Watcher>,
	/// The event handler of a key.
	pub callback: CallbackFactory,
	watches: HashMap<WatchKey, Box<dyn Watch>>,
	dispatch: HashMap<WatchKey, Arc<Dispatch>>,
	sets: Vec<SolutionWatchSet>,
	/// False while a wanted watch failed to open.
	pub armed: bool,
}

impl SolutionWatches {
	/// No watches yet.
	pub fn new(watcher: Arc<dyn Watcher>, callback: CallbackFactory) -> Self {
		Self {
			watcher,
			callback,
			watches: HashMap::new(),
			dispatch: HashMap::new(),
			sets: Vec::new(),
			armed: false,
		}
	}

	/// The open physical watches.
	#[must_use]
	#[cfg(test)]
	pub fn len(&self) -> usize {
		self.watches.len()
	}

	/// Whether no physical watch is open.
	#[must_use]
	#[cfg(test)]
	pub fn is_empty(&self) -> bool {
		self.watches.is_empty()
	}

	/// Whether no route remains.
	#[must_use]
	#[cfg(test)]
	pub fn dispatch_is_empty(&self) -> bool {
		self.dispatch.is_empty()
	}

	fn remove(&mut self, key: &WatchKey) {
		if let Some(dispatch) = self.dispatch.remove(key) {
			dispatch.set(Vec::new());
		}
		if let Some(mut watch) = self.watches.remove(key) {
			watch.close();
		}
	}

	/// `close`
	pub fn close(&mut self) {
		let keys: Vec<WatchKey> = self.watches.keys().chain(self.dispatch.keys()).cloned().collect();
		for key in keys {
			self.remove(&key);
		}
	}

	/// `refresh`: keeps live coverage, updates its routes, and retries missing or terminated
	/// watches. A descendant's watch is replaced only once its ancestor opened.
	#[expect(clippy::too_many_lines, reason = "one function in Go")]
	pub fn refresh(&mut self, sets: &[SolutionWatchSet], dead: &HashSet<WatchKey>) {
		if dead.is_empty() && self.armed && sets == self.sets.as_slice() {
			return;
		}
		// A terminal error belongs to the shared watch: reopen it even when the key that
		// reported it was one of its descendants.
		let physicals: Vec<WatchKey> = self.dispatch.keys().cloned().collect();
		for physical in physicals {
			let Some(dispatch) = self.dispatch.get(&physical) else {
				continue;
			};
			let mut is_dead = dispatch.routes().iter().any(|route| dead.contains(&route.key));
			if physical.kind == WatchKind::Config {
				let parent = gopath::dir(&physical.path);
				for key in dead {
					let (path, recursive) = key.directory();
					let path = canonical_path(&path);
					if !recursive && path_within(&path, &parent) && path_within(&parent, &path) {
						is_dead = true;
					}
				}
			}
			if is_dead {
				self.remove(&physical);
			}
		}

		let artifacts = artifact_directories(sets);
		let mut routes = Vec::new();
		let mut candidates: HashSet<WatchKey> = HashSet::new();
		for key in solution_watch_keys(sets) {
			let mut route = Route {
				path: canonical_path(&key.path),
				callback: (self.callback)(&key),
				excluded: Vec::new(),
				key,
			};
			let kind = if route.key.file_watch() {
				WatchKind::Config
			} else if route.key.kind == WatchKind::PackageDirectory {
				WatchKind::PackageDirectory
			} else {
				route.excluded = exclusions(&route.key, &artifacts);
				WatchKind::Root
			};
			candidates.insert(WatchKey {
				kind,
				project: String::new(),
				path: route.path.clone(),
			});
			routes.push(route);
		}
		let mut ordered: Vec<WatchKey> = candidates.into_iter().collect();
		// Ancestors first. Separate volumes and sibling names stay independent; containment
		// always goes through `rel`.
		ordered.sort_by(|left, right| {
			left.path
				.len()
				.cmp(&right.path.len())
				.then_with(|| left.path.cmp(&right.path))
				.then_with(|| left.kind.cmp(&right.kind))
		});
		let mut wanted: HashSet<WatchKey> = HashSet::new();
		self.armed = true;
		for physical in ordered {
			if physical.kind == WatchKind::Root
				&& wanted
					.iter()
					.any(|selected| selected.kind == WatchKind::Root && path_within(&physical.path, &selected.path))
			{
				continue;
			}
			let matched: Vec<Route> = routes
				.iter()
				.filter(|route| {
					if physical.file_watch() || route.key.file_watch() {
						return physical.file_watch() && route.key.file_watch() && route.path == physical.path;
					}
					if physical.kind == WatchKind::PackageDirectory || route.key.kind == WatchKind::PackageDirectory {
						return physical.kind == route.key.kind && route.path == physical.path;
					}
					path_within(&route.path, &physical.path) || path_within(&physical.path, &route.path)
				})
				.cloned()
				.collect();
			if let Some(dispatch) = self.dispatch.get(&physical) {
				dispatch.set(matched);
			} else {
				let dispatch = Arc::new(Dispatch::default());
				dispatch.set(matched);
				let deliver: WatchCallback = {
					let dispatch = Arc::clone(&dispatch);
					Arc::new(move |events: &[Event], error: Option<&WatchError>| dispatch.deliver(events, error))
				};
				let opened = if physical.file_watch() {
					self.watcher.watch_file(&physical.path, deliver)
				} else {
					self.watcher
						.watch_directory(&physical.path, deliver, physical.kind != WatchKind::PackageDirectory)
				};
				let Ok(watch) = opened else {
					self.armed = false;
					// A failed ancestor must not suppress working child watches.
					continue;
				};
				self.watches.insert(physical.clone(), watch);
				self.dispatch.insert(physical.clone(), dispatch);
			}
			wanted.insert(physical);
		}
		let stale: Vec<WatchKey> = self
			.watches
			.keys()
			.filter(|key| !wanted.contains(*key))
			.cloned()
			.collect();
		for key in stale {
			self.remove(&key);
		}
		self.sets = sets.to_vec();
	}
}

#[cfg(test)]
mod tests;
