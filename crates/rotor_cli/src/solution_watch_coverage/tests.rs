//! `solution_watch_coverage_test.go`

#![expect(
	clippy::significant_drop_tightening,
	reason = "each check reads the recorded events under one lock"
)]
#![expect(clippy::similar_names, reason = "Go's names: watcher, watches")]

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rotor_compile::solution_watch::SolutionWatchSet;

use super::{SolutionWatches, canonical_path};
use crate::fswatch::{Event, EventKind, WatchCallback, WatchError};
#[cfg(unix)]
use crate::solution_watch::solution_watched_file_count;
use crate::solution_watch::{SolutionWatchEvents, WatchKey, WatchKind};
use crate::test_fixtures::{FakeWatcher, s, temp_dir};

type Recorded = Arc<Mutex<SolutionWatchEvents>>;

/// `recordingSolutionWatches`: routes each key's events into `events` by kind.
fn recording(watcher: &FakeWatcher, events: &Recorded) -> SolutionWatches {
	let events = Arc::clone(events);
	SolutionWatches::new(
		Arc::new(watcher.clone()),
		Box::new(move |key: &WatchKey| -> WatchCallback {
			let (key, events) = (key.clone(), Arc::clone(&events));
			Arc::new(move |batch: &[Event], error: Option<&WatchError>| {
				let mut events = events.lock().expect("events");
				match key.kind {
					WatchKind::Root => events.add(&key, batch, error),
					WatchKind::Rojo => events.add_rojo(&key, batch, error),
					_ => events.add_config(&key, batch, error),
				}
			})
		}),
	)
}

fn reset(events: &Recorded) {
	*events.lock().expect("events") = SolutionWatchEvents::default();
}

fn projects(events: &Recorded) -> HashSet<String> {
	events.lock().expect("events").projects.clone()
}

fn set_of(names: &[&str]) -> HashSet<String> {
	names.iter().map(|name| (*name).to_owned()).collect()
}

fn update(path: impl AsRef<Path>) -> Event {
	Event::update(s(path))
}

fn strings(paths: &[&Path]) -> Vec<String> {
	paths.iter().map(s).collect()
}

/// Closes the watches when the test ends (`t.Cleanup(watches.close)`).
struct Closing(SolutionWatches);

impl Drop for Closing {
	fn drop(&mut self) {
		self.0.close();
	}
}

fn none() -> HashSet<WatchKey> {
	HashSet::new()
}

#[test]
fn solution_watch_shared_coverage_routes_every_owner() {
	let (_temp, root) = temp_dir();
	let source = root.join("src");
	let nested = source.join("nested");
	let (first, second, rojo) = ("first", "second", "assets");
	let sets = [
		SolutionWatchSet {
			project_path: first.to_owned(),
			root_dirs: strings(&[&source]),
			ts_config_paths: strings(&[&root.join("first.json")]),
			..SolutionWatchSet::default()
		},
		SolutionWatchSet {
			project_path: second.to_owned(),
			root_dirs: strings(&[&nested]),
			..SolutionWatchSet::default()
		},
		SolutionWatchSet {
			project_path: rojo.to_owned(),
			rojo_directories: strings(&[&root, &source, &nested]),
			..SolutionWatchSet::default()
		},
	];
	let watcher = FakeWatcher::default();
	let events = Recorded::default();
	let mut watches = Closing(recording(&watcher, &events));
	watches.0.refresh(&sets, &none());
	assert_eq!(
		watches.0.len(),
		2,
		"subscriptions, want one recursive root and one config"
	);
	assert_eq!(watcher.counts(&source).0, 0, "nested source opened subscriptions");
	watcher.deliver(&root, &[update(nested.join("file.ts"))], None);
	assert_eq!(projects(&events), set_of(&[first, second]), "source owners");
	{
		let recorded = events.lock().expect("events");
		assert!(
			recorded.assets.is_empty() && recorded.configs.is_empty(),
			"source event leaked into Rojo/config routes: {recorded:?}"
		);
	}
	for name in ["data.json", "folder", "icon.png"] {
		reset(&events);
		let path = s(nested.join(name));
		watcher.deliver(&root, &[Event::delete(&path)], None);
		let recorded = events.lock().expect("events");
		for owner in [first, second, rojo] {
			assert_eq!(
				recorded.assets.get(owner).and_then(|assets| assets.get(&path)),
				Some(&true),
				"{name}: owner {owner} did not receive deletion: {:?}",
				recorded.assets
			);
		}
		let want = name != "icon.png";
		assert_eq!(
			recorded.projects.len() == 2,
			want,
			"{name}: invalidated projects = {:?}",
			recorded.projects
		);
	}
	reset(&events);
	watcher.deliver(&root, &[update(root.join("elsewhere.ts"))], None);
	assert!(
		projects(&events).is_empty(),
		"unrelated source invalidated projects: {:?}",
		projects(&events)
	);
	reset(&events);
	let config = root.join("first.json");
	watcher.deliver(&config, &[update(&config)], None);
	assert!(
		events.lock().expect("events").configs.contains(&s(&config)),
		"config event missing"
	);

	// Reconfigure ownership without changing the shared physical root.
	watches.0.refresh(&sets[1..], &none());
	reset(&events);
	watcher.deliver(&root, &[update(nested.join("file.ts"))], None);
	assert_eq!(projects(&events), set_of(&[second]), "owners after removal");
	assert_eq!(watcher.counts(&root), (1, 0), "retained coverage (opens, closes)");
}

