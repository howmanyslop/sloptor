//! The `--watch --json` NDJSON stream (`cmd/rotor/watch_json.go`).
//!
//! One object per line, each written in a single write so a consumer reading a pipe always sees
//! whole lines. Every `buildStart` is followed by exactly one `buildEnd` before the next
//! `buildStart`. The first event carries sloptor's version, and one `watching` event follows the
//! initial `buildEnd`. Strings are encoded as Go's `json.Encoder` with `SetEscapeHTML(false)`.

use std::io::Write;

use rotor_compile::ProjectOptions;
use rotor_compile::progress::BuildProgress;

use crate::VERSION;
use crate::build::{Outcome, json_result, rel_display};
use crate::check::{CheckPass, Render};
use crate::json::{JsonResult, write_string};
use crate::watch::{BuildWatchReporter, CheckWatchReporter};

/// The clock events are stamped with.
pub type Clock = Box<dyn Fn() -> jiff::Timestamp + Send>;

/// `watchEventWriter`
pub struct WatchEventWriter<'a> {
	out: &'a mut (dyn Write + Send),
	/// Changed paths are reported relative to this directory.
	root: String,
	now: Clock,
	/// The first event (carrying the version) was written.
	started: bool,
}

impl<'a> WatchEventWriter<'a> {
	/// `newWatchEventWriter`
	pub fn new(out: &'a mut (dyn Write + Send), root: &str) -> Self {
		Self {
			out,
			root: root.to_owned(),
			now: Box::new(jiff::Timestamp::now),
			started: false,
		}
	}

	/// Replaces the clock (tests).
	#[cfg(test)]
	pub(crate) fn with_clock(mut self, now: Clock) -> Self {
		self.now = now;
		self
	}

	/// `stamp`: UTC, millisecond precision, truncated as Go's layout does.
	fn stamp(&self) -> String {
		let now = (self.now)();
		format!(
			"{}.{:03}Z",
			now.strftime("%Y-%m-%dT%H:%M:%S"),
			now.subsec_millisecond().max(0)
		)
	}

	fn emit(&mut self, line: &str) {
		let _ = self.out.write_all(line.as_bytes());
		let _ = self.out.flush();
	}

	/// `buildStart`: `None` (the initial build) reports an empty list.
	pub fn build_start(&mut self, changed: Option<&[String]>) {
		let mut line = format!("{{\"event\":\"buildStart\",\"at\":\"{}\"", self.stamp());
		if !self.started {
			line.push_str(",\"version\":");
			write_string(&mut line, VERSION);
			self.started = true;
		}
		line.push_str(",\"changed\":[");
		for (index, path) in changed.unwrap_or_default().iter().enumerate() {
			if index > 0 {
				line.push(',');
			}
			write_string(&mut line, &rel_display(&self.root, path));
		}
		line.push_str("]}\n");
		self.emit(&line);
	}

	/// `buildEnd`: the event, then the one-shot `--json` result's fields.
	pub fn build_end(&mut self, result: &JsonResult) {
		let encoded = result.encode();
		let fields = encoded.strip_prefix('{').unwrap_or(&encoded);
		let line = format!("{{\"event\":\"buildEnd\",\"at\":\"{}\",{fields}", self.stamp());
		self.emit(&line);
	}

	/// `watching`: startup is done; `files` are watched.
	pub fn watching(&mut self, files: usize) {
		let line = format!(
			"{{\"event\":\"watching\",\"at\":\"{}\",\"files\":{files}}}\n",
			self.stamp()
		);
		self.emit(&line);
	}

	/// The directory changed paths are relative to.
	#[must_use]
	pub fn root(&self) -> &str {
		&self.root
	}
}

/// `jsonBuildWatchReporter`: `build --watch --json`.
pub struct JsonBuildReporter<'a>(pub WatchEventWriter<'a>);

impl BuildWatchReporter for JsonBuildReporter<'_> {
	fn build_start(&mut self, changed: Option<&[String]>) {
		self.0.build_start(changed);
	}

	fn run(&mut self, options: &ProjectOptions, build: &dyn Fn(&ProjectOptions) -> Outcome) -> Outcome {
		build(options)
	}

	fn build_end(&mut self, outcome: &Outcome) {
		let result = json_result(self.0.root(), outcome);
		self.0.build_end(&result);
	}

	fn watching(&mut self, files: &dyn Fn() -> usize) {
		self.0.watching(files());
	}
}

/// `checkJSONResult`: one check pass in the `--json` shape.
#[must_use]
pub fn check_json_result(pass: &CheckPass) -> JsonResult {
	JsonResult {
		version: VERSION.to_owned(),
		ok: pass.errors == 0,
		files: pass.files,
		duration_ms: pass.elapsed.as_millis(),
		diagnostics: pass.json.clone(),
	}
}

