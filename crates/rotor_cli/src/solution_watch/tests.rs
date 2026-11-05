//! `solution_watch_test.go` and `package_watch_test.go`

#![expect(clippy::similar_names, reason = "Go's names: watcher, watches")]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rotor_compile::solution_watch::SolutionWatchSet;
use rotor_support::gopath;

use super::{
	CompileGate, SolutionWatchEvents, WatchKey, WatchKind, run_build_solution_watch_loop, solution_watch_directories,
};
use crate::build::{BuildArgs, Outcome, reload_options};
use crate::fswatch::{Event, WatchError, Watcher};
use crate::options::MergedOptions;
use crate::solution_watch_coverage::SolutionWatches;
use crate::test_fixtures::{
	EventStream, FakeWatcher, NO_LIB_GLOBAL_STUBS, add_type_roots, buildable_project, concurrency_solution, must_write,
	s,
};
use crate::watch::{BuildWatchReporter, Stop};
use crate::watch_json::{JsonBuildReporter, WatchEventWriter};

#[test]
fn compile_gate() {
	let (started, on_start) = mpsc::channel();
	let (release, on_release) = mpsc::channel::<()>();
	let transcript = Arc::new(Mutex::new(Vec::new()));
	let gate = Arc::new(CompileGate::default());
	let server = {
		let (gate, transcript) = (Arc::clone(&gate), Arc::clone(&transcript));
		std::thread::spawn(move || {
			gate.serve(&mut || {
				transcript.lock().expect("transcript").push("start");
				let _ = started.send(());
				let _ = on_release.recv();
				transcript.lock().expect("transcript").push("end");
			});
		})
	};

	gate.trigger();
	on_start.recv().expect("started");
	gate.trigger();
	gate.trigger();
	release.send(()).expect("release");
	on_start.recv().expect("started");
	release.send(()).expect("release");
	gate.drain();
	gate.close();
	server.join().expect("gate thread");

	assert_eq!(
		*transcript.lock().expect("transcript"),
		["start", "end", "start", "end"],
		"gate transcript"
	);
}

#[test]
fn solution_watch_rojo_directory_asset_event() {
	let project = "/project/tsconfig.json";
	let asset_path = "/project/assets/nested/icon.png";
	let config_path = "/project/assets/nested/subtree.project.json";
	let mut events = SolutionWatchEvents::default();
	let set = SolutionWatchSet {
		root_dirs: vec!["/project/src".to_owned()],
		rojo_directories: vec!["/project/assets".to_owned()],
		artifact_directories: vec![
			"/project/out".to_owned(),
			"/project/include".to_owned(),
			"/project/.rotor".to_owned(),
		],
		..SolutionWatchSet::default()
	};

	events.add_rojo(
		&WatchKey::new(WatchKind::Rojo, project, "/project/assets"),
		&[
			Event::delete(asset_path),
			Event::update(config_path),
			Event::update("/project/assets/nested/source.ts"),
		],
		None,
	);

	assert_eq!(
		solution_watch_directories(&set),
		["/project/src", "/project/assets"],
		"solutionWatchDirectories(), want root and Rojo directories"
	);
	assert_eq!(
		events.assets.get(project).and_then(|assets| assets.get(asset_path)),
		Some(&true),
		"asset event = {:?}, want deleted Rojo-directory asset",
		events.assets
	);
	assert!(
		events.configs.contains(config_path),
		"configs = {:?}, want Rojo project config {config_path:?}",
		events.configs
	);
	assert!(
		events.projects.is_empty(),
		"projects = {:?}, want no project invalidation for Rojo-directory source event",
		events.projects
	);
}

#[test]
fn watch_excludes_out_dir() {
	let set = SolutionWatchSet {
		root_dirs: vec!["/project/src".to_owned()],
		rojo_directories: [
			"/project/assets",
			"/project/out",
			"/project/out/nested",
			"/project/include",
			"/project/.rotor/cache",
		]
		.map(str::to_owned)
		.to_vec(),
		artifact_directories: ["/project/out", "/project/include", "/project/.rotor"]
			.map(str::to_owned)
			.to_vec(),
		..SolutionWatchSet::default()
	};
	assert_eq!(
		solution_watch_directories(&set),
		["/project/src", "/project/assets"],
		"solutionWatchDirectories(), want source and asset directories only"
	);
}