#[test]
fn solution_watch_shared_coverage_excludes_solution_artifacts() {
	let (_temp, root) = temp_dir();
	let out = root.join("game/out");
	let include = root.join("game/include");
	let cache = root.join("game/.rotor");
	let external = root.join("external-assets");
	let sets = [
		SolutionWatchSet {
			project_path: "game".to_owned(),
			root_dirs: strings(&[&root.join("game/src")]),
			artifact_directories: strings(&[&out, &include, &cache]),
			..SolutionWatchSet::default()
		},
		SolutionWatchSet {
			project_path: "logger".to_owned(),
			rojo_directories: strings(&[&root, &out, &out.join("nested"), &include, &cache, &external]),
			rojo_configs: strings(&[&out.join("generated.project.json")]),
			..SolutionWatchSet::default()
		},
		// Explicit source roots remain inputs, including generated dependencies.
		SolutionWatchSet {
			project_path: "generated-consumer".to_owned(),
			root_dirs: strings(&[&out]),
			..SolutionWatchSet::default()
		},
		SolutionWatchSet {
			project_path: "workspace".to_owned(),
			root_dirs: strings(&[&root]),
			..SolutionWatchSet::default()
		},
	];
	let watcher = FakeWatcher::default();
	let events = Recorded::default();
	let mut watches = Closing(recording(&watcher, &events));
	watches.0.refresh(&sets, &none());
	assert_eq!(watches.0.len(), 1, "subscriptions, want one recursive root");
	for artifact in [&out, &include, &cache] {
		reset(&events);
		watcher.deliver(&root, &[update(artifact.join("generated.json"))], None);
		let recorded = events.lock().expect("events");
		assert!(
			recorded
				.assets
				.get("logger")
				.is_none_or(std::collections::HashMap::is_empty)
				&& recorded.configs.is_empty(),
			"artifact {} reached inherited Rojo: {recorded:?}",
			artifact.display()
		);
	}
	reset(&events);
	watcher.deliver(&root, &[update(out.join("input.d.ts"))], None);
	assert_eq!(
		projects(&events),
		set_of(&["generated-consumer"]),
		"generated input, want only its explicit owner"
	);
	reset(&events);
	let asset = s(external.join("icon.png"));
	watcher.deliver(&root, &[Event::update(&asset)], None);
	let recorded = events.lock().expect("events");
	assert!(
		recorded
			.assets
			.get("logger")
			.is_some_and(|assets| assets.contains_key(&asset)),
		"external asset was filtered: {:?}",
		recorded.assets
	);
}