/// `jsonCheckWatchReporter`: `check --watch --json`.
pub struct JsonCheckReporter<'a>(pub WatchEventWriter<'a>);

impl CheckWatchReporter for JsonCheckReporter<'_> {
	fn build_start(&mut self, changed: Option<&[String]>) {
		self.0.build_start(changed);
	}

	fn run(&mut self, check: &dyn Fn(Render, Option<&BuildProgress>) -> CheckPass) -> CheckPass {
		check(Render::Json, None)
	}

	fn build_end(&mut self, pass: &CheckPass) {
		self.0.build_end(&check_json_result(pass));
	}

	fn watching(&mut self, files: &dyn Fn() -> usize) {
		self.0.watching(files());
	}
}

#[cfg(test)]
mod tests {
	//! The unit half of `watch_json_test.go`; the loops are in `tests/watch.rs`.

	use std::io::Write;
	use std::time::Duration;

	use rotor_compile::compile::DiagnosticInfo;
	use rotor_compile::output::BuildResult;

	use super::WatchEventWriter;
	use crate::VERSION;
	use crate::build::{Outcome, json_result};
	use crate::json::JsonResult;

	fn at() -> jiff::Timestamp {
		jiff::Timestamp::new(1_790_359_200, 142_000_000).expect("timestamp")
	}

	#[test]
	fn build_json_result_success_counts_outputs() {
		let mut result = BuildResult::default();
		result.outputs.insert("a.luau".to_owned(), Vec::new());
		result.outputs.insert("b.luau".to_owned(), Vec::new());
		let got = json_result(
			"",
			&Outcome {
				result: Some(result),
				diagnostics: Vec::new(),
				elapsed: Duration::from_millis(142),
				error: None,
			},
		);
		assert!(got.ok && got.files == 2 && got.duration_ms == 142 && got.version == VERSION);
		assert_eq!(got.diagnostics.len(), 0, "diagnostics = {:?}", got.diagnostics);
	}

	#[test]
	fn build_json_result_failure_without_diagnostics_reports_error() {
		let got = json_result(
			"",
			&Outcome {
				result: None,
				diagnostics: Vec::new(),
				elapsed: Duration::ZERO,
				error: Some("config broke".to_owned()),
			},
		);
		assert!(!got.ok);
		assert_eq!(got.diagnostics.len(), 1);
		assert_eq!(
			(got.diagnostics[0].message.as_str(), got.diagnostics[0].severity),
			("config broke", "error")
		);
	}

	#[test]
	fn build_json_result_failure_maps_diagnostics() {
		let got = json_result(
			"",
			&Outcome {
				result: None,
				diagnostics: vec![
					DiagnosticInfo {
						code: "TS2322".to_owned(),
						message: "bad".to_owned(),
						..DiagnosticInfo::default()
					},
					DiagnosticInfo {
						message: "meh".to_owned(),
						warning: true,
						..DiagnosticInfo::default()
					},
				],
				elapsed: Duration::ZERO,
				error: Some("build failed".to_owned()),
			},
		);
		assert_eq!(got.diagnostics.len(), 2);
		let first = &got.diagnostics[0];
		assert_eq!(
			(first.code.as_str(), first.severity, first.message.as_str()),
			("TS2322", "error", "bad")
		);
		assert_eq!(got.diagnostics[1].severity, "warning");
	}

	#[test]
	fn watch_event_writer_emits_one_object_per_line() {
		let dir = tempfile::tempdir().expect("temp dir");
		let root = dir.path().to_string_lossy().into_owned();
		let mut out: Vec<u8> = Vec::new();
		{
			let mut writer = WatchEventWriter::new(&mut out, &root).with_clock(Box::new(at));
			writer.build_start(None);
			writer.build_end(&JsonResult {
				version: "v".to_owned(),
				ok: true,
				files: 3,
				duration_ms: 7,
				diagnostics: Vec::new(),
			});
			writer.watching(12);
			writer.build_start(Some(&[dir
				.path()
				.join("src")
				.join("a.ts")
				.to_string_lossy()
				.into_owned()]));
		}
		let want = format!(
			"{{\"event\":\"buildStart\",\"at\":\"2026-09-25T18:00:00.142Z\",\"version\":\"{VERSION}\",\"changed\":[]}}\n\
			{{\"event\":\"buildEnd\",\"at\":\"2026-09-25T18:00:00.142Z\",\"version\":\"v\",\"ok\":true,\"files\":3,\"durationMs\":7,\"diagnostics\":[]}}\n\
			{{\"event\":\"watching\",\"at\":\"2026-09-25T18:00:00.142Z\",\"files\":12}}\n\
			{{\"event\":\"buildStart\",\"at\":\"2026-09-25T18:00:00.142Z\",\"changed\":[\"src/a.ts\"]}}\n"
		);
		assert_eq!(String::from_utf8_lossy(&out), want);
	}