/// `writeArtifactWatchSolution`: the concurrency solution under a root project of its own, with
/// a Rojo config mapping every output.
pub fn artifact_watch_solution() -> (tempfile::TempDir, PathBuf) {
	let (temp, root, configs) = concurrency_solution();
	add_type_roots(&configs);
	let config = std::fs::read_to_string(&configs[0]).expect("config");
	let config = format!(
		"{},\"references\":[{{\"path\":\"./left\"}},{{\"path\":\"./right\"}}]}}",
		config.strip_suffix('}').unwrap_or(&config)
	);
	must_write(root.join("tsconfig.json"), &config);
	must_write(root.join("package.json"), r#"{"name":"root"}"#);
	must_write(root.join("src/globals.d.ts"), NO_LIB_GLOBAL_STUBS);
	must_write(root.join("src/main.ts"), "export const value = 1;\n");
	must_write(
		root.join("default.project.json"),
		r#"{"name":"solution","tree":{"$className":"DataModel","ReplicatedStorage":{"left":{"$path":"left/out"},"right":{"$path":"right/out"},"root":{"$path":"out"},"include":{"$path":"include"}}}}"#,
	);
	(temp, root)
}

/// `startArtifactSolutionWatch`: a JSON solution watch over a fake watcher, past its initial
/// build.
pub fn start_artifact_solution_watch(root: &Path) -> (FakeWatcher, EventStream) {
	let watcher = FakeWatcher::default();
	let config = s(root.join("tsconfig.json"));
	let events = {
		let (watcher, root): (Arc<dyn Watcher>, String) = (Arc::new(watcher.clone()), s(root));
		EventStream::start(move |stop, out| {
			let args = BuildArgs {
				build: true,
				..BuildArgs::default()
			};
			let reload = reload_options(&config, &args);
			let mut reporter = JsonBuildReporter(WatchEventWriter::new(out, &root));
			let _code =
				run_build_solution_watch_loop(stop, &config, MergedOptions::default(), &reload, &mut reporter, watcher);
		})
	};
	events.next("buildStart");
	let end = events.next("buildEnd");
	assert!(end["ok"] == true, "initial build failed: {end}");
	events.next("watching");
	(watcher, events)
}

fn read(path: impl AsRef<Path>) -> String {
	std::fs::read_to_string(path).unwrap_or_default()
}

#[test]
fn solution_watch_rebuilds_macro_data_outside_source_roots() {
	let (_temp, root) = artifact_watch_solution();
	must_write(
		root.join("left/src/main.ts"),
		"export const value = $file('../data.txt');\n",
	);
	let data_path = root.join("left/data.txt");
	must_write(&data_path, "first");
	let (watcher, events) = start_artifact_solution_watch(&root);
	must_write(&data_path, "second");
	watcher.deliver(&data_path, &[Event::update(s(&data_path))], None);
	events.next("buildStart");
	let end = events.next("buildEnd");
	assert!(end["ok"] == true, "macro rebuild failed: {end}");
	let output = read(root.join("left/out/main.luau"));
	assert!(output.contains("second"), "macro data left stale output: {output}");
}

#[test]
fn solution_watch_does_not_watch_referenced_outputs() {
	let (_temp, root) = artifact_watch_solution();
	must_write(
		root.join("left/src/default.project.json"),
		r#"{"name":"nested","tree":{"main":{"$path":"main.luau"}}}"#,
	);
	let (watcher, _events) = start_artifact_solution_watch(&root);
	for name in ["left", "right"] {
		let (opens, _) = watcher.counts(root.join(name).join("out"));
		assert_eq!(opens, 0, "referenced {name} output directory watched {opens} times");
	}
	let (opens, _) = watcher.counts(root.join("left/out/default.project.json"));
	assert_eq!(opens, 0, "generated Rojo config watched {opens} times");
}

#[test]
fn solution_watch_does_not_watch_referenced_declaration_dir() {
	let (_temp, root) = artifact_watch_solution();
	let config_path = root.join("left/tsconfig.json");
	let config = read(&config_path);
	must_write(
		&config_path,
		&config.replacen(r#""outDir":"out""#, r#""outDir":"out","declarationDir":"types""#, 1),
	);
	must_write(
		root.join("default.project.json"),
		r#"{"name":"solution","tree":{"$className":"DataModel","ReplicatedStorage":{"left":{"$path":"left/out"},"types":{"$path":"left/types"},"right":{"$path":"right/out"},"root":{"$path":"out"},"include":{"$path":"include"}}}}"#,
	);
	let (watcher, _events) = start_artifact_solution_watch(&root);
	let (opens, _) = watcher.counts(root.join("left/types"));
	assert_eq!(opens, 0, "referenced declaration directory watched {opens} times");
}

const LEFT_TREE_PROJECT: &str = r#"{"name":"solution","tree":{"$className":"DataModel","ReplicatedStorage":{"left":{"$path":"left"},"right":{"$path":"right/out"},"root":{"$path":"out"},"include":{"$path":"include"}}}}"#;

#[test]
fn solution_watch_ignores_artifacts_from_ancestor_watch() {
	let (_temp, root) = artifact_watch_solution();
	must_write(root.join("default.project.json"), LEFT_TREE_PROJECT);
	let (watcher, events) = start_artifact_solution_watch(&root);

	let generated: Vec<Event> = [
		"out/main.d.ts",
		"out/main.d.ts.map",
		"out/main.luau",
		"out/main.luau.map",
		"out/.copyfiles-123.tmp",
		"out/rbxts.copyfiles.json",
		"out/default.project.json",
		"include/RuntimeLib.lua",
		".rotor/cache/rojo.json",
	]
	.iter()
	.map(|path| Event::update(s(root.join("left").join(path))))
	.collect();
	watcher.deliver(root.join("left"), &generated, None);
	if let Some(event) = events.within(Duration::from_millis(200)) {
		panic!("generated files triggered a watch event: {event}");
	}

	// A real source edit still rebuilds, and generated paths from the same batch never leak into
	// the reported change list.
	let source = root.join("left/src/main.ts");
	must_write(&source, "export const value = 2;\n");
	let mut batch = generated;
	batch.push(Event::update(s(&source)));
	watcher.deliver(root.join("left"), &batch, None);
	let start = events.next("buildStart");
	assert_eq!(
		start["changed"],
		serde_json::json!(["left/src/main.ts"]),
		"changed, want only left/src/main.ts"
	);
	let end = events.next("buildEnd");
	assert!(
		end["ok"] == true && end["files"] == 1,
		"source edit build = {end}, want one successfully compiled source"
	);
}

#[test]
fn solution_watch_reload_updates_artifact_directories() {
	let (_temp, root) = artifact_watch_solution();
	must_write(root.join("default.project.json"), LEFT_TREE_PROJECT);
	let (watcher, events) = start_artifact_solution_watch(&root);
	let config_path = root.join("left/tsconfig.json");
	let config = read(&config_path);
	must_write(
		&config_path,
		&config.replacen(
			r#""outDir":"out""#,
			r#""outDir":"generated","declarationDir":"types""#,
			1,
		),
	);
	watcher.deliver(&config_path, &[Event::update(s(&config_path))], None);
	events.next("buildStart");
	let end = events.next("buildEnd");
	assert!(end["ok"] == true, "reloaded build failed: {end}");
	for path in ["generated", "types"] {
		let (opens, _) = watcher.counts(root.join("left").join(path));
		assert_eq!(opens, 0, "reloaded artifact directory {path} watched {opens} times");
	}
	watcher.deliver(
		root.join("left"),
		&[
			Event::update(s(root.join("left/generated/default.project.json"))),
			Event::update(s(root.join("left/types/main.d.ts"))),
		],
		None,
	);
	if let Some(event) = events.within(Duration::from_millis(200)) {
		panic!("reloaded artifacts triggered a watch event: {event}");
	}
}

fn terminated() -> WatchError {
	WatchError::Terminated("watched directory removed".to_owned())
}

#[test]
fn solution_watch_terminated_root_is_reopened() {
	let mut events = SolutionWatchEvents::default();
	let key = WatchKey::new(WatchKind::Root, "/project/tsconfig.json", "/project/src");
	events.add(&key, &[Event::delete("/project/src")], Some(&terminated()));
	assert!(
		events.dead.contains(&key),
		"dead = {:?}, want the terminated root",
		events.dead
	);
	assert!(
		events.projects.contains(&key.project),
		"projects = {:?}, want the project invalidated",
		events.projects
	);
	assert!(
		events.paths.contains("/project/src"),
		"paths = {:?}, want the deleted root",
		events.paths
	);

	let rojo_key = WatchKey::new(WatchKind::Rojo, &key.project, "/project/assets");
	events.add_rojo(&rojo_key, &[], Some(&terminated()));
	assert!(
		events.dead.contains(&rojo_key),
		"dead = {:?}, want the terminated Rojo directory",
		events.dead
	);

	let mut overflow = SolutionWatchEvents::default();
	overflow.add(&key, &[], Some(&WatchError::Overflow));
	assert!(
		overflow.projects.contains(&key.project) && overflow.dead.is_empty(),
		"overflow: projects {:?} dead {:?}, want the project and a live watch",
		overflow.projects,
		overflow.dead
	);
}

#[test]
fn solution_watch_directory_and_json_events_invalidate() {
	let temp = tempfile::tempdir().expect("temp dir");
	let root = temp.path();
	let moved = root.join("src/new.name");
	std::fs::create_dir_all(&moved).expect("mkdir");
	let key = WatchKey::new(WatchKind::Root, &s(root.join("tsconfig.json")), &s(root.join("src")));
	for (name, event, want) in [
		("folder renamed from", Event::delete(s(root.join("src/old"))), true),
		("dotted folder renamed to", Event::update(s(&moved)), true),
		("json source", Event::update(s(root.join("src/data.json"))), true),
		("asset", Event::update(s(root.join("src/icon.png"))), false),
		("deleted asset", Event::delete(s(root.join("src/icon.png"))), false),
	] {
		let mut events = SolutionWatchEvents::default();
		events.add(&key, std::slice::from_ref(&event), None);
		assert_eq!(events.projects.contains(&key.project), want, "{name}: invalidated");
		assert!(
			events
				.assets
				.get(&key.project)
				.is_some_and(|assets| assets.contains_key(&event.path)),
			"{name}: assets = {:?}, want the path copied",
			events.assets
		);
	}
}

#[test]
fn solution_watches_reopen_only_failed_or_dead() {
	let watcher = FakeWatcher::default();
	let mut watches = SolutionWatches::new(
		Arc::new(watcher.clone()),
		Box::new(|_: &WatchKey| -> crate::fswatch::WatchCallback {
			Arc::new(|_: &[Event], _: Option<&WatchError>| {})
		}),
	);
	let sets = [
		SolutionWatchSet {
			project_path: "/a/tsconfig.json".to_owned(),
			ts_config_paths: vec!["/a/tsconfig.json".to_owned()],
			root_dirs: vec!["/a/src".to_owned()],
			rojo_directories: vec!["/a/assets".to_owned()],
			..SolutionWatchSet::default()
		},
		SolutionWatchSet {
			project_path: "/b/tsconfig.json".to_owned(),
			ts_config_paths: vec!["/b/tsconfig.json".to_owned()],
			root_dirs: vec!["/b/src".to_owned()],
			..SolutionWatchSet::default()
		},
	];
	let none = HashSet::new();
	watcher.set_fail(&gopath::clean("/a/assets"), true);
	watches.refresh(&sets, &none);
	assert!(!watches.armed, "armed with a failed watch");

	// The failed watch is retried; the open ones stay.
	watcher.set_fail(&gopath::clean("/a/assets"), false);
	watches.refresh(&sets, &none);
	for (path, want) in [
		("/a/assets", (2, 0)),
		("/a/src", (1, 0)),
		("/b/src", (1, 0)),
		("/a/tsconfig.json", (1, 0)),
	] {
		assert_eq!(watcher.counts(path), want, "retry: {path} (opens, closes)");
	}
	assert!(watches.armed, "not armed after the retry");

	// A dead watch is closed and reopened alone.
	let dead = HashSet::from([WatchKey::new(WatchKind::Root, "/b/tsconfig.json", "/b/src")]);
	watches.refresh(&sets, &dead);
	for (path, want) in [("/b/src", (2, 1)), ("/a/src", (1, 0)), ("/b/tsconfig.json", (1, 0))] {
		assert_eq!(watcher.counts(path), want, "dead: {path} (opens, closes)");
	}

	// Unchanged sets with no dead watch keep everything.
	watches.refresh(&sets, &none);
	assert_eq!(watcher.counts("/a/src").0, 1, "unchanged sets reopened /a/src");

	// A removed project closes its watches only.
	watches.refresh(&sets[..1], &none);
	assert_eq!(watcher.counts("/b/tsconfig.json").1, 1, "removed project config closes");
	assert_eq!(watcher.counts("/a/src").1, 0, "kept project root closes");
}

/// `buildEndReporter`: sends each build's error.
struct BuildEndReporter(Mutex<Sender<Option<String>>>);

impl BuildWatchReporter for BuildEndReporter {
	fn build_start(&mut self, _changed: Option<&[String]>) {}

	fn build_end(&mut self, outcome: &Outcome) {
		let _ = self.0.lock().expect("sender").send(outcome.error.clone());
	}

	fn watching(&mut self, _files: &dyn Fn() -> usize) {}
}

/// A cycle whose config reload fails still reopens the watches that died in the same batch of
/// events.
#[test]
fn solution_watch_reopens_dead_watch_after_reload_failure() {
	let (_temp, root, configs) = concurrency_solution();
	add_type_roots(&configs);
	rotor_tsrs::project::set_long_lived(true);
	let watcher = FakeWatcher::default();
	let ts_config_path = s(root.join("tsconfig.json"));
	let (reloading_sender, reloading) = mpsc::channel();
	let (release, released) = mpsc::channel::<()>();
	let (ends_sender, ends) = mpsc::channel();
	let stop = Stop::default();
	let done = {
		let (stop, watcher, ts_config_path) = (stop.clone(), Arc::new(watcher.clone()), ts_config_path);
		std::thread::spawn(move || {
			let base_reload = reload_options(
				&ts_config_path,
				&BuildArgs {
					build: true,
					..BuildArgs::default()
				},
			);
			let reloads = AtomicUsize::new(0);
			let (reloading_sender, released) = (Mutex::new(reloading_sender), Mutex::new(released));
			let reload = move || {
				if reloads.fetch_add(1, Ordering::SeqCst) == 0 {
					let _ = reloading_sender.lock().expect("sender").send(());
					let _ = released.lock().expect("receiver").recv();
					return base_reload();
				}
				Err("invalid config".to_owned())
			};
			let mut reporter = BuildEndReporter(Mutex::new(ends_sender));
			let _code = run_build_solution_watch_loop(
				&stop,
				&ts_config_path,
				MergedOptions::default(),
				&reload,
				&mut reporter,
				watcher,
			);
		})
	};
	let end = |what: &str| {
		ends.recv_timeout(Duration::from_secs(30))
			.unwrap_or_else(|_| panic!("no {what}"))
	};
	assert_eq!(end("initial build"), None, "initial build");
	let (mut config, mut source) = (String::new(), String::new());
	for path in watcher.paths() {
		if !gopath::base(&gopath::dir(&path)).eq_ignore_ascii_case("left") {
			continue;
		}
		match gopath::base(&path).as_str() {
			"tsconfig.json" => config = path,
			"src" => source = path,
			_ => {}
		}
	}
	assert!(
		!config.is_empty() && !source.is_empty(),
		"watches = {:?}, want left config and source",
		watcher.paths()
	);

	watcher.deliver(&config, &[Event::update(&config)], None);
	reloading.recv_timeout(Duration::from_secs(30)).expect("reloading");
	// These land in the next cycle, whose reload fails.
	watcher.deliver(&config, &[Event::update(&config)], None);
	watcher.deliver(&source, &[], Some(&terminated()));
	drop(release);
	assert_eq!(end("reloaded build"), None, "reloaded build");
	assert!(end("failed reload").is_some(), "reload did not fail");
	assert_eq!(
		watcher.counts(&source),
		(2, 1),
		"dead source watch (opens, closes), want reopened once"
	);
	stop.stop();
	done.join().expect("watch loop");
}

/// `writePackageWatchFixture`: a project importing `@rbxts/lib` from `node_modules`.
fn package_watch_fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
	let (temp, root) = buildable_project(r#"export { answer } from "@rbxts/lib";"#);
	must_write(root.join("package.json"), r#"{"name":"@scope/app"}"#);
	let package_dir = root.join("node_modules/@rbxts/lib");
	must_write(
		package_dir.join("package.json"),
		r#"{"name":"@rbxts/lib","main":"out/init.luau","types":"out/index.d.ts"}"#,
	);
	must_write(
		package_dir.join("out/index.d.ts"),
		"export declare const answer: number;",
	);
	must_write(package_dir.join("out/init.luau"), "return { answer = 42 }");
	(temp, root, package_dir)
}

#[test]
fn solution_watch_package_layout() {
	let (_temp, root, package_dir) = package_watch_fixture();
	let named = package_dir.join(".cache/place/clean.project.json");
	must_write(&named, r#"{"name":"testing","tree":{"$path":"../../out"}}"#);
	let (watcher, events) = start_artifact_solution_watch(&root);
	let config = package_dir.join("default.project.json");
	assert_ne!(
		watcher.counts(&config).0,
		0,
		"package default-project candidate is not watched"
	);
	// Ignored test projects must not enter the reload or asset-copy route.
	let named_dir = named.parent().expect("parent").to_path_buf();
	watcher.deliver(&named_dir, &[Event::update(s(&named))], None);
	if let Some(event) = events.within(Duration::from_millis(100)) {
		panic!("ignored project triggered a build: {event}");
	}
	std::fs::remove_file(&named).expect("remove");
	std::fs::remove_dir(&named_dir).expect("remove dir");
	watcher.deliver(&named_dir, &[Event::delete(s(&named_dir))], None);
	if let Some(event) = events.within(Duration::from_millis(100)) {
		panic!("removing an irrelevant directory triggered a build: {event}");
	}
	must_write(&config, r#"{"name":"runtime-lib","tree":{"$path":"out"}}"#);
	watcher.deliver(&config, &[Event::update(s(&config))], None);
	events.next("buildStart");
	let end = events.next("buildEnd");
	assert!(end["ok"] == true, "package layout edit failed: {end}");
	let output = read(root.join("out/main.luau"));
	assert!(
		output.contains(r#"TS.getModule(script, "@rbxts", "runtime-lib")"#),
		"watch left a stale import:\n{output}"
	);
	std::fs::remove_file(&config).expect("remove");
	watcher.deliver(&config, &[Event::delete(s(&config))], None);
	events.next("buildStart");
	let end = events.next("buildEnd");
	assert!(end["ok"] == true, "removing package layout failed: {end}");
	let output = read(root.join("out/main.luau"));
	assert!(
		output.contains(r#"TS.getModule(script, "@rbxts", "lib").out"#),
		"removing layout left a stale import:\n{output}"
	);
	// A newly installed subtree can contain a default before any exact-file watch exists. Its
	// parent listing event must discover the new layout.
	must_write(
		package_dir.join("examples/place/default.project.json"),
		r#"{"name":"example","tree":{"$path":"../../out"}}"#,
	);
	watcher.deliver(&package_dir, &[Event::update(s(package_dir.join("examples")))], None);
	events.next("buildStart");
	let end = events.next("buildEnd");
	assert!(end["ok"] == true, "new nested default failed: {end}");
	let output = read(root.join("out/main.luau"));
	assert!(
		output.contains(".examples.example"),
		"new nested layout was not discovered:\n{output}"
	);
	assert!(
		!root.join("out/node_modules").exists(),
		"dependency assets copied into output"
	);
}

/// File symlinks need Unix (Go skips where they are unavailable).
#[cfg(unix)]
#[test]
fn solution_watch_package_config_symlink_retarget() {
	let (_temp, root, package_dir) = package_watch_fixture();
	must_write(
		package_dir.join("default.project.json"),
		r#"{"name":"lib","tree":{"Mounted":{"$path":"layout.project.json"}}}"#,
	);
	for name in ["One", "Two"] {
		must_write(
			package_dir.join(format!("{name}.project.json")),
			&format!(r#"{{"name":"ignored","tree":{{"{name}":{{"$path":"out"}}}}}}"#),
		);
	}
	let alias = package_dir.join("layout.project.json");
	std::os::unix::fs::symlink(package_dir.join("One.project.json"), &alias).expect("symlink");
	let (watcher, events) = start_artifact_solution_watch(&root);
	std::fs::remove_file(&alias).expect("remove");
	std::os::unix::fs::symlink(package_dir.join("Two.project.json"), &alias).expect("symlink");
	// fswatch coalesces rapid delete/recreate into one update. The old target still exists, so
	// only the logical parent reports this change.
	watcher.deliver(&package_dir, &[Event::update(s(&alias))], None);
	events.next("buildStart");
	let end = events.next("buildEnd");
	assert!(end["ok"] == true, "symlink retarget failed: {end}");
	let output = read(root.join("out/main.luau"));
	assert!(output.contains(".Mounted.Two"), "retargeted config is stale:\n{output}");
}