#[test]
fn solution_watch_shared_coverage_recovery() {
	let (_temp, root) = temp_dir();
	let child = root.join("child");
	let sets = [
		SolutionWatchSet {
			project_path: "parent".to_owned(),
			root_dirs: strings(&[&root]),
			..SolutionWatchSet::default()
		},
		SolutionWatchSet {
			project_path: "child".to_owned(),
			root_dirs: strings(&[&child]),
			..SolutionWatchSet::default()
		},
	];
	let watcher = FakeWatcher::default();
	let root_key = rotor_support::gopath::to_slash(&s(&root)).into_owned();
	watcher.set_fail(&root_key, true);
	let events = Recorded::default();
	let mut watches = Closing(recording(&watcher, &events));
	watches.0.refresh(&sets, &none());
	assert!(
		!watches.0.armed && watches.0.len() == 1,
		"failed parent: armed {} subscriptions {}",
		watches.0.armed,
		watches.0.len()
	);
	watcher.deliver(&child, &[update(child.join("file.ts"))], None);
	assert_eq!(
		projects(&events).len(),
		2,
		"fallback coverage lost owners: {:?}",
		projects(&events)
	);
	watcher.clear_fail(&root_key);
	watches.0.refresh(&sets, &none());
	assert!(
		watches.0.armed && watches.0.len() == 1,
		"retry: armed {} subscriptions {}",
		watches.0.armed,
		watches.0.len()
	);
	assert_eq!(watcher.counts(&child).1, 1, "fallback child closes");
	reset(&events);
	watcher.deliver(&root, &[], Some(&WatchError::Overflow));
	let dead = events.lock().expect("events").dead.clone();
	watches.0.refresh(&sets, &dead);
	{
		let recorded = events.lock().expect("events");
		assert!(
			recorded.projects.len() == 2 && recorded.dead.is_empty(),
			"overflow routing: {recorded:?}"
		);
	}
	assert_eq!(
		watcher.counts(&root),
		(2, 0),
		"overflow reopened coverage (opens, closes)"
	);
	for error in [
		WatchError::Terminated(String::new()),
		WatchError::Failed("backend error".to_owned()),
	] {
		reset(&events);
		watcher.deliver(&root, &[], Some(&error));
		let dead = {
			let recorded = events.lock().expect("events");
			assert!(
				recorded.projects.len() == 2 && recorded.dead.len() == 2,
				"terminal error routing: {recorded:?}"
			);
			recorded.dead.clone()
		};
		watches.0.refresh(&sets, &dead);
	}
	assert_eq!(watcher.counts(&root), (4, 2), "terminal recovery (opens, closes)");
	// Deleting and recreating a child root under live ancestor coverage does not drop that
	// child's subscription or the parent's ownership.
	for kind in [EventKind::Delete, EventKind::Update] {
		reset(&events);
		watcher.deliver(&root, &[Event { kind, path: s(&child) }], None);
		let dead = {
			let recorded = events.lock().expect("events");
			assert_eq!(
				recorded.projects.len(),
				2,
				"root event {kind:?} lost owners: {recorded:?}"
			);
			recorded.dead.clone()
		};
		watches.0.refresh(&sets, &dead);
	}
	watches.0.close();
	assert!(
		watches.0.is_empty() && watches.0.dispatch_is_empty(),
		"close retained subscriptions or routes"
	);
	reset(&events);
	watcher.deliver(&root, &[], Some(&WatchError::Terminated(String::new())));
	assert!(projects(&events).is_empty(), "closed subscription retained callbacks");
}

#[test]
fn solution_watch_canonical_missing_path() {
	let (_temp, root) = temp_dir();
	let path = root.join("missing/nested/src");
	let canonical = rotor_support::fsutil::real_path(&root).expect("canonical");
	assert_eq!(
		canonical_path(&s(&path)),
		s(canonical.join("missing").join("nested").join("src")),
		"canonical missing path"
	);
}