	#[derive(Default)]
	struct Counting(usize);

	impl Write for Counting {
		fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
			self.0 += 1;
			Ok(buf.len())
		}

		fn flush(&mut self) -> std::io::Result<()> {
			Ok(())
		}
	}

	#[test]
	fn watch_event_writer_writes_each_event_in_one_write() {
		let mut counted = Counting::default();
		{
			let mut writer = WatchEventWriter::new(&mut counted, "/");
			writer.build_start(None);
			writer.build_end(&JsonResult::default());
		}
		assert_eq!(counted.0, 2, "one write per NDJSON line, so pipes see whole lines");
	}

	/// The loop tests of `watch_json_test.go`, over real projects.
	mod loops {
		use std::sync::Arc;
		use std::time::{Duration, SystemTime};

		use crate::VERSION;
		use crate::options::{MergedOptions, include_dir, project_compile_options};
		use crate::test_fixtures::{
			EventStream, add_type_roots, buildable_project, concurrency_solution, must_write, s,
		};
		use crate::watch::{run_build_watch_loop, run_check_watch_loop};
		use crate::watch_json::{JsonBuildReporter, JsonCheckReporter, WatchEventWriter};

		fn build_watch(dir: &str) -> EventStream {
			let dir = dir.to_owned();
			EventStream::start(move |stop, out| {
				let ts_config_path = rotor_support::gopath::join(&[&dir, "tsconfig.json"]);
				let opts = MergedOptions::default();
				let options = project_compile_options(&ts_config_path, &opts);
				let include = include_dir(&dir, &opts);
				let mut reporter = JsonBuildReporter(WatchEventWriter::new(out, &dir));
				run_build_watch_loop(stop, &dir, &ts_config_path, &options, &include, &mut reporter);
			})
		}

		fn files(event: &serde_json::Value) -> f64 {
			event["files"].as_f64().unwrap_or_default()
		}

		fn changed(event: &serde_json::Value) -> Option<Vec<String>> {
			event["changed"].as_array().map(|changed| {
				changed
					.iter()
					.map(|path| path.as_str().unwrap_or_default().to_owned())
					.collect()
			})
		}

		#[test]
		fn build_watch_json_emits_paired_events_per_build() {
			let (_temp, dir) = buildable_project("");
			let events = build_watch(&s(&dir));

			// Initial build: a pair with no changed files.
			let start = events.next("buildStart");
			assert_eq!(
				changed(&start),
				Some(Vec::new()),
				"initial changed = {}, want []",
				start["changed"]
			);
			assert!(
				start["version"] == VERSION,
				"initial buildStart version = {}, want {VERSION:?}",
				start["version"]
			);
			let end = events.next("buildEnd");
			assert!(
				end["ok"] == true && files(&end) > 0.0,
				"initial buildEnd = {end}, want ok with files"
			);
			let watching = events.next("watching");
			assert!(
				files(&watching) >= 2.0,
				"watching = {watching}, want the watched file count"
			);

			// A type error: the next pair names the file and carries the diagnostic.
			must_write(dir.join("src/main.ts"), "export const s: string = 5;\n");
			let start = events.next("buildStart");
			assert_eq!(
				changed(&start),
				Some(vec!["src/main.ts".to_owned()]),
				"changed = {}, want [src/main.ts]",
				start["changed"]
			);
			let end = events.next("buildEnd");
			let diags = end["diagnostics"].as_array().cloned().unwrap_or_default();
			assert!(
				end["ok"] == false && !diags.is_empty(),
				"buildEnd = {end}, want failure with diagnostics"
			);
			let diag = &diags[0];
			assert!(
				diag["code"] == "TS2322" && diag["severity"] == "error" && diag["file"] == "src/main.ts",
				"diagnostic = {diag}, want TS2322 error in src/main.ts"
			);

			// A save with no content change still yields exactly one pair.
			let later = SystemTime::now() + Duration::from_secs(2);
			std::fs::File::options()
				.write(true)
				.open(dir.join("src/main.ts"))
				.and_then(|file| file.set_times(std::fs::FileTimes::new().set_accessed(later).set_modified(later)))
				.expect("chtimes");
			events.next("buildStart");
			events.next("buildEnd");
		}

		#[test]
		fn build_watch_rebuilds_importer_relative_macro_data() {
			let (_temp, dir) = buildable_project("export const value = $file('../data.txt');\n");
			let data_path = dir.join("data.txt");
			must_write(&data_path, "first");
			let events = build_watch(&s(&dir));
			events.next("buildStart");
			let end = events.next("buildEnd");
			assert!(end["ok"] == true, "initial build: {end}");
			events.next("watching");
			must_write(&data_path, "second");
			events.next("buildStart");
			let end = events.next("buildEnd");
			assert!(end["ok"] == true, "rebuild: {end}");
			let output = std::fs::read_to_string(dir.join("out/main.luau")).unwrap_or_default();
			assert!(output.contains("second"), "stale macro output: {output}");
		}

		#[test]
		fn check_watch_json_emits_paired_events_per_check() {
			let (_temp, dir) = buildable_project("");
			let root = s(&dir);
			let events = EventStream::start(move |stop, out| {
				let mut reporter = JsonCheckReporter(WatchEventWriter::new(out, &root));
				run_check_watch_loop(stop, &root, None, &mut reporter);
			});

			let start = events.next("buildStart");
			assert_eq!(
				changed(&start),
				Some(Vec::new()),
				"initial changed = {}, want []",
				start["changed"]
			);
			assert!(
				start["version"] == VERSION,
				"initial buildStart version = {}, want {VERSION:?}",
				start["version"]
			);
			let end = events.next("buildEnd");
			assert!(
				end["ok"] == true && files(&end) > 0.0,
				"initial buildEnd = {end}, want ok with files"
			);
			let watching = events.next("watching");
			assert!(
				files(&watching) >= 2.0,
				"watching = {watching}, want the watched file count"
			);

			must_write(dir.join("src/main.ts"), "export const s: string = 5;\n");
			let start = events.next("buildStart");
			assert_eq!(
				changed(&start),
				Some(vec!["src/main.ts".to_owned()]),
				"changed = {}, want [src/main.ts]",
				start["changed"]
			);
			let end = events.next("buildEnd");
			let diags = end["diagnostics"].as_array().cloned().unwrap_or_default();
			assert!(
				end["ok"] == false && !diags.is_empty(),
				"buildEnd = {end}, want failure with diagnostics"
			);
			let diag = &diags[0];
			assert!(
				diag["code"] == "TS2322" && diag["file"] == "src/main.ts",
				"diagnostic = {diag}, want TS2322 in src/main.ts"
			);
		}

		#[test]
		fn build_solution_watch_json_emits_paired_events_per_build() {
			let (_temp, root, configs) = concurrency_solution();
			add_type_roots(&configs);
			let ts_config_path = s(root.join("tsconfig.json"));
			let events = {
				let root = s(&root);
				EventStream::start(move |stop, out| {
					let args = crate::build::BuildArgs {
						build: true,
						..crate::build::BuildArgs::default()
					};
					let reload = crate::build::reload_options(&ts_config_path, &args);
					let mut reporter = JsonBuildReporter(WatchEventWriter::new(out, &root));
					let _code = crate::solution_watch::run_build_solution_watch_loop(
						stop,
						&ts_config_path,
						MergedOptions::default(),
						&reload,
						&mut reporter,
						Arc::new(crate::fswatch::NotifyWatcher::new(false)),
					);
				})
			};

			let start = events.next("buildStart");
			assert!(
				changed(&start) == Some(Vec::new()) && start["version"] == VERSION,
				"initial buildStart = {start}, want version and changed []"
			);
			let end = events.next("buildEnd");
			assert!(
				end["ok"] == true && files(&end) > 0.0,
				"initial buildEnd = {end}, want ok with files"
			);
			let watching = events.next("watching");
			assert!(
				files(&watching) >= 4.0,
				"watching = {watching}, want the watched file count"
			);

			// Publish a complete edit: truncation can notify the OS watcher before the write lands,
			// producing an intermediate build.
			let scratch = tempfile::tempdir().expect("temp dir");
			let updated = scratch.path().join("main.ts");
			must_write(&updated, "export const s: string = 5;\n");
			std::fs::rename(&updated, root.join("left/src/main.ts")).expect("rename");
			let start = events.next("buildStart");
			assert!(
				changed(&start).is_some_and(|changed| changed.iter().any(|path| path == "left/src/main.ts")),
				"changed = {}, want left/src/main.ts",
				start["changed"]
			);
			let end = events.next("buildEnd");
			let diags = end["diagnostics"].as_array().cloned().unwrap_or_default();
			assert!(
				end["ok"] == false && !diags.is_empty(),
				"buildEnd = {end}, want failure with diagnostics"
			);
			let diag = &diags[0];
			assert!(
				diag["code"] == "TS2322" && diag["file"] == "left/src/main.ts",
				"diagnostic = {diag}, want TS2322 in left/src/main.ts"
			);
		}
	}
}
