//! Fixtures the watch tests share: Go's `writeBuildableProject`, `writeConcurrencySolution`,
//! `mustWrite`, `watchEventStream` and `fakeSolutionWatcher`.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rotor_support::gopath;
use serde_json::Value;

use crate::fswatch::{Event, Watch, WatchCallback, WatchError, Watcher};
use crate::solution_watch_coverage::canonical_path;
use crate::watch::Stop;

/// `noLibGlobalStubs`
pub const NO_LIB_GLOBAL_STUBS: &str = "declare function print(...params: Array<unknown>): void;\n\
	interface Array<T> {}\ninterface Boolean {}\ninterface CallableFunction {}\n\
	interface Function {}\ninterface IArguments {}\ninterface NewableFunction {}\n\
	interface Number {}\ninterface Object {}\ninterface RegExp {}\ninterface String {}\n";

/// `mustWrite`
pub fn must_write(path: impl AsRef<Path>, content: &str) {
	let path = path.as_ref();
	std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
	std::fs::write(path, content).expect("write");
}

/// `fsutil.EvalSymlinks(t.TempDir())`: a temp directory by its canonical path.
pub fn temp_dir() -> (tempfile::TempDir, PathBuf) {
	let dir = tempfile::tempdir().expect("temp dir");
	let path = dir.path().canonicalize().expect("canonical temp dir");
	(dir, path)
}

/// `writeBuildableProject`: a self-contained package project (a scoped name needs no Rojo
/// config) that builds cleanly; `main` replaces `src/main.ts` when not empty.
pub fn buildable_project(main: &str) -> (tempfile::TempDir, PathBuf) {
	let (temp, dir) = temp_dir();
	must_write(
		dir.join("tsconfig.json"),
		r#"{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"module": "CommonJS",
		"moduleResolution": "Node",
		"noLib": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"types": [],
		"typeRoots": ["node_modules/@rbxts"],
		"rootDir": "src",
		"outDir": "out"
	},
	"include": ["src"]
}"#,
	);
	must_write(dir.join("package.json"), r#"{"name":"@scope/build-json-fixture"}"#);
	must_write(dir.join("src/globals.d.ts"), NO_LIB_GLOBAL_STUBS);
	must_write(
		dir.join("src/main.ts"),
		if main.is_empty() { "export {};\n" } else { main },
	);
	(temp, dir)
}