#[test]
fn solution_watch_shared_coverage_dotted_root_removal() {
	for containing_directory in [false, true] {
		let name = if containing_directory {
			"containing directory"
		} else {
			"source root"
		};
		let (_temp, root) = temp_dir();
		let group = root.join("group.v1");
		let source = group.join("src.v1");
		std::fs::create_dir_all(&source).expect("mkdir");
		let sets = [
			SolutionWatchSet {
				project_path: "assets".to_owned(),
				rojo_directories: strings(&[&root]),
				..SolutionWatchSet::default()
			},
			SolutionWatchSet {
				project_path: "source".to_owned(),
				root_dirs: strings(&[&source]),
				..SolutionWatchSet::default()
			},
			SolutionWatchSet {
				project_path: "sibling".to_owned(),
				root_dirs: strings(&[&root.join("sibling")]),
				..SolutionWatchSet::default()
			},
		];
		let watcher = FakeWatcher::default();
		let events = Recorded::default();
		let mut watches = Closing(recording(&watcher, &events));
		watches.0.refresh(&sets, &none());
		assert_eq!(
			watches.0.len(),
			1,
			"{name}: subscriptions, want shared ancestor coverage"
		);
		let removed = if containing_directory { &group } else { &source };
		std::fs::rename(removed, root.join("moved")).expect("rename");
		// The ancestor watch remains live: the backend reports a directory deletion with no
		// terminal error for the coalesced source route.
		watcher.deliver(&root, &[Event::delete(s(removed))], None);
		let dead = {
			let recorded = events.lock().expect("events");
			assert_eq!(
				recorded.projects,
				set_of(&["source"]),
				"{name}: removed dotted root invalidated, want only its source owner"
			);
			let key = WatchKey::new(WatchKind::Root, "source", &s(&source));
			assert!(
				recorded.dead.contains(&key),
				"{name}: removed source route was not rearmed: {:?}",
				recorded.dead
			);
			recorded.dead.clone()
		};
		watches.0.refresh(&sets, &dead);
		assert_eq!(
			watcher.counts(&root),
			(2, 1),
			"{name}: rearmed coverage (opens, closes)"
		);
		std::fs::create_dir_all(&source).expect("mkdir");
		for path in [source.clone(), source.join("file.ts")] {
			reset(&events);
			watcher.deliver(&root, &[update(&path)], None);
			assert!(
				projects(&events).contains("source"),
				"{name}: recreated dotted root lost event {}: {:?}",
				path.display(),
				projects(&events)
			);
		}
	}
}

#[cfg(unix)]
#[test]
fn solution_watch_shared_coverage_symlink() {
	let (_temp, root) = temp_dir();
	let real = root.join("real");
	std::fs::create_dir_all(real.join("src")).expect("mkdir");
	let alias = root.join("alias");
	std::os::unix::fs::symlink(&real, &alias).expect("symlink");
	let sets = [
		SolutionWatchSet {
			project_path: "real".to_owned(),
			root_dirs: strings(&[&real]),
			artifact_directories: strings(&[&real.join("out")]),
			..SolutionWatchSet::default()
		},
		SolutionWatchSet {
			project_path: "alias".to_owned(),
			root_dirs: strings(&[&alias.join("src")]),
			rojo_configs: strings(&[&alias.join("out/generated.project.json")]),
			..SolutionWatchSet::default()
		},
	];
	let watcher = FakeWatcher::default();
	let events = Recorded::default();
	let mut watches = Closing(recording(&watcher, &events));
	watches.0.refresh(&sets, &none());
	assert_eq!(watches.0.len(), 1, "symlink subscriptions, want one canonical root");
	watcher.deliver(&real, &[update(real.join("src/data.json"))], None);
	let recorded = events.lock().expect("events");
	assert_eq!(recorded.projects.len(), 2, "symlink owners = {:?}", recorded.projects);
	assert!(
		recorded
			.assets
			.get("alias")
			.is_some_and(|assets| assets.contains_key(&s(alias.join("src/data.json")))),
		"symlink event was not rebased: {:?}",
		recorded.assets
	);
}

fn nfc(text: &str) -> String {
	icu_normalizer::ComposingNormalizerBorrowed::new_nfc()
		.normalize(text)
		.into_owned()
}