/// `writeConcurrencySolution`: a root referencing `left` and `right`.
pub fn concurrency_solution() -> (tempfile::TempDir, PathBuf, Vec<PathBuf>) {
	let (temp, root) = temp_dir();
	let mut configs = Vec::new();
	must_write(
		root.join("tsconfig.json"),
		r#"{"files":[],"references":[{"path":"./left"},{"path":"./right"}]}"#,
	);
	for name in ["left", "right"] {
		let dir = root.join(name);
		let config = dir.join("tsconfig.json");
		must_write(
			&config,
			r#"{"compilerOptions":{"allowSyntheticDefaultImports":true,"composite":true,"declaration":true,"module":"CommonJS","moduleResolution":"Node","noLib":true,"moduleDetection":"force","strict":true,"target":"ESNext","types":[],"rootDir":"src","outDir":"out","checkers":1},"include":["src"]}"#,
		);
		must_write(dir.join("package.json"), &format!(r#"{{"name":"@scope/{name}"}}"#));
		must_write(dir.join("src/globals.d.ts"), NO_LIB_GLOBAL_STUBS);
		must_write(dir.join("src/main.ts"), "export {};\n");
		configs.push(config);
	}
	(temp, root, configs)
}

/// Adds `"typeRoots":["node_modules/@rbxts"]` after `"types":[]` in each config.
pub fn add_type_roots(configs: &[PathBuf]) {
	for config in configs {
		let data = std::fs::read_to_string(config).expect("config");
		must_write(
			config,
			&data.replacen(r#""types":[]"#, r#""types":[],"typeRoots":["node_modules/@rbxts"]"#, 1),
		);
	}
}

/// Decodes each complete line written to it and sends it on.
struct LineWriter {
	pending: Vec<u8>,
	events: Sender<Value>,
}

impl Write for LineWriter {
	fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
		self.pending.extend_from_slice(buf);
		while let Some(end) = self.pending.iter().position(|&byte| byte == b'\n') {
			let line: Vec<u8> = self.pending.drain(..=end).collect();
			let event = serde_json::from_slice(&line[..end])
				.unwrap_or_else(|error| panic!("non-JSON line {:?}: {error}", String::from_utf8_lossy(&line)));
			let _ = self.events.send(event);
		}
		Ok(buf.len())
	}

	fn flush(&mut self) -> std::io::Result<()> {
		Ok(())
	}
}

/// `watchEventStream`: a JSON watch loop running aside, yielding its NDJSON lines; it stops when
/// the stream is dropped.
pub struct EventStream {
	events: Receiver<Value>,
	stop: Stop,
	done: Receiver<()>,
}

impl EventStream {
	/// Runs `run` (a watch loop) on a large stack in a long-lived session, as the CLI does.
	pub fn start(run: impl FnOnce(&Stop, &mut (dyn Write + Send)) + Send + 'static) -> Self {
		rotor_tsrs::project::set_long_lived(true);
		let (sender, events) = mpsc::channel();
		let (finished, done) = mpsc::channel();
		let stop = Stop::default();
		let loop_stop = stop.clone();
		std::thread::spawn(move || {
			let mut out = LineWriter {
				pending: Vec::new(),
				events: sender,
			};
			rotor_tsrs::with_large_stack(|| run(&loop_stop, &mut out));
			let _ = finished.send(());
		});
		Self { events, stop, done }
	}

	/// `nextWatchEvent`
	pub fn next(&self, want: &str) -> Value {
		match self.events.recv_timeout(Duration::from_secs(30)) {
			Ok(event) => {
				assert!(event["event"] == want, "event = {event}, want {want}");
				event
			}
			Err(RecvTimeoutError::Timeout) => panic!("timed out waiting for {want}"),
			Err(RecvTimeoutError::Disconnected) => panic!("stream closed, want {want}"),
		}
	}

	/// The next event within `timeout`, if any.
	pub fn within(&self, timeout: Duration) -> Option<Value> {
		self.events.recv_timeout(timeout).ok()
	}
}

impl Drop for EventStream {
	fn drop(&mut self) {
		self.stop.stop();
		let stopped = self.done.recv_timeout(Duration::from_secs(10));
		assert!(
			stopped.is_ok() || std::thread::panicking(),
			"watch loop did not stop after cancel"
		);
	}
}

#[derive(Default)]
struct FakeState {
	opens: HashMap<String, usize>,
	closes: HashMap<String, usize>,
	fail: HashMap<String, bool>,
	callbacks: HashMap<String, Vec<Option<WatchCallback>>>,
}

/// `fakeSolutionWatcher`: records the watches the solution watcher opens and lets a test
/// deliver events to them.
#[derive(Clone, Default)]
pub struct FakeWatcher {
	state: Arc<Mutex<FakeState>>,
}

fn key(path: &str) -> String {
	gopath::to_slash(&canonical_path(path)).into_owned()
}

impl FakeWatcher {
	#[expect(
		clippy::significant_drop_tightening,
		reason = "the fake records each open under one lock"
	)]
	fn open(&self, path: &str, callback: WatchCallback) -> Result<Box<dyn Watch>, String> {
		let path = key(path);
		let mut state = self.state.lock().expect("fake watcher");
		*state.opens.entry(path.clone()).or_default() += 1;
		if state.fail.iter().any(|(failed, fail)| *fail && key(failed) == path) {
			return Err("watch failed".to_owned());
		}
		let callbacks = state.callbacks.entry(path.clone()).or_default();
		let index = callbacks.len();
		callbacks.push(Some(callback));
		Ok(Box::new(FakeWatch {
			state: Arc::clone(&self.state),
			path,
			index,
		}))
	}

	/// `fail[path] = fail`
	pub fn set_fail(&self, path: &str, fail: bool) {
		self.state
			.lock()
			.expect("fake watcher")
			.fail
			.insert(path.to_owned(), fail);
	}

	/// `delete(fail, path)`
	pub fn clear_fail(&self, path: &str) {
		self.state.lock().expect("fake watcher").fail.remove(path);
	}

	/// `counts`: how often `path` was opened and closed.
	pub fn counts(&self, path: impl AsRef<Path>) -> (usize, usize) {
		let path = key(&path.as_ref().to_string_lossy());
		let state = self.state.lock().expect("fake watcher");
		(
			state.opens.get(&path).copied().unwrap_or_default(),
			state.closes.get(&path).copied().unwrap_or_default(),
		)
	}

	/// The watched paths (canonical, slash-separated).
	pub fn paths(&self) -> Vec<String> {
		self.state
			.lock()
			.expect("fake watcher")
			.callbacks
			.keys()
			.cloned()
			.collect()
	}

	/// `deliver`: runs every live callback on `path`.
	pub fn deliver(&self, path: impl AsRef<Path>, events: &[Event], error: Option<&WatchError>) {
		let path = key(&path.as_ref().to_string_lossy());
		let callbacks = self
			.state
			.lock()
			.expect("fake watcher")
			.callbacks
			.get(&path)
			.cloned()
			.unwrap_or_default();
		assert!(!callbacks.is_empty(), "no watch on {path}");
		for callback in callbacks.into_iter().flatten() {
			callback(events, error);
		}
	}
}

impl Watcher for FakeWatcher {
	fn watch_directory(&self, dir: &str, callback: WatchCallback, _recursive: bool) -> Result<Box<dyn Watch>, String> {
		self.open(dir, callback)
	}

	fn watch_file(&self, path: &str, callback: WatchCallback) -> Result<Box<dyn Watch>, String> {
		self.open(path, callback)
	}
}

struct FakeWatch {
	state: Arc<Mutex<FakeState>>,
	path: String,
	index: usize,
}

impl Watch for FakeWatch {
	fn close(&mut self) {
		let mut state = self.state.lock().expect("fake watcher");
		*state.closes.entry(self.path.clone()).or_default() += 1;
		if let Some(slot) = state
			.callbacks
			.get_mut(&self.path)
			.and_then(|callbacks| callbacks.get_mut(self.index))
		{
			*slot = None;
		}
	}
}

/// The path as a string.
pub fn s(path: impl AsRef<Path>) -> String {
	path.as_ref().to_string_lossy().into_owned()
}