#[test]
fn solution_watch_shared_coverage_unicode_paths() {
	let (_temp, root) = temp_dir();
	let decomposed = root.join("cafe\u{301}");
	let source = decomposed.join("src");
	let artifact = decomposed.join("out");
	let config = decomposed.join("tsconfig.json");
	std::fs::create_dir_all(&source).expect("mkdir");
	std::fs::write(&config, "{}").expect("write");
	let sets = [
		SolutionWatchSet {
			project_path: "assets".to_owned(),
			rojo_directories: strings(&[&root]),
			artifact_directories: strings(&[&artifact]),
			..SolutionWatchSet::default()
		},
		SolutionWatchSet {
			project_path: "source".to_owned(),
			root_dirs: strings(&[&source]),
			ts_config_paths: strings(&[&config]),
			..SolutionWatchSet::default()
		},
	];
	let watcher = FakeWatcher::default();
	let events = Recorded::default();
	let mut watches = Closing(recording(&watcher, &events));
	watches.0.refresh(&sets, &none());
	let want_equivalent = cfg!(target_os = "macos");
	let canonical_source = canonical_path(&s(&source));
	assert_eq!(
		nfc(&canonical_source) == canonical_source,
		want_equivalent,
		"canonical source {canonical_source:?}, want NFC={want_equivalent}"
	);
	// FSEvents delivers NFC even when the configured input root uses NFD.
	let composed_source = Path::new(&nfc(&canonical_source)).to_path_buf();
	watcher.deliver(&root, &[update(composed_source.join("file.ts"))], None);
	assert_eq!(
		projects(&events).contains("source"),
		want_equivalent,
		"NFC source invalidated"
	);
	reset(&events);
	watcher.deliver(&root, &[update(composed_source.join("icon.png"))], None);
	{
		let recorded = events.lock().expect("events");
		let rebased = recorded
			.assets
			.get("source")
			.is_some_and(|assets| assets.contains_key(&s(source.join("icon.png"))));
		assert_eq!(
			rebased, want_equivalent,
			"NFC source asset rebased: {:?}",
			recorded.assets
		);
	}
	reset(&events);
	let canonical_config = canonical_path(&s(&config));
	watcher.deliver(&canonical_config, &[Event::delete(nfc(&canonical_config))], None);
	{
		let recorded = events.lock().expect("events");
		assert_eq!(
			recorded.configs.contains(&s(&config)),
			want_equivalent,
			"NFC config reloaded"
		);
		assert_eq!(
			!recorded.dead.is_empty(),
			want_equivalent,
			"NFC config deletion lost watch recovery: {:?}",
			recorded.dead
		);
	}
	reset(&events);
	let artifact_path = Path::new(&nfc(&canonical_path(&s(&artifact)))).join("generated.png");
	watcher.deliver(&root, &[update(&artifact_path)], None);
	let delivered = events
		.lock()
		.expect("events")
		.assets
		.get("assets")
		.is_some_and(|assets| !assets.is_empty());
	assert_ne!(
		delivered, want_equivalent,
		"NFC artifact delivered, want {}",
		!want_equivalent
	);
}

#[cfg(unix)]
#[test]
fn solution_watched_file_count_canonicalizes_aliases() {
	let (_temp, root) = temp_dir();
	let real = root.join("real");
	for directory in ["src", "out"] {
		std::fs::create_dir_all(real.join(directory)).expect("mkdir");
	}
	for path in ["src/main.ts", "out/generated.luau", "tsconfig.json"] {
		std::fs::write(real.join(path), "fixture\n").expect("write");
	}
	let alias = root.join("alias");
	std::os::unix::fs::symlink(&real, &alias).expect("symlink");
	let mut sets = vec![
		SolutionWatchSet {
			project_path: "source".to_owned(),
			root_dirs: strings(&[&alias]),
			ts_config_paths: strings(&[&alias.join("tsconfig.json")]),
			artifact_directories: strings(&[&real.join("out")]),
			..SolutionWatchSet::default()
		},
		SolutionWatchSet {
			project_path: "rojo".to_owned(),
			rojo_directories: strings(&[&real]),
			rojo_configs: strings(&[&real.join("tsconfig.json")]),
			..SolutionWatchSet::default()
		},
	];
	assert_eq!(
		solution_watched_file_count(&sets),
		2,
		"aliased count, want one source and one config, excluding generated output"
	);
	sets.push(SolutionWatchSet {
		project_path: "generated-consumer".to_owned(),
		root_dirs: strings(&[&alias.join("out")]),
		..SolutionWatchSet::default()
	});
	assert_eq!(
		solution_watched_file_count(&sets),
		3,
		"count with explicit generated input, want source, config and declared generated input"
	);
}
